use crate::{App, fresh, history_ready};
use std::time::{Duration, Instant};
use trading_domain::data::now_ms;
use trading_paper::HOUR;

pub async fn run(app: App) {
    loop {
        let (ready, markets) = {
            let feed = app.feed.read().await;
            (
                fresh(&feed) && history_ready(&feed) && feed.analysis_ready,
                serde_json::from_value::<Vec<trading_domain::ListedMarket>>(feed.markets.clone())
                    .unwrap_or_default(),
            )
        };
        match trading_storage::paper::tick(&app.pool, app.engine.id(), ready, &markets).await {
            Ok(()) => app.feed.write().await.paper_update = Some(Instant::now()),
            Err(error) => {
                app.feed.write().await.paper_update = None;
                tracing::warn!(event="paper_tick_blocked",reason=%error);
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
pub async fn funding(app: App) {
    loop {
        if let Err(error) = sync_funding(&app).await {
            tracing::warn!(event="funding_sync_failed",reason=%error);
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
async fn sync_funding(app: &App) -> anyhow::Result<()> {
    let account = trading_storage::paper::read(&app.pool).await?;
    for symbol in trading_domain::SYMBOLS {
        let first = account
            .funding_due
            .iter()
            .filter(|d| d.symbol == symbol)
            .map(|d| d.hour)
            .chain(
                account
                    .positions
                    .iter()
                    .filter(|p| p.symbol == symbol)
                    .map(|p| p.funding_through + HOUR),
            )
            .min();
        if let Some(first) = first {
            let now = now_ms();
            if first > now {
                continue;
            }
            let rates = app
                .client
                .funding(symbol, first, now.min(first + 48 * HOUR - 1))
                .await?;
            trading_storage::paper::rates(&app.pool, &rates).await?;
        }
    }
    Ok(())
}
