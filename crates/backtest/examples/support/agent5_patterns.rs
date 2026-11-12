//! Research hypotheses only. No production registration or parameter changes.
use anyhow::Result;
use rust_decimal::Decimal as D;
use trading_domain::data::Candle;
use trading_paper::dec;
use trading_quant::{
    indicators::{atr, ema},
    strategies::{Decision, Direction},
};

pub fn decisions(cs: &[Candle], pattern: &str, original: Vec<Decision>) -> Result<Vec<Decision>> {
    if pattern == "baseline" {
        return Ok(original);
    }
    if pattern == "breakout_only" {
        return Ok(original
            .into_iter()
            .filter(|d| d.strategy == "breakout_v1")
            .collect());
    }
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
    let trend_up = fast > slow && slow > e50[n - 5].unwrap();
    let trend_down = fast < slow && slow < e50[n - 5].unwrap();
    let strong_up = last.c > last.o && (last.c - last.l) / (last.h - last.l) >= dec("0.75");
    let strong_down = last.c < last.o && (last.c - last.l) / (last.h - last.l) <= dec("0.25");
    let mut long = false;
    let mut short = false;
    match pattern {
        "trend_breakout" | "bounded_breakout" => {
            let breakout = original
                .iter()
                .find(|d| d.strategy == "breakout_v1")
                .unwrap();
            long = breakout.direction == Direction::Long
                && trend_up
                && strong_up
                && last.h - last.l <= a * dec("2.5");
            short = breakout.direction == Direction::Short
                && trend_down
                && strong_down
                && last.h - last.l <= a * dec("2.5");
            if pattern == "bounded_breakout" {
                let channel = &cs[n - 20..n];
                let high = channel.iter().map(|c| c.h).max().unwrap();
                let low = channel.iter().map(|c| c.l).min().unwrap();
                long &= last.c - high <= a * dec("0.5");
                short &= low - last.c <= a * dec("0.5");
            }
        }
        "breakout_retest" => {
            // Breakout 2..6 completed bars ago; retest and rejection of its old channel.
            for age in 2..=6 {
                let b = n - age;
                let breakout = &cs[b];
                let channel = &cs[b - 20..b];
                let high = channel.iter().map(|c| c.h).max().unwrap();
                let low = channel.iter().map(|c| c.l).min().unwrap();
                let volume = channel.iter().map(|c| c.v).sum::<D>() / dec("20");
                if volume <= D::ZERO || breakout.v < volume * dec("1.5") {
                    continue;
                }
                let since = &cs[b + 1..=n];
                long |= trend_up
                    && breakout.c > high
                    && last.l <= high + a * dec("0.25")
                    && last.c > high
                    && last.c > prev.h
                    && strong_up
                    && since.iter().all(|c| c.c >= high - a * dec("0.25"));
                short |= trend_down
                    && breakout.c < low
                    && last.h >= low - a * dec("0.25")
                    && last.c < low
                    && last.c < prev.l
                    && strong_down
                    && since.iter().all(|c| c.c <= low + a * dec("0.25"));
            }
        }
        "trend_pullback" => {
            let touch_up = (n - 3..n).any(|j| cs[j].l <= e20[j].unwrap());
            let touch_down = (n - 3..n).any(|j| cs[j].h >= e20[j].unwrap());
            let kept_up = (n - 3..=n).all(|j| cs[j].c > e50[j].unwrap());
            let kept_down = (n - 3..=n).all(|j| cs[j].c < e50[j].unwrap());
            let bounded = (last.c - fast).abs() <= a && last.h - last.l <= a * dec("2");
            long = trend_up && touch_up && kept_up && bounded && strong_up && last.c > prev.h;
            short =
                trend_down && touch_down && kept_down && bounded && strong_down && last.c < prev.l;
        }
        _ => anyhow::bail!("unknown research pattern"),
    }
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
        reason: "research_only_closed_price_action".into(),
    }])
}
