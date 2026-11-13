use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{
    str::FromStr,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use trading_domain::data::now_ms;
use trading_testnet::{
    automation,
    client::{ExchangeTransport, InfoTransport},
    executor,
    model::Intent,
    observer,
    signing::TestnetSigner,
    store, supervision,
};
use zeroize::Zeroizing;

const ACCOUNT: &str = "0x2222222222222222222222222222222222222222";
fn cfg() -> observer::Config {
    serde_json::from_str(include_str!(
        "../../../config/testnet-confirmed-breakout.json"
    ))
    .unwrap()
}
fn intent() -> Intent {
    serde_json::from_value(json!({"kind":"entry","symbol":"BTC","buy":true,"quantity":"0.2","limit":"100.02","stop":"99","take_profit":"102"})).unwrap()
}
fn signer() -> TestnetSigner {
    TestnetSigner::load(Zeroizing::new(format!("{}01", "00".repeat(31)))).unwrap()
}
fn markets() -> Value {
    json!({"universe":trading_domain::SYMBOLS.map(|s| json!({"name":s,"szDecimals":3,"maxLeverage":10}))})
}

struct Exchange {
    data: Mutex<Value>,
    unavailable: AtomicBool,
    lose_close_response: AtomicBool,
    missing_stop: bool,
    entry_calls: AtomicUsize,
    close_calls: AtomicUsize,
}
impl Exchange {
    fn new(missing_stop: bool) -> Self {
        Self {
            data: Mutex::new(
                json!({"orders":{},"fills":[],"position":"0","equity":"500","funding":[]}),
            ),
            unavailable: AtomicBool::new(false),
            lose_close_response: AtomicBool::new(false),
            missing_stop,
            entry_calls: AtomicUsize::new(0),
            close_calls: AtomicUsize::new(0),
        }
    }
}
impl InfoTransport for Exchange {
    async fn info(&self, request: Value) -> Result<Value> {
        ensure!(
            !self.unavailable.load(Ordering::SeqCst),
            "injected network failure"
        );
        let data = self.data.lock().unwrap();
        let positions = if data["position"] == "0" {
            json!([])
        } else {
            json!([{"position":{"coin":"BTC","szi":data["position"],"positionValue":"10","leverage":{"type":"isolated","value":1}}}])
        };
        match request["type"].as_str().unwrap() {
            "meta" => Ok(markets()),
            "clearinghouseState" => Ok(
                json!({"time":now_ms(),"assetPositions":positions,"marginSummary":{"accountValue":data["equity"]}}),
            ),
            "candleSnapshot" => {
                let first = request["req"]["startTime"].as_i64().unwrap();
                Ok(json!((0..100).map(|n| {let t=first+n*3_600_000;json!({"t":t,"T":t+3_599_999,"s":"BTC","i":"1h","o":"100","h":"100.1","l":"99.9","c":"100","v":"10","n":10})}).collect::<Vec<_>>()))
            }
            "userFunding" => Ok(data["funding"].clone()),
            "userFillsByTime" => Ok(data["fills"].clone()),
            "orderStatus" => Ok(data["orders"]
                .get(request["oid"].as_str().unwrap())
                .cloned()
                .unwrap_or(json!({"status":"unknownOid"}))),
            "openOrders" | "frontendOpenOrders" => Ok(json!(
                data["orders"]
                    .as_object()
                    .unwrap()
                    .values()
                    .filter(|v| v["order"]["status"] == "open")
                    .map(|v| v["order"]["order"].clone())
                    .collect::<Vec<_>>()
            )),
            "l2Book" => Ok(
                json!({"coin":"BTC","time":now_ms(),"levels":[[{"px":"100","sz":"1000","n":1}],[{"px":"100.01","sz":"1000","n":1}]]}),
            ),
            "activeAssetData" => {
                Ok(json!({"user":ACCOUNT,"coin":"BTC","leverage":{"type":"isolated","value":1}}))
            }
            _ => anyhow::bail!("unexpected test request"),
        }
    }
}
impl ExchangeTransport for Exchange {
    async fn send(&self, request: &Value) -> Result<Value> {
        let mut data = self.data.lock().unwrap();
        let orders = request["action"]["orders"].as_array().unwrap();
        let close = orders[0]["r"] == true && orders.len() == 1;
        if close {
            self.close_calls.fetch_add(1, Ordering::SeqCst);
            for v in data["orders"].as_object_mut().unwrap().values_mut() {
                if v["order"]["status"] == "open" {
                    v["order"]["status"] = json!("reduceOnlyCanceled");
                }
            }
            data["position"] = json!("0");
        } else {
            self.entry_calls.fetch_add(1, Ordering::SeqCst);
            data["position"] = json!("0.1"); // IOC partial fill; children can be absent.
        }
        let mut statuses = vec![];
        for (leg, o) in orders.iter().enumerate() {
            let oid = 100 + data["orders"].as_object().unwrap().len() as i64;
            let filled = leg == 0;
            let rejected = !filled && self.missing_stop;
            let status = if filled {
                "filled"
            } else if rejected {
                "rejected"
            } else {
                "open"
            };
            let size = if filled {
                if close {
                    "0".to_owned()
                } else {
                    (trading_testnet::model::decimal(&o["s"])? - trading_paper::dec("0.1"))
                        .to_string()
                }
            } else {
                o["s"].as_str().unwrap().to_owned()
            };
            let remote = json!({"status":"order","order":{"status":status,"order":{
                "coin":"BTC","cloid":o["c"],"oid":oid,"side":if o["b"]==true {"B"}else{"A"},
                "origSz":o["s"],"sz":size,"reduceOnly":o["r"],"isTrigger":!filled,
                "orderType":if leg==1 {"Stop Market"} else if leg==2 {"Take Profit Market"} else {"Limit"},
                "triggerPx":o["t"]["trigger"]["triggerPx"]}}});
            data["orders"][o["c"].as_str().unwrap()] = remote;
            if filled {
                data["fills"].as_array_mut().unwrap().push(json!({"coin":"BTC","oid":oid,"tid":oid,"side":if close {"A"}else{"B"},"sz":"0.1","px":"100","fee":"0.0045","feeToken":"USDC"}));
                statuses.push(json!({"filled":{"oid":oid,"totalSz":"0.1","avgPx":"100"}}));
            } else if rejected {
                statuses.push(json!({"error":"injected missing protection"}));
            } else {
                statuses.push(json!("waitingForTrigger"));
            }
        }
        if close && self.lose_close_response.load(Ordering::SeqCst) {
            anyhow::bail!("injected lost close response after acceptance");
        }
        Ok(json!({"status":"ok","response":{"type":"order","data":{"statuses":statuses}}}))
    }
}

async fn db(label: &str) -> (PgPool, PgPool, PgConnectOptions, String) {
    let url = std::env::var("TEST_DATABASE_URL").unwrap();
    let admin = trading_storage::connect(&url).await.unwrap();
    let schema = format!(
        "supervision_{label}_{}",
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
    (admin, pool, options, schema)
}
async fn drop_db(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
}
async fn prepare_entry(exchange: &Exchange, pool: &PgPool, id: &str) {
    let (action, state) = executor::preflight(exchange, pool, ACCOUNT, id, &intent())
        .await
        .unwrap();
    store::prepare(pool, ACCOUNT, id, &intent(), action, &signer(), &state)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema, dummy key and simulated exchange only"]
async fn partial_fill_missing_stop_closes_once_and_recovers_lost_response() {
    let (admin, pool, options, schema) = db("partial").await;
    let exchange = Exchange::new(true);
    let session = "breakout-session";
    automation::start(&pool, ACCOUNT, session, &cfg())
        .await
        .unwrap();
    supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    let entry = "entry-partial";
    prepare_entry(&exchange, &pool, entry).await;
    executor::dispatch(&exchange, &pool, ACCOUNT, entry)
        .await
        .unwrap();
    assert_eq!(exchange.entry_calls.load(Ordering::SeqCst), 1);
    let cycle = supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    assert!(
        cycle["assessment"]["blockers"]
            .as_array()
            .unwrap()
            .contains(&json!("missing_protective_stop"))
    );
    assert_eq!(cycle["assessment"]["emergency_symbols"], json!(["BTC"]));
    let mut unknown_children = cycle["report"].clone();
    unknown_children["requests"][0]["reconciliation"]["complete"] = json!(false);
    for leg in [1, 2] {
        unknown_children["requests"][0]["reconciliation"]["orders"][leg] = json!({"known":false});
    }
    assert_eq!(
        supervision::assess(&unknown_children, &json!([]), now_ms()).unwrap()["emergency_symbols"],
        json!(["BTC"])
    );
    unknown_children["requests"][0]["reconciliation"]["orders"][0]["status"] = json!("open");
    assert!(supervision::assess(&unknown_children, &json!([]), now_ms()).unwrap()["positions"][0]["entry_command"].is_null());
    assert_eq!(
        observer::status(&pool, ACCOUNT, session).await.unwrap()["paused"],
        true
    );
    exchange.lose_close_response.store(true, Ordering::SeqCst);
    supervision::protect_once(&exchange, &pool, &signer(), ACCOUNT, entry, &cycle)
        .await
        .unwrap();
    assert_eq!(exchange.close_calls.load(Ordering::SeqCst), 1);
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    supervision::protect_once(&exchange, &pool, &signer(), ACCOUNT, entry, &cycle)
        .await
        .unwrap();
    assert_eq!(exchange.close_calls.load(Ordering::SeqCst), 1);
    let recovered = supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    assert_eq!(recovered["assessment"]["safe_to_stop"], true);
    let close = &recovered["report"]["requests"][0];
    assert_eq!(close["state"], "uncertain");
    assert_eq!(close["reconciliation"]["complete"], true);
    assert_eq!(close["reconciliation"]["all_terminal"], true);
    drop_db(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema and simulated exchange only"]
async fn monitor_latches_daily_loss_preserves_funding_and_blocks_stale_dispatch() {
    let (admin, pool, options, schema) = db("gates").await;
    let exchange = Exchange::new(false);
    automation::start(&pool, ACCOUNT, "gates-session", &cfg())
        .await
        .unwrap();
    assert!(
        supervision::status(&pool, ACCOUNT).await.unwrap()["entries_blocked"]
            .as_bool()
            .unwrap()
    );
    assert!(
        executor::submit(
            &exchange,
            &pool,
            &signer(),
            ACCOUNT,
            "blocked-entry",
            &intent()
        )
        .await
        .is_err()
    );
    supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    prepare_entry(&exchange, &pool, "stale-entry").await;
    sqlx::query("UPDATE testnet_supervision SET observed_ms=1")
        .execute(&pool)
        .await
        .unwrap();
    executor::dispatch(&exchange, &pool, ACCOUNT, "stale-entry")
        .await
        .unwrap();
    assert_eq!(exchange.entry_calls.load(Ordering::SeqCst), 0);
    exchange.data.lock().unwrap()["equity"] = json!("494");
    let cycle = supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    assert_eq!(cycle["assessment"]["daily_loss_latched"], true);
    pool.close().await;
    let pool = PgPoolOptions::new().connect_with(options).await.unwrap();
    exchange.data.lock().unwrap()["equity"] = json!("501");
    let cycle = supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    assert_eq!(cycle["assessment"]["daily_loss_latched"], true);
    assert!(
        executor::submit(
            &exchange,
            &pool,
            &signer(),
            ACCOUNT,
            "second-entry",
            &intent()
        )
        .await
        .is_err()
    );
    exchange.unavailable.store(true, Ordering::SeqCst);
    assert!(supervision::tick(&exchange, &pool, ACCOUNT).await.is_err());
    assert_eq!(
        supervision::status(&pool, ACCOUNT).await.unwrap()["entries_blocked"],
        true
    );
    exchange.unavailable.store(false, Ordering::SeqCst);
    let failure = supervision::status(&pool, ACCOUNT).await.unwrap()["last_failure"].clone();
    assert_eq!(failure["details"]["diagnostic"]["stage"], "funding");
    assert_eq!(
        failure["details"]["diagnostic"]["category"],
        "validation_or_internal_error"
    );
    assert!(!failure.to_string().contains("injected network failure"));
    let time = now_ms();
    exchange.data.lock().unwrap()["funding"] = json!([{"time":time,"hash":"fixture","delta":{"type":"funding","coin":"BTC","usdc":"-0.01"}}]);
    supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    assert_eq!(
        supervision::status(&pool, ACCOUNT).await.unwrap()["last_failure"],
        failure
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM testnet_funding")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    sqlx::query("UPDATE testnet_supervision SET funding_through_ms=$1")
        .bind(time)
        .execute(&pool)
        .await
        .unwrap();
    exchange.data.lock().unwrap()["funding"][0]["delta"]["usdc"] = json!("-99");
    assert!(supervision::tick(&exchange, &pool, ACCOUNT).await.is_err());
    assert_eq!(
        supervision::status(&pool, ACCOUNT).await.unwrap()["entries_blocked"],
        true
    );
    drop_db(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; isolated schema, synthetic observations and exchange"]
async fn paused_expired_session_keeps_monitoring_accepted_protections() {
    let (admin, pool, _, schema) = db("expiry").await;
    let exchange = Exchange::new(false);
    let session = "expired-session";
    automation::start(&pool, ACCOUNT, session, &cfg())
        .await
        .unwrap();
    supervision::tick(&exchange, &pool, ACCOUNT).await.unwrap();
    prepare_entry(&exchange, &pool, "protected-entry").await;
    executor::dispatch(&exchange, &pool, ACCOUNT, "protected-entry")
        .await
        .unwrap();
    observer::pause(&pool, ACCOUNT, session).await.unwrap();
    sqlx::query(
        "UPDATE testnet_observer_sessions SET started_ms=1,expires_ms=2 WHERE session_id=$1",
    )
    .bind(session)
    .execute(&pool)
    .await
    .unwrap();
    let result = automation::tick(&exchange, &pool, &signer(), ACCOUNT, session, &cfg())
        .await
        .unwrap();
    assert_eq!(result["expired"], true);
    assert_eq!(result["paused"], true);
    assert_eq!(
        result["supervision"]["assessment"]["positions"][0]["stop_confirmed"],
        true
    );
    assert_eq!(
        result["supervision"]["assessment"]["positions"][0]["target_confirmed"],
        true
    );
    assert_eq!(result["supervision"]["assessment"]["safe_to_stop"], false);
    let evidence: Value = sqlx::query_scalar("SELECT payload FROM testnet_events WHERE kind='protection_observed' ORDER BY id DESC LIMIT 1")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        evidence["assessment"]["positions"][0]["stop_confirmed"],
        true
    );
    assert_eq!(
        evidence["assessment"]["positions"][0]["target_confirmed"],
        true
    );
    assert!(
        !evidence["frontend_open_orders"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(exchange.entry_calls.load(Ordering::SeqCst), 1);
    assert_eq!(exchange.close_calls.load(Ordering::SeqCst), 0);
    // Direction, quantity, type and price must all match the accepted, owned stop.
    let report = executor::reconcile(&exchange, &pool, ACCOUNT)
        .await
        .unwrap();
    let frontend = exchange
        .info(json!({"type":"frontendOpenOrders","user":ACCOUNT}))
        .await
        .unwrap();
    for (field, value) in [
        ("reduceOnly", json!(false)),
        ("side", json!("B")),
        ("sz", json!("0.01")),
        ("triggerPx", json!("98")),
        ("orderType", json!("Take Profit Market")),
    ] {
        let mut altered = frontend.clone();
        let stop = altered
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|o| o["orderType"] == "Stop Market")
            .unwrap();
        stop[field] = value;
        assert_eq!(
            supervision::assess(&report, &altered, now_ms()).unwrap()["positions"][0]["stop_confirmed"],
            false
        );
    }
    let mut divergent = report.clone();
    divergent["latest_snapshot"]["positions_reconciled"] = json!(false);
    assert_eq!(
        supervision::assess(&divergent, &json!([]), now_ms()).unwrap()["emergency_symbols"],
        json!([])
    );
    drop_db(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; synthetic observations only, no network"]
async fn hourly_claims_deduplicate_per_market_and_reject_session_mismatch() {
    use trading_quant::{
        Evaluation,
        strategies::{Decision, Direction},
    };
    let (admin, pool, _, schema) = db("claims").await;
    let close = now_ms() - 1000;
    let mut signals = vec![];
    for (session, symbol) in [
        ("claims-btc-1", "BTC"),
        ("claims-eth-1", "ETH"),
        ("claims-btc-2", "BTC"),
    ] {
        let mut config = cfg();
        config.symbol = symbol.into();
        automation::start(&pool, ACCOUNT, session, &config)
            .await
            .unwrap();
        sqlx::query("UPDATE testnet_auto_sessions SET started_ms=$1 WHERE session_id=$2")
            .bind(close - 1)
            .bind(session)
            .execute(&pool)
            .await
            .unwrap();
        let e = Evaluation {
            config_id: "synthetic-fixture".into(),
            symbol: symbol.into(),
            interval: "1h".into(),
            open_time: close - 3_599_999,
            close_time: close,
            window_start: close - 360_000_000,
            input_hash: "synthetic".into(),
            indicators: None,
            decisions: vec![Decision {
                strategy: "confirmed_breakout_1h_v1".into(),
                direction: Direction::Long,
                reason: "synthetic fixture".into(),
            }],
            analysis_only: true,
        };
        sqlx::query("INSERT INTO testnet_observations(account,session_id,close_ms,observed_ms,evaluation) VALUES($1,$2,$3,$4,$5)").bind(ACCOUNT).bind(session).bind(close).bind(now_ms()).bind(json!(e)).execute(&pool).await.unwrap();
        let mut proposed = intent();
        if let Intent::Entry { symbol: asset, .. } = &mut proposed {
            *asset = symbol.into();
        }
        signals.push((session, e, proposed));
    }
    assert!(
        automation::claim(&pool, ACCOUNT, signals[0].0, &signals[1].1, &signals[1].2)
            .await
            .is_err()
    );
    for (n, (session, e, intent)) in signals.iter().enumerate() {
        assert_eq!(
            automation::claim(&pool, ACCOUNT, session, e, intent)
                .await
                .unwrap()
                .is_some(),
            n < 2
        );
    }
    let writes: i64 = sqlx::query_scalar("SELECT count(*) FROM testnet_requests")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(writes, 0);
    drop_db(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL; synthetic signal and simulated exchange only"]
async fn synthetic_probe_is_separate_single_use_and_closes_owned_position() {
    use trading_testnet::{
        probe,
        runtime::{Shutdown, SupervisorLock},
    };
    let (admin, pool, _, schema) = db("probe").await;
    let exchange = Exchange::new(false);
    let mut owner = SupervisorLock::acquire(&pool, ACCOUNT).await.unwrap();
    let mut shutdown = Shutdown::install().unwrap();
    let report = probe::run(
        &exchange,
        &pool,
        &signer(),
        ACCOUNT,
        "synthetic-fixture",
        &mut owner,
        &mut shutdown,
    )
    .await
    .unwrap();
    assert_eq!(report["synthetic"], true);
    assert_eq!(report["natural_acceptance"], false);
    assert_eq!(report["final_assessment"]["safe_to_stop"], true);
    assert!(
        report["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "protection_observed"
                && e["payload"]["assessment"]["positions"][0]["stop_confirmed"] == true)
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM testnet_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    probe::run(
        &exchange,
        &pool,
        &signer(),
        ACCOUNT,
        "synthetic-fixture",
        &mut owner,
        &mut shutdown,
    )
    .await
    .unwrap();
    assert_eq!(exchange.entry_calls.load(Ordering::SeqCst), 1);
    assert_eq!(exchange.close_calls.load(Ordering::SeqCst), 1);
    drop(owner);
    drop_db(admin, pool, schema).await;
}
