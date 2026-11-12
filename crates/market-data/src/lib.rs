use anyhow::{Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::{
    sync::mpsc,
    time::{interval, timeout},
};
use tokio_tungstenite::{Connector, connect_async_tls_with_config, tungstenite::Message};
use trading_domain::{SYMBOLS, data::INTERVALS};

const INFO: &str = "https://api.hyperliquid.xyz/info";
const WS: &str = "wss://api.hyperliquid.xyz/ws";

pub mod history;

fn tls_connector() -> Result<Connector> {
    // Workspace feature unification can enable both ring and aws-lc-rs.
    // Select this connection's provider explicitly without global process state.
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(
        rustls::crypto::ring::default_provider().into(),
    )
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    Ok(Connector::Rustls(std::sync::Arc::new(config)))
}

/// One read-only session. The supervisor owns reconnects and health state.
pub async fn stream(tx: mpsc::Sender<serde_json::Value>) -> Result<()> {
    let (mut ws, _) = timeout(
        Duration::from_secs(10),
        connect_async_tls_with_config(WS, None, false, Some(tls_connector()?)),
    )
    .await??;
    for subscription in subscriptions() {
        timeout(
            Duration::from_secs(5),
            ws.send(Message::Text(
                json!({"method":"subscribe","subscription":subscription})
                    .to_string()
                    .into(),
            )),
        )
        .await??;
    }
    let mut heartbeat = interval(Duration::from_secs(20));
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                timeout(Duration::from_secs(5), ws.send(Message::Text(json!({"method":"ping"}).to_string().into()))).await??;
            }
            msg = ws.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    let value = serde_json::from_str(&text)?;
                    timeout(Duration::from_secs(5), tx.send(value)).await??;
                },
                Some(Ok(Message::Ping(bytes))) => { timeout(Duration::from_secs(5), ws.send(Message::Pong(bytes))).await??; },
                Some(Ok(Message::Close(_))) | None => bail!("websocket closed"),
                Some(Err(error)) => return Err(error.into()),
                _ => {}
            }
        }
    }
}

pub fn subscriptions() -> Vec<serde_json::Value> {
    let mut subscriptions = vec![json!({"type":"allMids"})];
    for symbol in SYMBOLS {
        subscriptions.push(json!({"type":"activeAssetCtx","coin":symbol}));
        subscriptions.push(json!({"type":"trades","coin":symbol}));
        subscriptions.push(json!({"type":"l2Book","coin":symbol}));
        for interval in INTERVALS {
            subscriptions.push(json!({"type":"candle","coin":symbol,"interval":interval}));
        }
    }
    subscriptions
}
#[cfg(test)]
mod tests {
    #[test]
    fn tls_configuration_does_not_need_an_implicit_crypto_provider() {
        assert!(matches!(
            super::tls_connector().unwrap(),
            tokio_tungstenite::Connector::Rustls(_)
        ));
    }

    #[test]
    fn subscriptions_are_unique_and_public() {
        let subscriptions = super::subscriptions();
        let unique: std::collections::BTreeSet<_> =
            subscriptions.iter().map(ToString::to_string).collect();
        assert_eq!(subscriptions.len(), 29);
        assert_eq!(unique.len(), 29);
        assert!(subscriptions.iter().all(|s| s.get("user").is_none()));
    }
}
