use super::*;
use trading_domain::{Market, data::Level};
struct Fixture {
    now: i64,
    markets: Vec<ListedMarket>,
    books: BTreeMap<String, Book>,
    oracles: BTreeMap<String, (i64, D)>,
}
impl Fixture {
    fn new() -> Self {
        let now = 1_000_000;
        let markets = trading_domain::SYMBOLS
            .iter()
            .enumerate()
            .map(|(index, s)| ListedMarket {
                asset_index: index,
                metadata: Market {
                    name: (*s).into(),
                    sz_decimals: 3,
                    max_leverage: 10,
                    is_delisted: false,
                },
            })
            .collect();
        let mut f = Self {
            now,
            markets,
            books: BTreeMap::new(),
            oracles: BTreeMap::new(),
        };
        for s in trading_domain::SYMBOLS {
            f.book(s, "100", "100.01", "1000");
        }
        f
    }
    fn book(&mut self, s: &str, bid: &str, ask: &str, quantity: &str) {
        self.books.insert(
            s.into(),
            Book {
                coin: s.into(),
                time: self.now,
                levels: [
                    vec![Level {
                        px: dec(bid),
                        sz: dec(quantity),
                        n: 1,
                    }],
                    vec![Level {
                        px: dec(ask),
                        sz: dec(quantity),
                        n: 1,
                    }],
                ],
            },
        );
        self.oracles
            .insert(s.into(), (self.now, (dec(bid) + dec(ask)) / dec("2")));
    }
    fn advance(&mut self, now: i64) {
        self.now = now;
        for b in self.books.values_mut() {
            b.time = now;
        }
        for o in self.oracles.values_mut() {
            o.0 = now;
        }
    }
    fn input(&self) -> Input<'_> {
        Input {
            now: self.now,
            ready: true,
            markets: &self.markets,
            books: &self.books,
            oracles: &self.oracles,
            funding: &[],
        }
    }
    fn signal(&self, id: i64, s: &str, long: bool) -> Signal {
        Signal {
            event_id: id,
            symbol: s.into(),
            long,
            close_time: self.now,
            atr: D::ONE,
        }
    }
    fn account(&self) -> Account {
        Account::new(Config::default(), self.now).unwrap()
    }
}
#[test]
fn default_limits_and_invalid_configuration() {
    let c = Config::default();
    assert_eq!(c.capital, dec("10000"));
    assert_eq!(c.risk_fraction, dec("0.0025"));
    c.validate().unwrap();
    for c in [
        Config {
            risk_fraction: D::ZERO,
            ..c.clone()
        },
        Config {
            leverage: dec("2"),
            ..c.clone()
        },
        Config {
            max_positions: 3,
            ..c.clone()
        },
        Config {
            fee_bps: D::ZERO,
            ..c.clone()
        },
        Config {
            exposure_fraction: D::ONE,
            ..c
        },
    ] {
        assert!(c.validate().is_err());
    }
}
#[test]
fn exchange_precision_and_minimum_notional() {
    assert_eq!(price(dec("1234.567"), 3, true), dec("1234.6"));
    assert_eq!(price(dec("0.1234567"), 3, false), dec("0.123"));
    assert_eq!(price(dec("100000.001"), 5, true), dec("100001"));
    assert_eq!(lot(dec("0.123456"), 3), dec("0.123"));
    let f = Fixture::new();
    let mut a = Account::new(
        Config {
            capital: dec("10"),
            ..Config::default()
        },
        f.now,
    )
    .unwrap();
    let c = a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    assert!(!c.decisions[0].accepted);
    assert!(c.decisions[0].reason.contains("minimum_notional"));
    assert!(a.positions.is_empty());
}
#[test]
fn sizing_accounts_for_costs_and_aggregate_exposure() {
    let f = Fixture::new();
    let mut a = f.account();
    let c = a
        .tick(
            &f.input(),
            &[
                f.signal(1, "BTC", true),
                f.signal(2, "ETH", true),
                f.signal(3, "SOL", true),
            ],
        )
        .unwrap();
    assert!(c.decisions[0].accepted);
    assert!(c.decisions[1].accepted);
    assert!(!c.decisions[2].accepted);
    assert!(a.positions.iter().map(|p| p.entry * p.quantity).sum::<D>() <= dec("1500"));
    for p in &a.positions {
        let risk = p.quantity
            * ((p.entry - p.stop).abs()
                + (p.entry + p.stop) * dec("0.00045")
                + p.stop * dec("0.0002"));
        assert!(risk <= dec("25"));
        assert!(p.stop > D::ZERO);
    }
    assert!(a.cash < dec("10000"));
}
#[test]
fn no_averaging_or_replaying_historical_signals() {
    let f = Fixture::new();
    let mut a = f.account();
    a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    let c = a.tick(&f.input(), &[f.signal(2, "BTC", false)]).unwrap();
    assert_eq!(c.decisions[0].reason, "position_limit_or_averaging");
    let mut old = f.signal(3, "ETH", true);
    old.close_time -= 1;
    assert_eq!(
        a.tick(&f.input(), &[old]).unwrap().decisions[0].reason,
        "historical_or_expired_signal"
    );
}
#[test]
fn partial_ioc_and_partial_reduce_only_exit_do_not_reuse_depth() {
    let mut f = Fixture::new();
    f.book("BTC", "100", "100.01", "10");
    let mut a = f.account();
    let c = a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    assert_eq!(c.orders[0].filled, D::ONE);
    assert_eq!(c.orders[0].status, "partial_fill_remainder_cancelled");
    f.advance(f.now + 1000);
    f.book("BTC", "97", "97.01", "1");
    let c = a.tick(&f.input(), &[]).unwrap();
    assert_eq!(c.orders[0].filled, dec("0.1"));
    assert!(c.orders[0].reduce_only);
    assert!(a.positions[0].closing);
    assert!(a.blockers.contains(&"exit_unreconciled".into()));
    let quantity = a.positions[0].quantity;
    assert!(a.tick(&f.input(), &[]).unwrap().orders.is_empty());
    assert_eq!(quantity, a.positions[0].quantity);
    f.advance(f.now + 1000);
    f.book("BTC", "96", "96.01", "100");
    a.tick(&f.input(), &[]).unwrap();
    assert!(a.positions.is_empty());
    assert!(a.cash < dec("9997"));
}
#[test]
fn long_and_short_profit_includes_both_fees() {
    for long in [true, false] {
        let mut f = Fixture::new();
        let mut a = f.account();
        let entry = a
            .tick(&f.input(), &[f.signal(1, "BTC", long)])
            .unwrap()
            .orders
            .remove(0);
        f.advance(f.now + 1000);
        if long {
            f.book("BTC", "105", "105.01", "1000");
        } else {
            f.book("BTC", "95", "95.01", "1000");
        }
        let exit = a.tick(&f.input(), &[]).unwrap().orders.remove(0);
        assert!(exit.reduce_only);
        assert_eq!(exit.filled, entry.filled);
        assert!(a.positions.is_empty());
        let expected = dec("10000")
            + (exit.fills[0].price - entry.fills[0].price) * entry.filled * sign(long)
            - entry.fills[0].fee
            - exit.fills[0].fee;
        assert!((a.cash - expected).abs() < dec("0.000000000001"));
        assert!(a.cash > dec("10000"));
    }
}
#[test]
fn funding_is_signed_exactly_once_and_survives_closed_positions() {
    for long in [true, false] {
        let mut f = Fixture::new();
        let mut a = f.account();
        a.tick(&f.input(), &[f.signal(1, "BTC", long)]).unwrap();
        let quantity = a.positions[0].quantity;
        f.advance(HOUR + 1000);
        if long {
            f.book("BTC", "105", "105.01", "1000");
        } else {
            f.book("BTC", "95", "95.01", "1000");
        }
        a.tick(&f.input(), &[]).unwrap();
        assert!(a.positions.is_empty());
        assert_eq!(a.funding_due.len(), 1);
        assert!(a.blockers.contains(&"funding_unreconciled".into()));
        let cash = a.cash;
        let rates = [Funding {
            symbol: "BTC".into(),
            hour: HOUR,
            rate: dec("0.0001"),
            oracle: dec("100"),
        }];
        let mut input = f.input();
        input.funding = &rates;
        let c = a.tick(&input, &[]).unwrap();
        assert_eq!(c.funding.len(), 1);
        assert_eq!(a.cash, cash - quantity * sign(long) * dec("0.01"));
        assert!(a.funding_due.is_empty());
        assert!(a.tick(&input, &[]).unwrap().funding.is_empty());
    }
}
#[test]
fn stale_and_inconsistent_data_block_entries_without_fake_exits() {
    let mut f = Fixture::new();
    let mut a = f.account();
    a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    f.now += 16000;
    let quantity = a.positions[0].quantity;
    let c = a.tick(&f.input(), &[f.signal(2, "ETH", true)]).unwrap();
    assert!(!c.decisions[0].accepted);
    assert!(c.orders.is_empty());
    assert_eq!(quantity, a.positions[0].quantity);
    f.advance(f.now);
    f.oracles.get_mut("BTC").unwrap().1 = dec("90");
    a.tick(&f.input(), &[]).unwrap();
    assert!(
        a.blockers
            .iter()
            .any(|b| b.starts_with("oracle_price_divergence"))
    );
}
#[test]
fn daily_loss_is_latched_and_extreme_moves_block_entries() {
    let mut f = Fixture::new();
    let mut a = f.account();
    a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    f.advance(f.now + 1000);
    f.book("BTC", "80", "80.01", "1000");
    a.tick(&f.input(), &[]).unwrap();
    assert!(a.loss_latched);
    assert!(a.blockers.contains(&"extreme_price_move".into()));
    assert!(a.positions.is_empty());
    f.advance(f.now + 61000);
    let c = a.tick(&f.input(), &[f.signal(2, "ETH", true)]).unwrap();
    assert!(!c.decisions[0].accepted);
    assert!(a.loss_latched);
    f.advance(DAY + 1000);
    a.tick(&f.input(), &[]).unwrap();
    assert!(!a.loss_latched);
}
#[test]
fn serialized_restart_produces_identical_continuation() {
    let mut f = Fixture::new();
    let mut a = f.account();
    a.tick(&f.input(), &[f.signal(1, "BTC", false)]).unwrap();
    let mut recovered: Account = serde_json::from_value(serde_json::to_value(&a).unwrap()).unwrap();
    f.advance(f.now + 1000);
    f.book("BTC", "103", "103.01", "1000");
    let first = a.tick(&f.input(), &[]).unwrap();
    let second = recovered.tick(&f.input(), &[]).unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(recovered).unwrap()
    );
}
#[test]
fn multi_level_fill_is_adverse_and_bounded() {
    let mut f = Fixture::new();
    f.book("BTC", "100", "100.01", "10");
    f.books.get_mut("BTC").unwrap().levels[1].push(Level {
        px: dec("100.02"),
        sz: dec("10"),
        n: 1,
    });
    let mut a = f.account();
    let c = a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    let o = &c.orders[0];
    assert_eq!(o.fills.len(), 2);
    assert_eq!(o.filled, dec("2"));
    assert!(o.fills[0].price > dec("100.01"));
    assert!(o.fills[1].price > dec("100.02"));
}

#[test]
fn negative_funding_and_expired_clock_are_handled() {
    let mut f = Fixture::new();
    let mut a = f.account();
    a.tick(&f.input(), &[f.signal(1, "BTC", true)]).unwrap();
    let quantity = a.positions[0].quantity;
    let cash = a.cash;
    f.advance(HOUR + 1000);
    let rates = [Funding {
        symbol: "BTC".into(),
        hour: HOUR,
        rate: dec("-0.0001"),
        oracle: dec("100"),
    }];
    let mut input = f.input();
    input.funding = &rates;
    a.tick(&input, &[]).unwrap();
    assert_eq!(a.cash, cash + quantity * dec("0.01"));
    f.now -= 1;
    assert!(a.tick(&f.input(), &[]).is_err());
}
#[test]
fn rejects_spread_depth_volatility_and_bad_metadata() {
    for reason in [
        "wide_spread",
        "insufficient_liquidity",
        "invalid_or_extreme_volatility",
        "invalid_market",
    ] {
        let mut f = Fixture::new();
        let mut a = f.account();
        let mut signal = f.signal(1, "BTC", true);
        match reason {
            "wide_spread" => f.book("BTC", "100", "101", "1000"),
            "insufficient_liquidity" => f.book("BTC", "100", "100.01", "1"),
            "invalid_or_extreme_volatility" => signal.atr = dec("4"),
            _ => f.markets[0].metadata.is_delisted = true,
        }
        let changes = a.tick(&f.input(), &[signal]).unwrap();
        assert_eq!(changes.decisions[0].reason, reason);
        assert!(changes.orders.is_empty());
    }
}
