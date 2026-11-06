use anyhow::{Result, ensure};
use rust_decimal::Decimal as D;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use trading_domain::{ListedMarket, data::Book};
use trading_paper::{dec, price};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Intent {
    Entry {
        symbol: String,
        buy: bool,
        quantity: D,
        limit: D,
        stop: D,
        take_profit: D,
    },
    Close {
        symbol: String,
        #[serde(default)]
        limit: Option<D>,
    },
    Cleanup {
        target: String,
    },
    Cancel {
        target: String,
    },
}
pub fn command_id(id: &str) -> Result<()> {
    ensure!(
        (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "command id must be 8..64 alphanumeric/hyphen characters"
    );
    Ok(())
}
pub fn cloid(account: &str, id: &str, leg: u8) -> String {
    let bytes = Sha256::digest(format!("trading-testnet-v1:{account}:{id}:{leg}"));
    format!("0x{}", hex::encode(&bytes[..16]))
}
pub fn decimal(value: &Value) -> Result<D> {
    value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing decimal string"))?
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid decimal"))
}
pub fn position(state: &Value, symbol: &str) -> Result<D> {
    let positions = state["assetPositions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid account positions"))?;
    let mut found = None;
    for p in positions {
        let p = &p["position"];
        if p["coin"] == symbol {
            ensure!(found.is_none(), "duplicate account position");
            found = Some(decimal(&p["szi"])?);
        }
    }
    Ok(found.unwrap_or(D::ZERO))
}
pub fn flat(state: &Value) -> Result<bool> {
    let ps = state["assetPositions"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid account positions"))?;
    for p in ps {
        if decimal(&p["position"]["szi"])? != D::ZERO {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn market<'a>(markets: &'a [ListedMarket], symbol: &str) -> Result<&'a ListedMarket> {
    markets
        .iter()
        .find(|m| m.metadata.name == symbol)
        .ok_or_else(|| anyhow::anyhow!("asset unavailable or not allowed"))
}
pub fn fresh_book(book: &Book, symbol: &str, now: i64) -> Result<()> {
    book.validate()?;
    ensure!(
        book.coin == symbol && (0..15_000).contains(&(now - book.time)),
        "book stale or wrong symbol"
    );
    Ok(())
}
fn valid_price(v: D, decimals: u32) -> bool {
    v > D::ZERO && v <= dec("1000000000") && price(v, decimals, false) == v
}
fn order(
    asset: usize,
    buy: bool,
    quantity: D,
    limit: D,
    reduce: bool,
    kind: Value,
    id: String,
) -> Value {
    json!({"a":asset,"b":buy,"p":limit.normalize().to_string(),"s":quantity.normalize().to_string(),"r":reduce,"t":kind,"c":id})
}
/// Narrow integration harness: one isolated bracket, <=25 test USDC and existing conservative risk caps.
pub fn entry(
    intent: &Intent,
    account: &str,
    id: &str,
    markets: &[ListedMarket],
    book: &Book,
    state: &Value,
    now: i64,
) -> Result<Value> {
    let Intent::Entry {
        symbol,
        buy,
        quantity,
        limit,
        stop,
        take_profit,
    } = intent
    else {
        anyhow::bail!("entry required")
    };
    let m = market(markets, symbol)?;
    let decimals = m.metadata.sz_decimals;
    fresh_book(book, symbol, now)?;
    ensure!(
        flat(state)?,
        "entry requires a flat dedicated testnet account"
    );
    let equity = decimal(&state["marginSummary"]["accountValue"])?;
    ensure!(
        equity >= dec("100"),
        "testnet account requires at least 100 test USDC"
    );
    ensure!(
        *quantity > D::ZERO
            && *quantity <= dec("1000000")
            && quantity.round_dp(decimals) == *quantity,
        "invalid quantity precision"
    );
    ensure!(
        [*limit, *stop, *take_profit]
            .into_iter()
            .all(|p| valid_price(p, decimals)),
        "invalid order price precision"
    );
    let notional = quantity * limit;
    ensure!(
        notional >= dec("10") && notional <= dec("25") && notional <= equity * dec("0.15"),
        "testnet entry notional outside conservative bounds"
    );
    let bid = book.levels[0][0].px;
    let ask = book.levels[1][0].px;
    let mid = (bid + ask) / dec("2");
    ensure!((ask - bid) / mid <= dec("0.002"), "spread too wide");
    ensure!(
        (*limit - mid).abs() / mid <= dec("0.002"),
        "limit too far from current market"
    );
    ensure!(
        if *buy {
            *stop < *limit && *take_profit > *limit
        } else {
            *stop > *limit && *take_profit < *limit
        },
        "stop/target on wrong side"
    );
    ensure!(
        (*limit - *stop).abs() / limit <= dec("0.03"),
        "stop exceeds integration risk limit"
    );
    let stop_limit = price(
        *stop * if *buy { dec("0.98") } else { dec("1.02") },
        decimals,
        !*buy,
    );
    let worst_loss = (*limit - stop_limit).abs() * quantity + notional * dec("0.002");
    ensure!(
        worst_loss <= equity * dec("0.0025"),
        "stop risk exceeds account limit"
    );
    let depth: D = book.levels[usize::from(*buy)]
        .iter()
        .filter(|l| if *buy { l.px <= *limit } else { l.px >= *limit })
        .map(|l| l.sz)
        .sum();
    ensure!(
        *quantity <= depth * dec("0.1"),
        "insufficient executable depth"
    );
    let entry = order(
        m.asset_index,
        *buy,
        *quantity,
        *limit,
        false,
        json!({"limit":{"tif":"Ioc"}}),
        cloid(account, id, 0),
    );
    let trigger = |px: D, kind: &str, leg| {
        order(
            m.asset_index,
            !*buy,
            *quantity,
            price(
                px * if *buy { dec("0.98") } else { dec("1.02") },
                decimals,
                !*buy,
            ),
            true,
            json!({"trigger":{"isMarket":true,"triggerPx":px.normalize().to_string(),"tpsl":kind}}),
            cloid(account, id, leg),
        )
    };
    Ok(
        json!({"type":"order","orders":[entry,trigger(*stop,"sl",1),trigger(*take_profit,"tp",2)],"grouping":"normalTpsl"}),
    )
}
pub fn close(
    intent: &Intent,
    account: &str,
    id: &str,
    markets: &[ListedMarket],
    book: &Book,
    state: &Value,
    now: i64,
) -> Result<Value> {
    let Intent::Close {
        symbol,
        limit: requested_limit,
    } = intent
    else {
        anyhow::bail!("close intent required")
    };
    let m = market(markets, symbol)?;
    fresh_book(book, symbol, now)?;
    let qty = position(state, symbol)?;
    ensure!(qty != D::ZERO, "no position to close");
    let buy = qty < D::ZERO;
    let px = book.levels[usize::from(buy)][0].px;
    let limit = price(
        px * if buy { dec("1.002") } else { dec("0.998") },
        m.metadata.sz_decimals,
        buy,
    );
    let (limit, tif) = if let Some(value) = requested_limit {
        ensure!(
            valid_price(*value, m.metadata.sz_decimals) && (*value - px).abs() / px <= dec("0.01"),
            "invalid reduce-only resting limit"
        );
        (*value, "Alo")
    } else {
        (limit, "Ioc")
    };
    Ok(
        json!({"type":"order","orders":[order(m.asset_index,buy,qty.abs(),limit,true,json!({"limit":{"tif":tif}}),cloid(account,id,0))],"grouping":"na"}),
    )
}
/// A transport acknowledgement is not authoritative order/fill state.
pub fn response_state(value: &Value, expected: usize) -> &'static str {
    if value["status"] == "err" {
        return "rejected";
    }
    if value["status"] != "ok" {
        return "uncertain";
    }
    let Some(statuses) = value["response"]["data"]["statuses"].as_array() else {
        return "uncertain";
    };
    if statuses.len() != expected {
        return "uncertain";
    }
    if statuses
        .iter()
        .all(|s| s.get("error").and_then(Value::as_str).is_some())
    {
        return "rejected";
    }
    if statuses.iter().all(|s| {
        s == "success"
            || s == "waitingForFill"
            || s == "waitingForTrigger"
            || s.get("resting").is_some()
            || s.get("filled").is_some()
            || s.get("error").is_some()
    }) {
        return "acknowledged";
    }
    "uncertain"
}

/// Build a small, reviewable test intent from current prices; never submits it.
pub fn plan(
    symbol: &str,
    buy: bool,
    markets: &[ListedMarket],
    book: &Book,
    state: &Value,
    now: i64,
) -> Result<Intent> {
    let m = market(markets, symbol)?;
    fresh_book(book, symbol, now)?;
    let px = book.levels[usize::from(buy)][0].px;
    let limit = price(
        px * if buy { dec("1.0002") } else { dec("0.9998") },
        m.metadata.sz_decimals,
        buy,
    );
    let stop = price(
        limit * if buy { dec("0.99") } else { dec("1.01") },
        m.metadata.sz_decimals,
        !buy,
    );
    let take_profit = price(
        limit * if buy { dec("1.02") } else { dec("0.98") },
        m.metadata.sz_decimals,
        !buy,
    );
    let worst = price(
        stop * if buy { dec("0.98") } else { dec("1.02") },
        m.metadata.sz_decimals,
        !buy,
    );
    let equity = decimal(&state["marginSummary"]["accountValue"])?;
    let quantity = (dec("20") / limit)
        .min(equity * dec("0.15") / limit)
        .min(equity * dec("0.0025") / ((limit - worst).abs() + limit * dec("0.002")))
        .round_dp_with_strategy(
            m.metadata.sz_decimals,
            rust_decimal::RoundingStrategy::ToZero,
        );
    let intent = Intent::Entry {
        symbol: symbol.into(),
        buy,
        quantity,
        limit,
        stop,
        take_profit,
    };
    entry(
        &intent,
        "unsigned-plan",
        "unsigned-plan",
        markets,
        book,
        state,
        now,
    )?;
    Ok(intent)
}
