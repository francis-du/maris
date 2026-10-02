use super::*;
use serde_json::json;

// pactl 16.1 get_sink_info_callback emits monitor_source (a name), not
// the C struct member spelling monitor_source_name. Keep this independent
// of the Rust field name so a mirrored fake server cannot hide wire drift.
fn sink_json() -> Value {
    json!({"index":1,"name":"speakers_a","monitor_source":"speakers_a.monitor",
        "owner_module":1,"flags":["DECIBEL_VOLUME","LATENCY"],
        "properties":{"device.api":"alsa"}})
}

#[test]
fn native_pactl_monitor_source_is_a_required_name() {
    let sink: Sink = serde_json::from_value(sink_json()).unwrap();
    assert_eq!(sink.monitor_source, "speakers_a.monitor");
    assert!(sink.physical());
    assert!(sink.routing_safe());
    for bad in [Value::Null, json!(1), json!({}), json!([])] {
        let mut value = sink_json();
        value["monitor_source"] = bad;
        assert!(serde_json::from_value::<Sink>(value).is_err());
    }
    let mut missing = sink_json();
    missing.as_object_mut().unwrap().remove("monitor_source");
    assert!(serde_json::from_value::<Sink>(missing).is_err());
}

#[test]
fn legacy_monitor_spelling_is_accepted_but_conflicting_names_are_rejected() {
    let mut value = sink_json();
    let monitor = value
        .as_object_mut()
        .unwrap()
        .remove("monitor_source")
        .unwrap();
    value["monitor_source_name"] = monitor;
    assert_eq!(
        serde_json::from_value::<Sink>(value.clone())
            .unwrap()
            .monitor_source,
        "speakers_a.monitor"
    );
    value["monitor_source"] = json!("other.monitor");
    assert!(serde_json::from_value::<Sink>(value).is_err());
}

#[test]
fn native_field_compatibility_does_not_weaken_routing_guards() {
    let mut value = sink_json();
    value["flags"] = json!(["FLAT_VOLUME"]);
    let sink: Sink = serde_json::from_value(value.clone()).unwrap();
    assert!(!sink.routing_safe());
    value.as_object_mut().unwrap().remove("flags");
    assert!(!serde_json::from_value::<Sink>(value)
        .unwrap()
        .routing_safe());
}

// Matches the independent native pactl 16.1 wire shape, including its string client.
fn input_json(client: Value) -> Value {
    json!({"index":7,"driver":"protocol-native.c","owner_module":"2",
        "client":client,"sink":1,"corked":false,"mute":false,
        "properties":{"application.process.id":"1234","application.id":"fixture.player"}})
}

#[test]
fn native_string_and_numeric_clients_preserve_the_same_stream_identity() {
    for value in [json!("2"), json!(2)] {
        let input: Input = serde_json::from_value(input_json(value)).unwrap();
        let identity = input.identity().unwrap();
        assert_eq!(identity.index, 7);
        assert_eq!(identity.client, 2);
        assert_eq!(identity.pid, Some(1234));
        assert_eq!(identity.app.as_deref(), Some("fixture.player"));
    }
}

#[test]
fn absent_client_never_becomes_an_owned_stream() {
    let input: Input = serde_json::from_value(input_json(Value::Null)).unwrap();
    assert!(input.identity().is_none());
    let mut missing = input_json(Value::Null);
    missing.as_object_mut().unwrap().remove("client");
    assert!(serde_json::from_value::<Input>(missing)
        .unwrap()
        .identity()
        .is_none());
}

#[test]
fn invalid_client_ids_cannot_be_coerced_or_truncated_into_another_client() {
    for value in [
        json!(""),
        json!("-1"),
        json!("+2"),
        json!(" 2"),
        json!("2 "),
        json!("2.0"),
        json!("2e0"),
        json!("4294967296"),
        json!(4294967296_u64),
        json!("4294967295"),
        json!(u32::MAX),
        json!(2.5),
        json!(2.0),
        json!(true),
        json!({}),
        json!([]),
        json!(-1),
        json!("00000000002"),
    ] {
        assert!(
            serde_json::from_value::<Input>(input_json(value.clone())).is_err(),
            "{value}"
        );
    }
    for value in [
        json!(0),
        json!("0"),
        json!(u32::MAX - 1),
        json!("4294967294"),
    ] {
        assert!(serde_json::from_value::<Input>(input_json(value)).is_ok());
    }
}

#[test]
fn indexed_native_modules_preserve_identity_across_list_reordering() {
    let own = "7\tmodule-null-sink\tsink_name=maris_fixture rate=48000\t\n";
    let unrelated = "2\tmodule-native-protocol-unix\tauth-anonymous=1\t\n";
    for output in [format!("{own}{unrelated}"), format!("{unrelated}{own}")] {
        assert!(module_owned(&output, 7, "maris_fixture").unwrap());
        assert!(!module_owned(&output, 8, "maris_fixture").unwrap());
    }
    assert!(!module_owned("", 7, "maris_fixture").unwrap());
}

#[test]
fn reused_or_ambiguous_native_module_ids_cannot_authorize_unloading() {
    for output in [
        "7\tmodule-loopback\tsink_name=maris_fixture\n",
        "7\tmodule-null-sink\tsink_name=other\n",
        "7\tmodule-null-sink\tsink_name=maris_fixture_extra\n",
        "7\tmodule-null-sink\tother_sink_name=maris_fixture\n",
        "7\tmodule-null-sink\tsink_name=maris_fixture sink_name=other\n",
        "7\tmodule-null-sink\tsink_name=maris_fixture sink_name=maris_fixture\n",
        "7\tmodule-null-sink\tsink_name=maris_fixture\n7\tmodule-null-sink\tsink_name=maris_fixture\n",
        "+7\tmodule-null-sink\tsink_name=maris_fixture\n",
        "4294967296\tmodule-null-sink\tsink_name=maris_fixture\n",
        "7\tmodule-null-sink\n",
        "[{\"name\":\"module-null-sink\",\"argument\":\"sink_name=maris_fixture\"}]",
    ] {
        assert!(module_owned(output,7,"maris_fixture").is_err(),"{output}");
    }
}
