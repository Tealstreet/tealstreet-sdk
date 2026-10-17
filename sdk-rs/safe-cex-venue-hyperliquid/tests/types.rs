use safe_cex_venue_hyperliquid::types::*;
use serde::Deserialize;
use serde_json::Value;
fn semantically_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Object(a), Value::Object(b)) => {
            a.iter()
                .all(|(k, v)| b.get(k).is_some_and(|w| semantically_equal(v, w)))
                && b.iter().all(|(k, v)| a.contains_key(k) || v.is_null())
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(v, w)| semantically_equal(v, w))
        }
        _ => a == b,
    }
}
#[derive(Deserialize)]
struct Case {
    name: String,
    raw: String,
}
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[test]
fn every_ts_type_parses_its_raw_source_shape() {
    let corpus: Corpus = serde_json::from_str(include_str!("fixtures/types.json")).unwrap();
    assert_eq!(corpus.cases.len(), 78, "TYPE-CARDINALITY source total");
    assert_eq!(
        corpus
            .cases
            .iter()
            .map(|case| case.name.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        42,
        "TYPE-CARDINALITY source roots"
    );
    let mut comparisons = 0;
    for index in 0..78 {
        let case = corpus
            .cases
            .get(index)
            .expect("TYPE-CARDINALITY native index coverage");
        let original: Value = serde_json::from_str(&case.raw).unwrap();
        let roundtrip = match case.name.as_str() {
            "HyperliquidUserAbstractionMode" => serde_json::to_value(
                serde_json::from_str::<HyperliquidUserAbstractionMode>(&case.raw).unwrap(),
            )
            .unwrap(),
            "Tif" => serde_json::to_value(serde_json::from_str::<Tif>(&case.raw).unwrap()).unwrap(),
            "Tpsl" => {
                serde_json::to_value(serde_json::from_str::<Tpsl>(&case.raw).unwrap()).unwrap()
            }
            "LimitOrderType" => {
                serde_json::to_value(serde_json::from_str::<LimitOrderType>(&case.raw).unwrap())
                    .unwrap()
            }
            "TriggerOrderType" => {
                serde_json::to_value(serde_json::from_str::<TriggerOrderType>(&case.raw).unwrap())
                    .unwrap()
            }
            "Grouping" => {
                serde_json::to_value(serde_json::from_str::<Grouping>(&case.raw).unwrap()).unwrap()
            }
            "OrderType" => {
                serde_json::to_value(serde_json::from_str::<OrderType>(&case.raw).unwrap()).unwrap()
            }
            "Cloid" => {
                serde_json::to_value(serde_json::from_str::<Cloid>(&case.raw).unwrap()).unwrap()
            }
            "AllMids" => {
                serde_json::to_value(serde_json::from_str::<AllMids>(&case.raw).unwrap()).unwrap()
            }
            "ClearinghouseState" => {
                serde_json::to_value(serde_json::from_str::<ClearinghouseState>(&case.raw).unwrap())
                    .unwrap()
            }
            "UserFills" => {
                serde_json::to_value(serde_json::from_str::<UserFills>(&case.raw).unwrap()).unwrap()
            }
            "WsTrade" => {
                serde_json::to_value(serde_json::from_str::<WsTrade>(&case.raw).unwrap()).unwrap()
            }
            "WsBook" => {
                serde_json::to_value(serde_json::from_str::<WsBook>(&case.raw).unwrap()).unwrap()
            }
            "WsBbo" => {
                serde_json::to_value(serde_json::from_str::<WsBbo>(&case.raw).unwrap()).unwrap()
            }
            "WsLevel" => {
                serde_json::to_value(serde_json::from_str::<WsLevel>(&case.raw).unwrap()).unwrap()
            }
            "WsOrder" => {
                serde_json::to_value(serde_json::from_str::<WsOrder>(&case.raw).unwrap()).unwrap()
            }
            "WsUserEvent" => {
                // This is one JSON-representable funding alternative. The four
                // native alternatives, including array-owned user, have their
                // own independent source/type witness in finding_user_event.
                let event = WsUserEvent::Funding {
                    entry: serde_json::from_str(&case.raw).unwrap(),
                    user: safe_cex_venue_hyperliquid::spot_json::JsString(
                        "wallet".encode_utf16().collect(),
                    ),
                };
                assert_eq!(
                    event.user().0,
                    "wallet".encode_utf16().collect::<Vec<_>>(),
                    "EVENT legacy native attached user"
                );
                let WsUserEvent::Funding { entry, user } = event else {
                    unreachable!()
                };
                assert_eq!(
                    entry.time.to_bits(),
                    17.0f64.to_bits(),
                    "EVENT legacy native funding time"
                );
                for value in [&entry.coin, &entry.usdc, &entry.szi, &entry.funding_rate] {
                    assert_eq!(
                        value.0,
                        "wire-雪\0".encode_utf16().collect::<Vec<_>>(),
                        "EVENT legacy native funding string"
                    );
                }
                let mut wire = serde_json::to_value(entry).unwrap();
                wire["user"] = serde_json::to_value(user).unwrap();
                wire
            }
            "WsFill" => {
                serde_json::to_value(serde_json::from_str::<WsFill>(&case.raw).unwrap()).unwrap()
            }
            "WsUserFunding" => {
                serde_json::to_value(serde_json::from_str::<WsUserFunding>(&case.raw).unwrap())
                    .unwrap()
            }
            "WsLiquidation" => {
                serde_json::to_value(serde_json::from_str::<WsLiquidation>(&case.raw).unwrap())
                    .unwrap()
            }
            "WsNonUserCancel" => {
                serde_json::to_value(serde_json::from_str::<WsNonUserCancel>(&case.raw).unwrap())
                    .unwrap()
            }
            "UserRateLimit" => {
                serde_json::to_value(serde_json::from_str::<UserRateLimit>(&case.raw).unwrap())
                    .unwrap()
            }
            "OrderStatus" => {
                serde_json::to_value(serde_json::from_str::<OrderStatus>(&case.raw).unwrap())
                    .unwrap()
            }
            "L2Book" => {
                serde_json::to_value(serde_json::from_str::<L2Book>(&case.raw).unwrap()).unwrap()
            }
            "FundingHistoryEntry" => serde_json::to_value(
                serde_json::from_str::<FundingHistoryEntry>(&case.raw).unwrap(),
            )
            .unwrap(),
            "FundingHistory" => {
                serde_json::to_value(serde_json::from_str::<FundingHistory>(&case.raw).unwrap())
                    .unwrap()
            }
            "UserOpenOrder" => {
                serde_json::to_value(serde_json::from_str::<UserOpenOrder>(&case.raw).unwrap())
                    .unwrap()
            }
            "UserOpenOrders" => {
                serde_json::to_value(serde_json::from_str::<UserOpenOrders>(&case.raw).unwrap())
                    .unwrap()
            }
            "CancelOrderRequest" => {
                serde_json::to_value(serde_json::from_str::<CancelOrderRequest>(&case.raw).unwrap())
                    .unwrap()
            }
            "CancelOrderRequests" => serde_json::to_value(
                serde_json::from_str::<CancelOrderRequests>(&case.raw).unwrap(),
            )
            .unwrap(),
            "CancelByCloidRequest" => serde_json::to_value(
                serde_json::from_str::<CancelByCloidRequest>(&case.raw).unwrap(),
            )
            .unwrap(),
            "Signature" => {
                serde_json::to_value(serde_json::from_str::<Signature>(&case.raw).unwrap()).unwrap()
            }
            "Notification" => {
                serde_json::to_value(serde_json::from_str::<Notification>(&case.raw).unwrap())
                    .unwrap()
            }
            "Candle" => {
                serde_json::to_value(serde_json::from_str::<Candle>(&case.raw).unwrap()).unwrap()
            }
            "HyperliquidCandle" => {
                serde_json::to_value(serde_json::from_str::<HyperliquidCandle>(&case.raw).unwrap())
                    .unwrap()
            }
            "WsUserFill" => {
                serde_json::to_value(serde_json::from_str::<WsUserFill>(&case.raw).unwrap())
                    .unwrap()
            }
            "WsUserFills" => {
                serde_json::to_value(serde_json::from_str::<WsUserFills>(&case.raw).unwrap())
                    .unwrap()
            }
            "WsUserFundings" => {
                serde_json::to_value(serde_json::from_str::<WsUserFundings>(&case.raw).unwrap())
                    .unwrap()
            }
            "WsUserNonFundingLedgerUpdate" => serde_json::to_value(
                serde_json::from_str::<WsUserNonFundingLedgerUpdate>(&case.raw).unwrap(),
            )
            .unwrap(),
            "WsUserNonFundingLedgerUpdates" => serde_json::to_value(
                serde_json::from_str::<WsUserNonFundingLedgerUpdates>(&case.raw).unwrap(),
            )
            .unwrap(),
            "BuilderInfo" => {
                serde_json::to_value(serde_json::from_str::<BuilderInfo>(&case.raw).unwrap())
                    .unwrap()
            }
            "BuilderFeeApprovalResult" => serde_json::to_value(
                serde_json::from_str::<BuilderFeeApprovalResult>(&case.raw).unwrap(),
            )
            .unwrap(),
            _ => panic!("unmapped TS type {}", case.name),
        };
        assert!(
            semantically_equal(&roundtrip, &original),
            "raw type roundtrip {}",
            case.name
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 78, "TYPE-CARDINALITY completed comparisons");
}
#[test]
fn endpoint_tables_match_live_ts() {
    let corpus: Value = serde_json::from_str(include_str!("fixtures/types.json")).unwrap();
    assert_eq!(
        serde_json::to_value(
            BASE_URLS
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        )
        .unwrap(),
        corpus["tables"]["BASE_URLS"]
    );
    assert_eq!(
        serde_json::to_value(
            WSS_URLS
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        )
        .unwrap(),
        corpus["tables"]["WSS_URLS"]
    );
    assert_eq!(
        serde_json::to_value(
            ENDPOINTS
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        )
        .unwrap(),
        corpus["tables"]["ENDPOINTS"]
    );
    assert_eq!(
        serde_json::to_value(GRAFANA_ENDPOINTS_EVERY).unwrap(),
        corpus["tables"]["GRAFANA_ENDPOINTS_EVERY"]
    );
    assert_eq!(
        serde_json::to_value(GRAFANA_ENDPOINTS_PERIODIC).unwrap(),
        corpus["tables"]["GRAFANA_ENDPOINTS_PERIODIC"]
    );
    assert_eq!(
        serde_json::to_value(
            INFO_TYPES
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        )
        .unwrap(),
        corpus["tables"]["INFO_TYPES"]
    );
    assert_eq!(
        serde_json::to_value(
            EXCHANGE_TYPES
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        )
        .unwrap(),
        corpus["tables"]["EXCHANGE_TYPES"]
    );
    assert_eq!(
        serde_json::to_value(
            WEBSOCKET
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect::<serde_json::Map<String, serde_json::Value>>()
        )
        .unwrap(),
        corpus["tables"]["WEBSOCKET"]
    );
}
#[test]
fn live_ts_oracle_is_not_a_snapshot_only_test() {
    let status = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/oracle.cjs"))
        .arg("--check")
        .status()
        .unwrap();
    assert!(status.success(), "live TS type/table oracle drift");
}

#[test]
fn typed_numeric_overflow_matches_js_ingress() {
    let overflow: BuilderInfo = serde_json::from_str(r#"{"b":"wallet","f":1e400}"#).unwrap();
    assert_eq!(overflow.f.to_bits(), f64::INFINITY.to_bits());
    let finite: BuilderInfo =
        serde_json::from_str(r#"{"b":"wallet","f":1.7976931348623157e308}"#).unwrap();
    assert!(finite.f.is_finite());
}
