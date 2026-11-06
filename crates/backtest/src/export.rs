//! Read-only, repeatable-read export; never downloads or synthesizes history.
use crate::data::{Dataset, Event, Record};
use anyhow::{Result, ensure};
use serde_json::Value;
use sqlx::{PgPool, Row};
pub async fn dataset(pool: &PgPool, start: i64, end: i64) -> Result<Dataset> {
    ensure!(
        start >= 0 && end > start && end - start <= 35 * 86_400_000,
        "invalid export window (maximum 35 days)"
    );
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let now: i64 =
        sqlx::query_scalar("SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint")
            .fetch_one(&mut *tx)
            .await?;
    ensure!(end <= now, "export end must be in the past");
    let mut records = vec![];
    // Last metadata known before capture, then actual changes during capture.
    let rows=sqlx::query("WITH prior AS (SELECT id FROM events WHERE kind='metadata' AND received_at<to_timestamp($1::double precision/1000) ORDER BY received_at DESC,id DESC LIMIT 1) SELECT ceil(extract(epoch FROM received_at)*1000)::bigint AS at,payload FROM events WHERE kind='metadata' AND (id IN (SELECT id FROM prior) OR (received_at>=to_timestamp($1::double precision/1000) AND received_at<to_timestamp($2::double precision/1000))) ORDER BY received_at,id LIMIT 2000001")
        .bind(start).bind(end).fetch_all(&mut *tx).await?;
    for r in rows {
        records.push(Record {
            available_at: r.get("at"),
            event: Event::Metadata {
                markets: serde_json::from_value(r.get::<Value, _>("payload"))?,
            },
        });
    }
    // At most 500 known warmup bars per series; later bars retain their actual finalization time.
    let rows=sqlx::query("WITH warmup AS (SELECT symbol,interval,open_time,row_number() OVER(PARTITION BY symbol,interval ORDER BY open_time DESC) AS n FROM candles WHERE finalized AND received_at<to_timestamp($1::double precision/1000)) SELECT ceil(extract(epoch FROM c.received_at)*1000)::bigint AS at,c.payload FROM candles c LEFT JOIN warmup w USING(symbol,interval,open_time) WHERE c.finalized AND (w.n<=500 OR (c.received_at>=to_timestamp($1::double precision/1000) AND c.received_at<to_timestamp($2::double precision/1000))) LIMIT 2000001")
        .bind(start).bind(end).fetch_all(&mut *tx).await?;
    for r in rows {
        let candle: trading_domain::data::Candle =
            serde_json::from_value(r.get::<Value, _>("payload"))?;
        records.push(Record {
            available_at: r.get::<i64, _>("at").max(candle.end + 1),
            event: Event::Candle { candle },
        });
    }
    let rows=sqlx::query("SELECT ceil(extract(epoch FROM received_at)*1000)::bigint AS at,payload FROM books WHERE received_at>=to_timestamp(($1::double precision-15000)/1000) AND received_at<to_timestamp($2::double precision/1000) LIMIT 2000001")
        .bind(start).bind(end).fetch_all(&mut *tx).await?;
    for r in rows {
        let book: trading_domain::data::Book =
            serde_json::from_value(r.get::<Value, _>("payload"))?;
        records.push(Record {
            available_at: r.get::<i64, _>("at").max(book.time),
            event: Event::Book { book },
        });
    }
    let rows=sqlx::query("SELECT symbol,observed_at,price FROM oracle_observations WHERE observed_at>=$1-15000 AND observed_at<$2 LIMIT 2000001").bind(start).bind(end).fetch_all(&mut *tx).await?;
    for r in rows {
        let at = r.get::<i64, _>("observed_at");
        records.push(Record {
            available_at: at,
            event: Event::Oracle {
                symbol: r.get("symbol"),
                observed_at: at,
                price: r.get::<String, _>("price").parse()?,
            },
        });
    }
    let rows=sqlx::query("SELECT symbol,hour,rate,ceil(extract(epoch FROM available_at)*1000)::bigint AS at FROM funding_rates WHERE hour>=$1 AND available_at<to_timestamp($2::double precision/1000) LIMIT 2000001").bind(start).bind(end).fetch_all(&mut *tx).await?;
    for r in rows {
        records.push(Record {
            available_at: r.get::<i64, _>("at").max(r.get("hour")),
            event: Event::Funding {
                symbol: r.get("symbol"),
                hour: r.get("hour"),
                rate: r.get::<String, _>("rate").parse()?,
            },
        });
    }
    tx.commit().await?;
    ensure!(
        records.len() <= 2_000_000,
        "export exceeds 2 million records; use a smaller capture window"
    );
    records.retain(|r| r.available_at < end);
    // Stable tie order independent of PostgreSQL query plans.
    let mut keyed = records
        .into_iter()
        .map(|r| Ok((r.available_at, serde_json::to_string(&r.event)?, r)))
        .collect::<Result<Vec<_>>>()?;
    keyed.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let data = Dataset {
        version: 1,
        source: "recorded_postgresql_receipt_times_v1".into(),
        capture_start: start,
        capture_end: end,
        records: keyed.into_iter().map(|(_, _, r)| r).collect(),
    };
    data.validate()?;
    Ok(data)
}
