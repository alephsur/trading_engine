use anyhow::{Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::time::Duration;
use tokio::{
    sync::mpsc,
    time::{interval, timeout},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use trading_domain::{ListedMarket, Metadata, select_markets};

const INFO: &str = "https://api.hyperliquid.xyz/info";
const WS: &str = "wss://api.hyperliquid.xyz/ws";

pub async fn metadata() -> Result<Vec<ListedMarket>> {
    let meta: Metadata = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?
        .post(INFO)
        .json(&json!({"type":"meta"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    select_markets(meta)
}

/// One read-only session. The supervisor owns reconnects and health state.
pub async fn stream(tx: mpsc::Sender<serde_json::Value>) -> Result<()> {
    let (mut ws, _) = timeout(Duration::from_secs(10), connect_async(WS)).await??;
    timeout(
        Duration::from_secs(5),
        ws.send(Message::Text(
            json!({"method":"subscribe","subscription":{"type":"allMids"}})
                .to_string()
                .into(),
        )),
    )
    .await??;
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
