use crate::{config::*, indicators::*};
use anyhow::Result;
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use trading_domain::data::Candle;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Long,
    Short,
    Hold,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub strategy: String,
    pub direction: Direction,
    pub reason: String,
}
/// Pure analysis only: implementations receive immutable closed candles, never risk or execution handles.
pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;
    fn parameters(&self) -> Value;
    fn evaluate(&self, candles: &[Candle]) -> Result<(Direction, &'static str)>;
}
pub struct EmaTrend(pub EmaConfig);
pub struct Momentum(pub MomentumConfig);
pub struct Breakout(pub BreakoutConfig);
pub struct MeanReversion(pub ReversionConfig);
/// Frozen rules validated in the 1h research; no lower-timeframe entries.
pub struct ConfirmedBreakout;
impl Strategy for ConfirmedBreakout {
    fn name(&self) -> &'static str {
        "confirmed_breakout_1h_v1"
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "interval": "1h", "history_window": 100,
            "ema_fast": 20, "ema_slow": 50, "slope_lookback": 5,
            "channel_lookback": 20, "previous_volume_period": 20,
            "min_volume_multiple": "1.5", "atr_period": 14,
            "max_range_atr": "2.5", "long_close_location_min": "0.75",
            "short_close_location_max": "0.25"
        })
    }
    fn evaluate(&self, candles: &[Candle]) -> Result<(Direction, &'static str)> {
        if candles.iter().any(|c| c.i != "1h") {
            return Ok((Direction::Hold, "unsupported_interval"));
        }
        if candles.len() < 100 {
            return Ok((Direction::Hold, "warmup"));
        }
        // Keep direct Strategy calls consistent with Engine's finite rolling window.
        let cs = &candles[candles.len() - 100..];
        let breakout = Breakout(BreakoutConfig {
            lookback: 20,
            volume_period: 20,
            min_volume_multiple: D::new(15, 1),
        });
        let (direction, reason) = breakout.evaluate(cs)?;
        if direction == Direction::Hold {
            return Ok((direction, reason));
        }
        let n = cs.len() - 1;
        let last = &cs[n];
        let range = last.h - last.l;
        let a = atr(cs, 14)?.unwrap();
        if a <= D::ZERO || range <= D::ZERO {
            return Ok((Direction::Hold, "zero_volatility"));
        }
        if range > a * D::new(25, 1) {
            return Ok((Direction::Hold, "excessive_signal_range"));
        }
        let closes: Vec<_> = cs.iter().map(|c| c.c).collect();
        let fast = ema(&closes, 20)?[n].unwrap();
        let slow = ema(&closes, 50)?;
        let current_slow = slow[n].unwrap();
        let previous_slow = slow[n - 5].unwrap();
        let trend = match direction {
            Direction::Long => fast > current_slow && current_slow > previous_slow,
            Direction::Short => fast < current_slow && current_slow < previous_slow,
            Direction::Hold => unreachable!(),
        };
        if !trend {
            return Ok((Direction::Hold, "trend_filter"));
        }
        let location = (last.c - last.l) / range;
        let confirmed = match direction {
            Direction::Long => last.c > last.o && location >= D::new(75, 2),
            Direction::Short => last.c < last.o && location <= D::new(25, 2),
            Direction::Hold => unreachable!(),
        };
        if !confirmed {
            return Ok((Direction::Hold, "weak_directional_close"));
        }
        Ok((direction, "confirmed_trend_breakout"))
    }
}
impl Strategy for EmaTrend {
    fn name(&self) -> &'static str {
        "ema_trend_v1"
    }
    fn parameters(&self) -> Value {
        serde_json::to_value(&self.0).unwrap()
    }
    fn evaluate(&self, cs: &[Candle]) -> Result<(Direction, &'static str)> {
        let xs: Vec<_> = cs.iter().map(|c| c.c).collect();
        let fast = ema(&xs, self.0.fast)?;
        let slow = ema(&xs, self.0.slow)?;
        if cs.len() <= self.0.slow {
            return Ok((Direction::Hold, "warmup"));
        }
        let n = cs.len() - 1;
        if fast[n - 1] <= slow[n - 1] && fast[n] > slow[n] {
            return Ok((Direction::Long, "bullish_ema_cross"));
        }
        if fast[n - 1] >= slow[n - 1] && fast[n] < slow[n] {
            return Ok((Direction::Short, "bearish_ema_cross"));
        }
        Ok((Direction::Hold, "no_ema_cross"))
    }
}
impl Strategy for Momentum {
    fn name(&self) -> &'static str {
        "momentum_rsi_v1"
    }
    fn parameters(&self) -> Value {
        serde_json::to_value(&self.0).unwrap()
    }
    fn evaluate(&self, cs: &[Candle]) -> Result<(Direction, &'static str)> {
        let xs: Vec<_> = cs.iter().map(|c| c.c).collect();
        let p = &self.0;
        let Some(r) = rsi(&xs, p.rsi_period)? else {
            return Ok((Direction::Hold, "warmup"));
        };
        if cs.len() <= p.lookback {
            return Ok((Direction::Hold, "warmup"));
        }
        let previous = xs[xs.len() - 1 - p.lookback];
        let last = *xs.last().unwrap();
        let delta = previous * p.min_change_bps / D::from(10_000);
        if last >= previous + delta && r >= p.long_min && r <= p.long_max {
            return Ok((Direction::Long, "positive_momentum_rsi"));
        }
        if last <= previous - delta && r >= p.short_min && r <= p.short_max {
            return Ok((Direction::Short, "negative_momentum_rsi"));
        }
        Ok((Direction::Hold, "momentum_or_rsi_filter"))
    }
}
impl Strategy for Breakout {
    fn name(&self) -> &'static str {
        "breakout_v1"
    }
    fn parameters(&self) -> Value {
        serde_json::to_value(&self.0).unwrap()
    }
    fn evaluate(&self, cs: &[Candle]) -> Result<(Direction, &'static str)> {
        let p = &self.0;
        if cs.len() <= p.lookback.max(p.volume_period) {
            return Ok((Direction::Hold, "warmup"));
        }
        let last = cs.last().unwrap();
        let previous = &cs[..cs.len() - 1];
        let volumes: Vec<_> = previous.iter().map(|c| c.v).collect();
        let mean = sma(&volumes, p.volume_period)?.unwrap();
        if mean == D::ZERO || last.v < mean * p.min_volume_multiple {
            return Ok((Direction::Hold, "volume_filter"));
        }
        let channel = &previous[previous.len() - p.lookback..];
        let high = channel.iter().map(|c| c.h).max().unwrap();
        let low = channel.iter().map(|c| c.l).min().unwrap();
        if last.c > high {
            return Ok((Direction::Long, "closed_above_previous_channel"));
        }
        if last.c < low {
            return Ok((Direction::Short, "closed_below_previous_channel"));
        }
        Ok((Direction::Hold, "inside_previous_channel"))
    }
}
impl Strategy for MeanReversion {
    fn name(&self) -> &'static str {
        "mean_reversion_v1"
    }
    fn parameters(&self) -> Value {
        serde_json::to_value(&self.0).unwrap()
    }
    fn evaluate(&self, cs: &[Candle]) -> Result<(Direction, &'static str)> {
        let p = &self.0;
        let xs: Vec<_> = cs.iter().map(|c| c.c).collect();
        let mean = ema(&xs, p.ema_period)?.last().copied().flatten();
        let (Some(mean), Some(a), Some(r)) =
            (mean, atr(cs, p.atr_period)?, rsi(&xs, p.rsi_period)?)
        else {
            return Ok((Direction::Hold, "warmup"));
        };
        if a == D::ZERO {
            return Ok((Direction::Hold, "zero_volatility"));
        }
        let last = *xs.last().unwrap();
        let distance = a * p.atr_multiple;
        if last < mean - distance && r <= p.oversold {
            return Ok((Direction::Long, "oversold_below_ema_atr_band"));
        }
        if last > mean + distance && r >= p.overbought {
            return Ok((Direction::Short, "overbought_above_ema_atr_band"));
        }
        Ok((Direction::Hold, "inside_reversion_filters"))
    }
}
pub fn builtins(config: &Config) -> Vec<Box<dyn Strategy>> {
    if config.profile == Profile::ConfirmedBreakout1hV1 {
        return vec![Box::new(ConfirmedBreakout)];
    }
    vec![
        Box::new(EmaTrend(config.ema.clone())),
        Box::new(Momentum(config.momentum.clone())),
        Box::new(Breakout(config.breakout.clone())),
        Box::new(MeanReversion(config.mean_reversion.clone())),
    ]
}
