//! Bounded, durable analysis of TESTNET candles. No signing or exchange-write boundary.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use trading_domain::data::{Candle, last_closed_open, now_ms};
use trading_quant::{ClosedCandle, Engine, Evaluation, config::Config as QuantConfig};

use crate::{client::InfoTransport, model};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub symbol: String,
    pub interval: String,
    pub strategy: String,
    pub duration_seconds: i64,
    pub poll_seconds: u64,
    /// Recorded preference for the subsequent execution phase, not an order permission.
    pub max_entries: u32,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.symbol == "BTC" && self.interval == "1m",
            "observer supports BTC 1m only"
        );
        ensure!(
            self.strategy == "ema_trend_v1",
            "observer supports EMA only"
        );
        ensure!(
            (60..=3600).contains(&self.duration_seconds),
            "duration must be 60..3600 seconds"
        );
        ensure!(
            (30..=300).contains(&self.poll_seconds),
            "poll must be 30..300 seconds"
        );
        ensure!(
            self.max_entries == 1,
            "session preference is one entry maximum"
        );
        Ok(())
    }
}

fn engine() -> Result<Engine> {
    Engine::builtin(QuantConfig::default())
}

/// Validate the entire response before selecting a complete, contiguous closed window.
/// The request-start clock is used, so a candle closing during HTTP transit is not finalized.
pub fn evaluate(value: Value, config: &Config, as_of: i64) -> Result<Evaluation> {
    config.validate()?;
    let engine = engine()?;
    let last = last_closed_open(as_of, 60_000);
    let first = last - (engine.window() as i64 - 1) * 60_000;
    let mut candles: Vec<Candle> = serde_json::from_value(value)?;
    ensure!(
        candles.len() <= engine.window() + 2,
        "oversized candle window"
    );
    for candle in &candles {
        candle.validate()?;
        ensure!(
            candle.s == config.symbol && candle.i == config.interval,
            "wrong candle market"
        );
        ensure!(
            candle.t >= first && candle.t <= as_of / 60_000 * 60_000,
            "candle outside requested window"
        );
    }
    candles.sort_by_key(|c| c.t);
    ensure!(
        candles.windows(2).all(|w| w[0].t != w[1].t),
        "duplicate candle"
    );
    let closed: Vec<_> = candles
        .into_iter()
        .filter(|c| c.t <= last)
        .map(|c| ClosedCandle::new(c, true, as_of))
        .collect::<Result<_>>()?;
    ensure!(
        closed.len() == engine.window(),
        "missing or stale closed candles"
    );
    let mut result = engine.evaluate(&closed)?;
    ensure!(
        result.open_time == last && result.window_start == first,
        "stale analysis window"
    );
    result.decisions.retain(|d| d.strategy == config.strategy);
    ensure!(result.decisions.len() == 1, "strategy unavailable");
    Ok(result)
}

pub async fn start(pool: &PgPool, account: &str, id: &str, config: &Config) -> Result<()> {
    model::command_id(id)?;
    config.validate()?;
    // Pin the algorithm as well as the operator configuration across restarts.
    let manifest = json!({"observer_version":1,"settings":config,"quant":engine()?.manifest()});
    let now = now_ms();
    sqlx::query("INSERT INTO testnet_observer_sessions(account,session_id,config,started_ms,expires_ms) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
        .bind(account).bind(id).bind(&manifest).bind(now).bind(now + config.duration_seconds * 1000)
        .execute(pool).await?;
    let saved: Value = sqlx::query_scalar(
        "SELECT config FROM testnet_observer_sessions WHERE account=$1 AND session_id=$2",
    )
    .bind(account)
    .bind(id)
    .fetch_one(pool)
    .await?;
    ensure!(
        saved == manifest,
        "session configuration changed; use a new session ID"
    );
    Ok(())
}

pub async fn status(pool: &PgPool, account: &str, id: &str) -> Result<Value> {
    let row = sqlx::query("SELECT config,started_ms,expires_ms,paused FROM testnet_observer_sessions WHERE account=$1 AND session_id=$2")
        .bind(account).bind(id).fetch_one(pool).await?;
    let expires: i64 = row.get("expires_ms");
    let latest: Option<Value> = sqlx::query_scalar("SELECT evaluation FROM testnet_observations WHERE account=$1 AND session_id=$2 ORDER BY close_ms DESC LIMIT 1")
        .bind(account).bind(id).fetch_optional(pool).await?;
    Ok(
        json!({"network":"TESTNET","mode":"observation_only","submitted":false,"session_id":id,
        "config":row.get::<Value,_>("config"),"started_ms":row.get::<i64,_>("started_ms"),
        "expires_ms":expires,"paused":row.get::<bool,_>("paused"),"expired":now_ms() >= expires,"latest_evaluation":latest}),
    )
}

pub async fn pause(pool: &PgPool, account: &str, id: &str) -> Result<()> {
    let result = sqlx::query(
        "UPDATE testnet_observer_sessions SET paused=TRUE WHERE account=$1 AND session_id=$2",
    )
    .bind(account)
    .bind(id)
    .execute(pool)
    .await?;
    ensure!(result.rows_affected() == 1, "unknown observer session");
    Ok(())
}

pub async fn tick<T: InfoTransport>(
    client: &T,
    pool: &PgPool,
    account: &str,
    id: &str,
    config: &Config,
) -> Result<Value> {
    start(pool, account, id, config).await?;
    let current = status(pool, account, id).await?;
    if current["paused"] == true || current["expired"] == true {
        return Ok(current);
    }
    let as_of = now_ms();
    let first = last_closed_open(as_of, 60_000) - (engine()?.window() as i64 - 1) * 60_000;
    let candles = client.info(json!({"type":"candleSnapshot","req":{"coin":config.symbol,"interval":config.interval,"startTime":first,"endTime":as_of}})).await?;
    let evaluation = evaluate(candles, config, as_of)?;
    let mut tx = pool.begin().await?;
    // Serialize persistence with pause, and reject expiry while the HTTP request was running.
    let row = sqlx::query("SELECT paused,expires_ms FROM testnet_observer_sessions WHERE account=$1 AND session_id=$2 FOR UPDATE")
        .bind(account).bind(id).fetch_one(&mut *tx).await?;
    if !row.get::<bool, _>("paused") && now_ms() < row.get::<i64, _>("expires_ms") {
        let value = serde_json::to_value(&evaluation)?;
        sqlx::query("INSERT INTO testnet_observations(account,session_id,close_ms,observed_ms,evaluation) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(account).bind(id).bind(evaluation.close_time).bind(now_ms()).bind(&value).execute(&mut *tx).await?;
        let saved: Value = sqlx::query_scalar("SELECT evaluation FROM testnet_observations WHERE account=$1 AND session_id=$2 AND close_ms=$3")
            .bind(account).bind(id).bind(evaluation.close_time).fetch_one(&mut *tx).await?;
        ensure!(
            saved == value,
            "closed candle history changed; observation rejected"
        );
    }
    tx.commit().await?;
    status(pool, account, id).await
}
