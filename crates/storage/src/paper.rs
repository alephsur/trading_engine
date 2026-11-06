//! State, order/fill ledger and signal consumption commit atomically.
use anyhow::{Result, ensure};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use trading_domain::{
    ListedMarket,
    data::{Book, now_ms},
};
use trading_paper::{Account, Config, Funding, Input, Signal};
pub const ACCOUNT: &str = "paper-default";
pub async fn initialize(pool: &PgPool, config: Config) -> Result<()> {
    let state = Account::new(config.clone(), now_ms())?;
    sqlx::query(
        "INSERT INTO paper_accounts(account_id,state) VALUES ($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(ACCOUNT)
    .bind(serde_json::to_value(state)?)
    .execute(pool)
    .await?;
    let saved = read(pool).await?;
    ensure!(
        saved.config == config,
        "paper configuration differs from persisted account; do not reset an existing ledger"
    );
    saved.validate()?;
    reconcile(&mut *pool.acquire().await?, &saved).await?;
    Ok(())
}
pub async fn read(pool: &PgPool) -> Result<Account> {
    Ok(serde_json::from_value(
        sqlx::query_scalar::<_, Value>("SELECT state FROM paper_accounts WHERE account_id=$1")
            .bind(ACCOUNT)
            .fetch_one(pool)
            .await?,
    )?)
}
/// Independently reconstruct quantity and cash from immutable fills and funding.
async fn reconcile(connection: &mut sqlx::PgConnection, state: &Account) -> Result<()> {
    let orders: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM paper_orders WHERE account_id=$1 ORDER BY order_id",
    )
    .bind(ACCOUNT)
    .fetch_all(&mut *connection)
    .await?;
    let mut positions = BTreeMap::<i64, (Decimal, Decimal, bool, String)>::new();
    let mut protections = BTreeMap::new();
    let mut cash = state.config.capital;
    let mut sequence = 0;
    for value in orders {
        let order: trading_paper::Order = serde_json::from_value(value)?;
        ensure!(order.id == sequence + 1, "order sequence gap");
        sequence = order.id;
        let filled: Decimal = order.fills.iter().map(|f| f.quantity).sum();
        ensure!(
            filled == order.filled && filled <= order.requested,
            "unreconciled fills"
        );
        if filled == Decimal::ZERO {
            continue;
        }
        let notional: Decimal = order.fills.iter().map(|f| f.quantity * f.price).sum();
        cash -= order.fills.iter().map(|f| f.fee).sum::<Decimal>();
        if order.reduce_only {
            let p = positions
                .get_mut(&order.position_id)
                .ok_or_else(|| anyhow::anyhow!("exit without entry"))?;
            ensure!(
                p.0 >= filled && p.2 != order.buy && p.3 == order.symbol,
                "invalid reduce-only ledger"
            );
            cash += (notional - p.1 * filled) * if p.2 { Decimal::ONE } else { -Decimal::ONE };
            p.0 -= filled;
        } else {
            ensure!(
                !positions.contains_key(&order.position_id),
                "duplicate position entry"
            );
            protections.insert(order.position_id, (order.stop, order.take_profit));
            positions.insert(
                order.position_id,
                (filled, notional / filled, order.buy, order.symbol),
            );
        }
    }
    let funding: Vec<Value> =
        sqlx::query_scalar("SELECT payload FROM paper_funding WHERE account_id=$1")
            .bind(ACCOUNT)
            .fetch_all(&mut *connection)
            .await?;
    for item in funding {
        cash += serde_json::from_value::<Decimal>(item["cash_change"].clone())?;
    }
    // Decimal distributivity can differ at the final 28th digit.
    ensure!(
        (cash - state.cash).abs() < trading_paper::dec("0.000000000001")
            && sequence == state.sequence,
        "cash/order ledger mismatch"
    );
    positions.retain(|_, p| p.0 > Decimal::ZERO);
    ensure!(
        positions.len() == state.positions.len(),
        "position ledger count mismatch"
    );
    for p in &state.positions {
        let q = positions
            .get(&p.id)
            .ok_or_else(|| anyhow::anyhow!("position missing in ledger"))?;
        ensure!(
            protections.get(&p.id) == Some(&(p.stop, p.take_profit)),
            "protection ledger mismatch"
        );
        ensure!(
            q.0 == p.quantity && q.1 == p.entry && q.2 == p.long && q.3 == p.symbol,
            "position ledger mismatch"
        );
    }
    Ok(())
}
pub async fn tick(
    pool: &PgPool,
    config_id: &str,
    ready: bool,
    markets: &[ListedMarket],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let value: Value =
        sqlx::query_scalar("SELECT state FROM paper_accounts WHERE account_id=$1 FOR UPDATE")
            .bind(ACCOUNT)
            .fetch_one(&mut *tx)
            .await?;
    let mut state: Account = serde_json::from_value(value)?;
    reconcile(&mut tx, &state).await?;
    // No high-water cursor: sequence IDs can commit out of order across producers.
    let rows=sqlx::query("SELECT s.event_id,s.symbol,s.direction,s.payload,e.payload AS evaluation FROM signals s JOIN evaluations e USING(config_id,symbol,interval,open_time) LEFT JOIN paper_decisions d ON d.account_id=$1 AND d.signal_id=s.event_id WHERE s.config_id=$2 AND d.signal_id IS NULL ORDER BY s.event_id LIMIT 100")
        .bind(ACCOUNT).bind(config_id).fetch_all(&mut *tx).await?;
    let mut signals = Vec::new();
    for row in rows {
        let payload: Value = row.get("payload");
        let evaluation: Value = row.get("evaluation");
        signals.push(Signal {
            event_id: row.get("event_id"),
            symbol: row.get("symbol"),
            long: row.get::<String, _>("direction") == "long",
            close_time: payload["close_time"].as_i64().unwrap_or(0),
            atr: serde_json::from_value(evaluation["indicators"]["atr_14"].clone())
                .unwrap_or(Decimal::ZERO),
        });
    }
    let mut books = BTreeMap::new();
    for symbol in trading_domain::SYMBOLS {
        let value: Option<Value> = sqlx::query_scalar(
            "SELECT payload FROM books WHERE symbol=$1 ORDER BY exchange_time DESC LIMIT 1",
        )
        .bind(symbol)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(value) = value {
            let book: Book = serde_json::from_value(value)?;
            books.insert(symbol.into(), book);
        }
    }
    let oracle_rows=sqlx::query("SELECT DISTINCT ON (symbol) symbol,observed_at,price FROM oracle_observations ORDER BY symbol,observed_at DESC").fetch_all(&mut *tx).await?;
    let mut oracles = BTreeMap::new();
    for row in oracle_rows {
        oracles.insert(
            row.get::<String, _>("symbol"),
            (
                row.get::<i64, _>("observed_at"),
                row.get::<String, _>("price").parse()?,
            ),
        );
    }
    let now = now_ms();
    let earliest = state
        .funding_due
        .iter()
        .map(|f| f.hour)
        .chain(
            state
                .positions
                .iter()
                .map(|p| p.funding_through + trading_paper::HOUR),
        )
        .min()
        .unwrap_or(now);
    let rows=sqlx::query("SELECT r.symbol,r.hour,r.rate,o.price FROM funding_rates r JOIN LATERAL (SELECT price FROM oracle_observations WHERE symbol=r.symbol AND observed_at<=r.hour AND observed_at>=r.hour-15000 ORDER BY observed_at DESC LIMIT 1) o ON true WHERE r.hour >= $1 AND r.hour <= $2")
        .bind(earliest).bind(now).fetch_all(&mut *tx).await?;
    let mut funding = Vec::new();
    for row in rows {
        funding.push(Funding {
            symbol: row.get("symbol"),
            hour: row.get("hour"),
            rate: row.get::<String, _>("rate").parse()?,
            oracle: row.get::<String, _>("price").parse()?,
        });
    }
    let previous_blockers = state.blockers.clone();
    let changes = state.tick(
        &Input {
            now,
            ready,
            markets,
            books: &books,
            funding: &funding,
            oracles: &oracles,
        },
        &signals,
    )?;
    if previous_blockers != state.blockers {
        sqlx::query("INSERT INTO paper_events(account_id,payload) VALUES ($1,$2)").bind(ACCOUNT).bind(json!({"time":now,"kind":"circuit_breakers","previous":previous_blockers,"current":state.blockers})).execute(&mut *tx).await?;
    }
    for d in &changes.decisions {
        sqlx::query("INSERT INTO paper_decisions(account_id,signal_id,payload) VALUES ($1,$2,$3)")
            .bind(ACCOUNT)
            .bind(d.signal_id)
            .bind(serde_json::to_value(d)?)
            .execute(&mut *tx)
            .await?;
    }
    for o in &changes.orders {
        sqlx::query("INSERT INTO paper_orders(account_id,order_id,payload) VALUES ($1,$2,$3)")
            .bind(ACCOUNT)
            .bind(o.id)
            .bind(serde_json::to_value(o)?)
            .execute(&mut *tx)
            .await?;
    }
    for f in &changes.funding {
        sqlx::query(
            "INSERT INTO paper_funding(account_id,position_id,hour,payload) VALUES ($1,$2,$3,$4)",
        )
        .bind(ACCOUNT)
        .bind(f["position_id"].as_i64().unwrap())
        .bind(f["hour"].as_i64().unwrap())
        .bind(f)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE paper_accounts SET state=$2,updated_at=clock_timestamp() WHERE account_id=$1",
    )
    .bind(ACCOUNT)
    .bind(serde_json::to_value(&state)?)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
pub async fn oracle(pool: &PgPool, context: &trading_domain::data::AssetContext) -> Result<()> {
    sqlx::query("INSERT INTO oracle_observations(symbol,observed_at,price) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING").bind(&context.coin).bind(now_ms()).bind(context.ctx.oracle_px.to_string()).execute(pool).await?;
    Ok(())
}
pub async fn rates(pool: &PgPool, rates: &[trading_domain::data::FundingRate]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for r in rates {
        let hour = r.time / trading_paper::HOUR * trading_paper::HOUR;
        sqlx::query(
            "INSERT INTO funding_rates(symbol,hour,rate) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
        )
        .bind(&r.coin)
        .bind(hour)
        .bind(r.funding_rate.to_string())
        .execute(&mut *tx)
        .await?;
        let saved: String =
            sqlx::query_scalar("SELECT rate FROM funding_rates WHERE symbol=$1 AND hour=$2")
                .bind(&r.coin)
                .bind(hour)
                .fetch_one(&mut *tx)
                .await?;
        ensure!(
            saved.parse::<Decimal>()? == r.funding_rate,
            "inconsistent funding revision"
        );
    }
    tx.commit().await?;
    Ok(())
}
pub async fn ledger(pool: &PgPool, kind: &str, limit: i64) -> Result<Value> {
    ensure!((1..=500).contains(&limit), "invalid limit");
    let query = match kind {
        "events" => {
            "SELECT payload FROM paper_events WHERE account_id=$1 ORDER BY event_id DESC LIMIT $2"
        }
        "orders" | "fills" => {
            "SELECT payload FROM paper_orders WHERE account_id=$1 ORDER BY order_id DESC LIMIT $2"
        }
        "decisions" => {
            "SELECT payload FROM paper_decisions WHERE account_id=$1 ORDER BY signal_id DESC LIMIT $2"
        }
        "funding" => {
            "SELECT payload FROM paper_funding WHERE account_id=$1 ORDER BY hour DESC,position_id DESC LIMIT $2"
        }
        _ => anyhow::bail!("unknown ledger"),
    };
    let rows: Vec<Value> = sqlx::query_scalar(query)
        .bind(ACCOUNT)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    if kind == "fills" {
        return Ok(json!(rows.iter().flat_map(|r|r["fills"].as_array().into_iter().flatten().map(|f|json!({"order_id":r["id"],"symbol":r["symbol"],"buy":r["buy"],"time":r["time"],"fill":f}))).collect::<Vec<_>>()));
    }
    Ok(json!(rows))
}
