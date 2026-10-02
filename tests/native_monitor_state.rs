use maris::ui::desktop::monitor_state;
use serde_json::json;

#[test]
fn native_monitor_rejects_stale_future_and_missing_measurements() {
    let fresh =
        json!({"active":true,"updated_at_ms":20_000,"peak_dbfs":-12.7,"adaptive_reduction_db":1.2});
    assert_eq!(
        monitor_state::status(&fresh, false, 20_010),
        "Processing · -12.7 dBFS · Dynamic EQ -1.2 dB"
    );
    assert!(monitor_state::status(&fresh, true, 20_010).starts_with("EQ bypassed"));
    assert!(!monitor_state::status(&fresh, true, 20_010).contains("Dynamic EQ"));
    assert_eq!(monitor_state::status(&fresh, false, 22_000), "STALE");
    assert_eq!(monitor_state::status(&fresh, false, 19_999), "STALE");
    let absent = json!({"active":true,"updated_at_ms":20_000});
    assert_eq!(
        monitor_state::status(&absent, false, 20_010),
        "Processing · Unavailable"
    );
    let malformed = json!({"active":true,"updated_at_ms":20_000,"peak_dbfs":"-6.0","adaptive_reduction_db":-1.0});
    assert_eq!(
        monitor_state::status(&malformed, false, 20_010),
        "Processing · Unavailable"
    );
    let stopped = json!({"active":false,"updated_at_ms":20_000,"peak_dbfs":-6.0});
    assert_eq!(monitor_state::status(&stopped, false, 20_010), "Standby");
}
