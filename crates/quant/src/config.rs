use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmaConfig {
    pub fast: usize,
    pub slow: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MomentumConfig {
    pub lookback: usize,
    pub rsi_period: usize,
    pub long_min: D,
    pub long_max: D,
    pub short_min: D,
    pub short_max: D,
    pub min_change_bps: D,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreakoutConfig {
    pub lookback: usize,
    pub volume_period: usize,
    pub min_volume_multiple: D,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReversionConfig {
    pub ema_period: usize,
    pub atr_period: usize,
    pub rsi_period: usize,
    pub atr_multiple: D,
    pub oversold: D,
    pub overbought: D,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: String,
    pub history_window: usize,
    pub ema: EmaConfig,
    pub momentum: MomentumConfig,
    pub breakout: BreakoutConfig,
    pub mean_reversion: ReversionConfig,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: "initial-v1".into(),
            history_window: 100,
            ema: EmaConfig { fast: 20, slow: 50 },
            momentum: MomentumConfig {
                lookback: 5,
                rsi_period: 14,
                long_min: D::from(55),
                long_max: D::from(70),
                short_min: D::from(30),
                short_max: D::from(45),
                min_change_bps: D::from(10),
            },
            breakout: BreakoutConfig {
                lookback: 20,
                volume_period: 20,
                min_volume_multiple: D::new(15, 1),
            },
            mean_reversion: ReversionConfig {
                ema_period: 20,
                atr_period: 14,
                rsi_period: 14,
                atr_multiple: D::from(2),
                oversold: D::from(30),
                overbought: D::from(70),
            },
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.version.is_empty() && self.version.len() <= 64,
            "invalid config version"
        );
        ensure!(
            (60..=500).contains(&self.history_window),
            "history_window must be 60..500"
        );
        for p in [
            self.ema.fast,
            self.ema.slow,
            self.momentum.lookback,
            self.momentum.rsi_period,
            self.breakout.lookback,
            self.breakout.volume_period,
            self.mean_reversion.ema_period,
            self.mean_reversion.atr_period,
            self.mean_reversion.rsi_period,
        ] {
            ensure!(
                (2..=200).contains(&p) && p < self.history_window,
                "invalid strategy period"
            );
        }
        ensure!(
            self.ema.fast < self.ema.slow,
            "EMA fast must be smaller than slow"
        );
        let m = &self.momentum;
        ensure!(
            D::ZERO <= m.short_min
                && m.short_min < m.short_max
                && m.short_max < m.long_min
                && m.long_min < m.long_max
                && m.long_max <= D::from(100),
            "invalid momentum RSI bands"
        );
        ensure!(
            m.min_change_bps > D::ZERO && m.min_change_bps <= D::from(1000),
            "invalid momentum change"
        );
        ensure!(
            self.breakout.min_volume_multiple >= D::ONE
                && self.breakout.min_volume_multiple <= D::from(100),
            "invalid volume multiple"
        );
        let r = &self.mean_reversion;
        ensure!(
            r.atr_multiple > D::ZERO && r.atr_multiple <= D::from(100),
            "invalid ATR multiple"
        );
        ensure!(
            D::ZERO < r.oversold && r.oversold < r.overbought && r.overbought < D::from(100),
            "invalid reversion RSI bands"
        );
        Ok(())
    }
}
