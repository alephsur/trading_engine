use crate::{App, fresh};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};
use trading_domain::data::{interval_ms, symbol};
use trading_storage::market;

type Response = Result<Json<Value>, (StatusCode, Json<Value>)>;
#[derive(Deserialize)]
pub struct DataQuery {
    symbol: String,
    #[serde(default = "default_interval")]
    interval: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default = "default_closed")]
    closed_only: bool,
}
fn default_interval() -> String {
    "1m".into()
}
fn default_limit() -> i64 {
    100
}
fn default_closed() -> bool {
    true
}
impl DataQuery {
    fn validate(&self) -> Result<(), (StatusCode, Json<Value>)> {
        if symbol(&self.symbol).is_err()
            || interval_ms(&self.interval).is_err()
            || !(1..=500).contains(&self.limit)
        {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid symbol, interval or limit (1..500)"})),
            ));
        }
        Ok(())
    }
}
fn unavailable(_: anyhow::Error) -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"error":"market storage unavailable"})),
    )
}
pub async fn candles(State(app): State<App>, Query(q): Query<DataQuery>) -> Response {
    q.validate()?;
    let rows = market::read_candles(&app.pool, &q.symbol, &q.interval, q.limit, q.closed_only)
        .await
        .map_err(unavailable)?;
    Ok(Json(json!({"candles":rows,"order":"newest_first"})))
}
pub async fn trades(State(app): State<App>, Query(q): Query<DataQuery>) -> Response {
    q.validate()?;
    let rows = market::read_trades(&app.pool, &q.symbol, q.limit)
        .await
        .map_err(unavailable)?;
    Ok(Json(
        json!({"trades":rows,"history_complete":false,"order":"newest_first"}),
    ))
}
pub async fn book(State(app): State<App>, Query(q): Query<DataQuery>) -> Response {
    q.validate()?;
    let book = market::read_book(&app.pool, &q.symbol)
        .await
        .map_err(unavailable)?;
    let feed = app.feed.read().await;
    let current = book
        .as_ref()
        .and_then(|b| b["time"].as_i64())
        .is_some_and(crate::book_recent);
    Ok(Json(json!({"book":book,"fresh":fresh(&feed) && current})))
}
pub async fn status(State(app): State<App>) -> Response {
    let coverage = market::read_coverage(&app.pool)
        .await
        .map_err(unavailable)?;
    let feed = app.feed.read().await;
    Ok(Json(
        json!({"candle_coverage":coverage,"history_ready":crate::history_ready(&feed),"trade_history_complete":false,"book_history_complete":false,"connected":feed.connected}),
    ))
}
