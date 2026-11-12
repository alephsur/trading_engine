use rust_decimal::Decimal as D;
use trading_domain::data::Candle;
use trading_quant::{
    ClosedCandle, Engine,
    config::{Config, Profile},
    strategies::{ConfirmedBreakout, Direction, Strategy},
};

fn long_fixture() -> Vec<Candle> {
    let mut candles: Vec<_> = (0..100)
        .map(|n| Candle {
            t: n * 3_600_000,
            end: (n + 1) * 3_600_000 - 1,
            s: "BTC".into(),
            i: "1h".into(),
            o: D::from(100 + n),
            h: D::from(101 + n),
            l: D::from(99 + n),
            c: D::from(100 + n),
            v: D::ONE,
            n: 1,
        })
        .collect();
    let last = candles.last_mut().unwrap();
    last.o = D::from(199);
    last.l = D::from(199);
    last.c = D::from(202);
    last.h = D::from(203);
    last.v = D::new(15, 1);
    candles
}
fn mirror(candles: &[Candle]) -> Vec<Candle> {
    candles
        .iter()
        .map(|c| Candle {
            o: D::from(400) - c.o,
            h: D::from(400) - c.l,
            l: D::from(400) - c.h,
            c: D::from(400) - c.c,
            ..c.clone()
        })
        .collect()
}
fn closed(cs: &[Candle]) -> Vec<ClosedCandle> {
    cs.iter()
        .map(|c| ClosedCandle::new(c.clone(), true, cs.last().unwrap().end + 1).unwrap())
        .collect()
}

#[test]
fn confirmed_long_and_short_include_volume_and_close_boundaries() {
    let cs = long_fixture();
    // Both thresholds are exactly met: volume 1.5x and close location 0.75 / 0.25.
    // The new high/low belongs to the signal bar and must not enter the channel.
    for (cs, direction) in [
        (cs.clone(), Direction::Long),
        (mirror(&cs), Direction::Short),
    ] {
        let engine = Engine::builtin(Config::confirmed_breakout_1h()).unwrap();
        let ev = engine.evaluate(&closed(&cs)).unwrap();
        assert_eq!(ev.decisions.len(), 1);
        assert_eq!(ev.decisions[0].strategy, "confirmed_breakout_1h_v1");
        assert_eq!(ev.decisions[0].direction, direction);
        assert!(ev.analysis_only);

        let mut weak_volume = cs.clone();
        weak_volume[99].v -= D::new(1, 6);
        assert_eq!(
            ConfirmedBreakout.evaluate(&weak_volume).unwrap(),
            (Direction::Hold, "volume_filter")
        );
        let mut weak_close = cs.clone();
        weak_close[99].c += if direction == Direction::Long {
            -D::new(1, 6)
        } else {
            D::new(1, 6)
        };
        assert_eq!(
            ConfirmedBreakout.evaluate(&weak_close).unwrap(),
            (Direction::Hold, "weak_directional_close")
        );
        let mut doji = cs.clone();
        doji[99].o = doji[99].c;
        assert_eq!(
            ConfirmedBreakout.evaluate(&doji).unwrap(),
            (Direction::Hold, "weak_directional_close")
        );
    }
}

#[test]
fn rejects_inside_channel_excessive_range_and_countertrend_breakouts() {
    let mut cs = long_fixture();
    cs[99].c = D::from(199); // equal to the previous high is not a breakout
    assert_eq!(
        ConfirmedBreakout.evaluate(&cs).unwrap(),
        (Direction::Hold, "inside_previous_channel")
    );
    let mut cs = long_fixture();
    cs[99].h = D::from(230);
    assert_eq!(
        ConfirmedBreakout.evaluate(&cs).unwrap(),
        (Direction::Hold, "excessive_signal_range")
    );
    let mut cs = mirror(&long_fixture());
    cs[99].o = D::from(230);
    cs[99].l = D::from(229);
    cs[99].c = D::from(232);
    cs[99].h = D::from(233);
    assert_eq!(
        ConfirmedBreakout.evaluate(&cs).unwrap(),
        (Direction::Hold, "trend_filter")
    );
    assert_eq!(
        ConfirmedBreakout.evaluate(&mirror(&cs)).unwrap(),
        (Direction::Hold, "trend_filter")
    );
}

#[test]
fn profile_is_hourly_only_and_requires_100_confirmed_candles() {
    let engine = Engine::builtin(Config::confirmed_breakout_1h()).unwrap();
    let cs = long_fixture();
    assert_eq!(
        engine.evaluate(&closed(&cs[..99])).unwrap().decisions[0].reason,
        "warmup"
    );
    assert_eq!(ConfirmedBreakout.evaluate(&[]).unwrap().0, Direction::Hold);
    for (interval, step) in [("1m", 60_000), ("5m", 300_000), ("15m", 900_000)] {
        let other: Vec<_> = cs
            .iter()
            .enumerate()
            .map(|(n, c)| Candle {
                i: interval.into(),
                t: n as i64 * step,
                end: (n as i64 + 1) * step - 1,
                ..c.clone()
            })
            .collect();
        let ev = engine.evaluate(&closed(&other)).unwrap();
        assert_eq!(ev.decisions[0].direction, Direction::Hold);
        assert_eq!(ev.decisions[0].reason, "unsupported_interval");
    }
    assert!(ClosedCandle::new(cs[99].clone(), false, cs[99].end + 1).is_err());
    assert!(ClosedCandle::new(cs[99].clone(), true, cs[99].end).is_err());
    let mut extended = cs.clone();
    let mut future = cs[99].clone();
    future.t += 3_600_000;
    future.end += 3_600_000;
    extended.push(future);
    assert_eq!(
        engine.replay(&closed(&extended)).unwrap()[99],
        engine.evaluate(&closed(&cs)).unwrap()
    );
    assert_eq!(
        engine.evaluate(&closed(&extended)).unwrap(),
        engine.evaluate(&closed(&extended[1..])).unwrap()
    );
}

#[test]
fn preserves_legacy_identity_and_freezes_the_approved_profile() {
    let legacy: Config =
        serde_json::from_str(include_str!("../../../config/strategies.example.json")).unwrap();
    assert_eq!(legacy.profile, Profile::LegacyFour);
    let legacy = Engine::builtin(legacy).unwrap();
    // Recorded in the original 180-day research manifest, before adding profiles.
    assert_eq!(
        legacy.id(),
        "b96df5324b54340858ece7232b104f8f23fc2a68347875fb69ac57bdc0c8bfa0"
    );
    let approved: Config = serde_json::from_str(include_str!(
        "../../../config/strategies.confirmed-breakout.json"
    ))
    .unwrap();
    let current = Engine::builtin(approved.clone()).unwrap();
    assert_ne!(legacy.id(), current.id());
    assert_eq!(
        current.id(),
        Engine::builtin(Config::confirmed_breakout_1h())
            .unwrap()
            .id()
    );
    let mut changed = approved.clone();
    changed.history_window = 120;
    assert!(changed.validate().is_err());
    let mut changed = approved.clone();
    changed.ema.fast = 19;
    assert!(changed.validate().is_err());
    let mut changed = approved;
    changed.breakout.min_volume_multiple = D::ONE;
    assert!(changed.validate().is_err());
}
