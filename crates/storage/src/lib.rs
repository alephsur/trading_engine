use anyhow::Result;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub async fn connect(url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(pool)
}
pub async fn record(pool: &PgPool, kind: &str, payload: serde_json::Value) -> Result<()> {
    sqlx::query("INSERT INTO events (kind, payload) VALUES ($1, $2)")
        .bind(kind)
        .bind(payload)
        .execute(pool)
        .await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL pointing to a disposable PostgreSQL database"]
    async fn migrations_and_event_roundtrip() {
        let pool = connect(&std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL"))
            .await
            .unwrap();
        connect(&std::env::var("TEST_DATABASE_URL").unwrap())
            .await
            .unwrap();
        record(
            &pool,
            "integration_test",
            serde_json::json!({"price":"0.123456789"}),
        )
        .await
        .unwrap();
        let payload: serde_json::Value = sqlx::query_scalar(
            "SELECT payload FROM events WHERE kind = 'integration_test' ORDER BY id DESC LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(payload["price"], "0.123456789");
    }
}
