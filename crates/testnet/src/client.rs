use crate::diagnostics::ApiFailure;
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{sync::Mutex, time::Instant};
use trading_domain::{ListedMarket, Metadata, select_markets};
pub const ORIGIN: &str = "https://api.hyperliquid-testnet.xyz";
/// Fixed endpoint, no redirects, and no automatic retry of exchange writes.
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
    async fn post(&self, path: &'static str, payload: &Value) -> Result<Value> {
        let mut next = self.next.lock().await;
        tokio::time::sleep_until(*next).await;
        *next = Instant::now() + Duration::from_secs(2);
        let mut response = self
            .http
            .post(format!("{ORIGIN}/{path}"))
            .json(payload)
            .send()
            .await
            .map_err(|e| ApiFailure {
                operation: path,
                category: if e.is_timeout() {
                    "timeout"
                } else if e.is_connect() {
                    "connection"
                } else {
                    "transport"
                },
                status: None,
            })?;
        if !response.status().is_success() {
            return Err(ApiFailure {
                operation: path,
                category: "http_status",
                status: Some(response.status().as_u16()),
            }
            .into());
        }
        ensure!(
            response.content_length().is_none_or(|n| n <= 8_000_000),
            "oversized exchange response"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| ApiFailure {
            operation: path,
            category: if e.is_timeout() {
                "timeout"
            } else {
                "incomplete_response"
            },
            status: None,
        })? {
            ensure!(
                bytes.len() + chunk.len() <= 8_000_000,
                "oversized exchange response"
            );
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            ApiFailure {
                operation: path,
                category: "invalid_json",
                status: None,
            }
            .into()
        })
    }
    pub async fn info(&self, request: Value) -> Result<Value> {
        retry_read(|| self.post("info", &request), Duration::from_secs(2)).await
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

// Only known transient failures of the read-only info endpoint qualify.
// Every attempt still passes through the existing request-rate limiter.
async fn retry_read<F, Fut>(mut read: F, delay: Duration) -> Result<Value>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Value>>,
{
    for attempt in 1..=3 {
        match read().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                let transient = error.downcast_ref::<ApiFailure>().is_some_and(|e| {
                    e.operation == "info"
                        && (matches!(e.category, "timeout" | "connection" | "incomplete_response")
                            || (e.category == "http_status" && matches!(e.status, Some(502..=504))))
                });
                if !transient || attempt == 3 {
                    return Err(error);
                }
                eprintln!(
                    "testnet read retry: {}",
                    json!({"failed_attempt":attempt,"max_attempts":3,"diagnostic":crate::diagnostics::describe(&error)})
                );
                tokio::time::sleep(delay * attempt).await;
            }
        }
    }
    unreachable!()
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

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[tokio::test]
    async fn transient_reads_recover_with_bounded_attempts() {
        let mut attempts = 0;
        let value = retry_read(
            || {
                attempts += 1;
                std::future::ready(if attempts < 3 {
                    Err(ApiFailure {
                        operation: "info",
                        category: "http_status",
                        status: Some(502),
                    }
                    .into())
                } else {
                    Ok(json!({"recovered":true}))
                })
            },
            Duration::ZERO,
        )
        .await
        .unwrap();
        assert_eq!(attempts, 3);
        assert_eq!(value["recovered"], true);
    }

    #[tokio::test]
    async fn exhausted_reads_fail_and_writes_or_invalid_data_are_never_retried() {
        for (operation, category, status, expected) in [
            ("info", "http_status", Some(502), 3),
            ("info", "http_status", Some(503), 3),
            ("info", "http_status", Some(504), 3),
            ("info", "timeout", None, 3),
            ("info", "connection", None, 3),
            ("info", "incomplete_response", None, 3),
            ("info", "http_status", Some(429), 1),
            ("info", "http_status", Some(400), 1),
            ("info", "invalid_json", None, 1),
            ("exchange", "http_status", Some(502), 1),
            ("exchange", "timeout", None, 1),
        ] {
            let mut attempts = 0;
            let error = retry_read(
                || {
                    attempts += 1;
                    std::future::ready(Err(ApiFailure {
                        operation,
                        category,
                        status,
                    }
                    .into()))
                },
                Duration::ZERO,
            )
            .await
            .unwrap_err();
            assert_eq!(attempts, expected);
            assert_eq!(error.downcast_ref::<ApiFailure>().unwrap().status, status);
        }
    }
}
