//! Frozen offline regime hypotheses. Production strategies remain untouched.
#[path = "patterns.rs"]
mod prior;
use anyhow::Result;
use rust_decimal::Decimal as D;
use trading_domain::data::Candle;
use trading_paper::dec;
use trading_quant::{
    indicators::{atr, ema},
    strategies::{Decision, Direction},
};

pub fn decisions(cs: &[Candle], pattern: &str, original: Vec<Decision>) -> Result<Vec<Decision>> {
    let signals = prior::decisions(cs, "trend_breakout", original)?;
    let n = cs.len() - 1;
    let prior_cs = &cs[..n];
    let prior_closes: Vec<_> = prior_cs.iter().map(|c| c.c).collect();
    let direction_ok = |long: bool| -> Result<bool> {
        match pattern {
            "efficiency_breakout" => {
                let displacement = cs[n - 1].c - cs[n - 21].c;
                let path: D = (n - 20..n).map(|j| (cs[j].c - cs[j - 1].c).abs()).sum();
                Ok(path > D::ZERO
                    && displacement.abs() / path >= dec("0.30")
                    && if long {
                        displacement > D::ZERO
                    } else {
                        displacement < D::ZERO
                    })
            }
            "separation_breakout" => {
                let fast = ema(&prior_closes, 20)?[n - 1].unwrap();
                let slow = ema(&prior_closes, 50)?[n - 1].unwrap();
                let a = atr(prior_cs, 14)?.unwrap();
                Ok(a > D::ZERO
                    && (fast - slow).abs() / a >= dec("0.50")
                    && if long { fast > slow } else { fast < slow })
            }
            _ => anyhow::bail!("unknown frozen regime hypothesis"),
        }
    };
    let mut filtered = vec![];
    for mut signal in signals {
        if direction_ok(signal.direction == Direction::Long)? {
            signal.strategy = pattern.into();
            signal.reason = "frozen_prebreakout_regime_filter".into();
            filtered.push(signal);
        }
    }
    Ok(filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn trend() -> Vec<Candle> {
        let mut cs: Vec<_> = (0..100)
            .map(|n| {
                let c = dec("100") + D::from(n) / dec("10");
                Candle {
                    t: n * 3_600_000,
                    end: (n + 1) * 3_600_000 - 1,
                    s: "BTC".into(),
                    i: "1h".into(),
                    o: c - dec("0.1"),
                    h: c + dec("0.3"),
                    l: c - dec("0.3"),
                    c,
                    v: dec("100"),
                    n: 10,
                }
            })
            .collect();
        cs[99].o = dec("110");
        cs[99].l = dec("110");
        cs[99].h = dec("110.9");
        cs[99].c = dec("110.8");
        cs[99].v = dec("160");
        cs
    }
    fn run(cs: &[Candle], pattern: &str, direction: Direction) -> Vec<Decision> {
        decisions(
            cs,
            pattern,
            vec![Decision {
                strategy: "breakout_v1".into(),
                direction,
                reason: "fixture".into(),
            }],
        )
        .unwrap()
    }
    #[test]
    fn prior_direction_is_required_and_price_reflection_is_symmetric() {
        let cs = trend();
        for pattern in ["efficiency_breakout", "separation_breakout"] {
            assert_eq!(run(&cs, pattern, Direction::Long).len(), 1);
            assert!(run(&cs, pattern, Direction::Short).is_empty());
            let mirror: Vec<_> = cs
                .iter()
                .map(|c| Candle {
                    o: dec("250") - c.o,
                    c: dec("250") - c.c,
                    h: dec("250") - c.l,
                    l: dec("250") - c.h,
                    ..c.clone()
                })
                .collect();
            assert_eq!(run(&mirror, pattern, Direction::Short).len(), 1);
        }
    }
}
