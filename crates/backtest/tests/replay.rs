use anyhow::Result;
use serde_json::{Value, json};
use trading_backtest::{
    Config,
    data::{Dataset, Event, Record},
    metrics::{self, Point},
};
use trading_domain::{
    ListedMarket, SYMBOLS,
    data::{Book, Candle},
};
use trading_paper::{Account, Config as Risk, dec};
use trading_quant::{
    Engine,
    config::Config as StrategyConfig,
    strategies::{Direction, Strategy},
};
const START: i64 = 6_000_000;
struct AlwaysLong;
impl Strategy for AlwaysLong {
    fn name(&self) -> &'static str {
        "fixture_long"
    }
    fn parameters(&self) -> Value {
        json!({"fixture":true})
    }
    fn evaluate(&self, _: &[Candle]) -> Result<(Direction, &'static str)> {
        Ok((Direction::Long, "fixture"))
    }
}
fn fixture() -> (Dataset, Config) {
    let markets: Vec<ListedMarket> = SYMBOLS
        .iter()
        .enumerate()
        .map(|(i, s)| {
            serde_json::from_value(
                json!({"asset_index":i,"metadata":{"name":s,"szDecimals":3,"maxLeverage":10}}),
            )
            .unwrap()
        })
        .collect();
    let mut records = vec![Record {
        available_at: 0,
        event: Event::Metadata { markets },
    }];
    for s in SYMBOLS {
        for n in 0..106 {
            let candle:Candle=serde_json::from_value(json!({"t":n*60000,"T":(n+1)*60000-1,"s":s,"i":"1m","o":"100","h":"100.1","l":"99.9","c":"100","v":"1000","n":1})).unwrap();
            records.push(Record {
                available_at: (n + 1) * 60000,
                event: Event::Candle { candle },
            });
        }
        for now in (START..START + 360_000).step_by(1000) {
            let bid = if now % 60000 == 2000 {
                dec("102")
            } else {
                dec("100")
            };
            let book:Book=serde_json::from_value(json!({"coin":s,"time":now,"levels":[[{"px":bid.to_string(),"sz":"1000","n":1}],[{"px":(bid+dec("0.01")).to_string(),"sz":"1000","n":1}]]})).unwrap();
            records.push(Record {
                available_at: now,
                event: Event::Book { book },
            });
            records.push(Record {
                available_at: now,
                event: Event::Oracle {
                    symbol: s.into(),
                    observed_at: now,
                    price: bid,
                },
            });
        }
    }
    records.retain(|r| r.available_at < START + 360_000);
    records.sort_by_key(|r| r.available_at);
    let data = Dataset {
        version: 1,
        source: "synthetic_test_only".into(),
        capture_start: START,
        capture_end: START + 360_000,
        records,
    };
    let config = Config {
        train_start: START,
        validation_start: START + 120_000,
        test_start: START + 240_000,
        end: START + 360_000,
        interval: "1m".into(),
        latency_ms: 1000,
        minimum_training_trades: 1,
        risk: Risk::default(),
        candidates: vec![StrategyConfig::default()],
    };
    (data, config)
}
#[test]
fn approved_profile_rejects_non_hourly_backtests() {
    let (data, mut config) = fixture();
    config.candidates = vec![StrategyConfig::confirmed_breakout_1h()];
    assert!(
        config
            .validate(&data)
            .unwrap_err()
            .to_string()
            .contains("requires a 1h backtest")
    );
    config.interval = "1h".into();
    config.validate(&data).unwrap();
}
#[test]
fn deterministic_latency_and_shared_execution() {
    let (data, config) = fixture();
    data.validate().unwrap();
    config.validate(&data).unwrap();
    let engine = Engine::new(StrategyConfig::default(), vec![Box::new(AlwaysLong)]).unwrap();
    let a = trading_backtest::partition(&data, &config, &engine, "train", START, START + 120_000)
        .unwrap();
    let b = trading_backtest::partition(&data, &config, &engine, "train", START, START + 120_000)
        .unwrap();
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
    assert!(!a.orders.is_empty());
    assert!(a.orders.iter().any(|o| o.reduce_only));
    for o in a.orders.iter().filter(|o| !o.reduce_only) {
        let p = a
            .proposals
            .iter()
            .find(|p| p.signal.event_id == o.position_id)
            .unwrap();
        assert!(o.time >= p.execute_after);
        assert!(o.time > p.signal.close_time);
    }
    // Independently feed the first entry inputs into the exact shared PAPER account.
    let first = a.orders.iter().find(|o| !o.reduce_only).unwrap();
    let p = a
        .proposals
        .iter()
        .find(|p| p.signal.event_id == first.position_id)
        .unwrap();
    let mut books = std::collections::BTreeMap::new();
    let mut oracles = std::collections::BTreeMap::new();
    let mut markets = vec![];
    for r in data.records.iter().filter(|r| r.available_at <= first.time) {
        match &r.event {
            Event::Book { book } => {
                books.insert(book.coin.clone(), book.clone());
            }
            Event::Oracle {
                symbol,
                observed_at,
                price,
            } => {
                oracles.insert(symbol.clone(), (*observed_at, *price));
            }
            Event::Metadata { markets: m } => markets = m.clone(),
            _ => {}
        }
    }
    let mut account = Account::new(config.risk.clone(), START).unwrap();
    let input = trading_paper::Input {
        now: first.time,
        ready: true,
        markets: &markets,
        books: &books,
        oracles: &oracles,
        funding: &[],
    };
    let changes = account
        .tick(&input, std::slice::from_ref(&p.signal))
        .unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(&changes.orders[0]).unwrap()
    );
    assert_eq!(a.metrics.closed_trades, 1);
    assert_eq!(a.metrics.win_rate, None);
    assert_eq!(a.metrics.sharpe_daily, None);
    assert!(a.metrics.fees > dec("0"));
    assert!(
        (a.final_account.cash
            - config.risk.capital
            - a.trades
                .iter()
                .map(|t| t.net_realized)
                .sum::<rust_decimal::Decimal>())
        .abs()
            < dec("0.000000000001")
    );
}
#[test]
fn future_changes_cannot_change_training_or_selection() {
    let (data, config) = fixture();
    let original = trading_backtest::run(&data, config.clone()).unwrap();
    let mut modified = data.clone();
    for r in &mut modified.records {
        if r.available_at >= config.validation_start
            && let Event::Book { book } = &mut r.event
        {
            for level in book.levels.iter_mut().flatten() {
                level.px *= dec("2");
            }
        }
    }
    let changed = trading_backtest::run(&modified, config.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&original.training).unwrap(),
        serde_json::to_value(&changed.training).unwrap()
    );
    assert_eq!(original.selected_strategy_id, changed.selected_strategy_id);
    assert_ne!(original.dataset_hash, changed.dataset_hash);
    let again = trading_backtest::run(&data, config).unwrap();
    assert_eq!(
        serde_json::to_value(original).unwrap(),
        serde_json::to_value(again).unwrap()
    );
}
#[test]
fn data_validation_rejects_future_duplicate_and_misordered_records() {
    let (data, config) = fixture();
    let mut bad = data.clone();
    bad.records.swap(0, 1);
    assert!(bad.validate().is_err());
    let mut bad = data.clone();
    bad.records.insert(2, bad.records[1].clone());
    bad.records.sort_by_key(|r| r.available_at);
    assert!(bad.validate().is_err());
    let mut bad = data.clone();
    let r = bad
        .records
        .iter_mut()
        .find(|r| matches!(r.event, Event::Book { .. }))
        .unwrap();
    if let Event::Book { book } = &mut r.event {
        book.time = r.available_at + 1;
    }
    assert!(bad.validate().is_err());
    let mut bad = config;
    bad.test_start = bad.train_start;
    assert!(bad.validate(&data).is_err());
}
#[test]
fn delayed_candle_cannot_trade_before_receipt_and_missing_books_are_explicit() {
    let (mut data, config) = fixture();
    for r in &mut data.records {
        if let Event::Candle { candle } = &r.event
            && candle.t == START
        {
            r.available_at += 20_000;
        }
    }
    data.records.sort_by_key(|r| r.available_at);
    let engine = Engine::new(StrategyConfig::default(), vec![Box::new(AlwaysLong)]).unwrap();
    let report =
        trading_backtest::partition(&data, &config, &engine, "train", START, START + 120_000)
            .unwrap();
    assert!(report.orders.iter().all(|o| o.time >= START + 81_000));
    data.records
        .retain(|r| !matches!(r.event, Event::Book { .. }) || r.available_at <= START + 61_000);
    let incomplete =
        trading_backtest::partition(&data, &config, &engine, "train", START, START + 120_000)
            .unwrap();
    assert!(incomplete.data_ready_ticks < incomplete.ticks);
}
#[test]
fn no_eligible_training_candidate_does_not_touch_holdout() {
    let (data, config) = fixture();
    let report = trading_backtest::run(&data, config).unwrap();
    assert_eq!(report.selection_status, "no_eligible_training_candidate");
    assert!(report.validation.is_none() && report.test.is_none());
}
#[test]
fn independently_calculated_metrics_and_sample_guards() {
    let point = |time, equity| Point {
        time,
        equity: Some(dec(equity)),
        cash: dec(equity),
        open_positions: 0,
        blockers: vec![],
    };
    let points = vec![
        point(0, "100"),
        point(1000, "120"),
        point(2000, "90"),
        point(3000, "110"),
    ];
    let mut drawdown = metrics::Drawdown::new(dec("100"));
    for p in &points {
        drawdown.observe(p.equity.unwrap());
    }
    let m = metrics::calculate(&points, &[], &[], dec("100"), true, drawdown.maximum);
    assert_eq!(m.net_return, Some(dec("0.1")));
    assert_eq!(m.max_drawdown, Some(dec("0.25")));
    assert!(m.win_rate.is_none());
    let returns: Vec<_> = (0..30)
        .map(|n| if n % 2 == 0 { dec("0.01") } else { dec("0.03") })
        .collect();
    let (sharpe, status) = metrics::sharpe(&returns);
    assert_eq!(status, "estimated_zero_risk_free_rate");
    let expected = 0.02 / (0.003_f64 / 29.0).sqrt();
    assert!((sharpe.unwrap() - expected).abs() < 1e-12);
    assert!(metrics::sharpe(&returns[..29]).0.is_none());
    assert!(metrics::sharpe(&vec![dec("0.01"); 30]).0.is_none());
    let m = metrics::calculate(&points, &[], &[], dec("100"), false, dec("0.25"));
    assert!(m.max_drawdown.is_none());
    assert_eq!(m.observed_max_drawdown, dec("0.25"));
}

#[test]
fn trade_metrics_require_settlement_and_include_costs() {
    let trades: Vec<_> = (0..30)
        .map(|i| metrics::TradeResult {
            position_id: i,
            symbol: "BTC".into(),
            long: true,
            opened_at: 0,
            closed_at: Some(1000),
            quantity: dec("1"),
            remaining: dec("0"),
            realized_gross: if i % 2 == 0 { dec("2.5") } else { dec("-0.5") },
            fees: dec("0.3"),
            funding: dec("-0.2"),
            net_realized: if i % 2 == 0 { dec("2") } else { dec("-1") },
            settled: true,
        })
        .collect();
    let m = metrics::calculate(&[], &[], &trades, dec("100"), true, dec("0"));
    assert_eq!(m.win_rate, Some(dec("0.5")));
    assert_eq!(m.profit_factor, Some(dec("2")));
    assert_eq!(m.fees, dec("9"));
    assert_eq!(m.funding_cash_flow, dec("-6"));
    assert_eq!(m.closed_trades, 30);
    let mut unknown = trades;
    unknown[0].settled = false;
    let m = metrics::calculate(&[], &[], &unknown, dec("100"), true, dec("0"));
    assert_eq!(m.closed_trades, 29);
    assert!(m.win_rate.is_none() && m.profit_factor.is_none());
}
#[test]
fn funding_is_not_visible_before_receipt_and_unknown_costs_invalidate_results() {
    let (mut data, mut config) = fixture();
    let offset = 1_080_000;
    let hour = 7_200_000;
    for r in &mut data.records {
        r.available_at += offset;
        match &mut r.event {
            Event::Candle { candle } => {
                candle.t += offset;
                candle.end += offset;
            }
            Event::Book { book } => {
                book.time += offset;
                book.levels[0][0].px = dec("100");
                book.levels[1][0].px = dec("100.01");
            }
            Event::Oracle {
                observed_at, price, ..
            } => {
                *observed_at += offset;
                *price = dec("100");
            }
            _ => {}
        }
    }
    data.capture_start += offset;
    data.capture_end += offset;
    config.train_start += offset;
    config.validation_start += offset;
    config.test_start += offset;
    config.end += offset;
    for symbol in SYMBOLS {
        data.records.push(Record {
            available_at: hour + 2000,
            event: Event::Funding {
                symbol: symbol.into(),
                hour,
                rate: dec("0.0001"),
            },
        });
    }
    data.records.sort_by_key(|r| r.available_at);
    data.validate().unwrap();
    let engine = Engine::new(StrategyConfig::default(), vec![Box::new(AlwaysLong)]).unwrap();
    let report = trading_backtest::partition(
        &data,
        &config,
        &engine,
        "test",
        config.train_start,
        config.train_start + 180_000,
    )
    .unwrap();
    assert!(!report.funding.is_empty());
    assert!(report.final_account.funding_due.is_empty());
    assert!(!report.metrics.valuation_complete);
    assert!(report.metrics.max_drawdown.is_none());
    assert!(report.metrics.net_return.is_some());
    let point = report.equity_curve.iter().find(|p| p.time == hour).unwrap();
    assert!(point.equity.is_none());
    data.records.retain(|r|!matches!(&r.event,Event::Oracle{observed_at,..} if *observed_at>=hour-15000 && *observed_at<=hour));
    let report = trading_backtest::partition(
        &data,
        &config,
        &engine,
        "test",
        config.train_start,
        config.train_start + 180_000,
    )
    .unwrap();
    assert!(!report.final_account.funding_due.is_empty());
    assert!(report.metrics.net_return.is_none());
    assert!(report.funding.is_empty());
}
