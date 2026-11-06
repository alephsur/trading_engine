use anyhow::{Result, ensure};
use rust_decimal::{Decimal as D, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trading_paper::{Account, Order};
pub const DAY: i64 = 86_400_000;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub time: i64,
    pub equity: Option<D>,
    pub cash: D,
    pub open_positions: usize,
    pub blockers: Vec<String>,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct TradeResult {
    pub position_id: i64,
    pub symbol: String,
    pub long: bool,
    pub opened_at: i64,
    pub closed_at: Option<i64>,
    pub quantity: D,
    pub remaining: D,
    pub realized_gross: D,
    pub fees: D,
    pub funding: D,
    pub net_realized: D,
    pub settled: bool,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Metrics {
    pub starting_equity: D,
    pub ending_equity: Option<D>,
    pub net_return: Option<D>,
    pub max_drawdown: Option<D>,
    pub observed_max_drawdown: D,
    pub valuation_complete: bool,
    pub closed_trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub breakeven: usize,
    pub win_rate: Option<D>,
    pub profit_factor: Option<D>,
    pub trade_metrics_status: String,
    pub fees: D,
    pub funding_cash_flow: D,
    pub daily_returns: Vec<D>,
    pub sharpe_daily: Option<f64>,
    pub sharpe_annualized_iid: Option<f64>,
    pub sharpe_status: String,
}
pub fn trades(
    orders: &[Order],
    funding: &[serde_json::Value],
    account: &Account,
) -> Result<Vec<TradeResult>> {
    let mut results = BTreeMap::<i64, TradeResult>::new();
    let mut entries = BTreeMap::new();
    for o in orders {
        if o.filled == D::ZERO {
            continue;
        }
        let notional = o.fills.iter().map(|f| f.quantity * f.price).sum::<D>();
        let fees = o.fills.iter().map(|f| f.fee).sum::<D>();
        if !o.reduce_only {
            ensure!(
                !results.contains_key(&o.position_id),
                "duplicate entry in report"
            );
            entries.insert(o.position_id, notional / o.filled);
            results.insert(
                o.position_id,
                TradeResult {
                    position_id: o.position_id,
                    symbol: o.symbol.clone(),
                    long: o.buy,
                    opened_at: o.time,
                    closed_at: None,
                    quantity: o.filled,
                    remaining: o.filled,
                    realized_gross: D::ZERO,
                    fees,
                    funding: D::ZERO,
                    net_realized: -fees,
                    settled: true,
                },
            );
        } else {
            let p = results
                .get_mut(&o.position_id)
                .ok_or_else(|| anyhow::anyhow!("exit missing entry"))?;
            ensure!(
                p.remaining >= o.filled && p.long != o.buy,
                "invalid exit ledger"
            );
            p.realized_gross += (notional - entries[&o.position_id] * o.filled)
                * if p.long { D::ONE } else { -D::ONE };
            p.fees += fees;
            p.remaining -= o.filled;
            if p.remaining == D::ZERO {
                p.closed_at = Some(o.time);
            }
        }
    }
    for f in funding {
        let id = f["position_id"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing funding position"))?;
        results
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("funding missing entry"))?
            .funding += serde_json::from_value::<D>(f["cash_change"].clone())?;
    }
    let pending: BTreeSet<_> = account.funding_due.iter().map(|f| f.position_id).collect();
    for p in results.values_mut() {
        p.settled = !pending.contains(&p.position_id);
        p.net_realized = p.realized_gross - p.fees + p.funding;
    }
    Ok(results.into_values().collect())
}
/// Daily simple excess returns, sample standard deviation. Annualization explicitly assumes IID.
pub fn sharpe(returns: &[D]) -> (Option<f64>, String) {
    if returns.len() < 30 {
        return (None, "insufficient_daily_returns_minimum_30".into());
    }
    if returns.iter().all(|r| *r == returns[0]) {
        return (None, "zero_variance".into());
    }
    let values: Vec<f64> = returns
        .iter()
        .map(|r| r.to_f64().expect("Decimal fits f64"))
        .collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    let value = mean / variance.sqrt();
    if !value.is_finite() {
        return (None, "numeric_range".into());
    }
    (Some(value), "estimated_zero_risk_free_rate".into())
}
pub fn calculate(
    points: &[Point],
    daily_points: &[Point],
    trades: &[TradeResult],
    initial: D,
    all_ticks_valid: bool,
    observed_drawdown: D,
) -> Metrics {
    let ending = points.last().and_then(|p| p.equity);
    let closed: Vec<_> = trades
        .iter()
        .filter(|t| t.closed_at.is_some() && t.settled)
        .collect();
    let wins = closed.iter().filter(|t| t.net_realized > D::ZERO).count();
    let losses = closed.iter().filter(|t| t.net_realized < D::ZERO).count();
    let funding_settled = trades.iter().all(|t| t.settled);
    let enough = closed.len() >= 30 && funding_settled;
    let gains = closed
        .iter()
        .map(|t| t.net_realized.max(D::ZERO))
        .sum::<D>();
    let loss = -closed
        .iter()
        .map(|t| t.net_realized.min(D::ZERO))
        .sum::<D>();
    let mut returns = vec![];
    if all_ticks_valid {
        for pair in daily_points.windows(2) {
            if let (Some(a), Some(b)) = (pair[0].equity, pair[1].equity)
                && pair[1].time - pair[0].time == DAY
                && a > D::ZERO
            {
                returns.push(b / a - D::ONE);
            }
        }
    }
    let (ratio, status) = if daily_points
        .iter()
        .any(|p| p.equity.is_some_and(|v| v <= D::ZERO))
    {
        (None, "nonpositive_daily_equity".into())
    } else if all_ticks_valid {
        sharpe(&returns)
    } else {
        (None, "incomplete_valuation_path".into())
    };
    Metrics {
        starting_equity: initial,
        ending_equity: ending,
        net_return: ending.map(|e| e / initial - D::ONE),
        max_drawdown: all_ticks_valid.then_some(observed_drawdown),
        observed_max_drawdown: observed_drawdown,
        valuation_complete: all_ticks_valid,
        closed_trades: closed.len(),
        wins,
        losses,
        breakeven: closed.len() - wins - losses,
        win_rate: enough.then(|| D::from(wins) / D::from(closed.len())),
        profit_factor: if enough && loss > D::ZERO {
            Some(gains / loss)
        } else {
            None
        },
        trade_metrics_status: if !funding_settled {
            "unsettled_funding"
        } else if !enough {
            "insufficient_settled_trades_minimum_30"
        } else if loss == D::ZERO {
            "no_losing_trades_profit_factor_undefined"
        } else {
            "sample_estimate"
        }
        .into(),
        fees: trades.iter().map(|t| t.fees).sum(),
        funding_cash_flow: trades.iter().map(|t| t.funding).sum(),
        daily_returns: returns,
        sharpe_daily: ratio,
        sharpe_annualized_iid: ratio.map(|r| r * 365_f64.sqrt()),
        sharpe_status: status,
    }
}

pub struct Drawdown {
    peak: D,
    pub maximum: D,
}
impl Drawdown {
    pub fn new(initial: D) -> Self {
        Self {
            peak: initial,
            maximum: D::ZERO,
        }
    }
    pub fn observe(&mut self, equity: D) {
        self.peak = self.peak.max(equity);
        if self.peak > D::ZERO {
            self.maximum = self.maximum.max((self.peak - equity) / self.peak);
        }
    }
}
