use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep_until};
use trading_domain::{ListedMarket, Metadata, data::Candle, select_markets};

/// Shared pacing across metadata, backfill and retries: <=30 requests/minute.
pub struct PublicClient {
    client: reqwest::Client,
    endpoint: String,
    next: Mutex<Instant>,
}
impl PublicClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
            endpoint: super::INFO.into(),
            next: Mutex::new(Instant::now()),
        })
    }
    async fn post(&self, body: Value) -> Result<Value> {
        for attempt in 0..3 {
            let mut next = self.next.lock().await;
            sleep_until(*next).await;
            *next = Instant::now() + Duration::from_secs(2);
            drop(next);
            let response = self.client.post(&self.endpoint).json(&body).send().await;
            match response {
                Ok(response)
                    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || response.status().is_server_error() =>
                {
                    if attempt == 2 {
                        return Err(response.error_for_status().unwrap_err().into());
                    }
                }
                Ok(response) => return Ok(response.error_for_status()?.json().await?),
                Err(error) if attempt == 2 => return Err(error.into()),
                Err(_) => {}
            }
            tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt + 1))).await;
        }
        unreachable!("bounded retries")
    }
    pub async fn metadata(&self) -> Result<Vec<ListedMarket>> {
        let meta: Metadata = serde_json::from_value(self.post(json!({"type":"meta"})).await?)?;
        select_markets(meta)
    }
    pub async fn recent_trades(&self, symbol: &str) -> Result<Vec<trading_domain::data::Trade>> {
        trading_domain::data::symbol(symbol)?;
        let trades: Vec<trading_domain::data::Trade> = serde_json::from_value(
            self.post(json!({"type":"recentTrades","coin":symbol}))
                .await?,
        )?;
        for trade in &trades {
            trade.validate()?;
            ensure!(trade.coin == symbol, "mismatched trade response");
        }
        Ok(trades)
    }
    pub async fn funding(
        &self,
        symbol: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<trading_domain::data::FundingRate>> {
        trading_domain::data::symbol(symbol)?;
        ensure!(
            start >= 0 && end >= start && end - start <= 48 * 3_600_000,
            "invalid funding window"
        );
        let rates: Vec<trading_domain::data::FundingRate> = serde_json::from_value(
            self.post(
                json!({"type":"fundingHistory","coin":symbol,"startTime":start,"endTime":end}),
            )
            .await?,
        )?;
        ensure!(rates.len() <= 49, "oversized funding response");
        let mut hours = std::collections::BTreeSet::new();
        for rate in &rates {
            ensure!(
                rate.coin == symbol
                    && rate.time >= start
                    && rate.time <= end
                    && rate.funding_rate.abs() <= "0.005".parse()?,
                "inconsistent funding response"
            );
            ensure!(
                hours.insert(rate.time / 3_600_000),
                "duplicate funding hour"
            );
        }
        Ok(rates)
    }
    pub async fn candles(
        &self,
        symbol: &str,
        interval: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<Candle>> {
        let step = trading_domain::data::interval_ms(interval)?;
        trading_domain::data::symbol(symbol)?;
        ensure!(
            start <= end && (end - start) / step < 500,
            "oversized historical request"
        );
        let candles: Vec<Candle> = serde_json::from_value(self.post(json!({"type":"candleSnapshot","req":{"coin":symbol,"interval":interval,"startTime":start,"endTime":end}})).await?)?;
        for candle in &candles {
            candle.validate()?;
            ensure!(
                candle.s == symbol && candle.i == interval,
                "mismatched candle response"
            );
        }
        // Exchange may include the candle containing endTime; only the requested closed window is eligible.
        Ok(candles
            .into_iter()
            .filter(|c| c.t >= start && c.t <= end)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    async fn fixture(
        responses: Vec<(u16, &'static str)>,
    ) -> (PublicClient, tokio::task::JoinHandle<usize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/info", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let count = responses.len();
            for (status, body) in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut reader = BufReader::new(&mut socket);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).await.unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut body_bytes = vec![0; length];
                reader.read_exact(&mut body_bytes).await.unwrap();
                drop(reader);
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            count
        });
        let mut client = PublicClient::new().unwrap();
        client.endpoint = endpoint;
        (client, server)
    }
    #[tokio::test]
    async fn retries_rate_limits_and_server_failures() {
        let (client, server) = fixture(vec![(429, "{}"), (503, "{}"), (200, "[]")]).await;
        assert!(client.candles("BTC", "1m", 0, 0).await.unwrap().is_empty());
        assert_eq!(server.await.unwrap(), 3);
    }
    #[tokio::test]
    async fn rejects_wrong_asset_and_does_not_retry_bad_requests() {
        let (client, server) = fixture(vec![(200,r#"[{"t":0,"T":59999,"s":"ETH","i":"1m","o":"1","h":"1","l":"1","c":"1","v":"1","n":1}]"#)]).await;
        assert!(client.candles("BTC", "1m", 0, 0).await.is_err());
        assert_eq!(server.await.unwrap(), 1);
        let (client, server) = fixture(vec![(400, "{}")]).await;
        assert!(client.candles("BTC", "1m", 0, 0).await.is_err());
        assert_eq!(server.await.unwrap(), 1);
    }
    #[tokio::test]
    async fn funding_rejects_inconsistent_rates_and_duplicate_hours() {
        for body in [
            r#"[{"coin":"ETH","time":3600001,"fundingRate":"0.0001"}]"#,
            r#"[{"coin":"BTC","time":3600001,"fundingRate":"0.01"}]"#,
            r#"[{"coin":"BTC","time":3600001,"fundingRate":"0.0001"},{"coin":"BTC","time":3600002,"fundingRate":"0.0001"}]"#,
        ] {
            let (client, server) = fixture(vec![(200, body)]).await;
            assert!(client.funding("BTC", 3600000, 7200000).await.is_err());
            assert_eq!(server.await.unwrap(), 1);
        }
        let (client, server) = fixture(vec![(
            200,
            r#"[{"coin":"BTC","time":3600001,"fundingRate":"-0.0001"}]"#,
        )])
        .await;
        assert_eq!(
            client.funding("BTC", 3600000, 7200000).await.unwrap().len(),
            1
        );
        assert_eq!(server.await.unwrap(), 1);
    }
}
