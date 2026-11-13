//! Explicit synthetic TESTNET signal, never written to natural observations.
use crate::{
    automation,
    client::{ExchangeTransport, InfoTransport},
    executor,
    model::Intent,
    observer,
    runtime::{Shutdown, SupervisorLock},
    signing::TestnetSigner,
    store, supervision,
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sqlx::PgPool;
use trading_domain::data::{Book, last_closed_open, now_ms};

pub async fn run<T: InfoTransport + ExchangeTransport>(
    client: &T,
    pool: &PgPool,
    signer: &TestnetSigner,
    account: &str,
    id: &str,
    owner: &mut SupervisorLock,
    shutdown: &mut Shutdown,
) -> Result<Value> {
    crate::model::command_id(id)?;
    let close_id = format!("{id}-close");
    crate::model::command_id(&close_id)?;
    let already: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM testnet_events WHERE account=$1 AND command_id=$2 AND kind='synthetic_signal')")
        .bind(account).bind(id).fetch_one(pool).await?;
    if !already {
        let cycle = supervision::tick(client, pool, account).await?;
        ensure!(
            cycle["assessment"]["safe_to_stop"] == true
                && cycle["assessment"]["blockers"] == json!([]),
            "probe requires reconciled flat account with no orders or blockers"
        );
        let collision: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM testnet_requests WHERE account=$1 AND command_id IN ($2,$3))")
            .bind(account).bind(id).bind(&close_id).fetch_one(pool).await?;
        ensure!(
            !collision,
            "probe ID already belongs to an existing request"
        );
        let config: observer::Config = serde_json::from_str(include_str!(
            "../../../config/testnet-confirmed-breakout.json"
        ))?;
        let as_of = now_ms();
        let first = last_closed_open(as_of, 3_600_000) - 99 * 3_600_000;
        let candles = client.info(json!({"type":"candleSnapshot","req":{"coin":"BTC","interval":"1h","startTime":first,"endTime":as_of}})).await?;
        let source = observer::evaluate(candles, &config, as_of)?;
        automation::leverage(
            &client
                .info(json!({"type":"activeAssetData","user":account,"coin":"BTC"}))
                .await?,
            account,
        )?;
        let markets = client.markets().await?;
        let state = client.account(account).await?;
        let book: Book =
            serde_json::from_value(client.info(json!({"type":"l2Book","coin":"BTC"})).await?)?;
        let emitted = now_ms();
        let intent = crate::model::plan("BTC", true, &markets, &book, &state, emitted)?;
        owner.check().await?;
        ensure!(!shutdown.requested(), "probe stopped before reservation");
        let mut tx = pool.begin().await?;
        store::event(&mut tx, account, id, "synthetic_signal", json!({"synthetic":true,"risk_profile":"existing_integration_stop_1pct_target_2pct","direction":"long","emitted_ms":emitted,"source_evaluation":source,"intent":intent,"natural_acceptance":false,"exit_policy":"single reduce-only IOC after protection assessment"})).await?;
        tx.commit().await?;
        // Reservation is single use, even if a crash happens before preparation.
        ensure!(
            (0..120_000).contains(&(now_ms() - emitted)),
            "synthetic signal expired"
        );
        executor::submit(client, pool, signer, account, id, &intent).await?;
    }
    let started = std::time::Instant::now();
    loop {
        owner.check().await?;
        ensure!(
            !shutdown.requested(),
            "probe stopped; reconcile existing IDs, never replace the entry ID"
        );
        let cycle = supervision::tick(client, pool, account).await?;
        let assessment = &cycle["assessment"];
        if assessment["safe_to_stop"] == true {
            let evidence: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'observed_ms',observed_ms,'payload',payload) FROM testnet_events WHERE account=$1 AND (command_id=$2 OR (kind='protection_observed' AND payload->'assessment'->'positions' @> $3::jsonb)) ORDER BY id")
                .bind(account).bind(id).bind(json!([{"entry_command":id}])).fetch_all(pool).await?;
            let requests: Vec<Value> = cycle["report"]["requests"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["command_id"] == id || r["command_id"] == close_id)
                .cloned()
                .collect();
            return Ok(
                json!({"network":"TESTNET","synthetic":true,"natural_acceptance":false,"entry_id":id,"close_id":close_id,"evidence":evidence,"requests":requests,"final_assessment":assessment}),
            );
        }
        if assessment["reliable_for_close"] == true {
            for position in assessment["positions"].as_array().unwrap() {
                if position["entry_command"] == id {
                    let intent = Intent::Close {
                        symbol: "BTC".into(),
                        limit: None,
                    };
                    if !store::existing(pool, account, &close_id, &intent).await? {
                        // Only the position attributed to this entry may be closed.
                        let (action, state) =
                            executor::preflight(client, pool, account, &close_id, &intent).await?;
                        ensure!(
                            crate::model::position(&state, "BTC")?
                                == crate::model::decimal(&position["quantity"])?,
                            "position changed before probe close"
                        );
                        if store::prepare(pool, account, &close_id, &intent, action, signer, &state)
                            .await?
                        {
                            executor::dispatch(client, pool, account, &close_id).await?;
                        }
                    }
                }
            }
        }
        ensure!(
            started.elapsed() < std::time::Duration::from_secs(600),
            "probe requires operator review; no additional entry or close attempts allowed"
        );
        tokio::select! { _ = shutdown.wait() => {}, _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {} }
    }
}
