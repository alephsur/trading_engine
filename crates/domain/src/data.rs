//! Validated public market data. Decimal fields must arrive as strings.
use crate::SYMBOLS;
use anyhow::{Result, ensure};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub const INTERVALS: [&str; 4] = ["1m", "5m", "15m", "1h"];
pub fn interval_ms(interval: &str) -> Result<i64> {
    Ok(match interval {
        "1m" => 60_000,
        "5m" => 300_000,
        "15m" => 900_000,
        "1h" => 3_600_000,
        _ => anyhow::bail!("unsupported interval"),
    })
}
pub fn symbol(symbol: &str) -> Result<()> {
    ensure!(SYMBOLS.contains(&symbol), "unsupported perpetual");
    Ok(())
}
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock before epoch")
        .as_millis() as i64
}
/// Two seconds of grace; only HTTP snapshots fetched after this boundary can finalize.
pub fn last_closed_open(now: i64, step: i64) -> i64 {
    (now - 2_000).div_euclid(step) * step - step
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candle {
    pub t: i64,
    #[serde(rename = "T")]
    pub end: i64,
    pub s: String,
    pub i: String,
    pub o: Decimal,
    pub h: Decimal,
    pub l: Decimal,
    pub c: Decimal,
    pub v: Decimal,
    pub n: i64,
}
impl Candle {
    pub fn validate(&self) -> Result<()> {
        symbol(&self.s)?;
        let step = interval_ms(&self.i)?;
        ensure!(
            self.t >= 0 && self.t % step == 0 && self.t.checked_add(step - 1) == Some(self.end),
            "invalid candle boundaries"
        );
        ensure!(
            self.l > Decimal::ZERO
                && self.h >= self.l
                && self.o >= self.l
                && self.o <= self.h
                && self.c >= self.l
                && self.c <= self.h,
            "invalid OHLC"
        );
        ensure!(
            self.v >= Decimal::ZERO && self.n >= 0,
            "invalid candle volume/count"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub coin: String,
    pub side: String,
    pub px: Decimal,
    pub sz: Decimal,
    pub time: i64,
    pub tid: i64,
    pub hash: String,
}
impl Trade {
    pub fn validate(&self) -> Result<()> {
        symbol(&self.coin)?;
        ensure!(self.side == "B" || self.side == "A", "invalid trade side");
        ensure!(
            self.px > Decimal::ZERO
                && self.sz > Decimal::ZERO
                && self.time >= 0
                && self.time <= now_ms() + 5_000
                && self.tid >= 0,
            "invalid trade"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    pub px: Decimal,
    pub sz: Decimal,
    pub n: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub coin: String,
    pub time: i64,
    pub levels: [Vec<Level>; 2],
}
impl Book {
    pub fn validate(&self) -> Result<()> {
        symbol(&self.coin)?;
        ensure!(self.time >= 0, "invalid book time");
        for (side, levels) in self.levels.iter().enumerate() {
            ensure!(
                !levels.is_empty() && levels.len() <= 20,
                "empty or oversized book"
            );
            for level in levels {
                ensure!(
                    level.px > Decimal::ZERO && level.sz > Decimal::ZERO && level.n > 0,
                    "invalid book level"
                );
            }
            for pair in levels.windows(2) {
                ensure!(
                    if side == 0 {
                        pair[0].px > pair[1].px
                    } else {
                        pair[0].px < pair[1].px
                    },
                    "unsorted book"
                );
            }
        }
        ensure!(self.levels[0][0].px < self.levels[1][0].px, "crossed book");
        Ok(())
    }
}
#[derive(Debug)]
pub enum MarketEvent {
    Candle(Candle),
    Trades(Vec<Trade>),
    Book(Book),
}
pub fn parse_event(value: &serde_json::Value) -> Result<Option<MarketEvent>> {
    let data = value["data"].clone();
    Ok(match value["channel"].as_str() {
        Some("candle") => {
            let candle: Candle = serde_json::from_value(data)?;
            candle.validate()?;
            Some(MarketEvent::Candle(candle))
        }
        Some("trades") => {
            let trades: Vec<Trade> = serde_json::from_value(data)?;
            for trade in &trades {
                trade.validate()?;
            }
            Some(MarketEvent::Trades(trades))
        }
        Some("l2Book") => {
            let book: Book = serde_json::from_value(data)?;
            book.validate()?;
            Some(MarketEvent::Book(book))
        }
        Some("error") => anyhow::bail!("exchange subscription error"),
        Some("allMids" | "pong" | "subscriptionResponse") => None,
        _ => anyhow::bail!("unexpected websocket channel"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finalization_never_uses_open_candle() {
        assert_eq!(last_closed_open(120_000, 60_000), 0);
        assert_eq!(last_closed_open(122_000, 60_000), 60_000);
    }
    #[test]
    fn rejects_bad_candle_and_float_prices() {
        let mut v = serde_json::json!({"t":60000,"T":119999,"s":"BTC","i":"1m","o":"2","h":"3","l":"1","c":"2","v":"0.01","n":1});
        let c: Candle = serde_json::from_value(v.clone()).unwrap();
        c.validate().unwrap();
        v["l"] = serde_json::json!("4");
        assert!(
            serde_json::from_value::<Candle>(v.clone())
                .unwrap()
                .validate()
                .is_err()
        );
        v["o"] = serde_json::json!(2.5);
        assert!(serde_json::from_value::<Candle>(v).is_err());
    }
    #[test]
    fn validates_books_and_trades() {
        let mut book: Book = serde_json::from_value(serde_json::json!({"coin":"BTC","time":1,"levels":[[{"px":"10","sz":"1","n":1}],[{"px":"11","sz":"2","n":1}]]})).unwrap();
        book.validate().unwrap();
        book.levels[1][0].px = Decimal::from(9);
        assert!(book.validate().is_err());
        let t: Trade = serde_json::from_value(serde_json::json!({"coin":"BTC","time":1,"tid":1,"side":"X","px":"10","sz":"1","hash":"h"})).unwrap();
        assert!(t.validate().is_err());
    }
}
