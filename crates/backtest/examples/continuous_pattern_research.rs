#[path = "support/patterns.rs"]
mod patterns;
// Offline research only; production and baseline approximation remain unchanged.
use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use trading_domain::{
    Metadata, SYMBOLS,
    data::{Book, Candle, Level, interval_ms},
    select_markets,
};
use trading_paper::{Account, Config, Funding, Input, Signal, dec};
use trading_quant::{
    ClosedCandle, Engine, config::Config as StrategyConfig, strategies::Direction,
};
fn read<T: serde::de::DeserializeOwned>(p: impl AsRef<Path>) -> Result<T> {
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}
// Cross by a negligible amount so Decimal division cannot round a stop out of reach.
fn barrier_mid(barrier: D, factor: D, rising: bool) -> D {
    barrier / factor
        * (D::ONE
            + if rising {
                dec("0.000000000001")
            } else {
                -dec("0.000000000001")
            })
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() == 5,
        "usage: pattern_research DATA_DIRECTORY NEW_REPORT.json 1h PATTERN"
    );
    let pattern = &args[4];
    ensure!(
        [
            "baseline",
            "breakout_only",
            "trend_breakout",
            "confirmed_breakout_1h_v1",
            "breakout_retest",
            "trend_pullback"
        ]
        .contains(&pattern.as_str()),
        "unknown pattern"
    );
    let root = Path::new(&args[1]);
    let markets = select_markets(read::<Metadata>(root.join("metadata.json"))?)?;
    let approved = pattern == "confirmed_breakout_1h_v1";
    let engine = Engine::builtin(if approved {
        StrategyConfig::confirmed_breakout_1h()
    } else {
        read::<StrategyConfig>("config/strategies.example.json")?
    })?;
    let risk: Config = read("config/paper.example.json")?;
    let mut results = vec![];
    let intervals = if let Some(interval) = args.get(3) {
        ensure!(
            ["15m", "1h"].contains(&interval.as_str()),
            "unsupported approximation interval"
        );
        vec![interval.as_str()]
    } else {
        vec!["15m", "1h"]
    };
    for interval in intervals {
        let step = interval_ms(interval)?;
        let mut candles: BTreeMap<String, Vec<Candle>> = BTreeMap::new();
        let mut rates: BTreeMap<(String, i64), D> = BTreeMap::new();
        for s in SYMBOLS {
            let bars: Vec<Candle> = read(root.join(format!("{s}-{interval}.json")))?;
            for c in &bars {
                c.validate()?;
                ensure!(c.s == s && c.i == interval, "wrong series");
            }
            ensure!(
                bars.windows(2).all(|w| w[1].t == w[0].t + step),
                "gapped candles"
            );
            candles.insert(s.into(), bars);
            for r in read::<Vec<Value>>(root.join(format!("{s}-funding.json")))? {
                let key = (
                    s.to_owned(),
                    r["time"].as_i64().unwrap() / 3_600_000 * 3_600_000,
                );
                ensure!(
                    rates
                        .insert(key, r["fundingRate"].as_str().unwrap().parse()?)
                        .is_none(),
                    "duplicate funding"
                );
            }
        }
        let count = candles["BTC"].len();
        ensure!(count > 100, "missing warmup");
        let start = candles["BTC"][100].t;
        let end = candles["BTC"][count - 1].t + step;
        ensure!(
            end - start == 180 * 86_400_000,
            "expected exactly 180 days continuous"
        );
        for s in SYMBOLS {
            ensure!(
                candles[s].len() == count && candles[s][100].t == start,
                "unaligned markets"
            );
            for hour in (start..end).step_by(3_600_000) {
                ensure!(
                    rates.contains_key(&(s.to_owned(), hour)),
                    "missing hourly funding for {s}"
                );
            }
        }
        // Compute each decision once from the preceding 100 closed bars, independent of costs.
        let mut all_signals = vec![];
        let mut sequence = 0;
        let mut attribution = BTreeMap::new();
        for n in 100..count {
            let mut signals = vec![];
            for s in SYMBOLS {
                let history: Vec<_> = candles[s][n - 100..n]
                    .iter()
                    .map(|c| ClosedCandle::new(c.clone(), true, candles[s][n].t))
                    .collect::<Result<_>>()?;
                let ev = engine.evaluate(&history)?;
                // No entry from a signal closed before the account started.
                if ev.close_time < start {
                    continue;
                }
                let decisions = if approved {
                    ev.decisions
                } else {
                    patterns::decisions(&candles[s][n - 100..n], pattern, ev.decisions)?
                };
                for decision in decisions {
                    if decision.direction == Direction::Hold {
                        continue;
                    }
                    sequence += 1;
                    attribution.insert(sequence, decision.strategy);
                    signals.push(Signal {
                        event_id: sequence,
                        symbol: s.into(),
                        long: decision.direction == Direction::Long,
                        close_time: ev.close_time,
                        atr: ev.indicators.as_ref().unwrap().atr_14.expect("warm ATR"),
                    });
                }
            }
            all_signals.push(signals);
        }
        for spread_bps in ["2", "5", "10"] {
            let half = dec(spread_bps) / dec("20000");
            let mut account = Account::new(risk.clone(), start)?;
            let mut orders = vec![];
            let mut funding_events = vec![];
            let mut rejections: BTreeMap<String, usize> = BTreeMap::new();
            let mut curve = vec![];
            let mut peak = risk.capital;
            let mut drawdown = D::ZERO;
            let mut last_mid: BTreeMap<String, D> = BTreeMap::new();
            for n in 100..count {
                let at = candles["BTC"][n].t;
                // Entry at next open + 1 second; adverse extreme first for held positions.
                let mut adverse_low: BTreeMap<String, bool> = BTreeMap::new();
                for phase in 0..4 {
                    let now = at
                        + match phase {
                            0 => 1000,
                            1 => step / 3,
                            2 => 2 * step / 3,
                            _ => step - 1,
                        };
                    if phase == 1 {
                        for s in SYMBOLS {
                            adverse_low.insert(
                                s.into(),
                                account
                                    .positions
                                    .iter()
                                    .find(|p| p.symbol == s)
                                    .is_none_or(|p| p.long),
                            );
                        }
                    }
                    let mut books = BTreeMap::new();
                    let mut oracles = BTreeMap::new();
                    for s in SYMBOLS {
                        let c = &candles[s][n];
                        let mut mid = match phase {
                            0 => c.o,
                            1 => {
                                if adverse_low[s] {
                                    c.l
                                } else {
                                    c.h
                                }
                            }
                            2 => {
                                if adverse_low[s] {
                                    c.h
                                } else {
                                    c.l
                                }
                            }
                            _ => c.c,
                        };
                        // Intrabar barrier executions at the barrier, not at a favorable extreme.
                        // Opening gaps still execute at the opening quote.
                        if phase > 0
                            && let Some(p) = account.positions.iter().find(|p| p.symbol == s)
                        {
                            let factor = if p.long { D::ONE - half } else { D::ONE + half };
                            let previous = last_mid[s] * factor;
                            let current = mid * factor;
                            for barrier in [p.stop, p.take_profit] {
                                if (previous > barrier && current <= barrier)
                                    || (previous < barrier && current >= barrier)
                                {
                                    mid = barrier_mid(barrier, factor, current > previous);
                                    break;
                                }
                            }
                        }
                        last_mid.insert(s.into(), mid);
                        // Assumed $1m displayed on each side; not measured historical depth.
                        let size = dec("1000000") / mid;
                        books.insert(
                            s.into(),
                            Book {
                                coin: s.into(),
                                time: now,
                                levels: [
                                    vec![Level {
                                        px: mid * (D::ONE - half),
                                        sz: size,
                                        n: 1,
                                    }],
                                    vec![Level {
                                        px: mid * (D::ONE + half),
                                        sz: size,
                                        n: 1,
                                    }],
                                ],
                            },
                        );
                        oracles.insert(s.into(), (now, mid));
                    }
                    let funding: Vec<_> = if phase == 0 && at % 3_600_000 == 0 {
                        SYMBOLS
                            .iter()
                            .map(|s| Funding {
                                symbol: (*s).into(),
                                hour: at,
                                rate: rates[&(s.to_string(), at)],
                                oracle: candles[*s][n].o,
                            })
                            .collect()
                    } else {
                        vec![]
                    };
                    let input = Input {
                        now,
                        ready: true,
                        markets: &markets,
                        books: &books,
                        funding: &funding,
                        oracles: &oracles,
                    };
                    let changes = account.tick(
                        &input,
                        if phase == 0 {
                            &all_signals[n - 100]
                        } else {
                            &[]
                        },
                    )?;
                    for d in changes.decisions {
                        if !d.accepted {
                            *rejections.entry(d.reason).or_default() += 1;
                        }
                    }
                    orders.extend(changes.orders);
                    for mut f in changes.funding {
                        f["oracle_model"] = json!("APPROXIMATION_hour_open_not_historical_oracle");
                        funding_events.push(f);
                    }
                    ensure!(account.funding_due.is_empty(), "unsettled funding");
                    ensure!(
                        !account
                            .blockers
                            .iter()
                            .any(|b| b == "valuation_unavailable"),
                        "unknown valuation"
                    );
                    peak = peak.max(account.equity);
                    drawdown = drawdown.max((peak - account.equity) / peak);
                }
                curve.push(json!({"time":at+step-1,"equity":account.equity}));
            }
            let mut trades: BTreeMap<i64, Value> = BTreeMap::new();
            let mut fees = D::ZERO;
            for o in &orders {
                for fill in &o.fills {
                    fees += fill.fee;
                    let tr=trades.entry(o.position_id).or_insert_with(||json!({"symbol":o.symbol,"strategy":attribution.get(&o.position_id),"entry":"0","net_realized":"0","closed":false}));
                    let mut net: D = tr["net_realized"].as_str().unwrap().parse()?;
                    if !o.reduce_only {
                        tr["entry"] = json!(fill.price);
                    } else {
                        let entry: D = tr["entry"].as_str().unwrap().parse()?;
                        net += (fill.price - entry)
                            * fill.quantity
                            * if o.buy { -D::ONE } else { D::ONE };
                        tr["closed"] = json!(true);
                    }
                    tr["net_realized"] = json!(net - fill.fee);
                }
            }
            let mut funding_total = D::ZERO;
            for f in &funding_events {
                let amount: D = f["cash_change"].as_str().unwrap().parse()?;
                funding_total += amount;
                if let Some(t) = trades.get_mut(&f["position_id"].as_i64().unwrap()) {
                    let net: D = t["net_realized"].as_str().unwrap().parse()?;
                    t["net_realized"] = json!(net + amount);
                }
            }
            let closed: Vec<_> = trades.values().filter(|t| t["closed"] == true).collect();
            let wins = closed
                .iter()
                .filter(|t| t["net_realized"].as_str().unwrap().parse::<D>().unwrap() > D::ZERO)
                .count();
            results.push(json!({"interval":interval,"start":start,"end_exclusive":end,"spread_bps":spread_bps,"capital":risk.capital,"final_equity":account.equity,"net_return":account.equity/risk.capital-D::ONE,"sampled_max_drawdown":drawdown,"closed_trades":closed.len(),"winning_trades":wins,"fees":fees,"funding_cash_flow":funding_total,"open_positions":account.positions,"rejections":rejections,"trades":trades,"orders":orders,"funding":funding_events,"equity_curve":curve}));
        }
    }
    let report = json!({"model":"ohlc_approx_v1_NOT_RECORDED_REPLAY","research_pattern":pattern,"account_continuity":"180days_no_resets","research_scope":"offline_only_no_production_change","strategy_manifest":engine.manifest(),"risk_config":risk,"assumptions":["15m and 1h are separate portfolios, not the full multi-timeframe production system", "Current metadata precision used throughout; fixed spread 2/5/10 bps; assumed $1m depth per side", "Next open with nominal 1s latency; 100 prior bars; no parameter optimization", "Position-adverse extreme first; barrier fills intrabar, opening gaps at open; 4 modeled observations per bar", "Historical funding rates with hourly opening-price proxy, not historical oracle; current quote proxy for oracle checks", "Drawdown sampled at modeled points, not observed tick data; no exchange outages or measured liquidity", "Ending positions marked to liquidation side; hypothetical closing fees excluded", "Shared quant Engine and paper Account; production configuration unchanged"],"results":results});
    use std::io::Write;
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    out.write_all(&serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn barrier_quote_crosses_despite_decimal_rounding() {
        for half in [dec("0.0001"), dec("0.00025"), dec("0.0005")] {
            for factor in [D::ONE - half, D::ONE + half] {
                for barrier in [dec("12.34567"), dec("95123.456789")] {
                    assert!(barrier_mid(barrier, factor, true) * factor > barrier);
                    assert!(barrier_mid(barrier, factor, false) * factor < barrier);
                }
            }
        }
    }
}

#[cfg(test)]
mod hypothesis_tests {
    use super::*;
    fn fixture() -> Vec<Candle> {
        (0..100)
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
            .collect()
    }
    fn evaluate(cs: &[Candle]) -> Vec<trading_quant::strategies::Decision> {
        let history: Vec<_> = cs
            .iter()
            .map(|c| ClosedCandle::new(c.clone(), true, 360_000_000).unwrap())
            .collect();
        let engine = Engine::builtin(StrategyConfig::default()).unwrap();
        patterns::decisions(
            cs,
            "trend_breakout",
            engine.evaluate(&history).unwrap().decisions,
        )
        .unwrap()
    }
    #[test]
    fn confirmed_breakout_requires_volume_and_strong_close_and_is_symmetric() {
        let mut cs = fixture();
        let last = cs.last_mut().unwrap();
        last.o = dec("110");
        last.l = dec("110");
        last.h = dec("110.9");
        last.c = dec("110.8");
        last.v = dec("160");
        assert_eq!(evaluate(&cs)[0].direction, Direction::Long);
        cs[99].v = dec("100");
        assert!(evaluate(&cs).is_empty());
        cs[99].v = dec("160");
        let mut weak = cs.clone();
        weak[99].h = dec("111.8");
        assert!(evaluate(&weak).is_empty());
        for c in &mut cs {
            let high = c.h;
            c.h = dec("250") - c.l;
            c.l = dec("250") - high;
            c.o = dec("250") - c.o;
            c.c = dec("250") - c.c;
        }
        assert_eq!(evaluate(&cs)[0].direction, Direction::Short);
    }
}
