//! Offline fixed hypotheses; production configuration unchanged.
use anyhow::Result;
use rust_decimal::Decimal as D;
use trading_domain::data::Candle;
use trading_paper::dec;
use trading_quant::{
    indicators::{atr, ema},
    strategies::{Decision, Direction},
};

pub fn decisions(cs: &[Candle], pattern: &str, _original: Vec<Decision>) -> Result<Vec<Decision>> {
    let n = cs.len() - 1;
    let last = &cs[n];
    let prev = &cs[n - 1];
    let closes: Vec<_> = cs.iter().map(|c| c.c).collect();
    let e20 = ema(&closes, 20)?;
    let e50 = ema(&closes, 50)?;
    let fast = e20[n].unwrap();
    let slow = e50[n].unwrap();
    let a = atr(cs, 14)?.unwrap();
    if a <= D::ZERO || last.h == last.l {
        return Ok(vec![]);
    }
    let strong_up = last.c > last.o && (last.c - last.l) / (last.h - last.l) >= dec("0.75");
    let strong_down = last.c < last.o && (last.c - last.l) / (last.h - last.l) <= dec("0.25");
    let bounded = last.h - last.l <= a * dec("2.5");
    let (long, short) = match pattern {
        "range_failed_breakout" => {
            let channel = &cs[n - 21..n - 1];
            let high = channel.iter().map(|c| c.h).max().unwrap();
            let low = channel.iter().map(|c| c.l).min().unwrap();
            let midpoint = (high + low) / dec("2");
            let neutral = (fast - slow).abs() <= a * dec("0.75")
                && (slow - e50[n - 5].unwrap()).abs() <= a * dec("0.30");
            let valid = neutral && bounded && prev.h - prev.l <= a * dec("2.5");
            let third_up = last.c > last.o
                && (last.c - last.l) / (last.h - last.l) >= dec("0.6666666666666666666666666667");
            let third_down = last.c < last.o
                && (last.c - last.l) / (last.h - last.l) <= dec("0.3333333333333333333333333333");
            (
                valid
                    && prev.l < low
                    && prev.c > low
                    && prev.c < high
                    && last.c > prev.h
                    && third_up
                    && last.c < midpoint,
                valid
                    && prev.h > high
                    && prev.c < high
                    && prev.c > low
                    && last.c < prev.l
                    && third_down
                    && last.c > midpoint,
            )
        }
        "trend_failed_counterbreak" => {
            let channel = &cs[n - 10..n];
            let high = channel.iter().map(|c| c.h).max().unwrap();
            let low = channel.iter().map(|c| c.l).min().unwrap();
            let volume = cs[n - 20..n].iter().map(|c| c.v).sum::<D>() / dec("20");
            let valid = bounded && volume > D::ZERO && last.v >= volume;
            (
                valid
                    && fast > slow
                    && slow > e50[n - 5].unwrap()
                    && last.l < low
                    && last.c > low
                    && last.c < high
                    && strong_up,
                valid
                    && fast < slow
                    && slow < e50[n - 5].unwrap()
                    && last.h > high
                    && last.c < high
                    && last.c > low
                    && strong_down,
            )
        }
        _ => anyhow::bail!("unknown agent2 research pattern"),
    };
    if long == short {
        return Ok(vec![]);
    }
    Ok(vec![Decision {
        strategy: pattern.into(),
        direction: if long {
            Direction::Long
        } else {
            Direction::Short
        },
        reason: "agent2_closed_price_action_only".into(),
    }])
}
