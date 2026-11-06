use serde_json::{Value, json};
use trading_paper::dec;
use trading_testnet::{
    executor::{Observation, reconcile_orders},
    model::{self, Intent},
    signing::TestnetSigner,
};
use zeroize::Zeroizing;
fn state() -> Value {
    json!({"assetPositions":[],"marginSummary":{"accountValue":"1000"}})
}
fn markets() -> Vec<trading_domain::ListedMarket> {
    trading_domain::select_markets(serde_json::from_value(json!({"universe":[{"name":"OTHER","szDecimals":3,"maxLeverage":10},{"name":"HYPE","szDecimals":3,"maxLeverage":10},{"name":"SOL","szDecimals":3,"maxLeverage":10},{"name":"BTC","szDecimals":3,"maxLeverage":10},{"name":"ETH","szDecimals":3,"maxLeverage":10}]})).unwrap()).unwrap()
}
fn book() -> trading_domain::data::Book {
    serde_json::from_value(json!({"coin":"BTC","time":1000,"levels":[[{"px":"100","sz":"1000","n":1}],[{"px":"100.01","sz":"1000","n":1}]]})).unwrap()
}
fn intent() -> Intent {
    Intent::Entry {
        symbol: "BTC".into(),
        buy: true,
        quantity: dec("0.2"),
        limit: dec("100.01"),
        stop: dec("99"),
        take_profit: dec("103"),
    }
}
#[test]
fn bracket_uses_dynamic_assets_decimal_precision_and_protective_exits() {
    let action = model::entry(
        &intent(),
        "account",
        "command-01",
        &markets(),
        &book(),
        &state(),
        1000,
    )
    .unwrap();
    assert_eq!(action["orders"][0]["a"], 3);
    assert_eq!(action["grouping"], "normalTpsl");
    assert_eq!(action["orders"][0]["t"]["limit"]["tif"], "Ioc");
    for i in [1, 2] {
        assert_eq!(action["orders"][i]["r"], true);
        assert_eq!(action["orders"][i]["b"], false);
    }
    assert_ne!(action["orders"][0]["c"], action["orders"][1]["c"]);
    assert!(
        model::entry(
            &intent(),
            "account",
            "command-01",
            &markets(),
            &book(),
            &state(),
            16000
        )
        .is_err()
    );
    let mut with_position = state();
    with_position["assetPositions"] = json!([{"position":{"coin":"SOL","szi":"0.01"}}]);
    assert!(
        model::entry(
            &intent(),
            "account",
            "command-01",
            &markets(),
            &book(),
            &with_position,
            1000
        )
        .is_err()
    );
    let mut poor = state();
    poor["marginSummary"]["accountValue"] = json!("99");
    assert!(
        model::entry(
            &intent(),
            "account",
            "command-01",
            &markets(),
            &book(),
            &poor,
            1000
        )
        .is_err()
    );
}
#[test]
fn rejects_unsafe_or_unsupported_intents() {
    for (field, value) in [
        ("symbol", json!("DOGE")),
        ("quantity", json!("100")),
        ("quantity", json!("0.2001")),
        ("stop", json!("101")),
        ("take_profit", json!("98")),
        ("limit", json!("100.001")),
        ("stop", json!("50")),
    ] {
        let mut raw = serde_json::to_value(intent()).unwrap();
        raw[field] = value;
        let intent: Intent = serde_json::from_value(raw).unwrap();
        assert!(
            model::entry(
                &intent,
                "account",
                "command-01",
                &markets(),
                &book(),
                &state(),
                1000
            )
            .is_err(),
            "{field}"
        );
    }
    assert!(serde_json::from_value::<Intent>(json!({"kind":"transfer","amount":"100"})).is_err());
    assert!(model::command_id("../secret").is_err());
    assert!(model::command_id("test-command-1").is_ok());
}
#[test]
fn close_is_reduce_only_and_never_opens_the_other_side() {
    let mut s = state();
    s["assetPositions"] = json!([{"position":{"coin":"BTC","szi":"-0.2"}}]);
    let action = model::close(
        &Intent::Close {
            symbol: "BTC".into(),
            limit: None,
        },
        "account",
        "close-01",
        &markets(),
        &book(),
        &s,
        1000,
    )
    .unwrap();
    assert_eq!(action["orders"][0]["r"], true);
    assert_eq!(action["orders"][0]["b"], true);
    assert_eq!(action["orders"][0]["s"], "0.2");
    assert!(
        model::close(
            &Intent::Close {
                symbol: "BTC".into(),
                limit: None
            },
            "account",
            "close-01",
            &markets(),
            &book(),
            &state(),
            1000
        )
        .is_err()
    );
}
#[test]
fn lost_partial_and_duplicate_fills_remain_explicit() {
    let cloid = model::cloid("account", "command-01", 0);
    let mut status = json!({"status":"order","order":{"status":"open","order":{"cloid":cloid,"oid":42,"coin":"BTC","side":"B","origSz":"1","sz":"0.6"}}});
    let fill = json!({"oid":42,"tid":1,"coin":"BTC","side":"B","sz":"0.4","px":"100","fee":"0.018","feeToken":"USDC"});
    let o = |value: Value| {
        vec![Observation {
            cloid: cloid.clone(),
            value,
        }]
    };
    let r = reconcile_orders(&o(status.clone()), &[fill.clone(), fill.clone()]).unwrap();
    assert_eq!(r["complete"], true);
    assert_eq!(r["all_terminal"], false);
    assert_eq!(r["orders"][0]["filled"], "0.4");
    assert_eq!(
        reconcile_orders(&o(status.clone()), &[]).unwrap()["complete"],
        false
    );
    status["order"]["status"] = json!("canceled");
    assert_eq!(
        reconcile_orders(&o(status), &[fill]).unwrap()["all_terminal"],
        true
    );
    assert_eq!(
        reconcile_orders(&o(json!({"status":"unknownOid"})), &[]).unwrap()["complete"],
        false
    );
    assert_eq!(
        model::response_state(
            &json!({"status":"ok","response":{"data":{"statuses":[{"filled":{"oid":1}}]}}}),
            3
        ),
        "uncertain"
    );
    assert_eq!(
        model::response_state(&json!({"status":"err","response":"rejected"}), 1),
        "rejected"
    );
}
#[test]
fn signatures_match_official_python_and_cannot_sign_transfers() {
    let signer = TestnetSigner::load(Zeroizing::new(format!("{}01", "00".repeat(31)))).unwrap();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/signing.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let signed = signer
            .sign(
                case["action"].clone(),
                case["nonce"].as_i64().unwrap(),
                case["expires"].as_i64().unwrap(),
            )
            .unwrap();
        assert_eq!(signed["signature"], case["signature"]);
        assert_eq!(signed["expiresAfter"], case["expires"]);
        assert!(signed["vaultAddress"].is_null());
    }
    assert!(signer.sign(json!({"type":"usdSend"}), 1000, 2000).is_err());
    assert!(signer.sign(json!({"type":"order"}), 1000, 40000).is_err());
}

#[test]
fn plans_are_small_unsigned_and_resting_closes_stay_reduce_only() {
    let planned = model::plan("BTC", true, &markets(), &book(), &state(), 1000).unwrap();
    if let Intent::Entry {
        quantity, limit, ..
    } = planned
    {
        assert!(quantity * limit <= dec("20"));
    } else {
        panic!("entry expected")
    }
    let mut state = state();
    state["assetPositions"] = json!([{"position":{"coin":"BTC","szi":"0.2"}}]);
    let wire = model::close(
        &Intent::Close {
            symbol: "BTC".into(),
            limit: Some(dec("100.5")),
        },
        "account",
        "close-01",
        &markets(),
        &book(),
        &state,
        1000,
    )
    .unwrap();
    assert_eq!(wire["orders"][0]["r"], true);
    assert_eq!(wire["orders"][0]["t"]["limit"]["tif"], "Alo");
    assert!(model::plan("BTC", true, &markets(), &book(), &state, 1000).is_err());
}
