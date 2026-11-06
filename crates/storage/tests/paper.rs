use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use trading_paper::{Config, dec};
use trading_storage::paper;
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema"]
async fn paper_atomicity_concurrency_restart_and_corrupt_ledger() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "test_paper_{}",
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
    paper::initialize(&pool, Config::default()).await.unwrap();
    let now = trading_domain::data::now_ms();
    let markets: Vec<trading_domain::ListedMarket> = trading_domain::SYMBOLS
        .iter()
        .enumerate()
        .map(|(i, s)| {
            serde_json::from_value(
                json!({"asset_index":i,"metadata":{"name":s,"szDecimals":3,"maxLeverage":10}}),
            )
            .unwrap()
        })
        .collect();
    for s in trading_domain::SYMBOLS {
        let book=serde_json::from_value(json!({"coin":s,"time":now,"levels":[[{"px":"100","sz":"1000","n":1}],[{"px":"100.01","sz":"1000","n":1}]]})).unwrap();
        trading_storage::market::book(&pool, &book).await.unwrap();
        let context = serde_json::from_value(json!({"coin":s,"ctx":{"oraclePx":"100"}})).unwrap();
        paper::oracle(&pool, &context).await.unwrap();
    }
    sqlx::query("INSERT INTO signal_configs(config_id,manifest) VALUES ('fixture','{}')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO evaluations(config_id,symbol,interval,open_time,payload) VALUES ('fixture','BTC','1m',0,$1)").bind(json!({"indicators":{"atr_14":"1"}})).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO signals(config_id,symbol,interval,open_time,strategy,direction,payload) VALUES ('fixture','BTC','1m',0,'fixture','long',$1)").bind(json!({"close_time":now})).execute(&pool).await.unwrap();
    // Failure after decisions and orders have been inserted must roll everything back.
    sqlx::query("ALTER TABLE paper_accounts ADD CONSTRAINT injected_failure CHECK ((state->>'sequence')::bigint=0)").execute(&pool).await.unwrap();
    assert!(paper::tick(&pool, "fixture", true, &markets).await.is_err());
    assert!(
        paper::ledger(&pool, "orders", 10)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        paper::ledger(&pool, "decisions", 10)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(paper::read(&pool).await.unwrap().cash, dec("10000"));
    sqlx::query("ALTER TABLE paper_accounts DROP CONSTRAINT injected_failure")
        .execute(&pool)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        paper::tick(&pool, "fixture", true, &markets),
        paper::tick(&pool, "fixture", true, &markets)
    );
    a.unwrap();
    b.unwrap();
    let saved = paper::read(&pool).await.unwrap();
    assert_eq!(saved.positions.len(), 1);
    assert!(saved.cash < dec("10000"));
    assert_eq!(
        paper::ledger(&pool, "orders", 10)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        paper::ledger(&pool, "decisions", 10)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    paper::initialize(&pool, Config::default()).await.unwrap();
    paper::tick(&pool, "fixture", true, &markets).await.unwrap();
    let recovered = paper::read(&pool).await.unwrap();
    assert_eq!(saved.cash, recovered.cash);
    assert_eq!(saved.positions[0].quantity, recovered.positions[0].quantity);
    assert!(
        paper::initialize(
            &pool,
            Config {
                capital: dec("9000"),
                ..Config::default()
            }
        )
        .await
        .is_err()
    );
    // A lower sequence ID committed later must still be consumed (no high-water cursor).
    sqlx::query("INSERT INTO signals(event_id,config_id,symbol,interval,open_time,strategy,direction,payload) OVERRIDING SYSTEM VALUE VALUES (0,'fixture','BTC','1m',0,'late','long',$1)").bind(json!({"close_time":now})).execute(&pool).await.unwrap();
    paper::tick(&pool, "fixture", true, &markets).await.unwrap();
    assert_eq!(
        paper::ledger(&pool, "decisions", 10)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // Ledger corruption blocks both startup and further transitions.
    sqlx::query("UPDATE paper_accounts SET state=jsonb_set(state,'{cash}','\"10000\"')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(paper::initialize(&pool, Config::default()).await.is_err());
    assert!(paper::tick(&pool, "fixture", true, &markets).await.is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM paper_orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let _: Value = paper::ledger(&pool, "events", 10).await.unwrap();
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
