use anyhow::{Result, ensure};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use trading_domain::{
    ListedMarket,
    data::{Book, Candle, symbol},
};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Metadata {
        markets: Vec<ListedMarket>,
    },
    Candle {
        candle: Candle,
    },
    Book {
        book: Book,
    },
    Oracle {
        symbol: String,
        observed_at: i64,
        price: Decimal,
    },
    Funding {
        symbol: String,
        hour: i64,
        rate: Decimal,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub available_at: i64,
    pub event: Event,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub version: u32,
    pub source: String,
    pub capture_start: i64,
    pub capture_end: i64,
    pub records: Vec<Record>,
}
impl Dataset {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1 && !self.source.is_empty() && self.source.len() <= 500,
            "invalid dataset identity"
        );
        ensure!(
            self.capture_start >= 0
                && self.capture_end > self.capture_start
                && self.capture_end < 4_000_000_000_000,
            "invalid capture range"
        );
        ensure!(
            !self.records.is_empty() && self.records.len() <= 2_000_000,
            "invalid dataset size"
        );
        let mut previous = 0;
        let mut keys = BTreeSet::new();
        for r in &self.records {
            ensure!(
                r.available_at >= previous && r.available_at < self.capture_end,
                "unordered or out-of-range availability"
            );
            previous = r.available_at;
            let key = match &r.event {
                Event::Metadata { markets } => {
                    let names: BTreeSet<_> =
                        markets.iter().map(|m| m.metadata.name.as_str()).collect();
                    ensure!(
                        names == trading_domain::SYMBOLS.into_iter().collect()
                            && markets.len() == 4
                            && markets
                                .iter()
                                .map(|m| m.asset_index)
                                .collect::<BTreeSet<_>>()
                                .len()
                                == 4,
                        "invalid metadata universe"
                    );
                    ensure!(
                        markets.iter().all(|m| m.metadata.sz_decimals <= 6
                            && m.metadata.max_leverage > 0
                            && !m.metadata.is_delisted),
                        "invalid market metadata"
                    );
                    continue;
                }
                Event::Candle { candle: c } => {
                    c.validate()?;
                    trading_quant::ClosedCandle::new(c.clone(), true, r.available_at)?;
                    format!("c:{}:{}:{}", c.s, c.i, c.t)
                }
                Event::Book { book: b } => {
                    b.validate()?;
                    ensure!(b.time <= r.available_at, "book not yet available");
                    format!("b:{}:{}", b.coin, b.time)
                }
                Event::Oracle {
                    symbol: s,
                    observed_at,
                    price,
                } => {
                    symbol(s)?;
                    ensure!(
                        *observed_at >= 0
                            && *observed_at <= r.available_at
                            && *price > Decimal::ZERO
                            && *price <= Decimal::from(1_000_000_000),
                        "invalid oracle"
                    );
                    format!("o:{s}:{observed_at}")
                }
                Event::Funding {
                    symbol: s,
                    hour,
                    rate,
                } => {
                    symbol(s)?;
                    ensure!(
                        *hour >= 0
                            && hour % trading_paper::HOUR == 0
                            && *hour <= r.available_at
                            && rate.abs() <= trading_paper::dec("0.005"),
                        "invalid funding settlement"
                    );
                    format!("f:{s}:{hour}")
                }
            };
            ensure!(keys.insert(key), "duplicate or revised historical event");
        }
        Ok(())
    }
}
