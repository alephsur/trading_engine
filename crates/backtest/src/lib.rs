//! Offline chronological replay. Trading decisions share the live PAPER risk/execution code.
pub mod data;
pub mod export;
pub mod metrics;
use anyhow::{Result, ensure};
use data::{Dataset, Event};
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use trading_domain::{
    ListedMarket,
    data::{Book, Candle, interval_ms},
};
use trading_paper::{Account, Config as RiskConfig, Decision, Funding, Input, Order, Signal};
use trading_quant::{
    ClosedCandle, Engine, config::Config as StrategyConfig, strategies::Direction,
};
const TICK: i64 = 1000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub train_start: i64,
    pub validation_start: i64,
    pub test_start: i64,
    pub end: i64,
    pub interval: String,
    pub latency_ms: i64,
    pub minimum_training_trades: usize,
    pub risk: RiskConfig,
    pub candidates: Vec<StrategyConfig>,
}
impl Config {
    pub fn validate(&self, data: &Dataset) -> Result<()> {
        self.risk.validate()?;
        interval_ms(&self.interval)?;
        ensure!(
            data.capture_start <= self.train_start
                && self.train_start < self.validation_start
                && self.validation_start < self.test_start
                && self.test_start < self.end
                && self.end <= data.capture_end,
            "invalid chronological splits"
        );
        ensure!(
            [
                self.train_start,
                self.validation_start,
                self.test_start,
                self.end
            ]
            .iter()
            .all(|t| t % TICK == 0)
                && (self.end - self.train_start) / TICK <= 3_000_000,
            "splits must align to seconds; maximum 3 million ticks"
        );
        ensure!(
            (1000..=60000).contains(&self.latency_ms)
                && (1..=1000).contains(&self.minimum_training_trades),
            "invalid latency or selection threshold"
        );
        ensure!(
            !self.candidates.is_empty() && self.candidates.len() <= 8,
            "expected 1..8 predeclared candidates"
        );
        let mut ids = std::collections::BTreeSet::new();
        for config in &self.candidates {
            let engine = Engine::builtin(config.clone())?;
            ensure!(ids.insert(engine.id().to_owned()), "duplicate candidate");
        }
        Ok(())
    }
}
#[derive(Serialize)]
pub struct Proposal {
    pub available_at: i64,
    pub execute_after: i64,
    pub strategy: String,
    pub signal: Signal,
}
#[derive(Serialize)]
pub struct Partition {
    pub name: String,
    pub start: i64,
    pub end_exclusive: i64,
    pub strategy_id: String,
    pub metrics: metrics::Metrics,
    pub equity_curve: Vec<metrics::Point>,
    pub trades: Vec<metrics::TradeResult>,
    pub proposals: Vec<Proposal>,
    pub decisions: Vec<Decision>,
    pub orders: Vec<Order>,
    pub funding: Vec<Value>,
    pub final_account: Account,
    pub data_ready_ticks: usize,
    pub ticks: usize,
    pub unexecuted_proposals: usize,
}
#[derive(Serialize)]
pub struct Report {
    pub algorithm: String,
    pub dataset_hash: String,
    pub run_id: String,
    pub source: String,
    pub config: Config,
    pub selection_rule: String,
    pub selected_strategy_id: Option<String>,
    pub selection_status: String,
    pub training: Vec<Partition>,
    pub validation: Option<Partition>,
    pub test: Option<Partition>,
    pub limitations: Vec<String>,
}
pub fn hash(value: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
struct Replay {
    markets: Vec<ListedMarket>,
    books: BTreeMap<String, Book>,
    oracles: BTreeMap<String, (i64, D)>,
    oracle_history: BTreeMap<String, BTreeMap<i64, D>>,
    rates: BTreeMap<(String, i64), D>,
    candles: BTreeMap<String, BTreeMap<i64, Candle>>,
    evaluated: BTreeMap<String, i64>,
}
impl Replay {
    fn new() -> Self {
        Self {
            markets: vec![],
            books: BTreeMap::new(),
            oracles: BTreeMap::new(),
            oracle_history: BTreeMap::new(),
            rates: BTreeMap::new(),
            candles: BTreeMap::new(),
            evaluated: BTreeMap::new(),
        }
    }
    fn apply(&mut self, event: &Event, interval: &str) {
        match event {
            Event::Metadata { markets } => self.markets = markets.clone(),
            Event::Book { book } => {
                if self
                    .books
                    .get(&book.coin)
                    .is_none_or(|b| book.time > b.time)
                {
                    self.books.insert(book.coin.clone(), book.clone());
                }
            }
            Event::Oracle {
                symbol,
                observed_at,
                price,
            } => {
                self.oracle_history
                    .entry(symbol.clone())
                    .or_default()
                    .insert(*observed_at, *price);
                if self.oracles.get(symbol).is_none_or(|o| *observed_at > o.0) {
                    self.oracles.insert(symbol.clone(), (*observed_at, *price));
                }
            }
            Event::Funding { symbol, hour, rate } => {
                self.rates.insert((symbol.clone(), *hour), *rate);
            }
            Event::Candle { candle } => {
                if candle.i == interval {
                    self.candles
                        .entry(candle.s.clone())
                        .or_default()
                        .insert(candle.t, candle.clone());
                }
            }
        }
    }
    fn history(
        &self,
        symbol: &str,
        window: usize,
        now: i64,
        step: i64,
    ) -> Option<Vec<ClosedCandle>> {
        let candles = self.candles.get(symbol)?;
        let mut cs: Vec<_> = candles.values().rev().take(window).cloned().collect();
        cs.reverse();
        if cs.len() != window
            || cs.windows(2).any(|p| p[0].t + step != p[1].t)
            || now - cs.last()?.end > step + 120_000
        {
            return None;
        }
        cs.into_iter()
            .map(|c| ClosedCandle::new(c, true, now).ok())
            .collect()
    }
    fn funding(&self) -> Vec<Funding> {
        self.rates
            .iter()
            .filter_map(|((symbol, hour), rate)| {
                let (time, oracle) = self
                    .oracle_history
                    .get(symbol)?
                    .range(..=*hour)
                    .next_back()?;
                if hour - time > 15_000 {
                    return None;
                }
                Some(Funding {
                    symbol: symbol.clone(),
                    hour: *hour,
                    rate: *rate,
                    oracle: *oracle,
                })
            })
            .collect()
    }
}
/// Half-open partition; positions/cash restart, causal indicator history may carry across its boundary.
fn replay_partition(
    data: &Dataset,
    config: &Config,
    engine: &Engine,
    name: &str,
    start: i64,
    end: i64,
) -> Result<Partition> {
    let step = interval_ms(&config.interval)?;
    let mut replay = Replay::new();
    let mut cursor = 0;
    let mut account = Account::new(config.risk.clone(), start)?;
    let mut proposals = vec![];
    let mut pending = Vec::<(i64, Signal)>::new();
    let mut next_signal = 1;
    let mut orders = vec![];
    let mut decisions = vec![];
    let mut funding = vec![];
    let mut curve = vec![];
    let mut daily = vec![metrics::Point {
        time: start,
        equity: Some(config.risk.capital),
        cash: config.risk.capital,
        open_positions: 0,
        blockers: vec![],
    }];
    let mut all_valid = true;
    let mut drawdown = metrics::Drawdown::new(config.risk.capital);
    let mut ready_ticks = 0;
    let mut ticks = 0;
    for now in (start..end).step_by(TICK as usize) {
        while cursor < data.records.len() && data.records[cursor].available_at <= now {
            replay.apply(&data.records[cursor].event, &config.interval);
            cursor += 1;
        }
        let mut ready = replay.markets.len() == 4;
        for symbol in trading_domain::SYMBOLS {
            if let Some(latest) = replay
                .candles
                .get(symbol)
                .and_then(|c| c.last_key_value())
                .map(|(_, c)| c)
                && now - latest.end <= step + 120_000
                && replay.evaluated.get(symbol) == Some(&latest.t)
            {
                continue;
            }
            let Some(history) = replay.history(symbol, engine.window(), now, step) else {
                ready = false;
                continue;
            };
            let latest = replay.candles[symbol].last_key_value().unwrap().1;
            if replay.evaluated.get(symbol).is_some_and(|t| *t >= latest.t) {
                continue;
            }
            let eval = engine.evaluate(&history)?;
            replay.evaluated.insert(symbol.into(), eval.open_time);
            if eval.close_time < start {
                continue;
            }
            for d in eval.decisions {
                if d.direction == Direction::Hold {
                    continue;
                }
                let signal = Signal {
                    event_id: next_signal,
                    symbol: symbol.into(),
                    long: d.direction == Direction::Long,
                    close_time: eval.close_time,
                    atr: eval
                        .indicators
                        .as_ref()
                        .and_then(|i| i.atr_14)
                        .unwrap_or(D::ZERO),
                };
                next_signal += 1;
                let after = now + config.latency_ms;
                proposals.push(Proposal {
                    available_at: now,
                    execute_after: after,
                    strategy: d.strategy,
                    signal: signal.clone(),
                });
                pending.push((after, signal));
            }
        }
        let executable: Vec<_> = pending
            .iter()
            .filter(|(at, _)| *at <= now)
            .map(|(_, s)| s.clone())
            .collect();
        pending.retain(|(at, _)| *at > now);
        let settlements = replay.funding();
        let changes = account.tick(
            &Input {
                now,
                ready,
                markets: &replay.markets,
                books: &replay.books,
                oracles: &replay.oracles,
                funding: &settlements,
            },
            &executable,
        )?;
        orders.extend(changes.orders);
        decisions.extend(changes.decisions);
        funding.extend(changes.funding);
        let data_ready = ready
            && !account
                .blockers
                .iter()
                .any(|b| b.starts_with("stale_book") || b.starts_with("oracle_unavailable"));
        ready_ticks += usize::from(data_ready);
        ticks += 1;
        let valid = !account
            .blockers
            .iter()
            .any(|b| b == "valuation_unavailable" || b == "funding_unreconciled");
        all_valid &= valid;
        if valid {
            drawdown.observe(account.equity);
        }
        let point = metrics::Point {
            time: now,
            equity: valid.then_some(account.equity),
            cash: account.cash,
            open_positions: account.positions.len(),
            blockers: account.blockers.clone(),
        };
        if now > start && (now - start) % metrics::DAY == 0 {
            daily.push(point.clone());
        }
        if (now - start) % 60_000 == 0 || now == end - TICK {
            curve.push(point);
        }
    }
    let trades = metrics::trades(&orders, &funding, &account)?;
    let metrics = metrics::calculate(
        &curve,
        &daily,
        &trades,
        config.risk.capital,
        all_valid,
        drawdown.maximum,
    );
    Ok(Partition {
        name: name.into(),
        start,
        end_exclusive: end,
        strategy_id: engine.id().into(),
        metrics,
        equity_curve: curve,
        trades,
        proposals,
        decisions,
        orders,
        funding,
        final_account: account,
        data_ready_ticks: ready_ticks,
        ticks,
        unexecuted_proposals: pending.len(),
    })
}
pub fn run(data: &Dataset, config: Config) -> Result<Report> {
    data.validate()?;
    config.validate(data)?;
    let mut training = vec![];
    for candidate in &config.candidates {
        training.push(replay_partition(
            data,
            &config,
            &Engine::builtin(candidate.clone())?,
            "train",
            config.train_start,
            config.validation_start,
        )?);
    }
    let selected = training
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.metrics.ending_equity.is_some()
                && p.metrics.closed_trades >= config.minimum_training_trades
                && p.data_ready_ticks == p.ticks
        })
        .max_by(|(i, a), (j, b)| {
            a.metrics
                .net_return
                .cmp(&b.metrics.net_return)
                .then_with(|| j.cmp(i))
        })
        .map(|(index, _)| index);
    let (validation, test, id) = if let Some(index) = selected {
        let engine = Engine::builtin(config.candidates[index].clone())?;
        (
            Some(replay_partition(
                data,
                &config,
                &engine,
                "validation",
                config.validation_start,
                config.test_start,
            )?),
            Some(replay_partition(
                data,
                &config,
                &engine,
                "test",
                config.test_start,
                config.end,
            )?),
            Some(engine.id().to_owned()),
        )
    } else {
        (None, None, None)
    };
    let dataset_hash = hash(data)?;
    let run_id =
        hash(&json!({"algorithm":"backtest-v1","dataset_hash":dataset_hash,"config":config}))?;
    Ok(Report{algorithm:"backtest-v1".into(),dataset_hash,run_id,source:data.source.clone(),config,selection_rule:"highest_training_net_return; complete_market_data_and_final_valuation; minimum_training_trades; ties_use_declared_order".into(),selected_strategy_id:id,selection_status:if selected.is_some(){"frozen_after_training"}else{"no_eligible_training_candidate"}.into(),training,validation,test,limitations:vec!["Recorded books only; no inferred intrabar path, missing depth or missing funding oracle.".into(),"Positions remain marked at partition end; no free liquidation or cross-partition positions.".into(),"Availability uses recorded receipt times, with configured signal latency; capture can be incomplete.".into(),"Sharpe requires 30 complete daily returns; annualization assumes IID and zero risk-free rate.".into(),"Test is used once per run; repeated external parameter selection can still contaminate holdout.".into()]})
}

/// Validated single-partition API for research with externally registered strategies.
pub fn partition(
    data: &Dataset,
    config: &Config,
    engine: &Engine,
    name: &str,
    start: i64,
    end: i64,
) -> Result<Partition> {
    data.validate()?;
    config.validate(data)?;
    ensure!(
        start >= config.train_start
            && end <= config.end
            && start < end
            && start % TICK == 0
            && end % TICK == 0,
        "invalid replay partition"
    );
    replay_partition(data, config, engine, name, start, end)
}
