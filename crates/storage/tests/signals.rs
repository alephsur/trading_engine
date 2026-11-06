use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use trading_domain::data::Candle;
use trading_quant::{
    ClosedCandle, Engine,
    config::Config,
    strategies::{Direction, Strategy},
};
use trading_storage::{market, signals};

struct Fixture;
impl Strategy for Fixture {
    fn name(&self) -> &'static str {
        "fixture_v1"
    }
    fn parameters(&self) -> Value {
        json!({})
    }
    fn evaluate(&self, _: &[Candle]) -> anyhow::Result<(Direction, &'static str)> {
        Ok((Direction::Long, "test_fixture"))
    }
}
fn candles(count: usize) -> Vec<Candle> {
    (0..count).map(|n|serde_json::from_value(json!({"t":n as i64*60000,"T":(n as i64+1)*60000-1,"s":"BTC","i":"1m","o":"100","h":"101","l":"99","c":"100","v":"1","n":1})).unwrap()).collect()
}
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema"]
async fn atomic_replay_cursor_and_config_versions() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "test_signals_{}",
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
    let engine = Engine::new(Config::default(), vec![Box::new(Fixture)]).unwrap();
    signals::register(&pool, &engine).await.unwrap();
    let cs = candles(106);
    market::candles(&pool, &cs[..105], true).await.unwrap();
    market::candles(&pool, &cs[105..], false).await.unwrap();
    market::anchor(&pool, "BTC", "1m", 0).await.unwrap();
    market::coverage(&pool, "BTC", "1m", 104 * 60000, 0)
        .await
        .unwrap();
    // Even incorrect coverage cannot make an unconfirmed candle enter analysis.
    market::coverage(&pool, "BTC", "1m", 105 * 60000, 0)
        .await
        .unwrap();
    assert!(
        signals::process_series(&pool, &engine, "BTC", "1m")
            .await
            .is_err()
    );
    market::coverage(&pool, "BTC", "1m", 104 * 60000, 0)
        .await
        .unwrap();
    // Fail AFTER the first directional signal to verify the entire batch rolls back.
    sqlx::query("ALTER TABLE signals ADD CONSTRAINT injected_failure CHECK (open_time <> 6000000)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        signals::process_series(&pool, &engine, "BTC", "1m")
            .await
            .is_err()
    );
    assert!(
        signals::evaluations(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        signals::cursors(&pool, engine.id())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        signals::signals(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .is_empty()
    );
    sqlx::query("ALTER TABLE signals DROP CONSTRAINT injected_failure")
        .execute(&pool)
        .await
        .unwrap();
    // Two concurrent consumers must still produce one evaluation per candle.
    let (a, b) = tokio::join!(
        signals::process_series(&pool, &engine, "BTC", "1m"),
        signals::process_series(&pool, &engine, "BTC", "1m")
    );
    assert!(a.unwrap() && b.unwrap());
    let rows = signals::evaluations(&pool, engine.id(), "BTC", "1m", 500)
        .await
        .unwrap();
    assert_eq!(rows.len(), 105);
    assert_eq!(
        signals::signals(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .len(),
        6
    );
    let history: Vec<_> = cs[..105]
        .iter()
        .map(|c| ClosedCandle::new(c.clone(), true, i64::MAX).unwrap())
        .collect();
    let expected = engine.replay(&history).unwrap();
    for (stored, replayed) in rows.iter().rev().zip(&expected) {
        assert_eq!(*stored, serde_json::to_value(replayed).unwrap());
    }
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    assert!(
        signals::process_series(&pool, &engine, "BTC", "1m")
            .await
            .unwrap()
    );
    assert_eq!(
        rows,
        signals::evaluations(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
    );
    // Confirming the next candle appends exactly one decision batch and one event.
    market::candles(&pool, &cs[105..], true).await.unwrap();
    market::coverage(&pool, "BTC", "1m", 105 * 60000, 0)
        .await
        .unwrap();
    signals::process_series(&pool, &engine, "BTC", "1m")
        .await
        .unwrap();
    assert_eq!(
        signals::evaluations(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .len(),
        106
    );
    assert_eq!(
        signals::signals(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .len(),
        7
    );
    // Missing data must not advance the checkpoint, even with stale coverage metadata.
    market::coverage(&pool, "BTC", "1m", 106 * 60000, 0)
        .await
        .unwrap();
    assert!(
        signals::process_series(&pool, &engine, "BTC", "1m")
            .await
            .is_err()
    );
    assert_eq!(
        signals::cursors(&pool, engine.id()).await.unwrap()[0]["last_open_time"],
        105 * 60000
    );
    market::coverage(&pool, "BTC", "1m", 105 * 60000, 0)
        .await
        .unwrap();
    let config = Config {
        version: "test-v2".into(),
        ..Default::default()
    };
    let second = Engine::new(config, vec![Box::new(Fixture)]).unwrap();
    signals::register(&pool, &second).await.unwrap();
    signals::process_series(&pool, &second, "BTC", "1m")
        .await
        .unwrap();
    assert_ne!(engine.id(), second.id());
    assert_eq!(
        signals::signals(&pool, second.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .len(),
        7
    );
    assert_eq!(
        signals::signals(&pool, engine.id(), "BTC", "1m", 500)
            .await
            .unwrap()
            .len(),
        7
    );
    // A fresh version catches up in bounded batches, with the same rolling-window state.
    let extended = candles(501);
    market::candles(&pool, &extended[106..], true)
        .await
        .unwrap();
    market::coverage(&pool, "BTC", "1m", 500 * 60000, 0)
        .await
        .unwrap();
    let third = Engine::new(
        Config {
            version: "test-v3".into(),
            ..Default::default()
        },
        vec![Box::new(Fixture)],
    )
    .unwrap();
    signals::register(&pool, &third).await.unwrap();
    assert!(
        !signals::process_series(&pool, &third, "BTC", "1m")
            .await
            .unwrap()
    );
    assert_eq!(
        signals::cursors(&pool, third.id()).await.unwrap()[0]["last_open_time"],
        499 * 60000
    );
    assert!(
        signals::process_series(&pool, &third, "BTC", "1m")
            .await
            .unwrap()
    );
    let closed: Vec<_> = extended
        .iter()
        .map(|c| ClosedCandle::new(c.clone(), true, i64::MAX).unwrap())
        .collect();
    let latest = signals::evaluations(&pool, third.id(), "BTC", "1m", 1)
        .await
        .unwrap();
    assert_eq!(
        latest[0],
        serde_json::to_value(third.evaluate(&closed).unwrap()).unwrap()
    );
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
