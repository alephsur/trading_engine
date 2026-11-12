//! Bounded diagnostic metadata: never persist request bodies, keys or raw errors.
use serde_json::{Value, json};

#[derive(Debug)]
pub struct ApiFailure {
    pub operation: &'static str,
    pub category: &'static str,
    pub status: Option<u16>,
}
impl std::fmt::Display for ApiFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "testnet {} {} (HTTP {:?}); write outcome may be unknown",
            self.operation, self.category, self.status
        )
    }
}
impl std::error::Error for ApiFailure {}

pub fn describe(error: &anyhow::Error) -> Value {
    let stage = error
        .chain()
        .find_map(|cause| match cause.to_string().as_str() {
            "supervision.funding" => Some("funding"),
            "supervision.reconcile" => Some("reconcile"),
            "supervision.open_orders" => Some("open_orders"),
            "supervision.assess" => Some("assess"),
            _ => None,
        })
        .unwrap_or("automatic_cycle");
    if let Some(api) = error.downcast_ref::<ApiFailure>() {
        return json!({"stage":stage,"category":api.category,"operation":api.operation,"http_status":api.status});
    }
    let category = if let Some(db) = error.downcast_ref::<sqlx::Error>() {
        match db {
            sqlx::Error::PoolTimedOut => "database_pool_timeout",
            sqlx::Error::Io(_) | sqlx::Error::Tls(_) => "database_connection",
            _ => "database_error",
        }
    } else if error.downcast_ref::<serde_json::Error>().is_some() {
        "response_schema"
    } else {
        match error.root_cause().to_string().as_str() {
            "stale supervision snapshot" => "stale_snapshot",
            "funding history truncated; entry gate remains blocked" => "funding_truncated",
            "conflicting funding replay" => "funding_conflict",
            "out-of-order supervision cycle" => "out_of_order",
            _ => "validation_or_internal_error",
        }
    };
    json!({"stage":stage,"category":category})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_retains_context_and_status_without_raw_messages() {
        let error = anyhow::Error::new(ApiFailure {
            operation: "info",
            category: "http_status",
            status: Some(429),
        })
        .context("supervision.funding");
        assert_eq!(
            describe(&error),
            json!({"stage":"funding","category":"http_status","operation":"info","http_status":429})
        );
        let secret =
            anyhow::anyhow!("private-key=SECRET response=PRIVATE").context("supervision.assess");
        assert_eq!(
            describe(&secret),
            json!({"stage":"assess","category":"validation_or_internal_error"})
        );
        assert_eq!(
            describe(&anyhow::Error::new(sqlx::Error::PoolTimedOut))["category"],
            "database_pool_timeout"
        );
        assert_eq!(
            describe(&anyhow::anyhow!("stale supervision snapshot"))["category"],
            "stale_snapshot"
        );
    }
}
