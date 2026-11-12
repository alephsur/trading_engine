//! Account-wide TESTNET monitoring. A stale monitor blocks entries at dispatch.
use crate::{
    client::{ExchangeTransport, InfoTransport},
    executor,
    model::{self, Intent, decimal},
    signing::TestnetSigner,
    store,
};
use anyhow::{Context, Result, ensure};
use rust_decimal::Decimal as D;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::BTreeSet;
use trading_domain::data::now_ms;
use trading_paper::dec;

pub const HEARTBEAT_MS: i64 = 60_000;

pub async fn enable(pool: &PgPool, account: &str) -> Result<()> {
    sqlx::query("INSERT INTO testnet_supervision(account,funding_through_ms) VALUES($1,COALESCE((SELECT min(created_ms) FROM testnet_requests WHERE account=$1),$2)) ON CONFLICT DO NOTHING")
        .bind(account).bind(now_ms()).execute(pool).await?;
    Ok(())
}

pub async fn status(pool: &PgPool, account: &str) -> Result<Value> {
    let row = sqlx::query("SELECT observed_ms,blocked,payload,funding_through_ms FROM testnet_supervision WHERE account=$1")
        .bind(account).fetch_one(pool).await?;
    let observed: i64 = row.get("observed_ms");
    let stale = !(0..HEARTBEAT_MS).contains(&(now_ms() - observed));
    let last_failure: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('observed_ms',observed_ms,'details',payload) FROM testnet_events WHERE account=$1 AND kind='monitor_unavailable' ORDER BY id DESC LIMIT 1")
        .bind(account).fetch_optional(pool).await?;
    Ok(
        json!({"network":"TESTNET","observed_ms":observed,"stale":stale,
        "entries_blocked":stale || row.get::<bool,_>("blocked"),
        "funding_through_ms":row.get::<i64,_>("funding_through_ms"),"assessment":row.get::<Value,_>("payload"),"last_failure":last_failure}),
    )
}

/// No secrets or arbitrary exchange error bodies are persisted in alerts.
pub async fn failed(pool: &PgPool, account: &str, reason: &str) -> Result<()> {
    failure(pool, account, reason, None).await
}

pub async fn failed_with_error(
    pool: &PgPool,
    account: &str,
    reason: &str,
    error: &anyhow::Error,
) -> Result<()> {
    failure(
        pool,
        account,
        reason,
        Some(crate::diagnostics::describe(error)),
    )
    .await
}

async fn failure(
    pool: &PgPool,
    account: &str,
    reason: &str,
    diagnostic: Option<Value>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,701))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE testnet_supervision SET blocked=true,payload=$2 WHERE account=$1")
        .bind(account)
        .bind(json!({"blockers":[reason],"safe_to_stop":false,"diagnostic":diagnostic}))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE testnet_observer_sessions SET paused=true WHERE account=$1 AND NOT paused")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    store::event(
        &mut tx,
        account,
        "supervision",
        "monitor_unavailable",
        json!({"reason":reason,"diagnostic":diagnostic}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

fn rows(value: &Value) -> Result<&Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid supervision response"))
}
fn ids(value: &Value) -> Result<BTreeSet<i64>> {
    let mut ids = BTreeSet::new();
    for row in rows(value)? {
        ensure!(
            ids.insert(
                row["oid"]
                    .as_i64()
                    .ok_or_else(|| anyhow::anyhow!("missing order id"))?
            ),
            "duplicate open order"
        );
    }
    Ok(ids)
}

/// Assess accepted exchange orders, not the submission acknowledgement.
pub fn assess(report: &Value, frontend: &Value, now: i64) -> Result<Value> {
    let snapshot = &report["latest_snapshot"];
    let observed = snapshot["observed_ms"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing snapshot clock"))?;
    ensure!(
        (0..15_000).contains(&(now - observed)),
        "stale supervision snapshot"
    );
    let state = &snapshot["account_state"];
    let equity = decimal(&state["marginSummary"]["accountValue"])?;
    ensure!(
        equity > D::ZERO && equity < dec("1000000000"),
        "invalid account equity"
    );
    let mut blockers = BTreeSet::new();
    let mut emergency = BTreeSet::new();
    let reconciled = snapshot["positions_reconciled"] == true;
    if !reconciled {
        blockers.insert("positions_unreconciled");
    }
    let actual_ids = ids(frontend)?;
    if actual_ids != ids(&snapshot["open_orders"])? {
        blockers.insert("open_orders_changed");
    }
    let requests = rows(&report["requests"])?;
    let mut known = BTreeSet::new();
    let mut unresolved = false;
    let mut terminal = true;
    for request in requests {
        for order in rows(&request["reconciliation"]["orders"])? {
            if let Some(id) = order["oid"].as_i64() {
                known.insert(id);
            }
        }
        let inactive = matches!(request["state"].as_str(), Some("rejected" | "expired"));
        if !inactive && request["reconciliation"]["complete"] != true {
            unresolved = true;
        }
        terminal &= inactive
            || (request["reconciliation"]["complete"] == true
                && request["reconciliation"]["all_terminal"] == true);
    }
    if unresolved {
        blockers.insert("orders_unreconciled");
    }
    if !actual_ids.is_subset(&known) {
        blockers.insert("unrecognized_open_order");
    }
    let mut positions = vec![];
    let mut exposure = D::ZERO;
    let mut symbols = BTreeSet::new();
    for p in rows(&state["assetPositions"])? {
        let p = &p["position"];
        let qty = decimal(&p["szi"])?;
        if qty == D::ZERO {
            continue;
        }
        let symbol = p["coin"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing position symbol"))?;
        ensure!(symbols.insert(symbol), "duplicate position");
        if !trading_domain::SYMBOLS.contains(&symbol) {
            blockers.insert("unsupported_position");
        }
        let isolated = p["leverage"]["type"] == "isolated" && p["leverage"]["value"] == 1;
        if !isolated {
            blockers.insert("unsafe_leverage");
        }
        let value = decimal(&p["positionValue"])?;
        ensure!(value >= D::ZERO, "negative position value");
        exposure += value;
        let owner = requests.iter().find(|r| {
            r["intent"]["kind"] == "entry"
                && r["intent"]["symbol"] == symbol
                && r["intent"]["buy"].as_bool() == Some(qty > D::ZERO)
                && r["reconciliation"]["orders"]
                    .as_array()
                    .and_then(|os| os.first())
                    .is_some_and(|o| {
                        decimal(&o["filled"]).is_ok_and(|q| q >= qty.abs())
                            && executor::terminal(
                                &json!({"status":"order","order":{"status":o["status"]}}),
                            )
                    })
        });
        let protection = |leg: u8, field: &str, order_type: &str| -> bool {
            let Some(owner) = owner else {
                return false;
            };
            let Some(command) = owner["command_id"].as_str() else {
                return false;
            };
            let cloid = model::cloid(report["account"].as_str().unwrap_or(""), command, leg);
            let oid = owner["reconciliation"]["orders"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["cloid"] == cloid)
                .and_then(|o| o["oid"].as_i64());
            rows(frontend).unwrap().iter().any(|o| {
                oid.is_some()
                    && o["oid"].as_i64() == oid
                    && o["coin"] == symbol
                    && o["side"] == if qty > D::ZERO { "A" } else { "B" }
                    && o["isTrigger"] == true
                    && o["reduceOnly"] == true
                    && o["orderType"] == order_type
                    && decimal(&o["sz"]).is_ok_and(|size| size >= qty.abs())
                    && decimal(&o["triggerPx"]).is_ok_and(|px| {
                        px > D::ZERO && Some(px) == decimal(&owner["intent"][field]).ok()
                    })
            })
        };
        let stop = protection(1, "stop", "Stop Market");
        let target = protection(2, "take_profit", "Take Profit Market");
        if !stop {
            blockers.insert("missing_protective_stop");
        }
        if !target {
            blockers.insert("missing_take_profit");
        }
        if !stop || !isolated {
            emergency.insert(symbol.to_owned());
        }
        positions.push(
            json!({"symbol":symbol,"quantity":qty,"position_value":value,
            "stop_confirmed":stop,"target_confirmed":target,
            "entry_command":owner.map(|r| r["command_id"].clone())}),
        );
    }
    if positions.len() > 1 {
        blockers.insert("testnet_position_limit");
    }
    if exposure > equity * dec("0.15") {
        blockers.insert("exposure_limit");
        emergency.extend(symbols.iter().map(|s| s.to_string()));
    }
    // Unknown children block entries, but must not prevent closing a reconciled partial
    // fill whose parent is terminal. Changing account observations still block any close.
    let reliable =
        reconciled && actual_ids == ids(&snapshot["open_orders"])? && actual_ids.is_subset(&known);
    if !reliable {
        emergency.clear();
    }
    let safe_to_stop =
        reliable && !unresolved && positions.is_empty() && actual_ids.is_empty() && terminal;
    if positions.is_empty() && !actual_ids.is_empty() {
        blockers.insert("orphan_open_orders");
    }
    Ok(
        json!({"observed_ms":observed,"equity":equity,"positions":positions,"blockers":blockers,
        "emergency_symbols":emergency,"reliable_for_close":reliable,"safe_to_stop":safe_to_stop}),
    )
}

pub async fn tick<T: InfoTransport>(client: &T, pool: &PgPool, account: &str) -> Result<Value> {
    enable(pool, account).await?;
    let result = cycle(client, pool, account).await;
    if let Err(error) = &result {
        let _ = failed_with_error(pool, account, "monitor_unavailable", error).await;
    }
    result
}

async fn cycle<T: InfoTransport>(client: &T, pool: &PgPool, account: &str) -> Result<Value> {
    let from: i64 =
        sqlx::query_scalar("SELECT funding_through_ms FROM testnet_supervision WHERE account=$1")
            .bind(account)
            .fetch_one(pool)
            .await?;
    let funding_end = now_ms();
    let funding = client.info(json!({"type":"userFunding","user":account,"startTime":from.saturating_sub(1),"endTime":funding_end})).await.context("supervision.funding")?;
    ensure!(
        rows(&funding)?.len() < 500,
        "funding history truncated; entry gate remains blocked"
    );
    let report = executor::reconcile(client, pool, account)
        .await
        .context("supervision.reconcile")?;
    let frontend = client
        .info(json!({"type":"frontendOpenOrders","user":account}))
        .await
        .context("supervision.open_orders")?;
    let mut assessment = assess(&report, &frontend, now_ms()).context("supervision.assess")?;
    let equity = decimal(&assessment["equity"])?;
    let observed = assessment["observed_ms"].as_i64().unwrap();
    let day = observed / 86_400_000;
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,701))")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    let prior = sqlx::query(
        "SELECT observed_ms,payload FROM testnet_supervision WHERE account=$1 FOR UPDATE",
    )
    .bind(account)
    .fetch_one(&mut *tx)
    .await?;
    ensure!(
        observed >= prior.get::<i64, _>("observed_ms"),
        "out-of-order supervision cycle"
    );
    for row in rows(&funding)? {
        let time = row["time"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("invalid funding time"))?;
        let symbol = row["delta"]["coin"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("invalid funding asset"))?;
        ensure!(
            time >= from.saturating_sub(1)
                && time <= funding_end
                && row["delta"]["type"] == "funding"
                && trading_domain::SYMBOLS.contains(&symbol),
            "invalid funding range/asset"
        );
        decimal(&row["delta"]["usdc"])?;
        sqlx::query("INSERT INTO testnet_funding(account,time_ms,symbol,payload) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(account).bind(time).bind(symbol).bind(row).execute(&mut *tx).await?;
        let saved: Value = sqlx::query_scalar(
            "SELECT payload FROM testnet_funding WHERE account=$1 AND time_ms=$2 AND symbol=$3",
        )
        .bind(account)
        .bind(time)
        .bind(symbol)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(saved == *row, "conflicting funding replay");
    }
    sqlx::query("INSERT INTO testnet_days(account,day,initial_equity) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(account).bind(day).bind(equity.to_string()).execute(&mut *tx).await?;
    let (initial, latched): (String,bool) = sqlx::query_as("SELECT initial_equity,loss_latched FROM testnet_days WHERE account=$1 AND day=$2 FOR UPDATE")
        .bind(account).bind(day).fetch_one(&mut *tx).await?;
    let loss = latched || equity <= initial.parse::<D>()? * dec("0.99");
    assessment["daily_initial_equity"] = json!(initial);
    assessment["daily_loss_latched"] = json!(loss);
    if loss {
        sqlx::query("UPDATE testnet_days SET loss_latched=true WHERE account=$1 AND day=$2")
            .bind(account)
            .bind(day)
            .execute(&mut *tx)
            .await?;
        assessment["blockers"]
            .as_array_mut()
            .unwrap()
            .push(json!("daily_loss_limit"));
        if assessment["reliable_for_close"] == true {
            assessment["emergency_symbols"] = json!(
                rows(&assessment["positions"])?
                    .iter()
                    .map(|p| p["symbol"].clone())
                    .collect::<Vec<_>>()
            );
        }
    }
    let blocked = !rows(&assessment["blockers"])?.is_empty();
    if blocked {
        sqlx::query(
            "UPDATE testnet_observer_sessions SET paused=true WHERE account=$1 AND NOT paused",
        )
        .bind(account)
        .execute(&mut *tx)
        .await?;
    }
    if prior.get::<Value, _>("payload")["blockers"] != assessment["blockers"] {
        store::event(
            &mut tx,
            account,
            "supervision",
            "supervision_changed",
            json!({"blockers":assessment["blockers"]}),
        )
        .await?;
    }
    // Keep external protection evidence after the position closes and current
    // status becomes flat. This never contains signing payloads or credentials.
    if !rows(&assessment["positions"])?.is_empty()
        || prior.get::<Value, _>("payload")["positions"]
            .as_array()
            .is_some_and(|positions| !positions.is_empty())
    {
        store::event(
            &mut tx,
            account,
            "supervision",
            "protection_observed",
            json!({"assessment":assessment,"frontend_open_orders":frontend}),
        )
        .await?;
    }
    sqlx::query("UPDATE testnet_supervision SET observed_ms=$2,blocked=$3,payload=$4,funding_through_ms=GREATEST(funding_through_ms,$5) WHERE account=$1")
        .bind(account).bind(observed).bind(blocked).bind(&assessment).bind(funding_end).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({"assessment":assessment,"report":report}))
}

/// At most one reduce-only recovery attempt for the entry owned by this session.
/// Never retry after an uncertain write, even under a different run/session ID.
pub async fn protect_once<T: InfoTransport + ExchangeTransport>(
    client: &T,
    pool: &PgPool,
    signer: &TestnetSigner,
    account: &str,
    entry: &str,
    cycle: &Value,
) -> Result<()> {
    let assessment = &cycle["assessment"];
    ensure!(
        (0..15_000).contains(&(now_ms() - assessment["observed_ms"].as_i64().unwrap_or(0))),
        "stale protection assessment"
    );
    if assessment["reliable_for_close"] != true {
        return Ok(());
    }
    for p in rows(&assessment["positions"])? {
        if p["entry_command"] != entry
            || !rows(&assessment["emergency_symbols"])?.contains(&p["symbol"])
        {
            continue;
        }
        let symbol = p["symbol"].as_str().unwrap();
        let id = format!(
            "protect-{}",
            model::cloid(account, entry, 8).trim_start_matches("0x")
        );
        let intent = Intent::Close {
            symbol: symbol.into(),
            limit: None,
        };
        if store::existing(pool, account, &id, &intent).await? {
            continue;
        }
        let (action, state) = executor::preflight(client, pool, account, &id, &intent).await?;
        // Recheck ownership/size after the new account read, before preparing a close.
        ensure!(
            model::position(&state, symbol)? == decimal(&p["quantity"])?,
            "position changed before protective close"
        );
        if store::prepare(pool, account, &id, &intent, action, signer, &state).await? {
            executor::dispatch(client, pool, account, &id).await?;
        }
    }
    Ok(())
}
