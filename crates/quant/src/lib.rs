pub mod config;
pub mod indicators;
pub mod strategies;

use anyhow::{Result, ensure};
use config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use strategies::{Decision, Direction, Strategy};
use trading_domain::data::{Candle, interval_ms};

/// Construct only from confirmed storage or a replay source at its simulated clock.
#[derive(Clone, Debug)]
pub struct ClosedCandle(Candle);
impl ClosedCandle {
    pub fn new(candle: Candle, finalized: bool, as_of: i64) -> Result<Self> {
        ensure!(
            finalized && candle.end < as_of,
            "unconfirmed or future candle"
        );
        candle.validate()?;
        let bound = rust_decimal::Decimal::from(1_000_000_000_000_000_i64);
        ensure!(
            candle.h <= bound && candle.v <= bound,
            "candle exceeds analysis numeric bounds"
        );
        Ok(Self(candle))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Evaluation {
    pub config_id: String,
    pub symbol: String,
    pub interval: String,
    pub open_time: i64,
    pub close_time: i64,
    pub window_start: i64,
    pub input_hash: String,
    pub indicators: Option<indicators::Snapshot>,
    pub decisions: Vec<Decision>,
    pub analysis_only: bool,
}
pub struct Engine {
    config: Config,
    strategies: Vec<Box<dyn Strategy>>,
    id: String,
    manifest: Value,
}
impl Engine {
    pub fn builtin(config: Config) -> Result<Self> {
        config.validate()?;
        let strategies = strategies::builtins(&config);
        Self::new(config, strategies)
    }
    /// Registration is external to the engine; new Strategy implementations need no engine changes.
    pub fn new(config: Config, strategies: Vec<Box<dyn Strategy>>) -> Result<Self> {
        config.validate()?;
        ensure!(!strategies.is_empty(), "no strategies registered");
        let names: std::collections::BTreeSet<_> = strategies.iter().map(|s| s.name()).collect();
        ensure!(names.len() == strategies.len(), "duplicate strategy names");
        let descriptors: Vec<_> = strategies
            .iter()
            .map(|s| json!({"name":s.name(),"parameters":s.parameters()}))
            .collect();
        let manifest = json!({"algorithm":"quant-v1","config":config,"strategies":descriptors});
        let id = hash(&manifest)?;
        Ok(Self {
            config,
            strategies,
            id,
            manifest,
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn window(&self) -> usize {
        self.config.history_window
    }
    pub fn manifest(&self) -> &Value {
        &self.manifest
    }
    pub fn evaluate(&self, history: &[ClosedCandle]) -> Result<Evaluation> {
        ensure!(!history.is_empty(), "no closed candles");
        let start = history.len().saturating_sub(self.window());
        let candles: Vec<_> = history[start..].iter().map(|c| c.0.clone()).collect();
        let last = candles.last().unwrap();
        let step = interval_ms(&last.i)?;
        for pair in candles.windows(2) {
            ensure!(
                pair[0].s == pair[1].s
                    && pair[0].i == pair[1].i
                    && pair[0].t.checked_add(step) == Some(pair[1].t),
                "mixed, duplicate, out-of-order or gapped series"
            );
        }
        let warmed = candles.len() == self.window();
        let mut decisions = Vec::new();
        for strategy in &self.strategies {
            let (direction, reason) = if warmed {
                strategy.evaluate(&candles)?
            } else {
                (Direction::Hold, "warmup")
            };
            decisions.push(Decision {
                strategy: strategy.name().into(),
                direction,
                reason: reason.into(),
            });
        }
        Ok(Evaluation {
            config_id: self.id.clone(),
            symbol: last.s.clone(),
            interval: last.i.clone(),
            open_time: last.t,
            close_time: last.end,
            window_start: candles[0].t,
            input_hash: hash(&serde_json::to_value(&candles)?)?,
            indicators: if warmed {
                Some(indicators::snapshot(&candles)?)
            } else {
                None
            },
            decisions,
            analysis_only: true,
        })
    }
    pub fn replay(&self, history: &[ClosedCandle]) -> Result<Vec<Evaluation>> {
        (1..=history.len())
            .map(|n| self.evaluate(&history[..n]))
            .collect()
    }
}
fn hash(value: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
