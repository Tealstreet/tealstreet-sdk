use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_woo::{v3_ws_commands::*, v3_ws_topics::WooV3WsChannel};
use serde_json::{Value, json};
use std::process::Command;
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-commands-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "WS-COMMAND source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn expected_cmd(text: &str) -> WooV3WsCommandName {
    match text {
        "SUBSCRIBE" => WooV3WsCommandName::Subscribe,
        "UN_SUBSCRIBE" => WooV3WsCommandName::Unsubscribe,
        "UN_SUBSCRIBE_ALL" => WooV3WsCommandName::UnsubscribeAll,
        "LIST_SUBSCRIPTION" => WooV3WsCommandName::ListSubscriptions,
        _ => panic!("unexpected source command {text}"),
    }
}
fn expected_channel(text: &str) -> WooV3WsChannel {
    match text {
        "public" => WooV3WsChannel::Public,
        "private" => WooV3WsChannel::Private,
        _ => panic!("unexpected source channel"),
    }
}
fn native_wire(actual: &WooV3WsWireCommand, expected: &Value) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "WS-COMMAND native id"
    );
    assert_eq!(
        actual.cmd,
        expected_cmd(expected["cmd"].as_str().unwrap()),
        "WS-COMMAND native command kind"
    );
    let params = expected.get("params").map(|v| {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        actual.params, params,
        "WS-COMMAND native params presence and values"
    );
}
fn native_command(actual: &WooV3WsCommand, expected: &Value) {
    assert_eq!(
        actual.channel,
        expected_channel(expected["channel"].as_str().unwrap()),
        "WS-COMMAND native channel"
    );
    native_wire(&actual.wire, &expected["wire"]);
}
fn native_error<T: std::fmt::Debug>(actual: Result<T, String>, expected: &Value) {
    assert_eq!(
        actual.unwrap_err(),
        expected["error"]["message"].as_str().unwrap(),
        "WS-COMMAND native validation error"
    );
    assert_eq!(expected["error"]["name"], "Error");
}
#[test]
fn decoded_native_command_declarations() {
    let data = source();
    let expected = [
        WooV3WsCommandName::Subscribe,
        WooV3WsCommandName::Unsubscribe,
        WooV3WsCommandName::UnsubscribeAll,
        WooV3WsCommandName::ListSubscriptions,
    ];
    assert_eq!(
        data["commandNames"].as_array().unwrap().len(),
        4,
        "WS-COMMAND literal floor"
    );
    for index in 0..4 {
        let actual: WooV3WsCommandName =
            serde_json::from_value(data["commandNames"][index].clone()).unwrap();
        assert_eq!(actual, expected[index], "WS-COMMAND native literal");
    }
    assert_eq!(data["wireRecords"].as_array().unwrap().len(), 3);
    assert_eq!(data["commandRecords"].as_array().unwrap().len(), 2);
    for index in 0..3 {
        let expected = &data["wireRecords"][index];
        let decoded: Result<WooV3WsWireCommand, _> = js_from_str(&expected.to_string());
        assert!(decoded.is_ok(), "WS-COMMAND native wire decode");
        native_wire(&decoded.unwrap(), expected);
    }
    for index in 0..2 {
        let expected = &data["commandRecords"][index];
        let actual: WooV3WsCommand = js_from_str(&expected.to_string()).unwrap();
        native_command(&actual, expected);
    }
    for raw in ["{}", "[]", "3", "{\"SUBSCRIBE\":null}", "\"subscribe\""] {
        assert!(
            serde_json::from_str::<WooV3WsCommandName>(raw).is_err(),
            "WS-COMMAND literal rejects {raw}"
        );
    }
    for raw in [
        "[]",
        "[\"id\",\"SUBSCRIBE\",[]]",
        "{\"id\":\"id\",\"cmd\":\"SUBSCRIBE\",\"params\":null}",
        "{\"id\":\"id\",\"cmd\":\"SUBSCRIBE\",\"params\":[3]}",
    ] {
        assert!(
            js_from_str::<WooV3WsWireCommand>(raw).is_err(),
            "WS-COMMAND wire rejects {raw}"
        );
    }
    for raw in [
        "[]",
        "{\"channel\":\"public\",\"wire\":[\"id\",\"SUBSCRIBE\"]}",
        "{\"channel\":\"other\",\"wire\":{\"id\":\"id\",\"cmd\":\"SUBSCRIBE\"}}",
    ] {
        assert!(
            js_from_str::<WooV3WsCommand>(raw).is_err(),
            "WS-COMMAND command rejects {raw}"
        );
    }
}
#[test]
fn native_helpers_match_actual_source_values_and_errors() {
    let data = source();
    assert_eq!(
        data["helpers"].as_array().unwrap().len(),
        9,
        "WS-COMMAND helper floor"
    );
    let records = [false, false, false, false, false, false, true, true, false];
    for index in 0..9 {
        let row = &data["helpers"][index];
        let input = row.get("input");
        assert_eq!(
            is_record(input),
            records[index],
            "WS-COMMAND native record predicate"
        );
        assert_eq!(row["record"], records[index]);
        if index == 6 || index == 7 {
            let actual = require_record(input, "test.object").unwrap();
            assert!(
                std::ptr::eq(actual, input.unwrap().as_object().unwrap()),
                "WS-COMMAND native object borrow"
            );
            assert_eq!(
                actual.len(),
                if index == 6 { 0 } else { 1 },
                "WS-COMMAND native object size"
            );
            if index == 7 {
                assert_eq!(actual["marker"], "kept", "WS-COMMAND native object field");
            }
        } else {
            native_error(require_record(input, "test.object"), &row["object"]);
        }
        if index == 5 || index == 8 {
            let actual = require_array(input, "test.array").unwrap();
            assert!(
                std::ptr::eq(actual, input.unwrap().as_array().unwrap().as_slice()),
                "WS-COMMAND native array borrow"
            );
            assert_eq!(
                actual.len(),
                if index == 5 { 0 } else { 2 },
                "WS-COMMAND native array size"
            );
            if index == 8 {
                assert_eq!(actual[0], "bbo@PERP_BTC_USDT");
                assert_eq!(actual[1], "account");
            }
        } else {
            native_error(require_array(input, "test.array"), &row["array"]);
        }
    }
    assert_eq!(
        data["enums"].as_array().unwrap().len(),
        9,
        "WS-COMMAND enum helper floor"
    );
    for index in 0..9 {
        let row = &data["enums"][index];
        let values: Vec<_> = row["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let actual = require_enum(row.get("input"), &values, "test.enum");
        if let Some(expected) = row.get("ok") {
            assert_eq!(
                actual.unwrap(),
                expected.as_str().unwrap(),
                "WS-COMMAND native enum value"
            );
        } else {
            native_error(actual, row);
        }
    }
    assert_eq!(
        data["lists"].as_array().unwrap().len(),
        10,
        "WS-COMMAND list helper floor"
    );
    for index in 0..10 {
        let row = &data["lists"][index];
        let actual = require_topic_list(row.get("input"), "test.topics");
        if index == 3 {
            assert_eq!(
                actual.unwrap(),
                Vec::<String>::new(),
                "WS-COMMAND native empty list"
            );
            assert_eq!(row["ok"], json!([]), "WS-COMMAND source empty list");
        } else if index == 4 {
            assert_eq!(
                actual.unwrap(),
                vec!["bbo@PERP_BTC_USDT", "account"],
                "WS-COMMAND native list values"
            );
            assert_eq!(
                row["ok"],
                json!(["bbo@PERP_BTC_USDT", "account"]),
                "WS-COMMAND source list values"
            );
        } else {
            native_error(actual, row);
        }
    }
    assert_eq!(
        data["lists"][6]["error"]["message"], "test.topics[2] must be a non-empty string",
        "WS-COMMAND source validates strings before duplicates"
    );
}
#[test]
fn native_builders_preserve_command_values_and_phase_order() {
    let data = source();
    assert_eq!(
        data["commands"].as_array().unwrap().len(),
        34,
        "WS-COMMAND operation floor"
    );
    for index in 0..34 {
        let row = &data["commands"][index];
        let channel = expected_channel(row["channel"].as_str().unwrap());
        let topics = row.get("topics").map(|x| {
            x.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        });
        let id = row.get("id");
        let actual = match row["operation"].as_str().unwrap() {
            "subscribe" => build_woo_v3_subscribe(channel, id, topics.as_ref().unwrap()),
            "unsubscribe" => build_woo_v3_unsubscribe(channel, id, topics.as_ref().unwrap()),
            "all" => build_woo_v3_unsubscribe_all(channel, id),
            "list" => build_woo_v3_list_subscriptions(channel, id),
            "generic" => build_command(
                channel,
                id,
                expected_cmd(row["cmd"].as_str().unwrap()),
                topics.as_deref(),
            ),
            _ => panic!("operation"),
        };
        if let Some(expected) = row.get("ok") {
            native_command(&actual.unwrap(), expected);
        } else {
            native_error(actual, row);
        }
    }
    assert_eq!(
        data["channels"].as_array().unwrap().len(),
        7,
        "WS-COMMAND channel floor"
    );
    for index in 0..7 {
        let row = &data["channels"][index];
        let topics: Vec<_> = row["topics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let actual = assert_topics_match_channel(
            &topics,
            expected_channel(row["channel"].as_str().unwrap()),
            "test.channel",
        );
        if row.get("ok").is_some() {
            assert_eq!(actual, Ok(()), "WS-COMMAND native channel acceptance");
            assert_eq!(
                row["ok"],
                json!({"$undefined":true}),
                "WS-COMMAND source channel void value"
            );
        } else {
            native_error(actual, row);
        }
    }
    let mut input = vec!["bbo@PERP_BTC_USDT".to_owned()];
    let id = json!("snapshot");
    let mut actual = build_woo_v3_subscribe(WooV3WsChannel::Public, Some(&id), &input).unwrap();
    input[0] = "ticker@PERP_ETH_USDT".into();
    input.push("account".into());
    assert_eq!(
        actual.wire.params,
        Some(vec!["bbo@PERP_BTC_USDT".into()]),
        "WS-COMMAND native params snapshot"
    );
    native_command(&actual, &data["sourceSnapshot"]);
    actual.wire.id = "mutable native copy".into();
    assert_eq!(
        actual.wire.id, "mutable native copy",
        "WS-COMMAND CHOSEN owned output mutation"
    );
}
