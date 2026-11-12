use serde_json::{Value, json};
use trading_testnet::{
    client::InfoTransport,
    observer::{self, Config},
};

fn config() -> Config {
    serde_json::from_str(include_str!("../../../config/testnet-observer.json")).unwrap()
}
fn candles(as_of: i64) -> Value {
    let last = trading_domain::data::last_closed_open(as_of, 60_000);
    json!((0..100).map(|n| {
        let t = last - (99-n)*60_000;
        json!({"t":t,"T":t+59999,"s":"BTC","i":"1m","o":"100","h":"102","l":"99","c":"101","v":"10","n":10})
    }).collect::<Vec<_>>())
}
#[test]
fn complete_closed_window_only_and_ema_only() {
    let now = 12_005_000;
    let mut data = candles(now);
    let result = observer::evaluate(data.clone(), &config(), now).unwrap();
    assert!(result.analysis_only);
    assert_eq!(result.decisions.len(), 1);
    assert_eq!(result.decisions[0].strategy, "ema_trend_v1");
    let mut forming = data[99].clone();
    forming["t"] = json!(12_000_000);
    forming["T"] = json!(12_059_999);
    data.as_array_mut().unwrap().push(forming);
    assert_eq!(observer::evaluate(data, &config(), now).unwrap(), result);
    // During the grace period the preceding candle remains unconfirmed.
    assert!(observer::evaluate(candles(now), &config(), 12_001_000).is_err());
}
#[test]
fn rejects_gaps_duplicates_wrong_market_and_stale_data() {
    let now = 12_005_000;
    let mut gap = candles(now);
    gap.as_array_mut().unwrap().remove(50);
    assert!(observer::evaluate(gap, &config(), now).is_err());
    let mut duplicate = candles(now);
    duplicate[50] = duplicate[49].clone();
    assert!(observer::evaluate(duplicate, &config(), now).is_err());
    let mut wrong = candles(now);
    wrong[0]["s"] = json!("ETH");
    assert!(observer::evaluate(wrong, &config(), now).is_err());
    assert!(observer::evaluate(candles(now - 60000), &config(), now).is_err());
    let mut invalid = config();
    invalid.max_entries = 2;
    assert!(invalid.validate().is_err());
}

#[test]
fn hourly_profile_uses_the_same_closed_window_as_paper_for_all_assets() {
    use trading_quant::{ClosedCandle, Engine, config::Config as QuantConfig};
    let now = 720_005_000;
    let last = trading_domain::data::last_closed_open(now, 3_600_000);
    let engine = Engine::builtin(QuantConfig::confirmed_breakout_1h()).unwrap();
    for symbol in trading_domain::SYMBOLS {
        let mut cfg: Config = serde_json::from_str(include_str!(
            "../../../config/testnet-confirmed-breakout.json"
        ))
        .unwrap();
        cfg.symbol = symbol.into();
        let candles: Vec<trading_domain::data::Candle> = (0..100).map(|n| {
            let t=last-(99-n)*3_600_000;
            serde_json::from_value(json!({"t":t,"T":t+3_599_999,"s":symbol,"i":"1h","o":"100","h":"101","l":"99","c":"100","v":"10","n":10})).unwrap()
        }).collect();
        let expected = engine
            .evaluate(
                &candles
                    .iter()
                    .cloned()
                    .map(|c| ClosedCandle::new(c, true, now).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            observer::evaluate(json!(candles), &cfg, now).unwrap(),
            expected
        );
        cfg.interval = "1m".into();
        assert!(cfg.validate().is_err());
    }
}

struct Feed;
impl InfoTransport for Feed {
    async fn info(&self, request: Value) -> anyhow::Result<Value> {
        assert_eq!(request["type"], "candleSnapshot");
        assert_eq!(request["req"]["coin"], "BTC");
        Ok(candles(request["req"]["endTime"].as_i64().unwrap()))
    }
}
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema, synthetic candle feed, no orders"]
async fn durable_session_deduplicates_and_preserves_deadline_and_pause() {
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::str::FromStr;
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "observer_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let options = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new()
        .connect_with(options.clone())
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let cfg = config();
    let (a, b) = tokio::join!(
        observer::tick(&Feed, &pool, "account", "session-001", &cfg),
        observer::tick(&Feed, &pool, "account", "session-001", &cfg)
    );
    let a = a.unwrap();
    b.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM testnet_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    // A real minute boundary can occur between the concurrent request clocks.
    assert!((1..=2).contains(&count));
    observer::pause(&pool, "account", "session-001")
        .await
        .unwrap();
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    let b = observer::tick(&Feed, &pool, "account", "session-001", &cfg)
        .await
        .unwrap();
    assert_eq!(b["expires_ms"], a["expires_ms"]);
    assert_eq!(b["paused"], true);
    let mut changed = config();
    changed.duration_seconds = 1800;
    assert!(
        observer::start(&pool, "account", "session-001", &changed)
            .await
            .is_err()
    );
    observer::start(&pool, "account", "expired-001", &cfg)
        .await
        .unwrap();
    sqlx::query("UPDATE testnet_observer_sessions SET started_ms=1,expires_ms=2 WHERE session_id='expired-001'").execute(&pool).await.unwrap();
    let expired = observer::tick(&Feed, &pool, "account", "expired-001", &cfg)
        .await
        .unwrap();
    assert_eq!(expired["expired"], true);
    assert!(expired["latest_evaluation"].is_null());
    let writes: i64 = sqlx::query_scalar("SELECT count(*) FROM testnet_requests")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(writes, 0);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
