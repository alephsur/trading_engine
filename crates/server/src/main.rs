mod api;
mod recovery;
use anyhow::{Result, ensure};
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{RwLock, mpsc};

#[derive(Default)]
struct Feed {
    markets: Value,
    prices: Value,
    last_update: Option<Instant>,
    connected: bool,
    books: BTreeMap<String, i64>,
    history_ready: bool,
    analysis_ready: bool,
    last_recovery: Option<Instant>,
}
#[derive(Clone)]
struct App {
    pool: PgPool,
    feed: Arc<RwLock<Feed>>,
    client: Arc<trading_market_data::history::PublicClient>,
    engine: Arc<trading_quant::Engine>,
}

fn validate_mode(mode: &str) -> Result<()> {
    ensure!(mode == "PAPER", "only PAPER read-only mode is supported");
    Ok(())
}
fn book_recent(time: i64) -> bool {
    let age = trading_domain::data::now_ms() - time;
    (-5_000..15_000).contains(&age)
}
fn fresh(feed: &Feed) -> bool {
    feed.connected
        && feed
            .last_update
            .is_some_and(|t| t.elapsed() < Duration::from_secs(15))
        && trading_domain::SYMBOLS
            .iter()
            .all(|s| feed.books.get(*s).is_some_and(|t| book_recent(*t)))
}
fn history_ready(feed: &Feed) -> bool {
    feed.history_ready
        && feed
            .last_recovery
            .is_some_and(|t| t.elapsed() < Duration::from_secs(180))
}
async fn health(State(app): State<App>) -> (StatusCode, Json<Value>) {
    let db_ok = matches!(
        tokio::time::timeout(
            Duration::from_secs(2),
            sqlx::query("SELECT 1").execute(&app.pool)
        )
        .await,
        Ok(Ok(_))
    );
    let feed = app.feed.read().await;
    let ready = db_ok && fresh(&feed) && history_ready(&feed) && feed.analysis_ready;
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(
            json!({"status":if ready {"ready"} else {"degraded"},"mode":"PAPER","execution_enabled":false,"database":db_ok,"market_data_fresh":fresh(&feed),"history_ready":history_ready(&feed),"analysis_ready":feed.analysis_ready}),
        ),
    )
}
async fn markets(State(app): State<App>) -> Json<Value> {
    Json(app.feed.read().await.markets.clone())
}
async fn prices(State(app): State<App>) -> Json<Value> {
    let feed = app.feed.read().await;
    Json(
        json!({"prices":feed.prices,"fresh":fresh(&feed),"age_ms":feed.last_update.map(|t| t.elapsed().as_millis())}),
    )
}
fn router(app: App) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/livez", get(|| async { Json(json!({"alive":true})) }))
        .route("/markets", get(markets))
        .route("/prices", get(prices))
        .route("/candles", get(api::candles))
        .route("/trades", get(api::trades))
        .route("/book", get(api::book))
        .route("/data-status", get(api::status))
        .route("/strategies", get(api::strategies))
        .route("/evaluations", get(api::evaluations))
        .route("/signals", get(api::signals))
        .with_state(app)
}
async fn session(app: &App) -> Result<()> {
    {
        let mut feed = app.feed.write().await;
        feed.history_ready = false;
        feed.analysis_ready = false;
        feed.books.clear();
    }
    trading_storage::record(&app.pool, "capture_session", json!({"trade_history_complete":false,"book_history_complete":false,"reason":"startup_or_reconnect"})).await?;
    let markets = app.client.metadata().await?;
    let metadata = serde_json::to_value(markets)?;
    trading_storage::record(&app.pool, "metadata", metadata.clone()).await?;
    app.feed.write().await.markets = metadata;
    tracing::info!(
        event = "metadata_validated",
        "four perpetual markets available"
    );
    let (tx, mut rx) = mpsc::channel(256);
    let stream = trading_market_data::stream(tx);
    tokio::pin!(stream);
    let stale = tokio::time::sleep(Duration::from_secs(15));
    tokio::pin!(stale);
    let recovery_loop = recovery::run(app);
    tokio::pin!(recovery_loop);
    loop {
        tokio::select! {
            result = &mut recovery_loop => return result,
            result = &mut stream => return result,
            _ = &mut stale => anyhow::bail!("market data stale"),
            value = rx.recv() => {
                let Some(value) = value else { anyhow::bail!("feed ended") };
                use trading_domain::data::{MarketEvent, parse_event};
                if let Some(event) = parse_event(&value)? {
                    match event {
                        MarketEvent::Candle(candle) => {
                            anyhow::ensure!(candle.t <= trading_domain::data::now_ms() + 5_000, "future candle");
                            tokio::time::timeout(Duration::from_secs(5), trading_storage::market::candles(&app.pool, &[candle], false)).await??;
                        }
                        MarketEvent::Trades(trades) => {
                            tokio::time::timeout(Duration::from_secs(5), trading_storage::market::trades(&app.pool, &trades)).await??;
                        }
                        MarketEvent::Book(book) => {
                            anyhow::ensure!(book.time <= trading_domain::data::now_ms() + 5_000, "future book");
                            tokio::time::timeout(Duration::from_secs(5), trading_storage::market::book(&app.pool, &book)).await??;
                            let mut feed = app.feed.write().await;
                            let latest = feed.books.entry(book.coin).or_default();
                            *latest = (*latest).max(book.time);
                        }
                    }
                }
                if let Some(mids) = trading_domain::parse_mids(&value)? {
                    let prices = serde_json::to_value(mids)?;
                    tokio::time::timeout(Duration::from_secs(5), trading_storage::record(&app.pool, "mids", prices.clone())).await??;
                    let mut feed = app.feed.write().await;
                    feed.prices = prices;
                    feed.last_update = Some(Instant::now());
                    feed.connected = true;
                    stale.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(15));
                    tracing::debug!(event="prices_persisted");
                }
            }
        }
    }
}
async fn supervise(app: App) {
    let mut delay = 1;
    loop {
        let started = Instant::now();
        if let Err(error) = session(&app).await {
            // Do not log arbitrary remote payloads or connection strings.
            tracing::warn!(event="feed_disconnected", reason=%error, retry_seconds=delay);
        }
        {
            let mut feed = app.feed.write().await;
            feed.connected = false;
            feed.history_ready = false;
            feed.analysis_ready = false;
        }
        if started.elapsed() > Duration::from_secs(60) {
            delay = 1;
        }
        tokio::time::sleep(Duration::from_secs(delay)).await;
        delay = (delay * 2).min(30);
    }
}
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    validate_mode(&std::env::var("TRADING_MODE").unwrap_or_else(|_| "PAPER".into()))?;
    let database_url = std::env::var("DATABASE_URL")?;
    let pool = trading_storage::connect(&database_url).await?;
    trading_storage::record(
        &pool,
        "startup",
        json!({"mode":"PAPER","execution_enabled":false}),
    )
    .await?;
    let config = match std::env::var("STRATEGY_CONFIG_PATH") {
        Ok(path) => serde_json::from_str(&std::fs::read_to_string(path)?)?,
        Err(std::env::VarError::NotPresent) => trading_quant::config::Config::default(),
        Err(error) => return Err(error.into()),
    };
    let engine = Arc::new(trading_quant::Engine::builtin(config)?);
    trading_storage::signals::register(&pool, &engine).await?;
    let app = App {
        engine,
        pool,
        feed: Arc::new(RwLock::new(Feed::default())),
        client: Arc::new(trading_market_data::history::PublicClient::new()?),
    };
    let listener = tokio::net::TcpListener::bind(
        std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into()),
    )
    .await?;
    let feed_task = tokio::spawn(supervise(app.clone()));
    tracing::info!(event="server_started", address=%listener.local_addr()?);
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown())
        .await?;
    feed_task.abort();
    let _ = feed_task.await;
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[test]
    fn execution_cannot_be_enabled() {
        assert!(validate_mode("PAPER").is_ok());
        for mode in ["LIVE", "TESTNET", "BACKTEST", "", "paper"] {
            assert!(validate_mode(mode).is_err());
        }
    }
    #[test]
    fn stale_or_disconnected_feed_is_unhealthy() {
        let mut feed = Feed::default();
        assert!(!fresh(&feed));
        feed.connected = true;
        feed.last_update = Some(Instant::now());
        assert!(!fresh(&feed));
        for symbol in trading_domain::SYMBOLS {
            feed.books
                .insert(symbol.into(), trading_domain::data::now_ms());
        }
        assert!(fresh(&feed));
        feed.last_update = Some(Instant::now() - Duration::from_secs(16));
        assert!(!fresh(&feed));
        feed.last_update = Some(Instant::now());
        feed.connected = false;
        assert!(!fresh(&feed));
    }
    #[tokio::test]
    async fn liveness_does_not_require_database() {
        let app = App {
            pool: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/unused")
                .unwrap(),
            feed: Arc::new(RwLock::new(Feed::default())),
            client: Arc::new(trading_market_data::history::PublicClient::new().unwrap()),
            engine: Arc::new(trading_quant::Engine::builtin(Default::default()).unwrap()),
        };
        let response = router(app)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/livez")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
