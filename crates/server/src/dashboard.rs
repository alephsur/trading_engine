use crate::App;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicU64, Ordering::Relaxed},
    time::Instant,
};
use trading_storage::dashboard::Command;
type Response = Result<Json<Value>, (StatusCode, Json<Value>)>;
pub struct Telemetry {
    started: Instant,
    feed: AtomicU64,
    paper: AtomicU64,
    funding: AtomicU64,
}
impl Default for Telemetry {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            feed: AtomicU64::new(0),
            paper: AtomicU64::new(0),
            funding: AtomicU64::new(0),
        }
    }
}
pub fn token(value: Option<String>) -> anyhow::Result<Option<String>> {
    let value = value.filter(|s| !s.is_empty());
    anyhow::ensure!(
        value
            .as_ref()
            .is_none_or(|s| s.len() >= 32 && s.bytes().all(|b| b.is_ascii_graphic())),
        "PAPER_CONTROL_TOKEN must contain at least 32 printable non-space ASCII characters"
    );
    Ok(value)
}
pub async fn failure(app: &App, source: &str) {
    let counter = match source {
        "feed" => &app.telemetry.feed,
        "paper" => &app.telemetry.paper,
        _ => &app.telemetry.funding,
    };
    let count = counter.fetch_add(1, Relaxed) + 1;
    // Bound repeated error audit writes while counters retain every failure.
    if count == 1 || count.is_multiple_of(60) {
        let _ = trading_storage::record(
            &app.pool,
            "runtime_error",
            json!({"source":source,"count":count,"time":trading_domain::data::now_ms()}),
        )
        .await;
    }
}
fn denied(status: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error":message})))
}
fn authorize(
    headers: &HeaderMap,
    configured: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    let expected = configured.ok_or_else(|| denied(StatusCode::FORBIDDEN, "controls disabled"))?;
    let supplied = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let equal = supplied.len() == expected.len()
        && supplied
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0;
    if !equal {
        return Err(denied(
            StatusCode::UNAUTHORIZED,
            "invalid control credential",
        ));
    }
    if let Some(origin) = headers.get("origin") {
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        let origin = origin.to_str().unwrap_or("");
        if host.is_empty()
            || !(origin == format!("http://{host}") || origin == format!("https://{host}"))
        {
            return Err(denied(StatusCode::FORBIDDEN, "cross-origin control denied"));
        }
    }
    Ok(())
}
pub async fn control(
    State(app): State<App>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    authorize(&headers, app.control_token.as_deref())?;
    command
        .validate()
        .map_err(|_| denied(StatusCode::BAD_REQUEST, "invalid command"))?;
    trading_storage::dashboard::command(&app.pool, &command)
        .await
        .map(Json)
        .map_err(|e| {
            if e.to_string() == "command identifier reused with different intent"
                || e.to_string() == "position no longer open"
            {
                denied(StatusCode::CONFLICT, "command conflicts with current state")
            } else {
                denied(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "control unavailable; retry with the same command identifier",
                )
            }
        })
}
pub async fn snapshot(State(app): State<App>) -> Response {
    let mut value = trading_storage::dashboard::snapshot(&app.pool)
        .await
        .map_err(|_| {
            denied(
                StatusCode::SERVICE_UNAVAILABLE,
                "dashboard storage unavailable",
            )
        })?;
    value["controls_enabled"] = json!(app.control_token.is_some());
    let f = app.feed.read().await;
    value["runtime"] = json!({"uptime_seconds":app.telemetry.started.elapsed().as_secs(),"feed_disconnects":app.telemetry.feed.load(Relaxed),"paper_errors":app.telemetry.paper.load(Relaxed),"funding_errors":app.telemetry.funding.load(Relaxed),"feed_age_ms":f.last_update.map(|t|t.elapsed().as_millis()),"paper_age_ms":f.paper_update.map(|t|t.elapsed().as_millis()),"recovery_age_ms":f.last_recovery.map(|t|t.elapsed().as_millis())});
    Ok(Json(value))
}
pub async fn metrics(
    State(app): State<App>,
) -> ([(axum::http::HeaderName, &'static str); 1], String) {
    let feed = app.feed.read().await;
    let mut text = String::new();
    for (name, kind, value) in [
        (
            "uptime_seconds",
            "gauge",
            app.telemetry.started.elapsed().as_secs(),
        ),
        (
            "feed_disconnects_total",
            "counter",
            app.telemetry.feed.load(Relaxed),
        ),
        (
            "paper_errors_total",
            "counter",
            app.telemetry.paper.load(Relaxed),
        ),
        (
            "funding_errors_total",
            "counter",
            app.telemetry.funding.load(Relaxed),
        ),
        ("market_data_fresh", "gauge", u64::from(crate::fresh(&feed))),
        (
            "history_ready",
            "gauge",
            u64::from(crate::history_ready(&feed)),
        ),
        ("analysis_ready", "gauge", u64::from(feed.analysis_ready)),
    ] {
        text.push_str(&format!(
            "# TYPE trading_{name} {kind}\ntrading_{name} {value}\n"
        ));
    }
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        text,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn controls_require_key_and_same_origin() {
        let key = "abcdefghijklmnopqrstuvwxyz123456";
        assert!(token(Some("short".into())).is_err());
        assert!(token(Some(key.into())).is_ok());
        let mut h = HeaderMap::new();
        assert_eq!(authorize(&h, None).unwrap_err().0, StatusCode::FORBIDDEN);
        assert_eq!(
            authorize(&h, Some(key)).unwrap_err().0,
            StatusCode::UNAUTHORIZED
        );
        h.insert("authorization", format!("Bearer {key}").parse().unwrap());
        assert!(authorize(&h, Some(key)).is_ok());
        h.insert("host", "localhost:3000".parse().unwrap());
        h.insert("origin", "http://evil.example".parse().unwrap());
        assert!(authorize(&h, Some(key)).is_err());
        h.insert("origin", "http://localhost:3000".parse().unwrap());
        assert!(authorize(&h, Some(key)).is_ok());
    }
}
