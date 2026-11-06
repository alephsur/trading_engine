use serde_json::json;
use sqlx::{
    Row,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::str::FromStr;
use trading_domain::data::{Book, Candle, Trade};
use trading_storage::market;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; uses an isolated temporary schema"]
async fn replay_ordering_gaps_and_finalization() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "test_market_{}",
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

    let mut candle: Candle = serde_json::from_value(json!({"t":60000,"T":119999,"s":"BTC","i":"1m","o":"2","h":"3","l":"1","c":"2","v":"0.123456789123456789","n":2})).unwrap();
    market::candles(&pool, &[candle.clone()], false)
        .await
        .unwrap();
    candle.n = 1;
    candle.c = "1".parse().unwrap();
    market::candles(&pool, &[candle.clone()], false)
        .await
        .unwrap();
    assert!(
        market::read_candles(&pool, "BTC", "1m", 100, true)
            .await
            .unwrap()
            .is_empty()
    );
    let rows = market::read_candles(&pool, "BTC", "1m", 100, false)
        .await
        .unwrap();
    assert_eq!(rows[0]["candle"]["n"], 2);
    assert_eq!(rows[0]["candle"]["v"], "0.123456789123456789");
    candle.n = 3;
    market::candles(&pool, &[candle.clone()], true)
        .await
        .unwrap();
    candle.n = 999;
    market::candles(&pool, &[candle.clone()], false)
        .await
        .unwrap();
    market::candles(&pool, &[candle.clone()], true)
        .await
        .unwrap();
    let rows = market::read_candles(&pool, "BTC", "1m", 100, true)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["candle"]["n"], 3);
    assert_eq!(
        market::missing(&pool, "BTC", "1m", 0, 120000, 60000)
            .await
            .unwrap(),
        vec![0, 120000]
    );
    candle.t = 120000;
    candle.end = 179999;
    market::candles(&pool, &[candle.clone()], true)
        .await
        .unwrap();
    assert_eq!(
        market::missing(&pool, "BTC", "1m", 0, 120000, 60000)
            .await
            .unwrap(),
        vec![0]
    );
    candle.t = 0;
    candle.end = 59999;
    market::candles(&pool, &[candle], true).await.unwrap();
    assert!(
        market::missing(&pool, "BTC", "1m", 0, 120000, 60000)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(market::anchor(&pool, "BTC", "1m", 0).await.unwrap(), 0);
    assert_eq!(market::anchor(&pool, "BTC", "1m", 60000).await.unwrap(), 0);

    let current_open = trading_domain::data::now_ms() / 60_000 * 60_000;
    let active: Candle = serde_json::from_value(json!({"t":current_open,"T":current_open+59999,"s":"BTC","i":"1m","o":"2","h":"3","l":"1","c":"2","v":"1","n":1})).unwrap();
    assert!(market::candles(&pool, &[active], true).await.is_err());

    let mut trade: Trade = serde_json::from_value(
        json!({"coin":"BTC","time":1000,"tid":99,"side":"B","px":"1","sz":"0.1","hash":"test"}),
    )
    .unwrap();
    market::trades(&pool, &[trade.clone(), trade.clone()])
        .await
        .unwrap();
    // tid alone is not globally unique: the block timestamp is part of the key.
    trade.time = 2000;
    market::trades(&pool, &[trade]).await.unwrap();
    assert_eq!(
        market::read_trades(&pool, "BTC", 100).await.unwrap().len(),
        2
    );
    let mut book: Book = serde_json::from_value(json!({"coin":"BTC","time":2000,"levels":[[{"px":"10","sz":"1","n":1}],[{"px":"11","sz":"1","n":1}]]})).unwrap();
    market::book(&pool, &book).await.unwrap();
    market::book(&pool, &book).await.unwrap();
    book.time = 1000;
    market::book(&pool, &book).await.unwrap();
    assert_eq!(
        market::read_book(&pool, "BTC").await.unwrap().unwrap()["time"],
        2000
    );
    let count = sqlx::query("SELECT count(*) AS n FROM books")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.get::<i64, _>("n"), 2);
    // Reopen the pool to verify durable state, not an in-memory deduplication cache.
    pool.close().await;
    let options = PgConnectOptions::from_str(&url)
        .unwrap()
        .options([("search_path", schema.as_str())]);
    let reopened = PgPoolOptions::new().connect_with(options).await.unwrap();
    assert_eq!(
        market::read_trades(&reopened, "BTC", 100)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        market::anchor(&reopened, "BTC", "1m", 60000).await.unwrap(),
        0
    );
    reopened.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
