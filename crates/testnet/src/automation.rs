//! Explicitly armed, single-attempt TESTNET sessions. Never retries a consumed entry.
use crate::{
    client::Client,
    executor,
    model::{self, Intent},
    observer,
    signing::TestnetSigner,
    store,
};
use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use trading_domain::{
    ListedMarket,
    data::{Book, now_ms},
};
use trading_paper::{dec, price};
use trading_quant::{Evaluation, strategies::Direction};

pub fn leverage(value: &Value, account: &str) -> Result<()> {
    ensure!(
        value["user"]
            .as_str()
            .is_some_and(|s| s.eq_ignore_ascii_case(account))
            && value["coin"] == "BTC"
            && value["leverage"]["type"] == "isolated"
            && value["leverage"]["value"].as_u64() == Some(1),
        "BTC must use isolated margin at 1x"
    );
    Ok(())
}

pub fn signal(e: &Evaluation, started: i64, now: i64) -> Result<bool> {
    ensure!(
        e.symbol == "BTC"
            && e.interval == "1m"
            && e.decisions.len() == 1
            && e.decisions[0].strategy == "ema_trend_v1",
        "unexpected automatic strategy"
    );
    ensure!(
        e.close_time >= started && (1..=120_000).contains(&(now - e.close_time)),
        "signal predates session or is stale"
    );
    match e.decisions[0].direction {
        Direction::Long => Ok(true),
        Direction::Short => Ok(false),
        Direction::Hold => anyhow::bail!("no EMA cross"),
    }
}

/// ATR-based protection layered over the existing conservative liquidity/risk planner.
pub fn plan(
    e: &Evaluation,
    markets: &[ListedMarket],
    book: &Book,
    state: &Value,
    started: i64,
    now: i64,
) -> Result<Intent> {
    let buy = signal(e, started, now)?;
    let atr = e
        .indicators
        .as_ref()
        .and_then(|s| s.atr_14)
        .ok_or_else(|| anyhow::anyhow!("ATR unavailable"))?;
    let mid = (book.levels[0]
        .first()
        .ok_or_else(|| anyhow::anyhow!("empty bids"))?
        .px
        + book.levels[1]
            .first()
            .ok_or_else(|| anyhow::anyhow!("empty asks"))?
            .px)
        / dec("2");
    ensure!(
        mid > D::ZERO && atr > D::ZERO && atr / mid <= dec("0.03"),
        "ATR outside risk limits"
    );
    let base = model::plan("BTC", buy, markets, book, state, now)?;
    let Intent::Entry {
        mut quantity,
        limit,
        ..
    } = base
    else {
        unreachable!()
    };
    let decimals = model::market(markets, "BTC")?.metadata.sz_decimals;
    let distance = (atr * dec("2")).max(mid * dec("0.005"));
    let stop = price(
        if buy {
            limit - distance
        } else {
            limit + distance
        },
        decimals,
        !buy,
    );
    let take_profit = price(
        if buy {
            limit + distance * dec("2")
        } else {
            limit - distance * dec("2")
        },
        decimals,
        !buy,
    );
    let worst = price(
        stop * if buy { dec("0.98") } else { dec("1.02") },
        decimals,
        !buy,
    );
    let equity = model::decimal(&state["marginSummary"]["accountValue"])?;
    quantity = quantity
        .min(equity * dec("0.0025") / ((limit - worst).abs() + limit * dec("0.002")))
        .round_dp_with_strategy(decimals, rust_decimal::RoundingStrategy::ToZero);
    let intent = Intent::Entry {
        symbol: "BTC".into(),
        buy,
        quantity,
        limit,
        stop,
        take_profit,
    };
    model::entry(
        &intent,
        "unsigned-auto",
        "unsigned-auto",
        markets,
        book,
        state,
        now,
    )?;
    Ok(intent)
}

pub async fn start(
    pool: &PgPool,
    account: &str,
    id: &str,
    config: &observer::Config,
) -> Result<()> {
    observer::start(pool, account, id, config).await?;
    sqlx::query("INSERT INTO testnet_auto_sessions(account,session_id,started_ms) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(account).bind(id).bind(now_ms()).execute(pool).await?;
    Ok(())
}

pub async fn status(pool: &PgPool, account: &str, id: &str) -> Result<Value> {
    let mut value = observer::status(pool, account, id).await?;
    let started: i64 = sqlx::query_scalar(
        "SELECT started_ms FROM testnet_auto_sessions WHERE account=$1 AND session_id=$2",
    )
    .bind(account)
    .bind(id)
    .fetch_one(pool)
    .await?;
    let claim: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('command_id',c.command_id,'close_ms',c.close_ms,'claimed_ms',c.claimed_ms,'intent',c.intent,'transport_state',r.state,'reconciliation',r.reconciliation) FROM testnet_auto_claims c LEFT JOIN testnet_requests r ON r.account=c.account AND r.command_id=c.command_id WHERE c.account=$1 AND c.session_id=$2")
        .bind(account).bind(id).fetch_optional(pool).await?;
    value.as_object_mut().unwrap().remove("submitted");
    value["mode"] = json!("testnet_single_attempt");
    value["auto_started_ms"] = json!(started);
    value["entry_budget_consumed"] = json!(claim.is_some());
    value["claim"] = json!(claim);
    Ok(value)
}

/// Durably consume the session and globally deduplicate this strategy's signal.
/// A crash after commit is deliberately a lost opportunity, never permission to retry.
pub async fn claim(
    pool: &PgPool,
    account: &str,
    id: &str,
    e: &Evaluation,
    intent: &Intent,
) -> Result<Option<String>> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query("SELECT o.paused,o.expires_ms,a.started_ms FROM testnet_observer_sessions o JOIN testnet_auto_sessions a USING(account,session_id) WHERE o.account=$1 AND o.session_id=$2 FOR UPDATE OF o")
        .bind(account).bind(id).fetch_one(&mut *tx).await?;
    ensure!(
        !row.get::<bool, _>("paused") && now_ms() < row.get::<i64, _>("expires_ms"),
        "automatic session paused or expired"
    );
    signal(e, row.get("started_ms"), now_ms())?;
    let saved: Value = sqlx::query_scalar("SELECT evaluation FROM testnet_observations WHERE account=$1 AND session_id=$2 AND close_ms=$3")
        .bind(account).bind(id).bind(e.close_time).fetch_one(&mut *tx).await?;
    ensure!(
        saved == serde_json::to_value(e)?,
        "signal differs from persisted observation"
    );
    let command = format!(
        "auto-{}",
        model::cloid(account, id, 7).trim_start_matches("0x")
    );
    let count = sqlx::query("INSERT INTO testnet_auto_claims(account,session_id,close_ms,command_id,intent,claimed_ms) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
        .bind(account).bind(id).bind(e.close_time).bind(&command).bind(serde_json::to_value(intent)?).bind(now_ms()).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    Ok((count == 1).then_some(command))
}

pub async fn tick(
    client: &Client,
    pool: &PgPool,
    signer: &TestnetSigner,
    account: &str,
    id: &str,
    config: &observer::Config,
) -> Result<Value> {
    start(pool, account, id, config).await?;
    let current = status(pool, account, id).await?;
    if current["paused"] == true
        || current["expired"] == true
        || current["entry_budget_consumed"] == true
    {
        // Recovery never sends an old prepared/uncertain command again.
        return Ok(current);
    }
    let report = executor::reconcile(client, pool, account).await?;
    let snapshot = &report["latest_snapshot"];
    ensure!(
        snapshot["positions_reconciled"] == true
            && model::flat(&snapshot["account_state"])?
            && snapshot["open_orders"]
                .as_array()
                .is_some_and(|v| v.is_empty()),
        "account must reconcile flat with no orders"
    );
    let observed = observer::tick(client, pool, account, id, config).await?;
    if observed["paused"] == true || observed["expired"] == true {
        return status(pool, account, id).await;
    }
    let e: Evaluation = serde_json::from_value(observed["latest_evaluation"].clone())?;
    let started = current["auto_started_ms"].as_i64().unwrap();
    if e.close_time < started || e.decisions.iter().all(|d| d.direction == Direction::Hold) {
        return status(pool, account, id).await;
    }
    let markets = client.markets().await?;
    let state = client.account(account).await?;
    let book: Book =
        serde_json::from_value(client.info(json!({"type":"l2Book","coin":"BTC"})).await?)?;
    let intent = plan(&e, &markets, &book, &state, started, now_ms())?;
    let command = format!(
        "auto-{}",
        model::cloid(account, id, 7).trim_start_matches("0x")
    );
    let (action, state) = executor::preflight(client, pool, account, &command, &intent).await?;
    leverage(
        &client
            .info(json!({"type":"activeAssetData","user":account,"coin":"BTC"}))
            .await?,
        account,
    )?;
    if claim(pool, account, id, &e, &intent).await?.is_some() {
        store::prepare(pool, account, &command, &intent, action, signer, &state).await?;
        executor::dispatch(client, pool, account, &command).await?;
        executor::reconcile(client, pool, account).await?;
    }
    status(pool, account, id).await
}
