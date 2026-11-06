use anyhow::Result;
use sqlx::PgPool;
use trading_domain::{
    SYMBOLS,
    data::{INTERVALS, interval_ms, last_closed_open, now_ms},
};
use trading_market_data::history::PublicClient;
use trading_storage::market;

pub async fn run(app: &crate::App) -> Result<()> {
    for symbol in SYMBOLS {
        let trades = app.client.recent_trades(symbol).await?;
        market::trades(&app.pool, &trades).await?;
    }
    loop {
        let ready = recover(&app.pool, &app.client).await?;
        {
            let mut feed = app.feed.write().await;
            feed.history_ready = ready;
            feed.last_recovery = Some(std::time::Instant::now());
        }
        tracing::info!(event = "history_checked", complete = ready);
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

/// Runs beside the WebSocket; one bounded, paced pass at a time.
pub async fn recover(pool: &PgPool, client: &PublicClient) -> Result<bool> {
    let mut complete = true;
    for symbol in SYMBOLS {
        for interval in INTERVALS {
            let step = interval_ms(interval)?;
            let end = last_closed_open(now_ms(), step);
            let start = market::anchor(pool, symbol, interval, end - 99 * step).await?;
            let missing = market::missing(pool, symbol, interval, start, end, step).await?;
            for (first, window_end) in windows(&missing, end, step) {
                let candles = client.candles(symbol, interval, first, window_end).await?;
                market::candles(pool, &candles, true).await?;
            }
            let missing = market::missing(pool, symbol, interval, start, end, step).await?;
            market::coverage(pool, symbol, interval, end, missing.len() as i64).await?;
            if !missing.is_empty() {
                complete = false;
            }
        }
    }
    trading_storage::record(
        pool,
        "candle_recovery",
        serde_json::json!({"complete":complete}),
    )
    .await?;
    Ok(complete)
}

// Leave two candles of margin within the exchange's rolling 5000 limit.
fn windows(missing: &[i64], end: i64, step: i64) -> Vec<(i64, i64)> {
    let floor = end - 4_997 * step;
    let mut remaining = missing.iter().copied().filter(|t| *t >= floor).peekable();
    let mut windows = Vec::new();
    while let Some(first) = remaining.next() {
        let window_end = (first + 499 * step).min(end);
        windows.push((first, window_end));
        while remaining.peek().is_some_and(|t| *t <= window_end) {
            remaining.next();
        }
    }
    windows
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_pages_and_excludes_unrecoverable_history() {
        let step = 60_000;
        let missing: Vec<_> = (0..=6_000).map(|i| i * step).collect();
        let windows = windows(&missing, 6_000 * step, step);
        assert_eq!(windows.len(), 10);
        assert_eq!(windows[0].0, 1_003 * step);
        assert_eq!(windows.last().unwrap().1, 6_000 * step);
        assert!(windows.iter().all(|(a, b)| (b - a) / step < 500));
        // The excluded prefix remains in the DB gap query; it cannot become complete.
        assert!(missing.iter().any(|t| *t < windows[0].0));
    }
    #[test]
    fn handles_sparse_internal_holes_without_repeated_requests() {
        assert_eq!(
            windows(&[0, 60_000, 600 * 60_000], 600 * 60_000, 60_000),
            vec![(0, 499 * 60_000), (600 * 60_000, 600 * 60_000)]
        );
        assert!(windows(&[], 0, 60_000).is_empty());
    }
}
