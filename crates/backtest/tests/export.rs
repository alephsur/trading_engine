use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use trading_backtest::data::Event;
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema"]
async fn export_preserves_availability_and_is_repeatable_read_only() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = PgPoolOptions::new().connect(&url).await.unwrap();
    let schema = format!(
        "test_export_{}",
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
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let start = trading_domain::data::now_ms() / 3_600_000 * 3_600_000 - 3_600_000;
    let end = start + 120_000;
    let markets: Vec<_> = trading_domain::SYMBOLS
        .iter()
        .enumerate()
        .map(
            |(i, s)| json!({"asset_index":i,"metadata":{"name":s,"szDecimals":3,"maxLeverage":10}}),
        )
        .collect();
    sqlx::query("INSERT INTO events(kind,payload,received_at) VALUES ('metadata',$1,to_timestamp(($2::double precision-10000)/1000))").bind(json!(markets)).bind(start).execute(&pool).await.unwrap();
    let candle = json!({"t":0,"T":59999,"s":"BTC","i":"1m","o":"100","h":"101","l":"99","c":"100","v":"1","n":1});
    sqlx::query("INSERT INTO candles(symbol,interval,open_time,close_time,trade_count,finalized,payload,received_at) VALUES ('BTC','1m',0,59999,1,true,$1,to_timestamp(($2::double precision+5000)/1000))").bind(candle).bind(start).execute(&pool).await.unwrap();
    let book = json!({"coin":"BTC","time":start,"levels":[[{"px":"100","sz":"1","n":1}],[{"px":"100.01","sz":"1","n":1}]]});
    sqlx::query("INSERT INTO books(symbol,exchange_time,payload,received_at) VALUES ('BTC',$1,$2,to_timestamp(($1::double precision+1234)/1000))").bind(start).bind(book).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO oracle_observations(symbol,observed_at,price) VALUES ('BTC',$1,'100')",
    )
    .bind(start)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO funding_rates(symbol,hour,rate,available_at) VALUES ('BTC',$1,'0.0001',to_timestamp(($1::double precision+8000)/1000))").bind(start).execute(&pool).await.unwrap();
    let first = trading_backtest::export::dataset(&pool, start, end)
        .await
        .unwrap();
    assert_eq!(first.records.len(), 5);
    for r in &first.records {
        match r.event {
            Event::Candle { .. } => assert_eq!(r.available_at, start + 5000),
            Event::Book { .. } => assert_eq!(r.available_at, start + 1234),
            Event::Funding { .. } => assert_eq!(r.available_at, start + 8000),
            _ => {}
        }
    }
    let second = trading_backtest::export::dataset(&pool, start, end)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    let counts:Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT count(*) FROM events),(SELECT count(*) FROM candles),(SELECT count(*) FROM books),(SELECT count(*) FROM funding_rates),(SELECT count(*) FROM paper_accounts))").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, json!([1, 1, 1, 1, 0]));
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
