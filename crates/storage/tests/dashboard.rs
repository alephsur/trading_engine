use serde_json::json;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;
use trading_paper::{Config, dec};
use trading_storage::dashboard::{self, Action, Command};
use trading_storage::paper;
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema"]
async fn dashboard_controls_survive_restart_and_close_only_on_fresh_liquidity() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "test_dashboard_{}",
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
    paper::tick(&pool, "fixture", true, &markets).await.unwrap();
    let position_id = paper::read(&pool).await.unwrap().positions[0].id;
    let pause = Command {
        command_id: "pause-command-1".into(),
        action: Action::Pause,
    };
    let (first, second) = tokio::join!(
        dashboard::command(&pool, &pause),
        dashboard::command(&pool, &pause)
    );
    assert_eq!(first.unwrap(), second.unwrap());
    assert!(
        dashboard::command(
            &pool,
            &Command {
                command_id: pause.command_id.clone(),
                action: Action::Resume
            }
        )
        .await
        .is_err()
    );
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    paper::initialize(&pool, Config::default()).await.unwrap();
    assert!(paper::read(&pool).await.unwrap().entries_paused);
    let close = Command {
        command_id: "close-command-1".into(),
        action: Action::ClosePosition { position_id },
    };
    dashboard::command(&pool, &close).await.unwrap();
    sqlx::query("DELETE FROM books")
        .execute(&pool)
        .await
        .unwrap();
    paper::tick(&pool, "fixture", false, &markets)
        .await
        .unwrap();
    let snapshot = dashboard::snapshot(&pool).await.unwrap();
    assert_eq!(snapshot["positions"][0]["position"]["closing"], true);
    assert!(snapshot["positions"][0]["mark"].is_null());
    assert!(snapshot["metrics"]["ending_equity"].is_null());
    assert_eq!(snapshot["monitor"]["complete"], false);
    let time = trading_domain::data::now_ms() + 1;
    let book=serde_json::from_value(json!({"coin":"BTC","time":time,"levels":[[{"px":"100","sz":"1000","n":1}],[{"px":"100.01","sz":"1000","n":1}]]})).unwrap();
    trading_storage::market::book(&pool, &book).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(3)).await;
    paper::tick(&pool, "fixture", false, &markets)
        .await
        .unwrap();
    let state = paper::read(&pool).await.unwrap();
    assert!(state.positions.is_empty());
    assert!(state.entries_paused);
    assert!(state.cash < dec("10000"));
    let snapshot = dashboard::snapshot(&pool).await.unwrap();
    assert_eq!(snapshot["metrics"]["closed_trades"], 1);
    assert_eq!(snapshot["trade_history"][0]["settled"], true);
    assert!(snapshot["metrics"]["win_rate"].is_null());
    assert!(snapshot["metrics"]["sharpe_daily"].is_null());
    // Retrying after execution returns the original acknowledgement, not a second exit.
    dashboard::command(&pool, &close).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM paper_orders")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    dashboard::command(
        &pool,
        &Command {
            command_id: "resume-command-1".into(),
            action: Action::Resume,
        },
    )
    .await
    .unwrap();
    assert!(!paper::read(&pool).await.unwrap().entries_paused);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
