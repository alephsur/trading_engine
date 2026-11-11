use serde_json::json;
use trading_live::{AssessmentInput, LiveExecutor, MANUAL_CONFIRMATION, assess};
#[test]
fn cli_refuses_before_reading_order_or_credentials_even_with_activation_environment() {
    for operation in ["submit", "cancel"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_trading-live"))
            .args([operation, "/nonexistent-order.json"])
            .env("TRADING_MODE", "LIVE")
            .env("LIVE_ENABLED", "true")
            .env("LIVE_CONFIRM", MANUAL_CONFIRMATION)
            .env("LIVE_PRIVATE_KEY_FILE", "/nonexistent-key")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("LIVE unavailable in this build"));
        assert!(!error.contains("nonexistent"));
    }
}
fn fixture() -> serde_json::Value {
    json!({"activation":{"enabled":false,"manual_confirmation":null},
        "expected_positions":[],"expected_open_order_ids":[],
        "snapshot":{"observed_ms":1000,"positions":[],"orders":[]}})
}
#[test]
fn all_gate_combinations_deny_and_secrets_are_not_input_fields() {
    for enabled in [false, true] {
        for confirmation in [None, Some("wrong"), Some(MANUAL_CONFIRMATION)] {
            let mut v = fixture();
            v["activation"] = json!({"enabled":enabled,"manual_confirmation":confirmation});
            let input = serde_json::from_value(v).unwrap();
            let report = assess(&input, 1000);
            assert!(!report.execution_enabled);
            assert!(report.blockers.contains(&"production_build_disabled"));
            assert_eq!(
                report.blockers.contains(&"configuration_disabled"),
                !enabled
            );
            assert_eq!(
                report.blockers.contains(&"manual_confirmation_missing"),
                confirmation != Some(MANUAL_CONFIRMATION)
            );
            assert!(LiveExecutor.submit().is_err());
            assert!(LiveExecutor.cancel().is_err());
        }
    }
    let mut v = fixture();
    v["private_key"] = json!("never accepted");
    assert!(serde_json::from_value::<AssessmentInput>(v).is_err());
}
#[test]
fn discrepancies_and_wrong_stop_direction_block() {
    let mut v = fixture();
    let p = json!({"symbol":"BTC","quantity":"0.01","leverage":"1"});
    v["snapshot"]["positions"] = json!([p]);
    let r = assess(&serde_json::from_value(v.clone()).unwrap(), 1000);
    assert!(r.blockers.contains(&"positions_unreconciled"));
    assert!(r.blockers.contains(&"unprotected_position"));
    v["expected_positions"] = v["snapshot"]["positions"].clone();
    v["expected_open_order_ids"] = json!(["stop-1"]);
    v["snapshot"]["orders"] = json!([{"id":"stop-1","symbol":"BTC","quantity":"0.01","buy":true,"reduce_only":true,"stop":true}]);
    assert!(
        assess(&serde_json::from_value(v.clone()).unwrap(), 1000)
            .blockers
            .contains(&"unprotected_position")
    );
    v["snapshot"]["orders"][0]["buy"] = json!(false);
    let r = assess(&serde_json::from_value(v.clone()).unwrap(), 1000);
    assert!(!r.blockers.contains(&"positions_unreconciled"));
    assert!(!r.blockers.contains(&"unprotected_position"));
    v["snapshot"]["orders"][0]["id"] = json!("foreign-order");
    assert!(
        assess(&serde_json::from_value(v.clone()).unwrap(), 1000)
            .blockers
            .contains(&"orders_unreconciled")
    );
    v["snapshot"]["positions"][0]["leverage"] = json!("2");
    assert!(
        assess(&serde_json::from_value(v).unwrap(), 1000)
            .blockers
            .contains(&"positions_unreconciled")
    );
}
#[test]
fn timestamps_and_duplicate_positions_fail_closed() {
    let input = serde_json::from_value(fixture()).unwrap();
    for now in [999, 16000, i64::MIN] {
        assert!(
            assess(&input, now)
                .blockers
                .contains(&"stale_or_future_snapshot")
        );
    }
    let mut v = fixture();
    let p = json!({"symbol":"BTC","quantity":"0.01","leverage":"1"});
    v["snapshot"]["positions"] = json!([p, p]);
    v["expected_positions"] = v["snapshot"]["positions"].clone();
    assert!(
        assess(&serde_json::from_value(v).unwrap(), 1000)
            .blockers
            .contains(&"positions_unreconciled")
    );
}
