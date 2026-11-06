use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use trading_domain::data::{Candle, interval_ms, now_ms};
use trading_quant::{ClosedCandle, Engine, strategies::Direction};

pub async fn register(pool: &PgPool, engine: &Engine) -> Result<()> {
    sqlx::query(
        "INSERT INTO signal_configs (config_id,manifest) VALUES ($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(engine.id())
    .bind(engine.manifest())
    .execute(pool)
    .await?;
    Ok(())
}
/// One bounded batch; decisions, directional signals, and cursor share a transaction.
pub async fn process_series(
    pool: &PgPool,
    engine: &Engine,
    symbol: &str,
    interval: &str,
) -> Result<bool> {
    let step = interval_ms(interval)?;
    let mut tx = pool.begin().await?;
    let Some(coverage)=sqlx::query("SELECT start_time,checked_through,missing_count FROM candle_coverage WHERE symbol=$1 AND interval=$2")
        .bind(symbol).bind(interval).fetch_optional(&mut *tx).await? else { return Ok(false); };
    if coverage.get::<i64, _>("missing_count") != 0 {
        return Ok(false);
    }
    let anchor: i64 = coverage.get("start_time");
    let end: i64 = coverage.get("checked_through");
    if end < anchor {
        return Ok(false);
    }
    sqlx::query("INSERT INTO signal_cursors (config_id,symbol,interval,last_open_time) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(engine.id()).bind(symbol).bind(interval).bind(anchor-step).execute(&mut *tx).await?;
    let cursor:i64=sqlx::query_scalar("SELECT last_open_time FROM signal_cursors WHERE config_id=$1 AND symbol=$2 AND interval=$3 FOR UPDATE")
        .bind(engine.id()).bind(symbol).bind(interval).fetch_one(&mut *tx).await?;
    if cursor >= end {
        tx.commit().await?;
        return Ok(true);
    }
    let batch_end = (cursor + 500 * step).min(end);
    let start = (cursor - (engine.window() as i64 - 2) * step).max(anchor);
    let rows:Vec<Value>=sqlx::query_scalar("SELECT payload FROM candles WHERE symbol=$1 AND interval=$2 AND finalized AND open_time BETWEEN $3 AND $4 ORDER BY open_time")
        .bind(symbol).bind(interval).bind(start).bind(batch_end).fetch_all(&mut *tx).await?;
    ensure!(
        rows.len() as i64 == (batch_end - start) / step + 1,
        "closed candle gap during analysis"
    );
    let mut history = Vec::new();
    let as_of = now_ms();
    for row in rows {
        let c: Candle = serde_json::from_value(row)?;
        let opened = c.t;
        history.push(ClosedCandle::new(c, true, as_of)?);
        if opened <= cursor {
            continue;
        }
        let evaluation = engine.evaluate(&history)?;
        let payload = serde_json::to_value(&evaluation)?;
        sqlx::query("INSERT INTO evaluations (config_id,symbol,interval,open_time,payload) VALUES ($1,$2,$3,$4,$5)")
            .bind(engine.id()).bind(symbol).bind(interval).bind(opened).bind(&payload).execute(&mut *tx).await?;
        for decision in &evaluation.decisions {
            if decision.direction == Direction::Hold {
                continue;
            }
            let direction = match decision.direction {
                Direction::Long => "long",
                Direction::Short => "short",
                Direction::Hold => unreachable!(),
            };
            let signal = json!({"config_id":engine.id(),"symbol":symbol,"interval":interval,"open_time":opened,"close_time":evaluation.close_time,"input_hash":evaluation.input_hash,"strategy":decision.strategy,"direction":direction,"reason":decision.reason,"analysis_only":true});
            sqlx::query("INSERT INTO signals (config_id,symbol,interval,open_time,strategy,direction,payload) VALUES ($1,$2,$3,$4,$5,$6,$7)")
                .bind(engine.id()).bind(symbol).bind(interval).bind(opened).bind(&decision.strategy).bind(direction).bind(signal).execute(&mut *tx).await?;
        }
    }
    sqlx::query("UPDATE signal_cursors SET last_open_time=$4 WHERE config_id=$1 AND symbol=$2 AND interval=$3")
        .bind(engine.id()).bind(symbol).bind(interval).bind(batch_end).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(batch_end == end)
}
pub async fn evaluations(
    pool: &PgPool,
    config: &str,
    symbol: &str,
    interval: &str,
    limit: i64,
) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT payload FROM evaluations WHERE config_id=$1 AND symbol=$2 AND interval=$3 ORDER BY open_time DESC LIMIT $4")
        .bind(config).bind(symbol).bind(interval).bind(limit).fetch_all(pool).await?)
}
pub async fn signals(
    pool: &PgPool,
    config: &str,
    symbol: &str,
    interval: &str,
    limit: i64,
) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT payload || jsonb_build_object('event_id',event_id) FROM signals WHERE config_id=$1 AND symbol=$2 AND interval=$3 ORDER BY event_id DESC LIMIT $4")
        .bind(config).bind(symbol).bind(interval).bind(limit).fetch_all(pool).await?)
}
pub async fn cursors(pool: &PgPool, config: &str) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('symbol',symbol,'interval',interval,'last_open_time',last_open_time) FROM signal_cursors WHERE config_id=$1 ORDER BY symbol,interval")
        .bind(config).fetch_all(pool).await?)
}
