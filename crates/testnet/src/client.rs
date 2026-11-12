use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{sync::Mutex, time::Instant};
use trading_domain::{ListedMarket, Metadata, select_markets};
pub const ORIGIN: &str = "https://api.hyperliquid-testnet.xyz";
/// There is deliberately no configurable endpoint, redirect or automatic POST retry.
pub struct Client {
    http: reqwest::Client,
    next: Mutex<Instant>,
}
impl Client {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .build()?,
            next: Mutex::new(Instant::now()),
        })
    }
    async fn post(&self, path: &str, payload: &Value) -> Result<Value> {
        let mut next = self.next.lock().await;
        tokio::time::sleep_until(*next).await;
        *next = Instant::now() + Duration::from_secs(2);
        let mut response = self
            .http
            .post(format!("{ORIGIN}/{path}"))
            .json(payload)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("testnet transport outcome unknown"))?;
        ensure!(
            response.status().is_success(),
            "testnet HTTP request unsuccessful; do not assume rejection"
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= 8_000_000),
            "oversized exchange response"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("incomplete testnet response"))?
        {
            ensure!(
                bytes.len() + chunk.len() <= 8_000_000,
                "oversized exchange response"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid testnet response"))
    }
    pub async fn info(&self, request: Value) -> Result<Value> {
        self.post("info", &request).await
    }
    pub(crate) async fn exchange(&self, request: &Value) -> Result<Value> {
        self.post("exchange", request).await
    }
    pub async fn markets(&self) -> Result<Vec<ListedMarket>> {
        let meta: Metadata = serde_json::from_value(self.info(json!({"type":"meta"})).await?)?;
        select_markets(meta)
    }
    pub async fn account(&self, account: &str) -> Result<Value> {
        self.info(json!({"type":"clearinghouseState","user":account}))
            .await
    }
}

/// Injectable write boundary for failure tests; production uses only the fixed testnet client.
pub trait ExchangeTransport {
    fn send(&self, request: &Value) -> impl std::future::Future<Output = Result<Value>> + Send;
}
impl ExchangeTransport for Client {
    async fn send(&self, request: &Value) -> Result<Value> {
        self.exchange(request).await
    }
}

pub trait InfoTransport: Sync {
    fn info(&self, request: Value) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn markets(&self) -> impl std::future::Future<Output = Result<Vec<ListedMarket>>> + Send {
        async move {
            select_markets(serde_json::from_value(
                self.info(json!({"type":"meta"})).await?,
            )?)
        }
    }
    fn account(&self, account: &str) -> impl std::future::Future<Output = Result<Value>> + Send {
        async move {
            self.info(json!({"type":"clearinghouseState","user":account}))
                .await
        }
    }
}
impl InfoTransport for Client {
    async fn info(&self, request: Value) -> Result<Value> {
        Client::info(self, request).await
    }
}
