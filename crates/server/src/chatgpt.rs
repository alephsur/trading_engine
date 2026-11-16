use crate::App;
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
pub struct Local {
    client: Mutex<trading_chatgpt::Client>,
    origin: String,
    host: String,
}
type Response = Result<Json<Value>, (StatusCode, Json<Value>)>;
fn denied(code: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (code, Json(json!({"error":message})))
}
pub fn configured() -> anyhow::Result<Option<Arc<Local>>> {
    if std::env::var("CHATGPT_ENABLED").as_deref() != Ok("1") {
        return Ok(None);
    }
    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let address: std::net::SocketAddr = bind.parse()?;
    anyhow::ensure!(
        address.ip() == std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        "ChatGPT requires a 127.0.0.1 binding"
    );
    let dir = std::env::var("CHATGPT_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".local/state/trading-engine/chatgpt")
        });
    Ok(Some(Arc::new(Local {
        client: Mutex::new(trading_chatgpt::Client::open(dir)?),
        origin: format!("http://{bind}"),
        host: bind,
    })))
}
fn guard<'a>(
    app: &'a App,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<&'a Arc<Local>, (StatusCode, Json<Value>)> {
    let local = app.chatgpt.as_ref().ok_or_else(|| {
        denied(
            StatusCode::SERVICE_UNAVAILABLE,
            "ChatGPT no está habilitado en este servidor local.",
        )
    })?;
    if headers.get("host").and_then(|h| h.to_str().ok()) != Some(&local.host) {
        return Err(denied(StatusCode::FORBIDDEN, "Host no permitido."));
    }
    if mutation
        && (headers.get("origin").and_then(|h| h.to_str().ok()) != Some(&local.origin)
            || headers
                .get("x-trading-client")
                .and_then(|h| h.to_str().ok())
                != Some("dashboard"))
    {
        return Err(denied(
            StatusCode::FORBIDDEN,
            "Solicitud externa rechazada.",
        ));
    }
    Ok(local)
}
fn failure() -> (StatusCode, Json<Value>) {
    denied(
        StatusCode::BAD_GATEWAY,
        "No se pudo completar la operación con ChatGPT. Comprueba la conexión, los permisos y los límites de tu plan; vuelve a conectar si la sesión ha caducado.",
    )
}
pub async fn status(State(app): State<App>, headers: HeaderMap) -> Response {
    if app.chatgpt.is_none() {
        return Ok(Json(
            json!({"available":false,"connected":false,"profiles":[]}),
        ));
    }
    let local = guard(&app, &headers, false)?;
    let client = local.client.try_lock().map_err(|_| {
        denied(
            StatusCode::CONFLICT,
            "ChatGPT está procesando otra solicitud.",
        )
    })?;
    Ok(Json(client.status()))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connect {
    profile: Option<String>,
}
pub async fn connect(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Connect>,
) -> Response {
    let local = guard(&app, &headers, true)?;
    let mut client = local
        .client
        .try_lock()
        .map_err(|_| denied(StatusCode::CONFLICT, "Operación en curso."))?;
    let url = client
        .begin(format!("{}/auth/callback", local.origin), input.profile)
        .map_err(|_| failure())?;
    Ok(Json(json!({"url":url})))
}
#[derive(Deserialize)]
pub struct Callback {
    state: Option<String>,
    code: Option<String>,
    client_id: Option<String>,
    error: Option<String>,
}
pub async fn callback(
    State(app): State<App>,
    headers: HeaderMap,
    Query(q): Query<Callback>,
) -> (StatusCode, &'static str) {
    let Ok(local) = guard(&app, &headers, false) else {
        return (StatusCode::FORBIDDEN, "Conexión no disponible.");
    };
    let Ok(mut client) = local.client.try_lock() else {
        return (
            StatusCode::CONFLICT,
            "Operación en curso. Vuelve a conectar desde el dashboard.",
        );
    };
    match client
        .callback(
            q.state.as_deref().unwrap_or(""),
            q.code.as_deref(),
            q.client_id.as_deref(),
            q.error.as_deref(),
        )
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            "ChatGPT conectado. Cierra esta pestaña y vuelve al dashboard para elegir el modelo.",
        ),
        Err(_) => (
            StatusCode::BAD_REQUEST,
            "No se pudo autorizar ChatGPT. Vuelve al dashboard e inicia una nueva conexión. No copies la URL ni sus códigos.",
        ),
    }
}
pub async fn models(State(app): State<App>, headers: HeaderMap) -> Response {
    let local = guard(&app, &headers, true)?;
    let mut client = local
        .client
        .try_lock()
        .map_err(|_| denied(StatusCode::CONFLICT, "Operación en curso."))?;
    Ok(Json(client.models().await.map_err(|_| failure())?))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    model: String,
}
pub async fn select(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Model>,
) -> Response {
    let local = guard(&app, &headers, true)?;
    let mut client = local
        .client
        .try_lock()
        .map_err(|_| denied(StatusCode::CONFLICT, "Operación en curso."))?;
    client.select(&input.model).await.map_err(|_| failure())?;
    Ok(Json(client.status()))
}
pub async fn disconnect(State(app): State<App>, headers: HeaderMap) -> Response {
    let local = guard(&app, &headers, true)?;
    let mut client = local
        .client
        .try_lock()
        .map_err(|_| denied(StatusCode::CONFLICT, "Operación en curso."))?;
    let revoked = client.disconnect().await.map_err(|_| failure())?;
    Ok(Json(
        json!({"disconnected":true,"remote_revocation_confirmed":revoked}),
    ))
}
pub async fn analyze(State(app): State<App>, headers: HeaderMap) -> Response {
    let local = guard(&app, &headers, true)?;
    let mut client = local
        .client
        .try_lock()
        .map_err(|_| denied(StatusCode::CONFLICT, "Operación en curso."))?;
    // Fixed public context: no account keys, wallet identifiers, orders or control handles.
    let context = {
        let feed = app.feed.read().await;
        json!({"purpose":"manual_connection_test","source":"public_market_feed_not_testnet","as_of_ms":trading_domain::data::now_ms(),"prices":feed.prices,"market_data_fresh":crate::fresh(&feed),"analysis_ready":feed.analysis_ready,"strategy_config_id":app.engine.id()})
    };
    Ok(Json(client.analyze(context).await.map_err(|_| failure())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    #[tokio::test]
    async fn oauth_controls_reject_cross_origin_and_rebinding_without_network() {
        let dir = std::env::temp_dir().join(format!(
            "chatgpt-http-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = App {
            chatgpt: Some(Arc::new(Local {
                client: Mutex::new(trading_chatgpt::Client::open(dir.clone()).unwrap()),
                origin: "http://127.0.0.1:3001".into(),
                host: "127.0.0.1:3001".into(),
            })),
            control_token: Arc::new(None),
            telemetry: Arc::new(crate::dashboard::Telemetry::default()),
            pool: sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://localhost/unused")
                .unwrap(),
            feed: Arc::new(tokio::sync::RwLock::new(crate::Feed::default())),
            client: Arc::new(trading_market_data::history::PublicClient::new().unwrap()),
            engine: Arc::new(trading_quant::Engine::builtin(Default::default()).unwrap()),
        };
        let router = crate::router(app);
        for (host, origin, marker, expected) in [
            ("127.0.0.1:3001", "https://evil.example", "dashboard", 403),
            (
                "evil.example:3001",
                "http://evil.example:3001",
                "dashboard",
                403,
            ),
            ("127.0.0.1:3001", "http://127.0.0.1:3001", "", 403),
            ("127.0.0.1:3001", "http://127.0.0.1:3001", "dashboard", 200),
        ] {
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/api/ai/connect")
                        .header("host", host)
                        .header("origin", origin)
                        .header("x-trading-client", marker)
                        .header("content-type", "application/json")
                        .body(axum::body::Body::from("{} "))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), expected);
        }
        let response = router
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/auth/callback?state=forged&code=forged")
                    .header("host", "127.0.0.1:3001")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        drop(router);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
