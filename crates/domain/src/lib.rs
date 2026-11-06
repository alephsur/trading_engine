use anyhow::{Result, ensure};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SYMBOLS: [&str; 4] = ["BTC", "ETH", "SOL", "HYPE"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub name: String,
    pub sz_decimals: u32,
    pub max_leverage: u32,
    #[serde(default)]
    pub is_delisted: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ListedMarket {
    pub asset_index: usize,
    pub metadata: Market,
}
#[derive(Deserialize)]
pub struct Metadata {
    pub universe: Vec<Market>,
}

pub fn select_markets(meta: Metadata) -> Result<Vec<ListedMarket>> {
    SYMBOLS
        .iter()
        .map(|symbol| {
            let matches: Vec<_> = meta
                .universe
                .iter()
                .enumerate()
                .filter(|(_, m)| m.name == *symbol)
                .collect();
            ensure!(matches.len() == 1, "missing or ambiguous market: {symbol}");
            let (asset_index, m) = matches[0];
            ensure!(
                !m.is_delisted && m.sz_decimals <= 6 && m.max_leverage > 0,
                "invalid market: {symbol}"
            );
            Ok(ListedMarket {
                asset_index,
                metadata: m.clone(),
            })
        })
        .collect()
}

pub fn parse_mids(value: &serde_json::Value) -> Result<Option<BTreeMap<String, Decimal>>> {
    if value["channel"] != "allMids" {
        return Ok(None);
    }
    let mut prices = BTreeMap::new();
    for symbol in SYMBOLS {
        let raw = value["data"]["mids"][symbol]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing mid: {symbol}"))?;
        let price: Decimal = raw.parse()?;
        ensure!(price > Decimal::ZERO, "nonpositive mid: {symbol}");
        prices.insert(symbol.to_owned(), price);
    }
    Ok(Some(prices))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_indices_instead_of_hardcoding() {
        let universe = ["OTHER", "HYPE", "SOL", "BTC", "ETH"]
            .into_iter()
            .map(|name| Market {
                name: name.into(),
                sz_decimals: 3,
                max_leverage: 10,
                is_delisted: false,
            })
            .collect();
        let markets = select_markets(Metadata { universe }).unwrap();
        assert_eq!(markets[0].asset_index, 3);
        assert_eq!(markets[3].asset_index, 1);
    }
    #[test]
    fn rejects_missing_and_delisted_markets() {
        assert!(select_markets(Metadata { universe: vec![] }).is_err());
        let universe = SYMBOLS
            .iter()
            .map(|name| Market {
                name: (*name).into(),
                sz_decimals: 3,
                max_leverage: 10,
                is_delisted: *name == "HYPE",
            })
            .collect();
        assert!(select_markets(Metadata { universe }).is_err());
    }
    #[test]
    fn validates_complete_positive_decimal_prices() {
        let mut v = serde_json::json!({"channel":"allMids","data":{"mids":{"BTC":"100.12345678","ETH":"20","SOL":"3","HYPE":"4","DOGE":"1"}}});
        let prices = parse_mids(&v).unwrap().unwrap();
        assert_eq!(prices.len(), 4);
        assert_eq!(prices["BTC"].to_string(), "100.12345678");
        v["data"]["mids"]["HYPE"] = serde_json::json!("0");
        assert!(parse_mids(&v).is_err());
        assert!(
            parse_mids(&serde_json::json!({"channel":"pong"}))
                .unwrap()
                .is_none()
        );
    }
}

pub mod data;
