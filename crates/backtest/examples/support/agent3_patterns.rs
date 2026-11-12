//! Preregistered agent 3 research; no production strategy registration.
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
    let closes: Vec<_> = cs.iter().map(|c| c.c).collect();
    let e20 = ema(&closes, 20)?;
    let e50 = ema(&closes, 50)?;
    let a = atr(cs, 14)?.unwrap();
    let prior_atr = atr(&cs[..=n - 4], 14)?.unwrap();
    if a <= D::ZERO || prior_atr <= D::ZERO || last.h <= last.l {
        return Ok(vec![]);
    }
    let trend_up = e20[n].unwrap() > e50[n].unwrap() && e50[n].unwrap() > e50[n - 5].unwrap();
    let trend_down = e20[n].unwrap() < e50[n].unwrap() && e50[n].unwrap() < e50[n - 5].unwrap();
    let strong_up = last.c > last.o && (last.c - last.l) / (last.h - last.l) >= dec("0.75");
    let strong_down = last.c < last.o && (last.c - last.l) / (last.h - last.l) <= dec("0.25");
    let bounded = last.h - last.l <= a * dec("2.5");
    let pause = &cs[n - 3..n];
    let ph = pause.iter().map(|c| c.h).max().unwrap();
    let pl = pause.iter().map(|c| c.l).min().unwrap();
    let pv = pause.iter().map(|c| c.v).sum::<D>() / dec("3");
    let mut long = false;
    let mut short = false;
    match pattern {
        "impulse_flag" => {
            let impulse = cs[n - 4].c - cs[n - 8].c;
            let iv = cs[n - 7..=n - 4].iter().map(|c| c.v).sum::<D>() / dec("4");
            if iv > D::ZERO && pv < iv && last.v >= pv {
                if impulse >= prior_atr * dec("2") {
                    let retrace = (cs[n - 4].c - pl) / impulse;
                    long = retrace >= dec("0.25")
                        && retrace <= dec("0.60")
                        && ph <= cs[n - 4].c + prior_atr * dec("0.25")
                        && last.c > ph;
                }
                if -impulse >= prior_atr * dec("2") {
                    let retrace = (ph - cs[n - 4].c) / (-impulse);
                    short = retrace >= dec("0.25")
                        && retrace <= dec("0.60")
                        && pl >= cs[n - 4].c - prior_atr * dec("0.25")
                        && last.c < pl;
                }
            }
        }
        "efficient_pause" => {
            let travel = (n - 20..n)
                .map(|j| (cs[j].c - cs[j - 1].c).abs())
                .sum::<D>();
            let efficiency = if travel > D::ZERO {
                (cs[n - 1].c - cs[n - 21].c).abs() / travel
            } else {
                D::ZERO
            };
            let impulse = cs[n - 4].c - cs[n - 16].c;
            let volume = cs[n - 20..n].iter().map(|c| c.v).sum::<D>() / dec("20");
            if efficiency >= dec("0.40")
                && ph - pl <= prior_atr * dec("1.5")
                && last.v >= volume
                && volume > D::ZERO
            {
                long = impulse >= prior_atr * dec("2")
                    && last.c > ph
                    && (n - 3..n).all(|j| cs[j].c >= e20[j].unwrap());
                short = -impulse >= prior_atr * dec("2")
                    && last.c < pl
                    && (n - 3..n).all(|j| cs[j].c <= e20[j].unwrap());
            }
        }
        _ => anyhow::bail!("unknown agent3 hypothesis"),
    }
    long &= trend_up && strong_up && bounded;
    short &= trend_down && strong_down && bounded;
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
        reason: "agent3_preregistered_closed_price_action".into(),
    }])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<Candle> {
        let mut cs: Vec<_> = (0..100)
            .map(|j| {
                let c = dec("100") + D::from(j) / dec("20");
                Candle {
                    t: j * 3_600_000,
                    end: (j + 1) * 3_600_000 - 1,
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
        for (j, price) in [
            (91, "105.1"),
            (92, "105.7"),
            (93, "106.3"),
            (94, "106.9"),
            (95, "107.5"),
        ] {
            let c = price.parse::<D>().unwrap();
            cs[j].o = c - dec("0.5");
            cs[j].l = c - dec("0.55");
            cs[j].h = c + dec("0.1");
            cs[j].c = c;
            cs[j].v = dec("200");
        }
        for (j, c, h, l) in [
            (96, "107.0", "107.2", "106.8"),
            (97, "106.8", "107.1", "106.5"),
            (98, "106.9", "107.1", "106.6"),
        ] {
            cs[j].o = c.parse().unwrap();
            cs[j].c = c.parse().unwrap();
            cs[j].h = h.parse().unwrap();
            cs[j].l = l.parse().unwrap();
            cs[j].v = dec("90");
        }
        cs[99].o = dec("106.9");
        cs[99].l = dec("106.8");
        cs[99].h = dec("107.5");
        cs[99].c = dec("107.4");
        cs[99].v = dec("150");
        cs
    }
    #[test]
    fn continuation_signals_are_symmetric_and_require_confirmation() {
        for pattern in ["impulse_flag", "efficient_pause"] {
            let cs = fixture();
            assert_eq!(
                decisions(&cs, pattern, vec![]).unwrap()[0].direction,
                Direction::Long
            );
            let mut mirrored = cs.clone();
            for c in &mut mirrored {
                let old_h = c.h;
                c.h = dec("250") - c.l;
                c.l = dec("250") - old_h;
                c.c = dec("250") - c.c;
                c.o = dec("250") - c.o;
            }
            assert_eq!(
                decisions(&mirrored, pattern, vec![]).unwrap()[0].direction,
                Direction::Short
            );
            let mut weak = cs.clone();
            weak[99].h = dec("108.5");
            assert!(decisions(&weak, pattern, vec![]).unwrap().is_empty());
            let mut no_volume = cs;
            no_volume[99].v = dec("1");
            assert!(decisions(&no_volume, pattern, vec![]).unwrap().is_empty());
        }
    }
}
