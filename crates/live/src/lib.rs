//! Production preparation only. No signer, credentials, HTTP client or execution transport.
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MANUAL_CONFIRMATION: &str = "I_AUTHORIZE_MAINNET_TRADING";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Activation {
    pub enabled: bool,
    pub manual_confirmation: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub symbol: String,
    pub quantity: D,
    pub leverage: D,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub buy: bool,
    pub quantity: D,
    pub reduce_only: bool,
    pub stop: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub observed_ms: i64,
    pub positions: Vec<Position>,
    pub orders: Vec<Order>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentInput {
    pub activation: Activation,
    pub expected_positions: Vec<Position>,
    pub expected_open_order_ids: Vec<String>,
    pub snapshot: Snapshot,
}
#[derive(Debug, Serialize)]
pub struct Assessment {
    pub mode: &'static str,
    pub execution_enabled: bool,
    pub source: &'static str,
    pub blockers: Vec<&'static str>,
}

fn positions(rows: &[Position]) -> Option<BTreeMap<String, D>> {
    let mut out = BTreeMap::new();
    for p in rows {
        if !trading_domain::SYMBOLS.contains(&p.symbol.as_str())
            || p.quantity == D::ZERO
            || p.quantity.abs() > D::from(1_000_000)
            || p.leverage <= D::ZERO
            || p.leverage > D::ONE
            || out.insert(p.symbol.clone(), p.quantity).is_some()
        {
            return None;
        }
    }
    Some(out)
}

/// Read-only audit of imported evidence. It never grants a production capability.
pub fn assess(input: &AssessmentInput, now_ms: i64) -> Assessment {
    let mut blockers = vec!["production_build_disabled"];
    if !input.activation.enabled {
        blockers.push("configuration_disabled");
    }
    if input.activation.manual_confirmation.as_deref() != Some(MANUAL_CONFIRMATION) {
        blockers.push("manual_confirmation_missing");
    }
    if !now_ms
        .checked_sub(input.snapshot.observed_ms)
        .is_some_and(|age| (0..15_000).contains(&age))
        || input.snapshot.observed_ms < 0
    {
        blockers.push("stale_or_future_snapshot");
    }
    let actual = positions(&input.snapshot.positions);
    let expected = positions(&input.expected_positions);
    if actual.is_none() || expected.is_none() || actual != expected {
        blockers.push("positions_unreconciled");
    }
    if input.snapshot.positions.len() > 2 {
        blockers.push("position_limit");
    }
    let expected_ids: BTreeSet<_> = input.expected_open_order_ids.iter().collect();
    let actual_ids: BTreeSet<_> = input.snapshot.orders.iter().map(|o| &o.id).collect();
    if expected_ids.len() != input.expected_open_order_ids.len()
        || actual_ids.len() != input.snapshot.orders.len()
        || actual_ids != expected_ids
        || input.snapshot.orders.iter().any(|o| {
            o.id.is_empty()
                || !trading_domain::SYMBOLS.contains(&o.symbol.as_str())
                || o.quantity <= D::ZERO
                || o.quantity > D::from(1_000_000)
        })
    {
        blockers.push("orders_unreconciled");
    }
    for p in &input.snapshot.positions {
        // Require one full protective stop, not unrelated profit-taking orders.
        if !input.snapshot.orders.iter().any(|o| {
            o.symbol == p.symbol
                && o.reduce_only
                && o.stop
                && o.buy == (p.quantity < D::ZERO)
                && o.quantity >= p.quantity.abs()
        }) {
            blockers.push("unprotected_position");
            break;
        }
    }
    Assessment {
        mode: "LIVE_PREPARATION",
        execution_enabled: false,
        source: "imported_snapshot_not_exchange_attestation",
        blockers,
    }
}

/// The production boundary has no transport or signer, even with both gates set.
pub struct LiveExecutor;
impl LiveExecutor {
    pub fn submit(&self) -> anyhow::Result<()> {
        anyhow::bail!("LIVE unavailable in this build; no order sent")
    }
    pub fn cancel(&self) -> anyhow::Result<()> {
        anyhow::bail!("LIVE unavailable in this build; no cancellation sent")
    }
}
