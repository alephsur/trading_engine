use anyhow::Result;
use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::{
    str::FromStr,
    sync::atomic::{AtomicUsize, Ordering},
};
use trading_testnet::{
    client::ExchangeTransport,
    executor,
    model::{self, Intent},
    signing::TestnetSigner,
    store,
};
use zeroize::Zeroizing;
struct Exchange {
    calls: AtomicUsize,
    lose_response: bool,
}
impl ExchangeTransport for Exchange {
    async fn send(&self, _: &Value) -> Result<Value> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.lose_response {
            anyhow::bail!("injected disconnect after exchange accepted request")
        };
        Ok(
            json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"filled":{"oid":42,"totalSz":"0.2","avgPx":"100"}}]}}}),
        )
    }
}
fn signer(n: u8) -> TestnetSigner {
    TestnetSigner::load(Zeroizing::new(format!("{}{:02x}", "00".repeat(31), n))).unwrap()
}
fn action(account: &str, id: &str) -> Value {
    json!({"type":"order","orders":[{"a":3,"b":false,"p":"100","s":"0.2","r":true,"t":{"limit":{"tif":"Ioc"}},"c":model::cloid(account,id,0)}],"grouping":"na"})
}
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema; fake exchange, no remote writes"]
async fn durable_nonces_at_most_once_dispatch_and_fail_closed_recovery() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "test_testnet_{}",
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
    let signer = signer(1);
    let state = json!({"assetPositions":[],"marginSummary":{"accountValue":"1000"}});
    let intent = Intent::Close {
        symbol: "BTC".into(),
        limit: None,
    };
    let (a, b) = tokio::join!(
        store::prepare(
            &pool,
            account,
            "request-01",
            &intent,
            action(account, "request-01"),
            &signer,
            &state
        ),
        store::prepare(
            &pool,
            account,
            "request-01",
            &intent,
            action(account, "request-01"),
            &signer,
            &state
        )
    );
    assert_ne!(a.unwrap(), b.unwrap());
    assert!(
        store::existing(
            &pool,
            account,
            "request-01",
            &Intent::Cancel {
                target: "request-99".into()
            }
        )
        .await
        .is_err()
    );
    let lost = Exchange {
        calls: AtomicUsize::new(0),
        lose_response: true,
    };
    let (a, b) = tokio::join!(
        executor::dispatch(&lost, &pool, account, "request-01"),
        executor::dispatch(&lost, &pool, account, "request-01")
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(lost.calls.load(Ordering::SeqCst), 1);
    let report = store::report(&pool, account).await.unwrap();
    assert_eq!(report["requests"][0]["state"], "uncertain");
    assert!(report.to_string().find("signature").is_none());
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    executor::dispatch(&lost, &pool, account, "request-01")
        .await
        .unwrap();
    assert_eq!(lost.calls.load(Ordering::SeqCst), 1);
    // Two distinct requests receive distinct monotonic nonces, even concurrently.
    let (a, b) = tokio::join!(
        store::prepare(
            &pool,
            account,
            "request-02",
            &intent,
            action(account, "request-02"),
            &signer,
            &state
        ),
        store::prepare(
            &pool,
            account,
            "request-03",
            &intent,
            action(account, "request-03"),
            &signer,
            &state
        )
    );
    a.unwrap();
    b.unwrap();
    let nonces: Vec<i64> = sqlx::query_scalar("SELECT nonce FROM testnet_requests ORDER BY nonce")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(nonces.len(), 3);
    assert!(nonces.windows(2).all(|w| w[0] < w[1]));
    // Failure to claim must prevent any network call.
    sqlx::query("ALTER TABLE testnet_requests ADD CONSTRAINT reject_claim CHECK(command_id<>'request-02' OR state<>'uncertain')").execute(&pool).await.unwrap();
    let exchange = Exchange {
        calls: AtomicUsize::new(0),
        lose_response: false,
    };
    assert!(
        executor::dispatch(&exchange, &pool, account, "request-02")
            .await
            .is_err()
    );
    assert_eq!(exchange.calls.load(Ordering::SeqCst), 0);
    sqlx::query("ALTER TABLE testnet_requests DROP CONSTRAINT reject_claim")
        .execute(&pool)
        .await
        .unwrap();
    // Exchange accepted, then PostgreSQL cannot save acknowledgement. Never send again.
    sqlx::query(
        "ALTER TABLE testnet_requests ADD CONSTRAINT reject_ack CHECK(state<>'acknowledged')",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        executor::dispatch(&exchange, &pool, account, "request-02")
            .await
            .is_err()
    );
    assert_eq!(exchange.calls.load(Ordering::SeqCst), 1);
    sqlx::query("ALTER TABLE testnet_requests DROP CONSTRAINT reject_ack")
        .execute(&pool)
        .await
        .unwrap();
    executor::dispatch(&exchange, &pool, account, "request-02")
        .await
        .unwrap();
    assert_eq!(exchange.calls.load(Ordering::SeqCst), 1);
    sqlx::query("UPDATE testnet_requests SET expires_ms=0 WHERE command_id='request-03'")
        .execute(&pool)
        .await
        .unwrap();
    executor::dispatch(&exchange, &pool, account, "request-03")
        .await
        .unwrap();
    assert_eq!(exchange.calls.load(Ordering::SeqCst), 1);
    let state3: String =
        sqlx::query_scalar("SELECT state FROM testnet_requests WHERE command_id='request-03'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state3, "expired");
    // Uncertainty blocks entries and cannot be evaded by choosing a new command id.
    let entry:Intent=serde_json::from_value(json!({"kind":"entry","symbol":"BTC","buy":true,"quantity":"0.2","limit":"100","stop":"99","take_profit":"102"})).unwrap();
    assert!(
        store::prepare(
            &pool,
            account,
            "entry-0001",
            &entry,
            action(account, "entry-0001"),
            &signer,
            &state
        )
        .await
        .is_err()
    );
    // API nonce ownership cannot be switched to another account.
    assert!(
        store::prepare(
            &pool,
            "other-account",
            "request-04",
            &intent,
            action(account, "request-04"),
            &signer,
            &state
        )
        .await
        .is_err()
    );
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}

struct ReadExchange {
    account: String,
    responses: std::sync::Mutex<Value>,
}
impl trading_testnet::client::InfoTransport for ReadExchange {
    async fn info(&self, request: Value) -> Result<Value> {
        anyhow::ensure!(
            request["user"] == self.account,
            "queries must use master account"
        );
        let values = self.responses.lock().unwrap();
        let kind = request["type"].as_str().unwrap();
        if kind == "orderStatus" {
            Ok(values["orders"][request["oid"].as_str().unwrap()].clone())
        } else {
            Ok(values[kind].clone())
        }
    }
}
fn remote_order(cloid: &str, oid: i64, buy: bool, status: &str, remaining: &str) -> Value {
    json!({"status":"order","order":{"status":status,"order":{"coin":"BTC","cloid":cloid,"oid":oid,"side":if buy{"B"}else{"A"},"origSz":"0.2","sz":remaining}}})
}
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema and deterministic exchange reads"]
async fn reconciliation_persists_unique_fills_and_detects_account_divergence() {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "test_reconcile_{}",
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
    let account = "0x2222222222222222222222222222222222222222";
    let key = signer(2);
    let state = json!({"assetPositions":[],"marginSummary":{"accountValue":"1000"}});
    let intent:Intent=serde_json::from_value(json!({"kind":"entry","symbol":"BTC","buy":true,"quantity":"0.2","limit":"100","stop":"99","take_profit":"102"})).unwrap();
    let id = "entry-0001";
    let cloid = model::cloid(account, id, 0);
    let mut wire = action(account, id);
    wire["orders"][0]["r"] = json!(false);
    wire["orders"][0]["b"] = json!(true);
    store::prepare(&pool, account, id, &intent, wire, &key, &state)
        .await
        .unwrap();
    store::claim(&pool, account, id).await.unwrap();
    let fill = json!({"coin":"BTC","oid":42,"tid":1,"side":"B","sz":"0.2","px":"100","fee":"0.009","feeToken":"USDC"});
    let exchange = ReadExchange {
        account: account.into(),
        responses: std::sync::Mutex::new(
            json!({"orders":{cloid.clone():remote_order(&cloid,42,true,"filled","0")},"userFillsByTime":[fill.clone(),fill],"clearinghouseState":{"assetPositions":[{"position":{"coin":"BTC","szi":"0.2"}}],"marginSummary":{"accountValue":"999.99"}},"openOrders":[]}),
        ),
    };
    let report = executor::reconcile(&exchange, &pool, account)
        .await
        .unwrap();
    assert_eq!(report["latest_snapshot"]["positions_reconciled"], true);
    assert_eq!(report["requests"][0]["reconciliation"]["complete"], true);
    assert_eq!(
        report["requests"][0]["reconciliation"]["safe_for_entry"],
        false
    );
    executor::reconcile(&exchange, &pool, account)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM testnet_fills")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    exchange.responses.lock().unwrap()["clearinghouseState"]["assetPositions"][0]["position"]["szi"] =
        json!("0.3");
    assert_eq!(
        executor::reconcile(&exchange, &pool, account)
            .await
            .unwrap()["latest_snapshot"]["positions_reconciled"],
        false
    );
    // If an accepted fill changes on replay, the transaction is rejected rather than overwritten.
    exchange.responses.lock().unwrap()["userFillsByTime"][0]["fee"] = json!("123");
    assert!(
        executor::reconcile(&exchange, &pool, account)
            .await
            .is_err()
    );
    // Daily loss is latched even when no new order is written, and recovery does not reset it.
    sqlx::query("UPDATE testnet_requests SET state='rejected'")
        .execute(&pool)
        .await
        .unwrap();
    let low = json!({"marginSummary":{"accountValue":"980"}});
    assert!(
        store::prepare(
            &pool,
            account,
            "entry-0002",
            &intent,
            action(account, "entry-0002"),
            &key,
            &low
        )
        .await
        .is_err()
    );
    assert!(
        store::prepare(
            &pool,
            account,
            "entry-0003",
            &intent,
            action(account, "entry-0003"),
            &key,
            &state
        )
        .await
        .is_err()
    );
    let latched: bool = sqlx::query_scalar("SELECT loss_latched FROM testnet_days")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(latched);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
