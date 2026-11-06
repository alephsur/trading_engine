use anyhow::Result;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use trading_domain::data::{Book, Candle, Trade};

pub async fn candles(pool: &PgPool, candles: &[Candle], finalized: bool) -> Result<()> {
    let mut tx = pool.begin().await?;
    for c in candles {
        c.validate()?;
        anyhow::ensure!(
            !finalized
                || c.t
                    <= trading_domain::data::last_closed_open(
                        trading_domain::data::now_ms(),
                        trading_domain::data::interval_ms(&c.i)?
                    ),
            "cannot finalize open candle"
        );
        sqlx::query("INSERT INTO candles (symbol, interval, open_time, close_time, trade_count, finalized, payload) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (symbol,interval,open_time) DO UPDATE SET trade_count=EXCLUDED.trade_count, finalized=EXCLUDED.finalized, payload=EXCLUDED.payload, received_at=clock_timestamp() WHERE NOT candles.finalized AND (EXCLUDED.finalized OR EXCLUDED.trade_count > candles.trade_count)")
            .bind(&c.s).bind(&c.i).bind(c.t).bind(c.end).bind(c.n).bind(finalized).bind(serde_json::to_value(c)?).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn trades(pool: &PgPool, trades: &[Trade]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for t in trades {
        t.validate()?;
        sqlx::query("INSERT INTO trades (symbol,exchange_time,trade_id,payload) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&t.coin).bind(t.time).bind(t.tid).bind(serde_json::to_value(t)?).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
pub async fn book(pool: &PgPool, book: &Book) -> Result<()> {
    book.validate()?;
    sqlx::query(
        "INSERT INTO books (symbol,exchange_time,payload) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(&book.coin)
    .bind(book.time)
    .bind(serde_json::to_value(book)?)
    .execute(pool)
    .await?;
    Ok(())
}
pub async fn anchor(pool: &PgPool, symbol: &str, interval: &str, initial: i64) -> Result<i64> {
    sqlx::query("INSERT INTO candle_coverage (symbol,interval,start_time,checked_through) VALUES ($1,$2,$3,$3-1) ON CONFLICT DO NOTHING")
        .bind(symbol).bind(interval).bind(initial).execute(pool).await?;
    Ok(
        sqlx::query_scalar(
            "SELECT start_time FROM candle_coverage WHERE symbol=$1 AND interval=$2",
        )
        .bind(symbol)
        .bind(interval)
        .fetch_one(pool)
        .await?,
    )
}
/// Detect internal holes too, not just a gap after the newest row.
pub async fn missing(
    pool: &PgPool,
    symbol: &str,
    interval: &str,
    start: i64,
    end: i64,
    step: i64,
) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar("SELECT t FROM generate_series($3::bigint,$4::bigint,$5::bigint) t LEFT JOIN candles c ON c.symbol=$1 AND c.interval=$2 AND c.open_time=t AND c.finalized WHERE c.open_time IS NULL ORDER BY t")
        .bind(symbol).bind(interval).bind(start).bind(end).bind(step).fetch_all(pool).await?)
}
pub async fn coverage(
    pool: &PgPool,
    symbol: &str,
    interval: &str,
    end: i64,
    missing_count: i64,
) -> Result<()> {
    sqlx::query("UPDATE candle_coverage SET checked_through=$3,missing_count=$4,updated_at=clock_timestamp() WHERE symbol=$1 AND interval=$2")
        .bind(symbol).bind(interval).bind(end).bind(missing_count).execute(pool).await?;
    Ok(())
}
pub async fn read_candles(
    pool: &PgPool,
    symbol: &str,
    interval: &str,
    limit: i64,
    closed_only: bool,
) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('candle',payload,'closed',finalized) FROM candles WHERE symbol=$1 AND interval=$2 AND (NOT $4 OR finalized) ORDER BY open_time DESC LIMIT $3")
        .bind(symbol).bind(interval).bind(limit).bind(closed_only).fetch_all(pool).await?)
}
pub async fn read_trades(pool: &PgPool, symbol: &str, limit: i64) -> Result<Vec<Value>> {
    Ok(sqlx::query_scalar("SELECT payload FROM trades WHERE symbol=$1 ORDER BY exchange_time DESC,trade_id DESC LIMIT $2").bind(symbol).bind(limit).fetch_all(pool).await?)
}
pub async fn read_book(pool: &PgPool, symbol: &str) -> Result<Option<Value>> {
    Ok(sqlx::query_scalar(
        "SELECT payload FROM books WHERE symbol=$1 ORDER BY exchange_time DESC LIMIT 1",
    )
    .bind(symbol)
    .fetch_optional(pool)
    .await?)
}
pub async fn read_coverage(pool: &PgPool) -> Result<Vec<Value>> {
    let rows = sqlx::query("SELECT symbol,interval,start_time,checked_through,missing_count FROM candle_coverage ORDER BY symbol,interval").fetch_all(pool).await?;
    Ok(rows.iter().map(|r| json!({"symbol":r.get::<String,_>("symbol"),"interval":r.get::<String,_>("interval"),"start_time":r.get::<i64,_>("start_time"),"checked_through":r.get::<i64,_>("checked_through"),"missing_count":r.get::<i64,_>("missing_count")})).collect())
}
