use serde_json::{Value, json};
use trading_domain::data::{Book, now_ms};
use trading_paper::dec;
use trading_quant::{Evaluation, strategies::Direction};
use trading_testnet::{
    automation,
    model::{self, Intent},
    observer,
    signing::TestnetSigner,
    store,
};
use zeroize::Zeroizing;
fn config() -> observer::Config {
    serde_json::from_str(include_str!("../../../config/testnet-observer.json")).unwrap()
}
fn evaluation(now: i64) -> Evaluation {
    let last = trading_domain::data::last_closed_open(now, 60000);
    let cs=json!((0..100).map(|n| {let t=last-(99-n)*60000;json!({"t":t,"T":t+59999,"s":"BTC","i":"1m","o":"100","h":"100.1","l":"99.9","c":"100","v":"10","n":10})}).collect::<Vec<_>>());
    let mut e = observer::evaluate(cs, &config(), now).unwrap();
    // Explicit synthetic signal fixture; never used against the real exchange.
    e.decisions[0].direction = Direction::Long;
    e
}
fn markets() -> Vec<trading_domain::ListedMarket> {
    trading_domain::select_markets(serde_json::from_value(json!({"universe":[{"name":"BTC","szDecimals":3,"maxLeverage":10},{"name":"ETH","szDecimals":3,"maxLeverage":10},{"name":"SOL","szDecimals":3,"maxLeverage":10},{"name":"HYPE","szDecimals":3,"maxLeverage":10}]})).unwrap()).unwrap()
}
fn book(now: i64) -> Book {
    serde_json::from_value(json!({"coin":"BTC","time":now,"levels":[[{"px":"100","sz":"1000","n":1}],[{"px":"100.01","sz":"1000","n":1}]]})).unwrap()
}
fn state() -> Value {
    json!({"assetPositions":[],"marginSummary":{"accountValue":"500"}})
}
#[test]
fn signal_freshness_leverage_and_atr_protection() {
    let now = 12_005_000;
    let mut e = evaluation(now);
    assert!(automation::signal(&e, e.close_time + 1, now).is_err());
    assert!(automation::signal(&e, 0, e.close_time + 120001).is_err());
    assert!(automation::signal(&e, 0, e.close_time).is_err());
    assert!(
        automation::leverage(
            &json!({"user":"account","coin":"BTC","leverage":{"type":"isolated","value":1}}),
            "account"
        )
        .is_ok()
    );
    for (kind, value) in [("cross", 1), ("isolated", 2)] {
        assert!(
            automation::leverage(
                &json!({"user":"account","coin":"BTC","leverage":{"type":kind,"value":value}}),
                "account"
            )
            .is_err()
        );
    }
    for direction in [Direction::Long, Direction::Short] {
        e.decisions[0].direction = direction;
        let intent = automation::plan(&e, &markets(), &book(now), &state(), 0, now).unwrap();
        let action = model::entry(
            &intent,
            "account",
            "test-auto-001",
            &markets(),
            &book(now),
            &state(),
            now,
        )
        .unwrap();
        assert_eq!(action["orders"][1]["r"], true);
        assert_eq!(action["orders"][2]["r"], true);
        if let Intent::Entry {
            quantity,
            limit,
            stop,
            ..
        } = intent
        {
            assert!(quantity * limit <= dec("20"));
            assert!((limit - stop).abs() / limit >= dec("0.0049"));
        } else {
            panic!("expected entry")
        }
    }
    e.indicators.as_mut().unwrap().atr_14 = Some(dec("10"));
    assert!(automation::plan(&e, &markets(), &book(now), &state(), 0, now).is_err());
    e.decisions[0].direction = Direction::Hold;
    assert!(automation::signal(&e, 0, now).is_err());
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; synthetic signals and dummy signer; no network writes"]
async fn single_attempt_survives_restart_and_pause_blocks_dispatch() {
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::str::FromStr;
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "auto_{}",
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
    let account = "0x2222222222222222222222222222222222222222";
    let now = now_ms();
    let e = evaluation(now);
    let intent = automation::plan(&e, &markets(), &book(now), &state(), 0, now).unwrap();
    for id in ["session-001", "session-002"] {
        automation::start(&pool, account, id, &config())
            .await
            .unwrap();
        // Move the fixture's activation clock before the synthetic closed candle.
        sqlx::query("UPDATE testnet_auto_sessions SET started_ms=$1 WHERE session_id=$2")
            .bind(e.close_time - 1000)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO testnet_observations(account,session_id,close_ms,observed_ms,evaluation) VALUES($1,$2,$3,$4,$5)").bind(account).bind(id).bind(e.close_time).bind(now).bind(serde_json::to_value(&e).unwrap()).execute(&pool).await.unwrap();
    }
    let (a, b) = tokio::join!(
        automation::claim(&pool, account, "session-001", &e, &intent),
        automation::claim(&pool, account, "session-001", &e, &intent)
    );
    let a = a.unwrap();
    let b = b.unwrap();
    assert_ne!(a.is_some(), b.is_some());
    let command = a.or(b).unwrap();
    assert!(
        automation::claim(&pool, account, "session-002", &e, &intent)
            .await
            .unwrap()
            .is_none()
    );
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    assert!(
        automation::claim(&pool, account, "session-001", &e, &intent)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        automation::status(&pool, account, "session-001")
            .await
            .unwrap()["entry_budget_consumed"],
        true
    );
    let signer = TestnetSigner::load(Zeroizing::new(format!("{}01", "00".repeat(31)))).unwrap();
    let now = now_ms();
    let action = model::entry(
        &intent,
        account,
        &command,
        &markets(),
        &book(now),
        &state(),
        now,
    )
    .unwrap();
    store::prepare(&pool, account, &command, &intent, action, &signer, &state())
        .await
        .unwrap();
    observer::pause(&pool, account, "session-001")
        .await
        .unwrap();
    assert!(
        store::claim(&pool, account, &command)
            .await
            .unwrap()
            .is_none()
    );
    let transport: String =
        sqlx::query_scalar("SELECT state FROM testnet_requests WHERE command_id=$1")
            .bind(&command)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(transport, "expired");
    assert!(
        automation::claim(&pool, account, "session-001", &e, &intent)
            .await
            .is_err()
    );
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
