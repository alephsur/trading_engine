use anyhow::{Result, ensure};
use hypersdk::hypercore::{Chain, PrivateKeySigner, api::Action};
use serde_json::Value;
use zeroize::Zeroizing;

pub struct TestnetSigner(PrivateKeySigner);
impl TestnetSigner {
    pub fn load(key: Zeroizing<String>) -> Result<Self> {
        let signer = key
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid testnet signing key"))?;
        Ok(Self(signer))
    }
    pub fn address(&self) -> String {
        self.0.address().to_string().to_lowercase()
    }
    pub fn sign(&self, action: Value, nonce: i64, expires: i64) -> Result<Value> {
        ensure!(
            nonce > 0 && expires > nonce && expires - nonce <= 30_000,
            "invalid signing window"
        );
        ensure!(
            matches!(action["type"].as_str(), Some("order" | "cancelByCloid")),
            "action not permitted"
        );
        let action: Action = serde_json::from_value(action)?;
        let request = action.sign_sync(
            &self.0,
            nonce as u64,
            None,
            Some(
                chrono::DateTime::from_timestamp_millis(expires)
                    .ok_or_else(|| anyhow::anyhow!("expiry outside timestamp range"))?,
            ),
            Chain::Testnet,
        )?;
        Ok(serde_json::to_value(request)?)
    }
}
pub fn address(value: &str) -> Result<String> {
    ensure!(
        value.len() == 42
            && value.starts_with("0x")
            && value[2..].bytes().all(|b| b.is_ascii_hexdigit())
            && value[2..].bytes().any(|b| b != b'0'),
        "invalid account address"
    );
    Ok(value.to_lowercase())
}
