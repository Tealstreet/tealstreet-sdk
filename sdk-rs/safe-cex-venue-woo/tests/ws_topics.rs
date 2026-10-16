use safe_cex_venue_woo::v3_ws_topics::*;
use serde_json::{Value, json};
use std::process::Command;
fn oracle() -> Value {
    let result = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-topics-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "WS-TOPIC source oracle: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn expected_interval(literal: &str) -> WooV3WsKlineInterval {
    match literal {
        "1m" => WooV3WsKlineInterval::M1,
        "3m" => WooV3WsKlineInterval::M3,
        "5m" => WooV3WsKlineInterval::M5,
        "15m" => WooV3WsKlineInterval::M15,
        "30m" => WooV3WsKlineInterval::M30,
        "1h" => WooV3WsKlineInterval::H1,
        "2h" => WooV3WsKlineInterval::H2,
        "4h" => WooV3WsKlineInterval::H4,
        "6h" => WooV3WsKlineInterval::H6,
        "12h" => WooV3WsKlineInterval::H12,
        "1d" => WooV3WsKlineInterval::D1,
        "3d" => WooV3WsKlineInterval::D3,
        "1w" => WooV3WsKlineInterval::W1,
        "1M" => WooV3WsKlineInterval::Month1,
        _ => panic!("unexpected source literal {literal}"),
    }
}
fn expected_private(literal: &str) -> WooV3WsPrivateTopic {
    match literal {
        "account" => WooV3WsPrivateTopic::Account,
        "balance" => WooV3WsPrivateTopic::Balance,
        "position" => WooV3WsPrivateTopic::Position,
        "executionreport" => WooV3WsPrivateTopic::Executionreport,
        "algoexecutionreport" => WooV3WsPrivateTopic::Algoexecutionreport,
        _ => panic!("unexpected source literal {literal}"),
    }
}
fn expected_plain(literal: &str) -> PlainKind {
    match literal {
        "orderbook10" => PlainKind::Orderbook10,
        "bbo" => PlainKind::Bbo,
        "bborpi" => PlainKind::Bborpi,
        "trade" => PlainKind::Trade,
        "ticker" => PlainKind::Ticker,
        "indexprice" => PlainKind::Indexprice,
        "markprice" => PlainKind::Markprice,
        "estfundingrate" => PlainKind::Estfundingrate,
        "openinterest" => PlainKind::Openinterest,
        _ => panic!("unexpected source literal {literal}"),
    }
}
fn assert_native(actual: &WooV3WsTopicDescriptor, expected: &Value) {
    match actual {
        WooV3WsTopicDescriptor::Book {
            channel,
            kind,
            symbol,
            depth,
            includes_rpi,
            topic,
        } => {
            assert_eq!(
                *channel,
                PublicChannel::Public,
                "WS-TOPIC native book channel"
            );
            let wanted = match expected["kind"].as_str().unwrap() {
                "orderbookupdate" => BookKind::Update,
                "orderbookupdaterpi" => BookKind::UpdateRpi,
                other => panic!("unexpected book kind {other}"),
            };
            assert_eq!(*kind, wanted, "WS-TOPIC native book kind");
            assert_eq!(
                symbol,
                expected["symbol"].as_str().unwrap(),
                "WS-TOPIC native book symbol"
            );
            assert_eq!(
                *depth,
                match expected["depth"].as_u64().unwrap() {
                    50 => WooV3WsBookDepth::D50,
                    200 => WooV3WsBookDepth::D200,
                    500 => WooV3WsBookDepth::D500,
                    _ => panic!("unexpected depth"),
                },
                "WS-TOPIC native book depth"
            );
            assert_eq!(
                *includes_rpi,
                expected["includesRpi"].as_bool().unwrap(),
                "WS-TOPIC native includes RPI"
            );
            assert_eq!(
                topic,
                expected["topic"].as_str().unwrap(),
                "WS-TOPIC native book topic"
            );
            assert_eq!(expected["channel"], "public");
        }
        WooV3WsTopicDescriptor::Kline {
            channel,
            kind,
            symbol,
            interval,
            topic,
        } => {
            assert_eq!(
                *channel,
                PublicChannel::Public,
                "WS-TOPIC native kline channel"
            );
            assert_eq!(*kind, KlineKind::Kline, "WS-TOPIC native kline kind");
            assert_eq!(
                symbol,
                expected["symbol"].as_str().unwrap(),
                "WS-TOPIC native kline symbol"
            );
            assert_eq!(
                *interval,
                expected_interval(expected["interval"].as_str().unwrap()),
                "WS-TOPIC native kline interval"
            );
            assert_eq!(
                topic,
                expected["topic"].as_str().unwrap(),
                "WS-TOPIC native kline topic"
            );
            assert_eq!(expected["channel"], "public");
            assert_eq!(expected["kind"], "kline");
        }
        WooV3WsTopicDescriptor::Public {
            channel,
            kind,
            symbol,
            topic,
        } => {
            assert_eq!(
                *channel,
                PublicChannel::Public,
                "WS-TOPIC native public channel"
            );
            assert_eq!(
                *kind,
                expected_plain(expected["kind"].as_str().unwrap()),
                "WS-TOPIC native public kind"
            );
            assert_eq!(
                symbol,
                expected["symbol"].as_str().unwrap(),
                "WS-TOPIC native public symbol"
            );
            assert_eq!(
                topic,
                expected["topic"].as_str().unwrap(),
                "WS-TOPIC native public topic"
            );
            assert_eq!(expected["channel"], "public");
        }
        WooV3WsTopicDescriptor::Private {
            channel,
            kind,
            topic,
        } => {
            assert_eq!(
                *channel,
                PrivateChannel::Private,
                "WS-TOPIC native private channel"
            );
            assert_eq!(
                *kind,
                expected_private(expected["kind"].as_str().unwrap()),
                "WS-TOPIC native private kind"
            );
            assert_eq!(
                *topic,
                expected_private(expected["topic"].as_str().unwrap()),
                "WS-TOPIC native private topic"
            );
            assert_eq!(expected["channel"], "private");
        }
    }
}
fn compare(actual: Result<WooV3WsTopicDescriptor, String>, row: &Value) {
    if let Some(expected) = row.get("ok") {
        let actual = actual.expect("WS-TOPIC native builder must succeed");
        assert_native(&actual, expected);
        let decoded: WooV3WsTopicDescriptor =
            safe_cex_num::numeric_ingress::js_from_str(&expected.to_string())
                .expect("WS-TOPIC native descriptor decode");
        assert_native(&decoded, expected);
    } else {
        assert_eq!(
            actual.unwrap_err(),
            row["error"]["message"].as_str().unwrap(),
            "WS-TOPIC native error message"
        );
        assert_eq!(
            row["error"]["name"],
            if row["input"].get("toString").is_some() {
                "TypeError"
            } else {
                "Error"
            }
        );
    }
}
#[test]
fn source_declarations_and_native_literals() {
    let source = oracle();
    use WooV3WsPublicTopicKind as P;
    let expected_public = [
        P::Orderbookupdate,
        P::Orderbookupdaterpi,
        P::Orderbook10,
        P::Bbo,
        P::Bborpi,
        P::Trade,
        P::Ticker,
        P::Kline,
        P::Indexprice,
        P::Markprice,
        P::Estfundingrate,
        P::Openinterest,
    ];
    use WooV3WsPrivateTopic as Q;
    let expected_private = [
        Q::Account,
        Q::Balance,
        Q::Position,
        Q::Executionreport,
        Q::Algoexecutionreport,
    ];
    use WooV3WsKlineInterval as I;
    let expected_intervals = [
        I::M1,
        I::M3,
        I::M5,
        I::M15,
        I::M30,
        I::H1,
        I::H2,
        I::H4,
        I::H6,
        I::H12,
        I::D1,
        I::D3,
        I::W1,
        I::Month1,
    ];
    assert_eq!(source["publicKinds"].as_array().unwrap().len(), 12);
    assert_eq!(source["privateKinds"].as_array().unwrap().len(), 5);
    assert_eq!(source["intervals"].as_array().unwrap().len(), 14);
    assert_eq!(
        WOO_V3_WS_PUBLIC_TOPIC_KINDS, expected_public,
        "WS-TOPIC native public constant order"
    );
    assert_eq!(
        WOO_V3_WS_PRIVATE_TOPICS, expected_private,
        "WS-TOPIC native private constant order"
    );
    for index in 0..12 {
        let actual: P = serde_json::from_value(source["publicKinds"][index].clone())
            .expect("WS-TOPIC native public enum decode");
        assert_eq!(
            actual, expected_public[index],
            "WS-TOPIC native public enum"
        );
    }
    for index in 0..5 {
        let actual: Q = serde_json::from_value(source["privateKinds"][index].clone()).unwrap();
        assert_eq!(
            actual, expected_private[index],
            "WS-TOPIC native private enum"
        );
    }
    for index in 0..14 {
        let actual: I = serde_json::from_value(source["intervals"][index].clone()).unwrap();
        assert_eq!(
            actual, expected_intervals[index],
            "WS-TOPIC native interval enum"
        );
    }
    for (literal, expected) in [
        ("public", WooV3WsChannel::Public),
        ("private", WooV3WsChannel::Private),
    ] {
        let actual: WooV3WsChannel = serde_json::from_value(json!(literal)).unwrap();
        assert_eq!(actual, expected, "WS-TOPIC native channel enum");
    }
    for (literal, expected) in [
        (50, WooV3WsBookDepth::D50),
        (200, WooV3WsBookDepth::D200),
        (500, WooV3WsBookDepth::D500),
    ] {
        let actual: WooV3WsBookDepth =
            safe_cex_num::numeric_ingress::js_from_str(&literal.to_string()).unwrap();
        assert_eq!(actual, expected, "WS-TOPIC native depth enum");
    }
    assert!(serde_json::from_value::<P>(json!("unknown")).is_err());
    assert!(serde_json::from_value::<Q>(json!("unknown")).is_err());
    assert!(serde_json::from_value::<I>(json!("1s")).is_err());
    assert!(serde_json::from_value::<WooV3WsChannel>(json!("unknown")).is_err());
    for raw in ["0", "51", "null", "\"50\"", "-0", "1e400"] {
        assert!(
            safe_cex_num::numeric_ingress::js_from_str::<WooV3WsBookDepth>(raw).is_err(),
            "WS-TOPIC native invalid depth/{raw}"
        );
    }
    for raw in ["{}", "[]", "{\"bbo\":null}", "true", "3"] {
        assert!(
            serde_json::from_str::<P>(raw).is_err(),
            "WS-TOPIC native string-only union/{raw}"
        );
    }
    let limits = WOO_V3_WS_LIMITS;
    assert_eq!(
        limits.maximum_connection_lifetime_ms, 86_400_000,
        "WS-TOPIC native lifetime"
    );
    assert_eq!(limits.maximum_connections_per_five_minutes_per_ip, 100);
    assert_eq!(limits.maximum_active_connections_per_ip, 100);
    assert_eq!(limits.maximum_topics_per_connection, 100);
    assert_eq!(
        limits.maximum_topics_per_command, 20,
        "WS-TOPIC native maximum command topics"
    );
    assert_eq!(
        limits.recommended_maximum_subscription_commands_per_second,
        200
    );
    assert_eq!(
        source["limits"],
        json!({"maximumConnectionLifetimeMs":86400000,"maximumConnectionsPerFiveMinutesPerIp":100,"maximumActiveConnectionsPerIp":100,"maximumTopicsPerConnection":100,"maximumTopicsPerCommand":20,"recommendedMaximumSubscriptionCommandsPerSecond":200})
    );
    assert_eq!(WOO_V3_WS_CHANNELS.len(), 2);
    for index in 0..2 {
        let (channel, config) = WOO_V3_WS_CHANNELS[index];
        let expected = if index == 0 {
            WooV3WsChannel::Public
        } else {
            WooV3WsChannel::Private
        };
        assert_eq!(channel, expected);
        let source_config = &source["channels"][channel.as_str()];
        assert_eq!(
            config.livenet_url,
            source_config["livenetUrl"].as_str().unwrap()
        );
        assert_eq!(
            config.testnet_url,
            source_config["testnetUrl"].as_str().unwrap()
        );
        assert_eq!(
            config.authentication,
            source_config["authentication"].as_str().unwrap()
        );
        assert_eq!(
            config.listen_key_endpoint,
            source_config["listenKeyEndpoint"].as_str()
        );
    }
}
#[test]
fn actual_source_topic_builders_and_parser() {
    let source = oracle();
    for (field, count) in [("buildRows", 44), ("privateRows", 11), ("parseRows", 56)] {
        assert_eq!(
            source[field].as_array().unwrap().len(),
            count,
            "WS-TOPIC independent floor/{field}"
        );
    }
    for index in 0..44 {
        let row = &source["buildRows"][index];
        let kind: WooV3WsPublicTopicKind = serde_json::from_value(row["kind"].clone()).unwrap();
        compare(
            build_woo_v3_public_topic(kind, &row["symbol"], row.get("option")),
            row,
        );
    }
    for index in 0..11 {
        let row = &source["privateRows"][index];
        compare(build_woo_v3_private_topic(&row["input"]), row);
    }
    for index in 0..56 {
        let row = &source["parseRows"][index];
        compare(parse_woo_v3_ws_topic(row.get("input")), row);
    }
    for raw in [
        "[]",
        "[\"public\",\"bbo\",\"s\",\"t\"]",
        "{\"channel\":\"private\",\"kind\":\"bbo\",\"topic\":\"bbo\"}",
        "{\"channel\":\"public\",\"kind\":\"orderbookupdate\",\"symbol\":\"s\",\"depth\":51,\"includesRpi\":false,\"topic\":\"t\"}",
    ] {
        assert!(
            safe_cex_num::numeric_ingress::js_from_str::<WooV3WsTopicDescriptor>(raw).is_err(),
            "WS-TOPIC native descriptor rejects/{raw}"
        );
    }
}
#[test]
fn actual_source_string_and_symbol_validation() {
    let source = oracle();
    for (field, count) in [("stringRows", 8), ("symbolRows", 8)] {
        assert_eq!(
            source[field].as_array().unwrap().len(),
            count,
            "WS-TOPIC independent validation floor"
        );
        for index in 0..count {
            let row = &source[field][index];
            let result = if field == "stringRows" {
                require_string(
                    row.get("input"),
                    "test.path",
                    row["allowEmpty"].as_bool().unwrap_or(false),
                )
            } else {
                require_symbol(row.get("input"), "test.symbol")
            };
            if let Some(expected) = row.get("ok") {
                assert_eq!(
                    result.unwrap(),
                    expected.as_str().unwrap(),
                    "WS-TOPIC native validation value"
                );
            } else {
                assert_eq!(
                    result.unwrap_err(),
                    row["error"]["message"].as_str().unwrap(),
                    "WS-TOPIC native validation error"
                );
            }
        }
    }
}
