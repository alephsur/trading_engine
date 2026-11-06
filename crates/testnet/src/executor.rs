use crate::{
    client::Client,
    model::{self, Intent, decimal},
    signing::TestnetSigner,
    store,
};
use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde_json::{Value, json};
use sqlx::PgPool;
use trading_domain::data::{Book, now_ms};

pub async fn preflight(
    client: &Client,
    pool: &PgPool,
    account: &str,
    id: &str,
    intent: &Intent,
) -> Result<(Value, Value)> {
    let started = std::time::Instant::now();
    model::command_id(id)?;
    let markets = client.markets().await?;
    let state = client.account(account).await?;
    let opens = client
        .info(json!({"type":"openOrders","user":account}))
        .await?;
    let opens = opens
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid open order response"))?;
    let action = match intent {
        Intent::Entry { symbol, .. } => {
            ensure!(opens.is_empty(), "entry requires no open account orders");
            let book: Book =
                serde_json::from_value(client.info(json!({"type":"l2Book","coin":symbol})).await?)?;
            model::entry(intent, account, id, &markets, &book, &state, now_ms())?
        }
        Intent::Close { symbol, .. } => {
            let book: Book =
                serde_json::from_value(client.info(json!({"type":"l2Book","coin":symbol})).await?)?;
            model::close(intent, account, id, &markets, &book, &state, now_ms())?
        }
        Intent::Cancel { target } | Intent::Cleanup { target } => {
            let request: Value = sqlx::query_scalar(
                "SELECT signed_request FROM testnet_requests WHERE account=$1 AND command_id=$2",
            )
            .bind(account)
            .bind(target)
            .fetch_one(pool)
            .await?;
            let orders = request["action"]["orders"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("target is not an order request"))?;
            let cleanup = matches!(intent, Intent::Cleanup { .. });
            if cleanup {
                ensure!(model::flat(&state)?, "cleanup requires a flat account");
                let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM testnet_requests WHERE account=$1 AND intent->>'kind'='entry' AND expires_ms>$2)").bind(account).bind(now_ms()-30_000).fetch_one(pool).await?;
                ensure!(
                    !pending,
                    "wait for all entry signing windows to expire before cleanup"
                );
                let parent = orders
                    .first()
                    .ok_or_else(|| anyhow::anyhow!("missing parent"))?;
                let status = client
                    .info(json!({"type":"orderStatus","user":account,"oid":parent["c"]}))
                    .await?;
                ensure!(
                    terminal(&status),
                    "parent must be confirmed terminal before cleanup"
                );
                let open = client
                    .info(json!({"type":"frontendOpenOrders","user":account}))
                    .await?;
                ensure!(
                    open.as_array().is_some_and(|rows| rows
                        .iter()
                        .all(|o| o["isTrigger"] == true && o["reduceOnly"] == true)),
                    "non-protective open orders prevent cleanup"
                );
            }
            // Ordinary cancellation never removes protective trigger orders.
            let cancels: Vec<Value> = orders
                .iter()
                .filter(|o| {
                    if cleanup {
                        o["r"] == true && o["t"].get("trigger").is_some()
                    } else {
                        o["t"].get("limit").is_some()
                    }
                })
                .map(|o| json!({"asset":o["a"],"cloid":o["c"]}))
                .collect();
            ensure!(
                !cancels.is_empty(),
                "no cancellable entry parent; protective orders are preserved"
            );
            json!({"type":"cancelByCloid","cancels":cancels})
        }
    };
    ensure!(
        started.elapsed() < std::time::Duration::from_secs(15),
        "preflight expired; refresh account and market data"
    );
    Ok((action, state))
}
pub async fn submit(
    client: &Client,
    pool: &PgPool,
    signer: &TestnetSigner,
    account: &str,
    id: &str,
    intent: &Intent,
) -> Result<Value> {
    // A repeated CLI command never rebuilds/resigns a previously submitted intent.
    if !store::existing(pool, account, id, intent).await? {
        let (action, state) = preflight(client, pool, account, id, intent).await?;
        store::prepare(pool, account, id, intent, action, signer, &state).await?;
    }
    dispatch(client, pool, account, id).await?;
    store::report(pool, account).await
}
pub async fn dispatch<T: crate::client::ExchangeTransport>(
    transport: &T,
    pool: &PgPool,
    account: &str,
    id: &str,
) -> Result<()> {
    if let Some(request) = store::claim(pool, account, id).await? {
        let expected = request["action"]["orders"]
            .as_array()
            .or_else(|| request["action"]["cancels"].as_array())
            .map_or(0, Vec::len);
        // Failure deliberately leaves the durable uncertainty marker; no POST retry.
        if let Ok(response) = transport.send(&request).await {
            store::record_response(pool, account, id, response, expected).await?;
        }
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct Observation {
    pub cloid: String,
    pub value: Value,
}
fn observed_order(value: &Value) -> Option<&Value> {
    (value["status"] == "order").then_some(&value["order"]["order"])
}
pub fn terminal(value: &Value) -> bool {
    let Some(s) = value["order"]["status"].as_str() else {
        return false;
    };
    // Only documented terminal forms; new/unknown statuses stay blocked.
    value["status"] == "order"
        && (matches!(s, "filled" | "canceled" | "rejected")
            || s.ends_with("Canceled")
            || s.ends_with("Rejected"))
}
/// Reconcile original quantity, remaining quantity and unique fills. No synthetic fills.
pub fn reconcile_orders(observations: &[Observation], fills: &[Value]) -> Result<Value> {
    let mut complete = true;
    let mut terminal_all = true;
    let mut rows = vec![];
    for observation in observations {
        let Some(order) = observed_order(&observation.value) else {
            complete = false;
            terminal_all = false;
            rows.push(json!({"cloid":observation.cloid,"known":false}));
            continue;
        };
        ensure!(
            order["cloid"] == observation.cloid,
            "exchange returned a different client order id"
        );
        let oid = order["oid"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("invalid order id"))?;
        let original = decimal(&order["origSz"])?;
        let remaining = decimal(&order["sz"])?;
        ensure!(
            original >= D::ZERO && remaining >= D::ZERO && remaining <= original,
            "invalid order quantities"
        );
        let mut ids = std::collections::BTreeSet::new();
        let mut filled = D::ZERO;
        let mut fees = D::ZERO;
        for fill in fills.iter().filter(|f| f["oid"].as_i64() == Some(oid)) {
            let tid = fill["tid"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing fill id"))?;
            if !ids.insert(tid) {
                continue;
            }
            ensure!(
                fill["coin"] == order["coin"] && fill["side"] == order["side"],
                "fill/order mismatch"
            );
            let size = decimal(&fill["sz"])?;
            ensure!(
                size > D::ZERO && decimal(&fill["px"])? > D::ZERO,
                "invalid fill"
            );
            ensure!(fill["feeToken"] == "USDC", "unsupported fill fee currency");
            filled += size;
            fees += decimal(&fill["fee"])?;
        }
        let matched = filled == original - remaining;
        complete &= matched;
        terminal_all &= terminal(&observation.value);
        rows.push(json!({"cloid":observation.cloid,"oid":oid,"status":observation.value["order"]["status"],"original":original,"remaining":remaining,"filled":filled,"fees":fees,"fills_reconciled":matched}));
    }
    Ok(json!({"complete":complete,"all_terminal":terminal_all,"orders":rows}))
}
pub async fn reconcile<T: crate::client::InfoTransport>(
    client: &T,
    pool: &PgPool,
    account: &str,
) -> Result<Value> {
    let requests:Vec<(String,Value,i64,String,Value)>=sqlx::query_as("SELECT command_id,signed_request,created_ms,state,intent FROM testnet_requests WHERE account=$1 ORDER BY created_ms LIMIT 101").bind(account).fetch_all(pool).await?;
    ensure!(
        requests.len() <= 100,
        "testnet harness limit reached: archive/export and review before further operation"
    );
    let start = requests.first().map_or(now_ms(), |r| r.2) - 1000;
    let fills=client.info(json!({"type":"userFillsByTime","user":account,"startTime":start,"aggregateByTime":false})).await?;
    let fills = fills
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid fills response"))?;
    ensure!(
        fills.len() < 2000,
        "fill history response truncated; reconciliation blocked"
    );
    let mut observations = vec![];
    for (_, request, _, _, intent) in &requests {
        if let Some(orders) = request["action"]["orders"].as_array() {
            for order in orders {
                let cloid = order["c"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("missing persisted cloid"))?
                    .to_owned();
                let value = client
                    .info(json!({"type":"orderStatus","user":account,"oid":cloid}))
                    .await?;
                if let Some(remote) = observed_order(&value) {
                    ensure!(
                        remote["coin"] == intent["symbol"]
                            && remote["side"] == if order["b"] == true { "B" } else { "A" },
                        "observed order does not match persisted intent"
                    );
                    ensure!(
                        decimal(&remote["origSz"])? <= decimal(&order["s"])?,
                        "observed quantity exceeds requested quantity"
                    );
                }
                observations.push(Observation { cloid, value });
            }
        }
    }
    let state = client
        .info(json!({"type":"clearinghouseState","user":account}))
        .await?;
    let opens = client
        .info(json!({"type":"openOrders","user":account}))
        .await?;
    let open_count = opens
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid open order response"))?
        .len();
    let mut tx = pool.begin().await?;
    let known: std::collections::BTreeSet<_> = observations
        .iter()
        .filter_map(|o| observed_order(&o.value).and_then(|v| v["oid"].as_i64()))
        .collect();
    for fill in fills
        .iter()
        .filter(|f| f["oid"].as_i64().is_some_and(|id| known.contains(&id)))
    {
        let oid = fill["oid"].as_i64().unwrap();
        let tid = fill["tid"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("invalid fill id"))?;
        sqlx::query("INSERT INTO testnet_fills(account,oid,tid,payload) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(account).bind(oid).bind(tid).bind(fill).execute(&mut *tx).await?;
        let saved: Value = sqlx::query_scalar(
            "SELECT payload FROM testnet_fills WHERE account=$1 AND oid=$2 AND tid=$3",
        )
        .bind(account)
        .bind(oid)
        .bind(tid)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(saved == *fill, "conflicting fill replay");
    }
    let saved: Vec<Value> =
        sqlx::query_scalar("SELECT payload FROM testnet_fills WHERE account=$1")
            .bind(account)
            .fetch_all(&mut *tx)
            .await?;
    let mut positions_match = true;
    for p in state["assetPositions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid account positions"))?
    {
        if decimal(&p["position"]["szi"])? != D::ZERO
            && !trading_domain::SYMBOLS.contains(&p["position"]["coin"].as_str().unwrap_or(""))
        {
            positions_match = false;
        }
    }
    for symbol in trading_domain::SYMBOLS {
        let mut net = D::ZERO;
        for f in saved.iter().filter(|f| f["coin"] == symbol) {
            ensure!(
                matches!(f["side"].as_str(), Some("B" | "A")),
                "invalid fill side"
            );
            net += decimal(&f["sz"])? * if f["side"] == "B" { D::ONE } else { -D::ONE };
        }
        positions_match &= net == model::position(&state, symbol)?;
    }
    let snapshot = json!({"observed_ms":now_ms(),"account_state":state,"open_orders":opens,"positions_reconciled":positions_match,"fill_response_truncated":false,"full_exchange_history_available":false});
    sqlx::query("INSERT INTO testnet_snapshots(account,observed_ms,payload) VALUES ($1,$2,$3)")
        .bind(account)
        .bind(now_ms())
        .bind(&snapshot)
        .execute(&mut *tx)
        .await?;
    for (id, request, _, status, _) in &requests {
        let cloids: Vec<&str> = if let Some(orders) = request["action"]["orders"].as_array() {
            orders.iter().filter_map(|o| o["c"].as_str()).collect()
        } else {
            request["action"]["cancels"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid persisted action"))?
                .iter()
                .filter_map(|o| o["cloid"].as_str())
                .collect()
        };
        let selected: Vec<_> = observations
            .iter()
            .filter(|o| cloids.contains(&o.cloid.as_str()))
            .cloned()
            .collect();
        let mut report = reconcile_orders(&selected, &saved)?;
        let safe = positions_match
            && model::flat(&state)?
            && open_count == 0
            && (matches!(status.as_str(), "rejected" | "expired")
                || (selected.len() == cloids.len()
                    && !selected.is_empty()
                    && report["complete"] == true
                    && report["all_terminal"] == true));
        report["safe_for_entry"] = json!(safe);
        report["observed_ms"] = json!(now_ms());
        sqlx::query(
            "UPDATE testnet_requests SET reconciliation=$3 WHERE account=$1 AND command_id=$2",
        )
        .bind(account)
        .bind(id)
        .bind(&report)
        .execute(&mut *tx)
        .await?;
        store::event(&mut tx, account, id, "reconciled", report).await?;
    }
    tx.commit().await?;
    store::report(pool, account).await
}
