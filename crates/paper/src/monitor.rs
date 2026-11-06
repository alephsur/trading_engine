//! Persistent observation window; never reconstruct missing equity from a later price.
use crate::Account;
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
const DAY: i64 = 86_400_000;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monitor {
    pub since: i64,
    pub last_observed: i64,
    pub samples: u64,
    pub complete: bool,
    pub peak: Option<D>,
    pub observed_max_drawdown: D,
    pub day: i64,
    pub daily_boundary: Option<D>,
    pub daily_returns: Vec<D>,
    pub gap_count: u64,
}
impl Monitor {
    pub fn new(now: i64) -> Self {
        Self {
            since: now,
            last_observed: now,
            samples: 0,
            complete: true,
            peak: None,
            observed_max_drawdown: D::ZERO,
            day: now / DAY,
            daily_boundary: None,
            daily_returns: vec![],
            gap_count: 0,
        }
    }
    pub fn observe(&mut self, now: i64, equity: Option<D>) {
        let gap =
            self.samples > 0 && (now < self.last_observed || now - self.last_observed > 15000);
        if gap {
            self.gap_count += 1;
        }
        self.complete &= !gap && equity.is_some();
        self.samples += 1;
        if let Some(value) = equity {
            let peak = self.peak.unwrap_or(value).max(value);
            self.peak = Some(peak);
            if peak > D::ZERO {
                self.observed_max_drawdown = self.observed_max_drawdown.max((peak - value) / peak);
            }
        }
        let day = now / DAY;
        if day != self.day {
            if day == self.day + 1 && now % DAY < 15000 && !gap {
                if let (Some(previous), Some(current)) = (self.daily_boundary, equity) {
                    if previous > D::ZERO && current > D::ZERO {
                        self.daily_returns.push(current / previous - D::ONE);
                    } else {
                        self.complete = false;
                    }
                }
                self.daily_boundary = equity;
            } else {
                self.daily_boundary = None;
                self.complete = false;
            }
            self.day = day;
        }
        self.last_observed = now;
    }
}
pub fn equity(account: &Account) -> Option<D> {
    (!account
        .blockers
        .iter()
        .any(|b| b == "valuation_unavailable" || b == "funding_unreconciled"))
    .then_some(account.equity)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dec;
    #[test]
    fn tracks_intraminute_drawdown_and_marks_gaps_without_repairing_them() {
        let mut m = Monitor::new(1000);
        for (time, value) in [(1000, "100"), (2000, "120"), (3000, "90"), (4000, "110")] {
            m.observe(time, Some(dec(value)));
        }
        assert_eq!(m.observed_max_drawdown, dec("0.25"));
        assert!(m.complete);
        m.observe(20001, Some(dec("115")));
        assert!(!m.complete);
        assert_eq!(m.gap_count, 1);
        m.observe(21000, None);
        m.observe(22000, Some(dec("120")));
        assert!(!m.complete);
        let recovered: Monitor = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(recovered.since, 1000);
    }
    #[test]
    fn ignores_first_partial_day_and_rejects_missing_boundaries() {
        let mut m = Monitor::new(DAY - 1000);
        m.observe(DAY - 1000, Some(dec("100")));
        m.observe(DAY + 1000, Some(dec("110")));
        assert!(m.daily_returns.is_empty());
        m.last_observed = 2 * DAY - 1000;
        m.observe(2 * DAY + 1000, Some(dec("121")));
        assert_eq!(m.daily_returns, vec![dec("0.1")]);
        m.observe(3 * DAY + 20000, Some(dec("130")));
        assert!(!m.complete);
        assert_eq!(m.daily_returns.len(), 1);
    }
}
