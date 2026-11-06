use crate::paper::{ACCOUNT, reconcile};
use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use trading_paper::{
    Account, Order,
    monitor::{Monitor, equity},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Pause,
    Resume,
    CloseAll,
    ClosePosition { position_id: i64 },
}
#[derive(Clone, Debug, Serialize)]
pub struct Command {
    pub command_id: String,
    #[serde(flatten)]
    pub action: Action,
}
impl<'de> Deserialize<'de> for Command {
    fn deserialize<T: serde::Deserializer<'de>>(
        deserializer: T,
    ) -> std::result::Result<Self, T::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            command_id: String,
            action: String,
            position_id: Option<i64>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let action = match (wire.action.as_str(), wire.position_id) {
            ("pause", None) => Action::Pause,
            ("resume", None) => Action::Resume,
            ("close_all", None) => Action::CloseAll,
            ("close_position", Some(position_id)) => Action::ClosePosition { position_id },
            _ => {
                return Err(serde::de::Error::custom(
                    "invalid control action or position",
                ));
            }
        };
        Ok(Self {
            command_id: wire.command_id,
            action,
        })
    }
}
impl Command {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (8..=64).contains(&self.command_id.len())
                && self
                    .command_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid command identifier"
        );
        if let Action::ClosePosition { position_id } = self.action {
            ensure!(position_id > 0, "invalid position identifier");
        }
        Ok(())
    }
}
/// Serialized with the execution worker. A confirmed pause cannot race a later entry.
pub async fn command(pool: &PgPool, command: &Command) -> Result<Value> {
    command.validate()?;
    let mut tx = pool.begin().await?;
    let value: Value =
        sqlx::query_scalar("SELECT state FROM paper_accounts WHERE account_id=$1 FOR UPDATE")
            .bind(ACCOUNT)
            .fetch_one(&mut *tx)
            .await?;
    let request = serde_json::to_value(command)?;
    let previous: Option<(Value, Value)> = sqlx::query_as(
        "SELECT request,response FROM paper_commands WHERE account_id=$1 AND command_id=$2",
    )
    .bind(ACCOUNT)
    .bind(&command.command_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((original, response)) = previous {
        ensure!(
            original == request,
            "command identifier reused with different intent"
        );
        tx.commit().await?;
        return Ok(response);
    }
    let mut account: Account = serde_json::from_value(value)?;
    account.validate()?;
    reconcile(&mut tx, &account).await?;
    match command.action {
        Action::Pause => account.entries_paused = true,
        Action::Resume => account.entries_paused = false,
        Action::CloseAll => {
            account.entries_paused = true;
            for p in &mut account.positions {
                p.closing = true;
            }
        }
        Action::ClosePosition { position_id } => {
            let p = account
                .positions
                .iter_mut()
                .find(|p| p.id == position_id)
                .ok_or_else(|| anyhow::anyhow!("position no longer open"))?;
            p.closing = true;
            account.entries_paused = true;
        }
    }
    // Keep the pause visible immediately, even if the next worker tick cannot reach the database.
    account.blockers.retain(|b| b != "operator_paused");
    if account.entries_paused {
        account.blockers.push("operator_paused".into());
    }
    let response = json!({"command_id":command.command_id,"accepted":true,"mode":"PAPER","entries_paused":account.entries_paused,"closing_positions":account.positions.iter().filter(|p|p.closing).map(|p|p.id).collect::<Vec<_>>()});
    sqlx::query(
        "INSERT INTO paper_commands(account_id,command_id,request,response) VALUES ($1,$2,$3,$4)",
    )
    .bind(ACCOUNT)
    .bind(&command.command_id)
    .bind(&request)
    .bind(&response)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO paper_events(account_id,payload) VALUES ($1,$2)").bind(ACCOUNT).bind(json!({"kind":"operator_command","time":trading_domain::data::now_ms(),"command":request,"result":response})).execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE paper_accounts SET state=$2,updated_at=clock_timestamp() WHERE account_id=$1",
    )
    .bind(ACCOUNT)
    .bind(serde_json::to_value(account)?)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(response)
}
pub(crate) async fn observe(tx: &mut Transaction<'_, Postgres>, account: &Account) -> Result<()> {
    let prior: Option<Value> =
        sqlx::query_scalar("SELECT payload FROM paper_monitor WHERE account_id=$1")
            .bind(ACCOUNT)
            .fetch_optional(&mut **tx)
            .await?;
    let mut monitor = match prior {
        Some(v) => serde_json::from_value::<Monitor>(v)?,
        None => Monitor::new(account.last_tick),
    };
    let value = equity(account);
    monitor.observe(account.last_tick, value);
    sqlx::query("INSERT INTO paper_monitor(account_id,payload) VALUES ($1,$2) ON CONFLICT(account_id) DO UPDATE SET payload=EXCLUDED.payload").bind(ACCOUNT).bind(serde_json::to_value(&monitor)?).execute(&mut **tx).await?;
    let point = json!({"time":account.last_tick,"equity":value,"cash":account.cash,"open_positions":account.positions.len(),"blockers":account.blockers});
    sqlx::query("INSERT INTO paper_equity(account_id,observed_at,payload) VALUES ($1,$2,$3) ON CONFLICT(account_id,observed_at) DO UPDATE SET payload=EXCLUDED.payload").bind(ACCOUNT).bind(account.last_tick).bind(point).execute(&mut **tx).await?;
    Ok(())
}
pub async fn snapshot(pool: &PgPool) -> Result<Value> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let value: Value = sqlx::query_scalar("SELECT state FROM paper_accounts WHERE account_id=$1")
        .bind(ACCOUNT)
        .fetch_one(&mut *tx)
        .await?;
    let account: Account = serde_json::from_value(value)?;
    let monitor: Option<Value> =
        sqlx::query_scalar("SELECT payload FROM paper_monitor WHERE account_id=$1")
            .bind(ACCOUNT)
            .fetch_optional(&mut *tx)
            .await?;
    let monitor: Option<Monitor> = monitor.map(serde_json::from_value).transpose()?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM paper_orders WHERE account_id=$1 ORDER BY order_id",
    )
    .bind(ACCOUNT)
    .fetch_all(&mut *tx)
    .await?;
    let orders: Vec<Order> = rows
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<std::result::Result<_, _>>()?;
    let funding: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM paper_funding WHERE account_id=$1 ORDER BY hour,position_id",
    )
    .bind(ACCOUNT)
    .fetch_all(&mut *tx)
    .await?;
    let trades = trading_backtest::metrics::trades(&orders, &funding, &account)?;
    let events: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM paper_events WHERE account_id=$1 ORDER BY event_id DESC LIMIT 50",
    )
    .bind(ACCOUNT)
    .fetch_all(&mut *tx)
    .await?;
    let decisions:Vec<Value>=sqlx::query_scalar("SELECT d.payload || jsonb_build_object('symbol',s.symbol,'strategy',s.strategy,'direction',s.direction,'close_time',s.payload->'close_time') FROM paper_decisions d JOIN signals s ON s.event_id=d.signal_id WHERE d.account_id=$1 ORDER BY d.signal_id DESC LIMIT 50").bind(ACCOUNT).fetch_all(&mut *tx).await?;
    let curve:Vec<Value>=sqlx::query_scalar("SELECT payload FROM (SELECT DISTINCT ON (observed_at/60000) observed_at,payload FROM paper_equity WHERE account_id=$1 ORDER BY observed_at/60000 DESC,observed_at DESC LIMIT 1440) points ORDER BY observed_at").bind(ACCOUNT).fetch_all(&mut *tx).await?;
    let errors: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM events WHERE kind='runtime_error' ORDER BY id DESC LIMIT 20",
    )
    .fetch_all(&mut *tx)
    .await?;
    let now = trading_domain::data::now_ms();
    let current = (0..15000).contains(&(now - account.last_tick));
    let latest = trading_backtest::metrics::Point {
        time: account.last_tick,
        equity: if current { equity(&account) } else { None },
        cash: account.cash,
        open_positions: account.positions.len(),
        blockers: account.blockers.clone(),
    };
    let complete = monitor.as_ref().is_some_and(|m| m.complete) && current;
    let mut metrics = trading_backtest::metrics::calculate(
        &[latest],
        &[],
        &trades,
        account.config.capital,
        complete,
        monitor
            .as_ref()
            .map_or(D::ZERO, |m| m.observed_max_drawdown),
    );
    if complete {
        let (value, status) =
            trading_backtest::metrics::sharpe(&monitor.as_ref().unwrap().daily_returns);
        metrics.daily_returns = monitor.as_ref().unwrap().daily_returns.clone();
        metrics.sharpe_daily = value;
        metrics.sharpe_annualized_iid = value.map(|r| r * 365_f64.sqrt());
        metrics.sharpe_status = status;
    }
    let mut positions = vec![];
    let mut asset_statistics = vec![];
    for symbol in trading_domain::SYMBOLS {
        let book: Option<Value> = sqlx::query_scalar(
            "SELECT payload FROM books WHERE symbol=$1 ORDER BY exchange_time DESC LIMIT 1",
        )
        .bind(symbol)
        .fetch_optional(&mut *tx)
        .await?;
        for p in account.positions.iter().filter(|p| p.symbol == symbol) {
            let book = book
                .clone()
                .map(serde_json::from_value::<trading_domain::data::Book>)
                .transpose()?;
            let mark = book
                .filter(|b| current && (0..15000).contains(&(now - b.time)) && b.validate().is_ok())
                .map(|b| b.levels[usize::from(!p.long)][0].px);
            let pnl =
                mark.map(|px| (px - p.entry) * p.quantity * if p.long { D::ONE } else { -D::ONE });
            positions.push(json!({"position":p,"mark":mark,"unrealized_pnl":pnl,"notional":mark.map(|px|px*p.quantity)}));
        }
        let selected: Vec<_> = trades.iter().filter(|t| t.symbol == symbol).collect();
        asset_statistics.push(json!({"symbol":symbol,"closed_trades":selected.iter().filter(|t|t.closed_at.is_some()&&t.settled).count(),"realized_net":selected.iter().map(|t|t.net_realized).sum::<D>(),"fees":selected.iter().map(|t|t.fees).sum::<D>(),"funding":selected.iter().map(|t|t.funding).sum::<D>()}));
    }
    tx.commit().await?;
    Ok(
        json!({"mode":"PAPER","generated_at":now,"account":account,"account_current":current,"monitor":monitor,"metrics":metrics,"positions":positions,"asset_statistics":asset_statistics,"equity_curve":curve,"orders":rows.iter().rev().take(50).collect::<Vec<_>>(),"decisions":decisions,"events":events,"errors":errors,"funding":funding.iter().rev().take(50).collect::<Vec<_>>(),"trade_history":trades.iter().rev().take(100).collect::<Vec<_>>(),"history_limits":{"orders":50,"decisions":50,"events":50,"trade_history":100,"curve_minutes":1440}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_contract_round_trips_and_rejects_invalid_actions() {
        for action in [
            Action::Pause,
            Action::Resume,
            Action::CloseAll,
            Action::ClosePosition { position_id: 1 },
        ] {
            let command = Command {
                command_id: "test-command-1".into(),
                action,
            };
            let value = serde_json::to_value(&command).unwrap();
            let decoded: Command = serde_json::from_value(value.clone()).unwrap();
            decoded.validate().unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        assert!(
            serde_json::from_value::<Command>(
                json!({"command_id":"test-command-1","action":"live"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<Command>(
                json!({"command_id":"test-command-1","action":"pause","capital":"1000000"})
            )
            .is_err()
        );
    }
}
