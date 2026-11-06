use rust_decimal::Decimal as D;
use serde_json::{Value, json};
use trading_domain::data::Candle;
use trading_quant::{ClosedCandle, Engine, config::*, indicators::*, strategies::*};

fn candles(prices: &[i64]) -> Vec<Candle> {
    prices
        .iter()
        .enumerate()
        .map(|(n, p)| Candle {
            t: n as i64 * 60000,
            end: (n as i64 + 1) * 60000 - 1,
            s: "BTC".into(),
            i: "1m".into(),
            o: D::from(*p),
            h: D::from(p + 1),
            l: D::from(p - 1),
            c: D::from(*p),
            v: D::ONE,
            n: 1,
        })
        .collect()
}
fn closed(cs: &[Candle]) -> Vec<ClosedCandle> {
    cs.iter()
        .map(|c| ClosedCandle::new(c.clone(), true, i64::MAX).unwrap())
        .collect()
}
fn near(actual: D, expected: &Value) {
    // Reference is calculated at 40 digits; Rust Decimal has up to 28 digits.
    let expected = expected.as_str().unwrap().parse::<D>().unwrap();
    assert!(
        (actual - expected).abs() <= D::new(1, 20),
        "{actual} != {expected}"
    );
}
#[test]
fn independently_calculated_indicator_fixture() {
    let f: Value = serde_json::from_str(include_str!("fixtures/indicators.json")).unwrap();
    let values = |key: &str| {
        f[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().parse::<D>().unwrap())
            .collect::<Vec<_>>()
    };
    let xs = values("closes");
    let highs = values("highs");
    let lows = values("lows");
    let volumes = values("volumes");
    let cs: Vec<_> = xs
        .iter()
        .enumerate()
        .map(|(n, c)| Candle {
            t: n as i64 * 60000,
            end: (n as i64 + 1) * 60000 - 1,
            s: "BTC".into(),
            i: "1m".into(),
            o: *c,
            h: highs[n],
            l: lows[n],
            c: *c,
            v: volumes[n],
            n: 1,
        })
        .collect();
    let e = &f["expected"];
    near(sma(&xs, 3).unwrap().unwrap(), &e["sma_3"]);
    near(ema(&xs, 3).unwrap().last().unwrap().unwrap(), &e["ema_3"]);
    near(rsi(&xs, 3).unwrap().unwrap(), &e["rsi_3"]);
    near(atr(&cs, 3).unwrap().unwrap(), &e["atr_3"]);
    let m = macd(&xs, 2, 3, 2).unwrap().unwrap();
    near(m.line, &e["macd_line"]);
    near(m.signal, &e["macd_signal"]);
    near(m.histogram, &e["macd_histogram"]);
    near(
        sma(&volumes[..volumes.len() - 1], 3).unwrap().unwrap(),
        &e["previous_volume_sma_3"],
    );
}
#[test]
fn warmup_flat_series_and_invalid_periods() {
    assert_eq!(rsi(&[D::ONE; 20], 14).unwrap(), Some(D::from(50)));
    assert_eq!(rsi(&[D::ONE; 14], 14).unwrap(), None);
    assert!(ema(&[D::ONE; 5], 0).is_err());
    assert!(macd(&[D::ONE; 30], 26, 12, 9).is_err());
    let engine = Engine::builtin(Default::default()).unwrap();
    let history = closed(&candles(&[100; 100]));
    let early = engine.evaluate(&history[..99]).unwrap();
    assert!(early.indicators.is_none());
    assert!(early.decisions.iter().all(|d| d.reason == "warmup"));
    assert!(
        engine
            .evaluate(&history)
            .unwrap()
            .decisions
            .iter()
            .all(|d| d.direction == Direction::Hold)
    );
}
#[test]
fn four_strategies_emit_both_directions_and_apply_filters() {
    let ema = EmaTrend(EmaConfig { fast: 2, slow: 3 });
    assert_eq!(
        ema.evaluate(&candles(&[100, 100, 100, 90, 110])).unwrap().0,
        Direction::Long
    );
    assert_eq!(
        ema.evaluate(&candles(&[100, 100, 100, 110, 90])).unwrap().0,
        Direction::Short
    );
    let momentum = Momentum(MomentumConfig {
        lookback: 2,
        rsi_period: 2,
        long_min: D::from(55),
        long_max: D::from(90),
        short_min: D::from(10),
        short_max: D::from(45),
        min_change_bps: D::from(10),
    });
    assert_eq!(
        momentum
            .evaluate(&candles(&[100, 100, 100, 101, 100, 102]))
            .unwrap()
            .0,
        Direction::Long
    );
    assert_eq!(
        momentum
            .evaluate(&candles(&[100, 100, 100, 99, 100, 98]))
            .unwrap()
            .0,
        Direction::Short
    );
    assert_eq!(
        momentum
            .evaluate(&candles(&[100, 101, 102, 103, 104, 105]))
            .unwrap()
            .0,
        Direction::Hold
    );
    let breakout = Breakout(BreakoutConfig {
        lookback: 3,
        volume_period: 3,
        min_volume_multiple: D::new(15, 1),
    });
    for (last, direction) in [(105, Direction::Long), (95, Direction::Short)] {
        let mut cs = candles(&[100, 100, 100, 100, last]);
        assert_eq!(breakout.evaluate(&cs).unwrap().0, Direction::Hold);
        cs.last_mut().unwrap().v = D::from(2);
        assert_eq!(breakout.evaluate(&cs).unwrap().0, direction);
    }
    let reversion = MeanReversion(Config::default().mean_reversion);
    for (last, direction) in [(120, Direction::Short), (80, Direction::Long)] {
        let mut xs = vec![100; 99];
        xs.push(last);
        assert_eq!(reversion.evaluate(&candles(&xs)).unwrap().0, direction);
    }
}
#[test]
fn replay_is_causal_and_rejects_unconfirmed_or_discontinuous_data() {
    let engine = Engine::builtin(Default::default()).unwrap();
    let mut xs = vec![100; 100];
    xs.push(120);
    xs.push(50);
    let cs = candles(&xs);
    let history = closed(&cs);
    let prefix = engine.replay(&history[..100]).unwrap();
    assert_eq!(prefix, engine.replay(&history).unwrap()[..100]);
    assert!(ClosedCandle::new(cs[0].clone(), false, 60000).is_err());
    assert!(ClosedCandle::new(cs[1].clone(), true, 60000).is_err());
    let mut gap = history[..100].to_vec();
    gap.remove(50);
    assert!(engine.evaluate(&gap).is_err());
    let mut unordered = history[..100].to_vec();
    unordered.swap(5, 6);
    assert!(engine.evaluate(&unordered).is_err());
    let mut mixed = cs[..100].to_vec();
    mixed[50].s = "ETH".into();
    assert!(engine.evaluate(&closed(&mixed)).is_err());
    // Restart/replay needs only the declared rolling window, never hidden recursive state.
    assert_eq!(
        engine.evaluate(&history).unwrap(),
        engine.evaluate(&history[2..]).unwrap()
    );
}
#[test]
fn config_identity_validation_and_extension() {
    let config = Config::default();
    let a = Engine::builtin(config.clone()).unwrap();
    let b =
        Engine::builtin(serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap())
            .unwrap();
    assert_eq!(a.id(), b.id());
    let mut changed = config.clone();
    changed.ema.fast = 19;
    assert_ne!(a.id(), Engine::builtin(changed).unwrap().id());
    let mut invalid = config.clone();
    invalid.momentum.rsi_period = 0;
    assert!(Engine::builtin(invalid).is_err());
    let mut v = serde_json::to_value(&config).unwrap();
    v["risk_override"] = json!(true);
    assert!(serde_json::from_value::<Config>(v).is_err());
    struct Custom;
    impl Strategy for Custom {
        fn name(&self) -> &'static str {
            "custom_test_v1"
        }
        fn parameters(&self) -> Value {
            json!({})
        }
        fn evaluate(&self, _: &[Candle]) -> anyhow::Result<(Direction, &'static str)> {
            Ok((Direction::Long, "fixture"))
        }
    }
    let custom = Engine::new(config.clone(), vec![Box::new(Custom)]).unwrap();
    assert_eq!(
        custom
            .evaluate(&closed(&candles(&[100; 100])))
            .unwrap()
            .decisions[0]
            .direction,
        Direction::Long
    );
    assert!(Engine::new(config, vec![Box::new(Custom), Box::new(Custom)]).is_err());
}
