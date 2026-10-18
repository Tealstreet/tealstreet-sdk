use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_venue_woo::{
    v3_ws_commands::{WooV3WsCommand, WooV3WsCommandName},
    v3_ws_state::*,
    v3_ws_topics::WooV3WsChannel,
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-state-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WS-STATE source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value) -> Option<JsNumberInput> {
    row["input"]
        .as_str()
        .map(|s| serde_json::from_str(s).unwrap())
}
fn number(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        expected["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "{label}"
    );
}
fn strings(expected: &Value) -> Vec<String> {
    expected
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}
fn error<T: std::fmt::Debug>(actual: Result<T, String>, expected: &Value) {
    assert_eq!(
        actual.unwrap_err(),
        expected["error"]["message"].as_str().unwrap(),
        "WS-STATE native first error"
    );
    assert_eq!(expected["error"]["name"], "Error");
}
fn channel(text: &str) -> WooV3WsChannel {
    match text {
        "public" => WooV3WsChannel::Public,
        "private" => WooV3WsChannel::Private,
        _ => panic!("unexpected source channel"),
    }
}
fn command_name(text: &str) -> WooV3WsCommandName {
    match text {
        "SUBSCRIBE" => WooV3WsCommandName::Subscribe,
        "UN_SUBSCRIBE" => WooV3WsCommandName::Unsubscribe,
        "UN_SUBSCRIBE_ALL" => WooV3WsCommandName::UnsubscribeAll,
        "LIST_SUBSCRIPTION" => WooV3WsCommandName::ListSubscriptions,
        _ => panic!("unexpected source command"),
    }
}
fn ack(actual: &WooV3WsCommandAck, expected: &Value) {
    assert_eq!(
        actual.id,
        expected["id"].as_str().unwrap(),
        "WS-STATE native ack id"
    );
    assert_eq!(
        actual.cmd,
        command_name(expected["cmd"].as_str().unwrap()),
        "WS-STATE native ack cmd"
    );
    assert_eq!(
        actual.success,
        expected["success"].as_bool().unwrap(),
        "WS-STATE native ack success"
    );
    number(actual.time, &expected["time"], "WS-STATE native ack time");
    assert_eq!(
        actual.data,
        strings(&expected["data"]),
        "WS-STATE native ack data"
    );
}
fn state(actual: &WooV3WsSubscriptionState, expected: &Value) {
    assert_eq!(
        actual.channel,
        channel(expected["channel"].as_str().unwrap()),
        "WS-STATE native channel"
    );
    number(
        actual.generation,
        &expected["generation"],
        "WS-STATE native generation",
    );
    assert_eq!(
        actual.intended_topics,
        strings(&expected["intendedTopics"]),
        "WS-STATE native intended topics"
    );
    assert_eq!(
        actual.acknowledged_topics,
        strings(&expected["acknowledgedTopics"]),
        "WS-STATE native acknowledged topics"
    );
    assert_eq!(
        actual.incremental_books_require_fresh_snapshot,
        expected["incrementalBooksRequireFreshSnapshot"]
            .as_bool()
            .unwrap(),
        "WS-STATE native snapshot flag"
    );
}
fn command(actual: &WooV3WsCommand, expected: &Value) {
    assert_eq!(
        actual.channel,
        channel(expected["channel"].as_str().unwrap()),
        "WS-STATE native planned channel"
    );
    assert_eq!(
        actual.wire.id,
        expected["wire"]["id"].as_str().unwrap(),
        "WS-STATE native planned id"
    );
    assert_eq!(
        actual.wire.cmd,
        command_name(expected["wire"]["cmd"].as_str().unwrap()),
        "WS-STATE native planned cmd"
    );
    assert_eq!(
        actual.wire.params,
        expected["wire"].get("params").map(strings),
        "WS-STATE native planned params"
    );
}
#[test]
fn decoded_native_contracts_and_declarations() {
    let data = source();
    let expected = [
        WooV3WsBlockerCode::CommandFailureEnvelopeUndocumented,
        WooV3WsBlockerCode::CommandRequestIdTypeUndocumented,
        WooV3WsBlockerCode::ListenKeyRotationOverlapUndocumented,
        WooV3WsBlockerCode::PrivateAccountReplacementSemanticsUndocumented,
        WooV3WsBlockerCode::PrivateBalanceReplacementSemanticsUndocumented,
        WooV3WsBlockerCode::PrivatePositionReplacementSemanticsUndocumented,
        WooV3WsBlockerCode::ExecutionStatusLifecycleIncomplete,
        WooV3WsBlockerCode::AlgoShapeAndLifecycleIncomplete,
    ];
    assert_eq!(data["codeNames"].as_array().unwrap().len(), 8);
    assert_eq!(data["blockers"].as_array().unwrap().len(), 8);
    for index in 0..8 {
        let native: WooV3WsBlockerCode =
            serde_json::from_value(data["codeNames"][index].clone()).unwrap();
        assert_eq!(native, expected[index], "WS-STATE native blocker code");
        assert_eq!(native.as_str(), data["codeNames"][index].as_str().unwrap());
        let decoded: WooV3WsBlocker = js_from_str(&data["blockers"][index].to_string()).unwrap();
        for actual in [&decoded, &WOO_V3_WS_BLOCKERS[index]] {
            assert_eq!(
                actual.code, expected[index],
                "WS-STATE native blocker record code"
            );
            assert_eq!(
                actual.reason,
                data["blockers"][index]["reason"].as_str().unwrap(),
                "WS-STATE native blocker reason"
            );
            assert_eq!(
                actual.requires,
                if index < 2 {
                    WsBlockerRequirement::OfficialContract
                } else {
                    WsBlockerRequirement::CredentialedDemo
                },
                "WS-STATE native blocker requirement"
            );
            assert_eq!(
                actual.requires.as_str(),
                data["blockers"][index]["requires"].as_str().unwrap()
            );
        }
    }
    assert_eq!(
        WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS.emitted_subset,
        data["idContract"]["emittedSubset"].as_str().unwrap(),
        "WS-STATE native id subset"
    );
    assert_eq!(
        WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS.blocker.code, expected[1],
        "WS-STATE native id blocker code"
    );
    assert_eq!(
        WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS.blocker.reason,
        data["idContract"]["blocker"]["reason"].as_str().unwrap(),
        "WS-STATE native id blocker reason"
    );
    assert_eq!(
        WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS.blocker.requires,
        WsBlockerRequirement::OfficialContract,
        "WS-STATE native id blocker requirement"
    );
    let row = &data["ackDeclaration"];
    let native: WooV3WsCommandAck = js_from_str(row["input"].as_str().unwrap()).unwrap();
    ack(&native, &row["expected"]);
    let row = &data["stateDeclaration"];
    let native: WooV3WsSubscriptionState = js_from_str(row["input"].as_str().unwrap()).unwrap();
    state(&native, &row["expected"]);
    for raw in [
        "[]",
        "null",
        "{}",
        r#"{"command-failure-envelope-undocumented":null}"#,
        r#""bad""#,
    ] {
        assert!(
            serde_json::from_str::<WooV3WsBlockerCode>(raw).is_err(),
            "WS-STATE strict blocker code {raw}"
        );
    }
    for raw in [
        "[]",
        r#"{"code":"command-failure-envelope-undocumented","reason":"reason","requires":"bad"}"#,
    ] {
        assert!(
            js_from_str::<WooV3WsBlocker>(raw).is_err(),
            "WS-STATE strict blocker record {raw}"
        );
    }
    for raw in [
        "[]",
        r#"{"id":"id","cmd":"SUBSCRIBE","success":false,"time":1,"data":[]}"#,
        r#"{"id":"id","cmd":"SUBSCRIBE","success":true,"time":1,"data":[3]}"#,
    ] {
        assert!(
            js_from_str::<WooV3WsCommandAck>(raw).is_err(),
            "WS-STATE strict ack record {raw}"
        );
    }
    for raw in [
        "[]",
        "{}",
        r#"{"channel":"public","generation":0,"intendedTopics":[],"acknowledgedTopics":[],"incrementalBooksRequireFreshSnapshot":"true"}"#,
    ] {
        assert!(
            js_from_str::<WooV3WsSubscriptionState>(raw).is_err(),
            "WS-STATE strict state record {raw}"
        );
    }
}
#[test]
fn native_ack_validation_matches_source() {
    let data = source();
    assert_eq!(data["acks"].as_array().unwrap().len(), 31);
    for index in 0..31 {
        let row = &data["acks"][index];
        let value = input(row);
        let expected = row
            .get("expected")
            .map(|v| js_from_str::<WooV3WsCommand>(&v.to_string()).unwrap());
        let actual = decode_woo_v3_ws_command_ack(value.as_ref(), expected.as_ref());
        if let Some(ok) = row.get("ok") {
            ack(&actual.unwrap(), ok);
            let native: WooV3WsCommandAck = js_from_str(row["decoded"].as_str().unwrap()).unwrap();
            ack(&native, ok);
        } else {
            error(actual, row);
        }
    }
}
#[test]
fn native_subscription_creation_planning_ack_and_reconnect() {
    let data = source();
    assert_eq!(data["creates"].as_array().unwrap().len(), 12);
    for index in 0..12 {
        let row = &data["creates"][index];
        let value = input(row);
        let actual = create_woo_v3_ws_subscription_state(
            channel(row["channel"].as_str().unwrap()),
            value.as_ref(),
        );
        if let Some(ok) = row.get("ok") {
            state(&actual.unwrap(), ok);
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["plans"].as_array().unwrap().len(), 12);
    for index in 0..12 {
        let row = &data["plans"][index];
        let current: WooV3WsSubscriptionState =
            js_from_str(row["state"].as_str().unwrap()).unwrap();
        let prefix: Option<JsNumberInput> = row["prefix"]
            .as_str()
            .map(|s| serde_json::from_str(s).unwrap());
        let actual = plan_woo_v3_ws_resubscribe(&current, prefix.as_ref());
        if let Some(ok) = row.get("ok") {
            let actual = actual.unwrap();
            let count = row["count"].as_u64().unwrap() as usize;
            assert_eq!(actual.len(), count, "WS-STATE native planned batch count");
            assert_eq!(
                ok.as_array().unwrap().len(),
                count,
                "WS-STATE independent source batch count"
            );
            for batch in 0..count {
                command(&actual[batch], &ok[batch]);
            }
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["acknowledgements"].as_array().unwrap().len(), 9);
    for index in 0..9 {
        let row = &data["acknowledgements"][index];
        let current: WooV3WsSubscriptionState =
            js_from_str(row["state"].as_str().unwrap()).unwrap();
        let command: WooV3WsCommand = js_from_str(&row["command"].to_string()).unwrap();
        let ack: JsNumberInput = serde_json::from_str(row["ack"].as_str().unwrap()).unwrap();
        let actual = acknowledge_woo_v3_ws_subscription(&current, &command, Some(&ack));
        if let Some(ok) = row.get("ok") {
            state(&actual.unwrap(), ok);
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["reconnects"].as_array().unwrap().len(), 6);
    for index in 0..6 {
        let row = &data["reconnects"][index];
        let current: WooV3WsSubscriptionState =
            js_from_str(row["input"].as_str().unwrap()).unwrap();
        let next = reconnect_woo_v3_ws_subscription(&current);
        state(&next, &row["ok"]);
        assert_eq!(
            current.acknowledged_topics,
            if index < 3 || index == 5 {
                vec!["ticker@PERP_ETH_USDT".to_owned()]
            } else {
                Vec::new()
            },
            "WS-STATE input acknowledgement snapshot"
        );
    }
}
