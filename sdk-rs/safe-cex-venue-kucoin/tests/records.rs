use safe_cex_venue_kucoin::spot_types::{KucoinSpotWsServer, KucoinSpotWsTokenResponse};
use serde::Deserialize;
use serde_json::{Value, json};

#[test]
fn spot_records_reject_positional_and_null_optional_shapes() {
    let server = json!({"endpoint":"wss://example", "encrypt":false,"protocol":"websocket","pingInterval":18000,"pingTimeout":10000});
    assert_server(
        &KucoinSpotWsServer::deserialize(server.clone()).unwrap(),
        &server,
    );
    for value in [
        json!(["wss://example", false, "websocket", 18000, 10000]),
        Value::Null,
        json!(false),
        json!(1),
        json!("server"),
    ] {
        assert!(<KucoinSpotWsServer as Deserialize>::deserialize(value).is_err());
    }
    let response = json!({"code":"200000", "data":{"token":"token","instanceServers":[server]}});
    assert_response(
        &KucoinSpotWsTokenResponse::deserialize(response.clone()).unwrap(),
        &response,
    );
    assert_response(
        &KucoinSpotWsTokenResponse::deserialize(json!({"code":"200000"})).unwrap(),
        &json!({"code":"200000"}),
    );
    for value in [
        json!(["200000"]),
        json!({"code":"200000","data":null}),
        json!({"code":"200000","msg":null}),
        json!({"code":"200000","data":["token",[]]}),
        json!({"code":"200000","data":{"token":"token","instanceServers":[["url",false,"ws",1,2]]}}),
    ] {
        assert!(
            <KucoinSpotWsTokenResponse as Deserialize>::deserialize(value.clone()).is_err(),
            "accepted {value}"
        );
    }
    for key in [
        "endpoint",
        "encrypt",
        "protocol",
        "pingInterval",
        "pingTimeout",
    ] {
        let mut value = response.clone();
        value["data"]["instanceServers"][0]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            KucoinSpotWsTokenResponse::deserialize(value).is_err(),
            "missing {key}"
        );
    }
}

#[test]
fn live_typescript_record_acceptance() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let server =
        json!({"endpoint":"url","encrypt":false,"protocol":"ws","pingInterval":1,"pingTimeout":2});
    let token =
        json!({"code":"200000","data":{"token":"t","instanceServers":[server.clone()]},"msg":"ok"});
    let mut cases = vec![];
    for (name, valid) in [
        ("KucoinSpotWsServer", server),
        ("KucoinSpotWsTokenResponse", token),
    ] {
        cases.push(json!({"type":name,"value":valid}));
        for value in [json!([]), json!(null), json!(false), json!(42), json!("x")] {
            cases.push(json!({"type":name,"value":value}));
        }
        for key in valid.as_object().unwrap().keys() {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            cases.push(json!({"type":name,"value":missing}));
            for replacement in [
                json!(null),
                json!([]),
                json!({}),
                json!(true),
                json!(0),
                json!(""),
            ] {
                let mut value = valid.clone();
                value[key] = replacement;
                cases.push(json!({"type":name,"value":value}));
            }
        }
    }
    let valid = json!({"code":"200000","data":{"token":"t","instanceServers":[{"endpoint":"url","encrypt":false,"protocol":"ws","pingInterval":1,"pingTimeout":2}]}});
    for path in [vec![], vec!["data"], vec!["data", "instanceServers", "0"]] {
        let mut value = valid.clone();
        let target = match path.len() {
            0 => &mut value,
            1 => &mut value["data"],
            _ => &mut value["data"]["instanceServers"][0],
        };
        target
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), json!(true));
        cases.push(json!({"type":"KucoinSpotWsTokenResponse","value":value}));
    }
    for key in ["token", "instanceServers"] {
        let mut value = valid.clone();
        value["data"].as_object_mut().unwrap().remove(key);
        cases.push(json!({"type":"KucoinSpotWsTokenResponse","value":value}));
        for replacement in [
            json!(null),
            json!(false),
            json!(1),
            json!(""),
            json!({}),
            json!([]),
            json!([{}]),
            json!([null]),
        ] {
            let mut value = valid.clone();
            value["data"][key] = replacement;
            cases.push(json!({"type":"KucoinSpotWsTokenResponse","value":value}));
        }
    }
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/records.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<bool> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), cases.len());
    for (case, expected) in cases.iter().zip(expected) {
        let actual = if case["type"] == "KucoinSpotWsServer" {
            safe_cex_num::numeric_ingress::js_from_value::<KucoinSpotWsServer>(
                case["value"].clone(),
            )
            .map(|value| assert_server(&value, &case["value"]))
            .is_ok()
        } else {
            safe_cex_num::numeric_ingress::js_from_value::<KucoinSpotWsTokenResponse>(
                case["value"].clone(),
            )
            .map(|value| assert_response(&value, &case["value"]))
            .is_ok()
        };
        assert_eq!(actual, expected, "{}: {}", case["type"], case["value"]);
    }
}

fn assert_server(value: &KucoinSpotWsServer, raw: &Value) {
    assert_eq!(
        value.endpoint,
        raw["endpoint"].as_str().unwrap(),
        "native server endpoint identity"
    );
    assert_eq!(
        value.protocol,
        raw["protocol"].as_str().unwrap(),
        "native server protocol identity"
    );
    assert_eq!(
        value.encrypt,
        raw["encrypt"].as_bool().unwrap(),
        "native server encrypt identity"
    );
    assert_eq!(
        value.ping_interval.to_bits(),
        raw["pingInterval"].as_f64().unwrap().to_bits(),
        "native server pingInterval identity"
    );
    assert_eq!(
        value.ping_timeout.to_bits(),
        raw["pingTimeout"].as_f64().unwrap().to_bits(),
        "native server pingTimeout identity"
    );
}
fn assert_response(value: &KucoinSpotWsTokenResponse, raw: &Value) {
    assert_eq!(
        value.code,
        raw["code"].as_str().unwrap(),
        "native response code identity"
    );
    assert_eq!(
        value.msg.as_deref(),
        raw.get("msg").and_then(Value::as_str),
        "native response message presence"
    );
    assert_eq!(
        value.data.is_some(),
        raw.get("data").is_some(),
        "native response data presence"
    );
    if let Some(data) = &value.data {
        assert_eq!(
            data.token,
            raw["data"]["token"].as_str().unwrap(),
            "native token identity"
        );
        let servers = raw["data"]["instanceServers"].as_array().unwrap();
        assert_eq!(
            data.instance_servers.len(),
            servers.len(),
            "native server count"
        );
        for (server, raw) in data.instance_servers.iter().zip(servers) {
            assert_server(server, raw);
        }
    }
}

#[test]
fn server_cardinality_supplement_retains_both_native_records() {
    let servers = json!([
        {"endpoint":"wss://first","encrypt":false,"protocol":"first","pingInterval":17,"pingTimeout":23},
        {"endpoint":"wss://second","encrypt":true,"protocol":"second","pingInterval":31,"pingTimeout":47}
    ]);
    assert_eq!(
        servers.as_array().unwrap().len(),
        2,
        "KUCOIN-CARDINALITY server fixture total"
    );
    let raw = json!({"code":"200000","data":{"token":"token","instanceServers":servers}});
    let decoded = KucoinSpotWsTokenResponse::deserialize(raw).unwrap();
    let native = decoded.data.unwrap().instance_servers;
    assert_eq!(
        native.len(),
        2,
        "KUCOIN-CARDINALITY server native retained rows"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        let row = native
            .get(index)
            .expect("KUCOIN-CARDINALITY server native index coverage");
        assert_server(row, &servers[index]);
        assert_eq!(row.endpoint, ["wss://first", "wss://second"][index]);
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 2,
        "KUCOIN-CARDINALITY server native comparisons"
    );
}
