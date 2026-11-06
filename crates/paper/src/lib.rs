//! Deterministic paper account. No clock, network, credentials, or exchange order API.
use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use trading_domain::{ListedMarket, data::Book};
pub const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;
pub fn dec(s: &str) -> D {
    s.parse().expect("decimal constant")
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub capital: D,
    pub risk_fraction: D,
    pub daily_loss_fraction: D,
    pub max_positions: usize,
    pub exposure_fraction: D,
    pub leverage: D,
    pub correlated_exposure_fraction: D,
    pub fee_bps: D,
    pub slippage_bps: D,
    pub participation: D,
    pub max_spread_bps: D,
    pub max_atr_fraction: D,
    pub stop_atr: D,
    pub reward_multiple: D,
    pub max_signal_age_ms: i64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            capital: dec("10000"),
            risk_fraction: dec("0.0025"),
            daily_loss_fraction: dec("0.01"),
            max_positions: 2,
            exposure_fraction: dec("0.20"),
            leverage: D::ONE,
            correlated_exposure_fraction: dec("0.15"),
            fee_bps: dec("4.5"),
            slippage_bps: dec("2"),
            participation: dec("0.10"),
            max_spread_bps: dec("20"),
            max_atr_fraction: dec("0.03"),
            stop_atr: dec("2"),
            reward_multiple: dec("2"),
            max_signal_age_ms: 120_000,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.capital > D::ZERO && self.capital <= dec("1000000000"),
            "invalid capital"
        );
        for (v, max) in [
            (self.risk_fraction, dec("0.0025")),
            (self.daily_loss_fraction, dec("0.01")),
            (self.exposure_fraction, dec("0.20")),
            (self.leverage, D::ONE),
            (self.correlated_exposure_fraction, dec("0.15")),
            (self.participation, dec("0.10")),
            (self.max_atr_fraction, dec("0.03")),
        ] {
            ensure!(
                v > D::ZERO && v <= max,
                "risk bounds cannot be disabled or loosened"
            );
        }
        ensure!(
            (1..=2).contains(&self.max_positions),
            "invalid position cap"
        );
        ensure!(
            self.fee_bps >= dec("4.5")
                && self.fee_bps <= dec("100")
                && self.slippage_bps >= dec("2")
                && self.slippage_bps <= dec("100"),
            "invalid costs"
        );
        ensure!(
            self.max_spread_bps > D::ZERO
                && self.max_spread_bps <= dec("20")
                && self.stop_atr >= D::ONE
                && self.stop_atr <= dec("10")
                && self.reward_multiple >= D::ONE
                && self.reward_multiple <= dec("10")
                && (1..=120_000).contains(&self.max_signal_age_ms),
            "invalid execution parameters"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Signal {
    pub event_id: i64,
    pub symbol: String,
    pub long: bool,
    pub close_time: i64,
    pub atr: D,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub id: i64,
    pub symbol: String,
    pub long: bool,
    pub quantity: D,
    pub entry: D,
    pub stop: D,
    pub take_profit: D,
    pub risk_per_unit: D,
    pub funding_through: i64,
    pub closing: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FundingDue {
    pub position_id: i64,
    pub symbol: String,
    pub hour: i64,
    pub signed_quantity: D,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Funding {
    pub symbol: String,
    pub hour: i64,
    pub rate: D,
    pub oracle: D,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fill {
    pub quantity: D,
    pub price: D,
    pub fee: D,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    pub id: i64,
    pub position_id: i64,
    pub symbol: String,
    pub buy: bool,
    pub reduce_only: bool,
    pub requested: D,
    pub stop: D,
    pub take_profit: D,
    pub filled: D,
    pub status: String,
    pub reason: String,
    pub time: i64,
    pub fills: Vec<Fill>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Decision {
    pub signal_id: i64,
    pub accepted: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub config: Config,
    pub activated_at: i64,
    pub last_tick: i64,
    pub cash: D,
    pub equity: D,
    pub day: i64,
    pub day_start_equity: D,
    pub loss_latched: bool,
    pub positions: Vec<Position>,
    pub funding_due: Vec<FundingDue>,
    pub blockers: Vec<String>,
    pub sequence: i64,
    // One IOC per side of each book snapshot; unused volume is deliberately discarded.
    pub used_books: BTreeMap<String, i64>,
    pub previous_prices: BTreeMap<String, D>,
    pub extreme_until: i64,
}
pub struct Input<'a> {
    pub now: i64,
    pub ready: bool,
    pub markets: &'a [ListedMarket],
    pub books: &'a BTreeMap<String, Book>,
    pub funding: &'a [Funding],
    pub oracles: &'a BTreeMap<String, (i64, D)>,
}
#[derive(Default, Debug, Serialize)]
pub struct Changes {
    pub orders: Vec<Order>,
    pub decisions: Vec<Decision>,
    pub funding: Vec<serde_json::Value>,
}
fn sign(long: bool) -> D {
    if long { D::ONE } else { -D::ONE }
}
fn lot(quantity: D, decimals: u32) -> D {
    let step = D::new(1, decimals);
    (quantity / step).floor() * step
}
/// Perpetual prices: five significant digits, at most 6-szDecimals decimals; integers always valid.
pub fn price(price: D, decimals: u32, up: bool) -> D {
    let mut significant = D::ONE;
    let mut threshold = dec("10000");
    while price < threshold && threshold > dec("0.00000001") {
        significant /= dec("10");
        threshold /= dec("10");
    }
    let tick = significant.max(D::new(1, 6 - decimals)).min(D::ONE);
    let units = price / tick;
    (if up { units.ceil() } else { units.floor() }) * tick
}
fn fresh(book: &Book, now: i64) -> bool {
    (0..15_000).contains(&(now - book.time))
        && book.validate().is_ok()
        && book
            .levels
            .iter()
            .flatten()
            .all(|l| l.px <= dec("1000000000") && l.sz <= dec("1000000000"))
}
impl Account {
    pub fn new(config: Config, now: i64) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            cash: config.capital,
            equity: config.capital,
            day_start_equity: config.capital,
            config,
            activated_at: now,
            last_tick: now,
            day: now / DAY,
            loss_latched: false,
            positions: vec![],
            funding_due: vec![],
            blockers: vec![],
            sequence: 0,
            used_books: BTreeMap::new(),
            previous_prices: BTreeMap::new(),
            extreme_until: 0,
        })
    }
    pub fn validate(&self) -> Result<()> {
        self.config.validate()?;
        ensure!(
            self.positions.len() <= self.config.max_positions,
            "unreconciled position count"
        );
        let mut symbols = std::collections::BTreeSet::new();
        for p in &self.positions {
            ensure!(
                trading_domain::SYMBOLS.contains(&p.symbol.as_str())
                    && symbols.insert(&p.symbol)
                    && p.quantity > D::ZERO
                    && p.risk_per_unit > D::ZERO
                    && p.entry > D::ZERO
                    && p.stop > D::ZERO
                    && p.take_profit > D::ZERO
                    && if p.long {
                        p.stop < p.entry && p.take_profit > p.entry
                    } else {
                        p.stop > p.entry && p.take_profit < p.entry
                    },
                "unreconciled position"
            );
        }
        Ok(())
    }
    fn order(
        &mut self,
        p: &Position,
        quantity: D,
        reduce: bool,
        reason: &str,
        input: &Input<'_>,
    ) -> Option<Order> {
        let book = input.books.get(&p.symbol)?;
        if !fresh(book, input.now) {
            return None;
        }
        let market = input.markets.iter().find(|m| m.metadata.name == p.symbol)?;
        if market.metadata.sz_decimals > 6 {
            return None;
        }
        let buy = p.long != reduce;
        let key = format!("{}:{buy}", p.symbol);
        if self.used_books.get(&key).is_some_and(|t| *t >= book.time) {
            return None;
        }
        self.used_books.insert(key, book.time);
        self.sequence += 1;
        let mut order = Order {
            id: self.sequence,
            position_id: p.id,
            symbol: p.symbol.clone(),
            buy,
            reduce_only: reduce,
            requested: quantity,
            stop: p.stop,
            take_profit: p.take_profit,
            filled: D::ZERO,
            status: "cancelled".into(),
            reason: reason.into(),
            time: input.now,
            fills: vec![],
        };
        for level in &book.levels[usize::from(buy)] {
            let qty = lot(
                (quantity - order.filled).min(level.sz * self.config.participation),
                market.metadata.sz_decimals,
            );
            if qty <= D::ZERO {
                continue;
            }
            let px = price(
                level.px * (D::ONE + sign(buy) * self.config.slippage_bps / dec("10000")),
                market.metadata.sz_decimals,
                buy,
            );
            if px <= D::ZERO {
                continue;
            }
            order.fills.push(Fill {
                quantity: qty,
                price: px,
                fee: qty * px * self.config.fee_bps / dec("10000"),
            });
            order.filled += qty;
            if order.filled == quantity {
                break;
            }
        }
        order.status = if order.filled == quantity {
            "filled"
        } else if order.filled > D::ZERO {
            "partial_fill_remainder_cancelled"
        } else {
            "cancelled"
        }
        .into();
        Some(order)
    }
    fn mark(&self, input: &Input<'_>) -> Option<D> {
        let mut equity = self.cash;
        for p in &self.positions {
            let b = input.books.get(&p.symbol)?;
            if !fresh(b, input.now) {
                return None;
            }
            equity += (b.levels[usize::from(!p.long)][0].px - p.entry) * p.quantity * sign(p.long);
        }
        Some(equity)
    }
    pub fn tick(&mut self, input: &Input<'_>, signals: &[Signal]) -> Result<Changes> {
        self.validate()?;
        ensure!(input.now >= self.last_tick, "clock moved backwards");
        self.last_tick = input.now;
        let mut changes = Changes::default();
        self.blockers.clear();
        // Capture each hourly liability before any partial exit changes its quantity.
        for p in &mut self.positions {
            let through = input.now / HOUR * HOUR;
            let until = through.min(p.funding_through + 48 * HOUR);
            while p.funding_through < until {
                p.funding_through += HOUR;
                self.funding_due.push(FundingDue {
                    position_id: p.id,
                    symbol: p.symbol.clone(),
                    hour: p.funding_through,
                    signed_quantity: p.quantity * sign(p.long),
                });
            }
        }
        self.funding_due.retain(|due| {
            if let Some(f)=input.funding.iter().find(|f| f.symbol==due.symbol && f.hour==due.hour && f.oracle>D::ZERO && f.oracle<=dec("1000000000") && f.rate.abs()<=dec("0.005")) {
                let cash_change = -due.signed_quantity*f.oracle*f.rate;
                self.cash += cash_change;
                changes.funding.push(serde_json::json!({"position_id":due.position_id,"symbol":due.symbol,"hour":due.hour,"quantity":due.signed_quantity,"rate":f.rate,"oracle":f.oracle,"cash_change":cash_change,"oracle_model":"last_observation_before_boundary"})); false
            } else { true }
        });
        for index in (0..self.positions.len()).rev() {
            let p = &self.positions[index];
            if let Some(b) = input.books.get(&p.symbol).filter(|b| fresh(b, input.now)) {
                let px = b.levels[usize::from(!p.long)][0].px;
                let stop = if p.long { px <= p.stop } else { px >= p.stop };
                let target = if p.long {
                    px >= p.take_profit
                } else {
                    px <= p.take_profit
                };
                if p.closing || stop || target {
                    self.positions[index].closing = true;
                    let p = self.positions[index].clone();
                    // Do not lose an unrecorded funding boundary during multi-day catch-up.
                    if p.funding_through < input.now / HOUR * HOUR {
                        continue;
                    }
                    if let Some(order) = self.order(
                        &p,
                        p.quantity,
                        true,
                        if stop {
                            "stop"
                        } else if target {
                            "take_profit"
                        } else {
                            "pending_exit"
                        },
                        input,
                    ) {
                        for f in &order.fills {
                            self.cash += (f.price - p.entry) * f.quantity * sign(p.long) - f.fee;
                        }
                        self.positions[index].quantity -= order.filled;
                        changes.orders.push(order);
                        if self.positions[index].quantity == D::ZERO {
                            self.positions.remove(index);
                        }
                    }
                }
            }
        }
        if !input.ready {
            self.blockers
                .push("market_data_or_analysis_unavailable".into());
        }
        for symbol in trading_domain::SYMBOLS {
            if let Some(b) = input.books.get(symbol).filter(|b| fresh(b, input.now)) {
                let px = (b.levels[0][0].px + b.levels[1][0].px) / dec("2");
                if self
                    .previous_prices
                    .get(symbol)
                    .is_some_and(|old| (px / *old - D::ONE).abs() > dec("0.05"))
                {
                    self.extreme_until = input.now + 60_000;
                }
                self.previous_prices.insert(symbol.into(), px);
                match input.oracles.get(symbol) {
                    Some((time, oracle))
                        if (0..15_000).contains(&(input.now - time))
                            && *oracle > D::ZERO
                            && *oracle <= dec("1000000000") =>
                    {
                        if (px / *oracle - D::ONE).abs() > dec("0.02") {
                            self.blockers
                                .push(format!("oracle_price_divergence:{symbol}"));
                        }
                    }
                    _ => self.blockers.push(format!("oracle_unavailable:{symbol}")),
                }
            } else {
                self.blockers.push(format!("stale_book:{symbol}"));
            }
        }
        if input.now < self.extreme_until {
            self.blockers.push("extreme_price_move".into());
        }
        if !self.funding_due.is_empty() {
            self.blockers.push("funding_unreconciled".into());
        }
        if self.positions.iter().any(|p| p.closing) {
            self.blockers.push("exit_unreconciled".into());
        }
        match self.mark(input) {
            Some(equity) => {
                self.equity = equity;
                if self.day != input.now / DAY && self.funding_due.is_empty() {
                    self.day = input.now / DAY;
                    self.day_start_equity = equity;
                    self.loss_latched = false;
                }
                if equity <= self.day_start_equity * (D::ONE - self.config.daily_loss_fraction) {
                    self.loss_latched = true;
                }
            }
            None => self.blockers.push("valuation_unavailable".into()),
        }
        if self.loss_latched || self.equity <= D::ZERO {
            self.blockers.push("daily_loss_limit".into());
        }
        for signal in signals {
            let result = self.enter(input, signal, &mut changes.orders);
            changes.decisions.push(Decision {
                signal_id: signal.event_id,
                accepted: result.is_ok(),
                reason: result.err().unwrap_or_else(|| "paper_fill".into()),
            });
            if let Some(equity) = self.mark(input) {
                self.equity = equity;
            }
            if self.equity <= self.day_start_equity * (D::ONE - self.config.daily_loss_fraction) {
                self.loss_latched = true;
                if !self.blockers.iter().any(|b| b == "daily_loss_limit") {
                    self.blockers.push("daily_loss_limit".into());
                }
            }
        }
        self.validate()?;
        Ok(changes)
    }
    fn enter(
        &mut self,
        input: &Input<'_>,
        s: &Signal,
        orders: &mut Vec<Order>,
    ) -> std::result::Result<(), String> {
        if s.close_time < self.activated_at
            || !(0..=self.config.max_signal_age_ms).contains(&(input.now - s.close_time))
        {
            return Err("historical_or_expired_signal".into());
        }
        if !self.blockers.is_empty() {
            return Err(self.blockers.join(","));
        }
        if self.positions.len() >= self.config.max_positions
            || self.positions.iter().any(|p| p.symbol == s.symbol)
        {
            return Err("position_limit_or_averaging".into());
        }
        let market = input
            .markets
            .iter()
            .find(|m| {
                m.metadata.name == s.symbol
                    && !m.metadata.is_delisted
                    && m.metadata.sz_decimals <= 6
                    && m.metadata.max_leverage > 0
            })
            .ok_or("invalid_market")?;
        let b = input.books.get(&s.symbol).ok_or("missing_book")?;
        let mid = (b.levels[0][0].px + b.levels[1][0].px) / dec("2");
        if (b.levels[1][0].px - b.levels[0][0].px) / mid * dec("10000") > self.config.max_spread_bps
        {
            return Err("wide_spread".into());
        }
        if s.atr <= D::ZERO || s.atr / mid > self.config.max_atr_fraction {
            return Err("invalid_or_extreme_volatility".into());
        }
        let side = &b.levels[usize::from(s.long)];
        if side.iter().map(|l| l.px * l.sz).sum::<D>() < dec("1000") {
            return Err("insufficient_liquidity".into());
        }
        let best = side[0].px;
        let worst = price(
            side.last().ok_or("missing_depth")?.px
                * (D::ONE + sign(s.long) * self.config.slippage_bps / dec("10000")),
            market.metadata.sz_decimals,
            s.long,
        );
        let distance = (s.atr * self.config.stop_atr).max(mid * dec("0.005"));
        let stop = price(
            best - sign(s.long) * distance,
            market.metadata.sz_decimals,
            !s.long,
        );
        let target = price(
            worst + sign(s.long) * distance * self.config.reward_multiple,
            market.metadata.sz_decimals,
            s.long,
        );
        if stop <= D::ZERO || target <= D::ZERO || (worst - stop) * sign(s.long) <= D::ZERO {
            return Err("invalid_protection".into());
        }
        let exit_estimate = price(
            stop * (D::ONE - sign(s.long) * self.config.slippage_bps / dec("10000")),
            market.metadata.sz_decimals,
            !s.long,
        );
        let unit_risk = (worst - exit_estimate).abs()
            + (worst + exit_estimate) * self.config.fee_bps / dec("10000");
        let mut exposure = D::ZERO;
        let mut portfolio_risk = D::ZERO;
        let mut correlated = D::ZERO;
        for p in &self.positions {
            let book = input.books.get(&p.symbol).ok_or("missing_position_book")?;
            exposure += p.quantity * book.levels[1][0].px.max(p.entry);
            if p.long == s.long {
                correlated += p.quantity * book.levels[1][0].px.max(p.entry);
            }
            portfolio_risk += p.quantity * p.risk_per_unit;
        }
        let equity = self.equity.min(self.config.capital); // Never increase size after profits/loss recovery.
        let remaining = (equity * self.config.exposure_fraction - exposure)
            .min(equity * self.config.leverage - exposure)
            .min(equity * self.config.correlated_exposure_fraction - correlated);
        let risk = (equity * self.config.risk_fraction).min(
            equity * self.config.risk_fraction * D::from(self.config.max_positions)
                - portfolio_risk,
        );
        let risk = risk
            .min(self.equity - self.day_start_equity * (D::ONE - self.config.daily_loss_fraction));
        let quantity = lot(
            (risk / unit_risk)
                .min(remaining / (worst.max(best) * (D::ONE + self.config.fee_bps / dec("10000")))),
            market.metadata.sz_decimals,
        );
        if quantity <= D::ZERO || quantity * worst.min(best) < dec("10") {
            return Err("minimum_notional_incompatible_with_risk".into());
        }
        let mut p = Position {
            id: s.event_id,
            symbol: s.symbol.clone(),
            long: s.long,
            quantity,
            entry: worst,
            stop,
            take_profit: target,
            risk_per_unit: unit_risk,
            funding_through: input.now / HOUR * HOUR,
            closing: false,
        };
        let order = self
            .order(&p, quantity, false, "signal", input)
            .ok_or("book_liquidity_already_consumed")?;
        if order.filled == D::ZERO {
            orders.push(order);
            return Err("ioc_no_fill".into());
        }
        p.quantity = order.filled;
        p.entry = order.fills.iter().map(|f| f.quantity * f.price).sum::<D>() / order.filled;
        self.cash -= order.fills.iter().map(|f| f.fee).sum::<D>();
        self.positions.push(p);
        orders.push(order);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
