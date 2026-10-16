use safe_cex_venue_apexomni::{interface, protocol::*};
use serde::de::{self, DeserializeOwned, Visitor};
use serde_json::{Value, json};
use std::{fmt, process::Command};

fn oracle() -> Value {
    static CORPUS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CORPUS.get_or_init(capture_oracle).clone()
}
fn capture_oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/protocol.cjs"))
        .output()
        .expect("Node protocol oracle must execute");
    assert!(
        output.status.success(),
        "live TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("oracle emits JSON")
}
#[derive(Debug)]
struct Inventory(Vec<String>);
impl fmt::Display for Inventory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "variant inventory")
    }
}
impl std::error::Error for Inventory {}
impl de::Error for Inventory {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self(vec![])
    }
    fn unknown_variant(_: &str, variants: &'static [&'static str]) -> Self {
        Self(variants.iter().map(|s| (*s).into()).collect())
    }
}
struct Unknown;
impl<'de> serde::Deserializer<'de> for Unknown {
    type Error = Inventory;
    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        visitor.visit_string("__UNKNOWN__".into())
    }
    serde::forward_to_deserialize_any! { bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any }
}
fn spellings<T: DeserializeOwned>() -> Vec<String> {
    match T::deserialize(Unknown) {
        Err(Inventory(values)) => {
            assert!(
                !values.is_empty(),
                "production variant inventory must exist"
            );
            values
        }
        Ok(_) => panic!("unknown literal accepted"),
    }
}
#[test]
fn protocol_matches_live_typescript() {
    let actual = json!({
        "tables": {
            "RECV_WINDOW": RECV_WINDOW, "BROKER_ID": BROKER_ID,
            "BASE_URL": BASE_URL, "BASE_WS_URL": BASE_WS_URL, "ENDPOINTS": ENDPOINTS,
            "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY, "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
            "INTERVAL": INTERVAL, "ORDER_STATUS": ORDER_STATUS, "ORDER_TYPE": ORDER_TYPE,
            "ORDER_SIDE": ORDER_SIDE, "ORDER_TIME_IN_FORCE": ORDER_TIME_IN_FORCE, "ORDER_TRIGGER_TYPE": ORDER_TRIGGER_TYPE,
            "ORDER_CANCEL_REASON": spellings::<OrderCancelReason>(),
        },
        "unions": { "ApexMarket": spellings::<interface::ApexMarket>(), "OrderSide": spellings::<interface::OrderSide>(), "OrderType": spellings::<interface::OrderType>(), "TimeInForce": spellings::<interface::TimeInForce>(), "OrderStatus": spellings::<interface::OrderStatus>() }
    });
    let mut expected = oracle();
    expected.as_object_mut().unwrap().remove("variants");
    expected.as_object_mut().unwrap().remove("marketAlias");
    let mut failures = Vec::new();
    let mut comparisons = 0;
    for section in ["tables", "unions"] {
        assert_eq!(
            actual[section].as_object().unwrap().len(),
            expected[section].as_object().unwrap().len(),
            "ApexOmni protocol row census"
        );
        for (name, value) in actual[section].as_object().unwrap() {
            comparisons += 1;
            if std::panic::catch_unwind(|| {
                if let Some(native) = value.as_array() {
                    let source = expected[section][name].as_array().unwrap();
                    let count = match name.as_str() {
                        "BASE_URL"
                        | "GRAFANA_ENDPOINTS_PERIODIC"
                        | "ORDER_SIDE"
                        | "ORDER_TRIGGER_TYPE"
                        | "OrderSide" => 2,
                        "BASE_WS_URL" | "ORDER_TIME_IN_FORCE" => 4,
                        "ENDPOINTS" | "INTERVAL" => 12,
                        "GRAFANA_ENDPOINTS_EVERY" | "OrderStatus" => 5,
                        "ORDER_STATUS" | "ORDER_TYPE" => 6,
                        "ORDER_CANCEL_REASON" => 7,
                        "ApexMarket" => 37,
                        "OrderType" | "TimeInForce" => 3,
                        _ => panic!("unclassified protocol collection {name}"),
                    };
                    assert_eq!(source.len(), count, "APEX-CARDINALITY {name} source total");
                    assert_eq!(
                        native.len(),
                        source.len(),
                        "APEX-CARDINALITY {name} expected/native length"
                    );
                    assert_eq!(
                        native.len(),
                        count,
                        "APEX-CARDINALITY {name} native comparison total"
                    );
                }
                assert_eq!(
                    value, &expected[section][name],
                    "ApexOmni protocol row {name}"
                )
            })
            .is_err()
            {
                failures.push(name.clone());
            }
        }
    }
    assert_eq!(
        comparisons, 19,
        "APEX-CARDINALITY protocol comparison total"
    );
    assert!(
        failures.is_empty(),
        "ApexOmni protocol values and accepted spellings: {failures:?}"
    );
}
fn strict<T: DeserializeOwned + serde::Serialize>(
    name: &str,
    literals: &Value,
    concrete: impl Fn(Value) -> bool,
) {
    let literals = literals.as_array().unwrap();
    for literal in literals {
        let value: T = serde_json::from_value(literal.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            *literal,
            "{name}: serialize spelling"
        );
        assert!(concrete(literal.clone()), "{name}: concrete positive");
        let text = literal.as_str().unwrap();
        for wrong in [
            json!({text: null}),
            json!([text]),
            json!(format!(" {text}")),
            json!(format!("{text} ")),
            json!(format!("\u{feff}{text}")),
            json!(format!("\u{85}{text}")),
            json!(text.to_lowercase()),
            json!(text.to_uppercase()),
        ] {
            if literals.contains(&wrong) {
                continue;
            }
            assert!(
                serde_json::from_value::<T>(wrong.clone()).is_err(),
                "{name}: accepted {wrong}"
            );
            assert!(
                !concrete(wrong.clone()),
                "{name}: concrete accepted {wrong}"
            );
        }
    }
    for wrong in [
        json!(null),
        json!(true),
        json!(false),
        json!(0),
        json!([]),
        json!({}),
        json!("UNKNOWN"),
    ] {
        assert!(
            serde_json::from_value::<T>(wrong.clone()).is_err(),
            "{name}: accepted {wrong}"
        );
        assert!(
            !concrete(wrong.clone()),
            "{name}: concrete accepted {wrong}"
        );
    }
}
#[test]
fn public_unions_reject_extra_shapes() {
    use serde::Deserialize;
    let expected = oracle();
    strict::<OrderCancelReason>(
        "OrderCancelReason",
        &expected["tables"]["ORDER_CANCEL_REASON"],
        |v| OrderCancelReason::deserialize(v).is_ok(),
    );
    strict::<interface::ApexMarket>("ApexMarket", &expected["unions"]["ApexMarket"], |v| {
        interface::ApexMarket::deserialize(v).is_ok()
    });
    strict::<interface::OrderSide>("OrderSide", &expected["unions"]["OrderSide"], |v| {
        interface::OrderSide::deserialize(v).is_ok()
    });
    strict::<interface::TimeInForce>("TimeInForce", &expected["unions"]["TimeInForce"], |v| {
        interface::TimeInForce::deserialize(v).is_ok()
    });
    strict::<interface::OrderStatus>("OrderStatus", &expected["unions"]["OrderStatus"], |v| {
        interface::OrderStatus::deserialize(v).is_ok()
    });
    strict::<interface::OrderType>("OrderType", &expected["unions"]["OrderType"], |v| {
        interface::OrderType::deserialize(v).is_ok()
    });
}
#[test]
fn live_typescript_change_fails_named_assertion() {
    for mutation in ["protocol", "ws-add"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "protocol_matches_live_typescript", "--nocapture"])
            .env("APEXOMNI_MUTATE_TS", mutation)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(101),
            "oracle mutation must fail Rust assertion"
        );
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("protocol_matches_live_typescript")
                && text.contains("FAILED")
                && text.contains("ApexOmni protocol values and accepted spellings"),
            "{text}"
        );
    }
}

#[test]
fn decoded_enum_variants_match_live_typescript_members() {
    let reference = oracle();
    macro_rules! variants {
        ($ty:ty, $name:literal, $( $variant:path => $member:literal ),+ $(,)?) => {{
            let members = reference["variants"][$name].as_object().unwrap();
            let cases = [$(($variant, $member)),+];
            assert_eq!(cases.len(), members.len(), "{} native variant census", $name);
            for (expected, member) in cases {
                let actual: $ty = serde_json::from_value(members[member].clone()).unwrap();
                assert_eq!(actual, expected, "{}.{} decoded native variant", $name, member);
                assert_eq!(serde_json::to_value(expected).unwrap(), members[member], "{}.{} named wire value", $name, member);
            }
        }};
    }
    variants!(interface::ApexMarket, "ApexMarket", interface::ApexMarket::BtcUsdt => "BTC_USDT", interface::ApexMarket::BtcUsdc => "BTC_USDC", interface::ApexMarket::BtcUsd => "BTC_USD", interface::ApexMarket::EthUsd => "ETH_USD", interface::ApexMarket::EthUsdc => "ETH_USDC", interface::ApexMarket::LinkUsd => "LINK_USD", interface::ApexMarket::AaveUsd => "AAVE_USD", interface::ApexMarket::UniUsd => "UNI_USD", interface::ApexMarket::SushiUsd => "SUSHI_USD", interface::ApexMarket::SolUsd => "SOL_USD", interface::ApexMarket::YfiUsd => "YFI_USD", interface::ApexMarket::OneinchUsd => "ONEINCH_USD", interface::ApexMarket::AvaxUsd => "AVAX_USD", interface::ApexMarket::SnxUsd => "SNX_USD", interface::ApexMarket::CrvUsd => "CRV_USD", interface::ApexMarket::UmaUsd => "UMA_USD", interface::ApexMarket::DotUsd => "DOT_USD", interface::ApexMarket::DogeUsd => "DOGE_USD", interface::ApexMarket::MaticUsd => "MATIC_USD", interface::ApexMarket::MkrUsd => "MKR_USD", interface::ApexMarket::FilUsd => "FIL_USD", interface::ApexMarket::AdaUsd => "ADA_USD", interface::ApexMarket::AtomUsd => "ATOM_USD", interface::ApexMarket::CompUsd => "COMP_USD", interface::ApexMarket::BchUsd => "BCH_USD", interface::ApexMarket::LtcUsd => "LTC_USD", interface::ApexMarket::EosUsd => "EOS_USD", interface::ApexMarket::AlgoUsd => "ALGO_USD", interface::ApexMarket::ZrxUsd => "ZRX_USD", interface::ApexMarket::XmrUsd => "XMR_USD", interface::ApexMarket::ZecUsd => "ZEC_USD", interface::ApexMarket::EnjUsd => "ENJ_USD", interface::ApexMarket::EtcUsd => "ETC_USD", interface::ApexMarket::XlmUsd => "XLM_USD", interface::ApexMarket::TrxUsd => "TRX_USD", interface::ApexMarket::XtzUsd => "XTZ_USD", interface::ApexMarket::HntUsd => "HNT_USD");
    variants!(interface::OrderSide, "OrderSide", interface::OrderSide::Buy => "BUY", interface::OrderSide::Sell => "SELL");
    variants!(interface::OrderType, "OrderType", interface::OrderType::Limit => "LIMIT", interface::OrderType::Market => "MARKET", interface::OrderType::Condition => "CONDITION");
    variants!(OrderCancelReason, "ORDER_CANCEL_REASON", OrderCancelReason::UnknownOrderCancelReason => "UNKNOWN_ORDER_CANCEL_REASON", OrderCancelReason::Expired => "EXPIRED", OrderCancelReason::UserCanceled => "USER_CANCELED", OrderCancelReason::CouldNotFill => "COULD_NOT_FILL", OrderCancelReason::ReduceOnlyCanceled => "REDUCE_ONLY_CANCELED", OrderCancelReason::LiquidateCanceled => "LIQUIDATE_CANCELED", OrderCancelReason::InternalFailed => "INTERNAL_FAILED");
    variants!(interface::TimeInForce, "TimeInForce", interface::TimeInForce::Gtt => "GTT", interface::TimeInForce::Fok => "FOK", interface::TimeInForce::Ioc => "IOC");
    variants!(interface::OrderStatus, "OrderStatus", interface::OrderStatus::Pending => "PENDING", interface::OrderStatus::Open => "OPEN", interface::OrderStatus::Filled => "FILLED", interface::OrderStatus::Canceled => "CANCELED", interface::OrderStatus::Untriggered => "UNTRIGGERED");
}

#[test]
fn oracles_require_explicit_typescript_path() {
    for script in ["protocol.cjs", "records.cjs"] {
        let output = Command::new("node")
            .arg(format!("{}/scripts/{script}", env!("CARGO_MANIFEST_DIR")))
            .env_remove("SAFE_CEX_TYPESCRIPT_PATH")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{script}: missing compiler must fail"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("SAFE_CEX_TYPESCRIPT_PATH is required"),
            "{script}: named compiler prerequisite"
        );
    }
}

#[test]
fn market_alias_matches_live_typescript() {
    use std::any::TypeId;
    let expected = oracle();
    assert_eq!(
        expected["marketAlias"]["identity"], true,
        "Market actual TypeScript identity"
    );
    assert_eq!(
        expected["marketAlias"]["typeIdentity"], true,
        "ApexOmni protocol row Market type: TypeScript declared alias identity"
    );
    let same_type = TypeId::of::<interface::Market>() == TypeId::of::<interface::ApexMarket>();
    let mut failures = Vec::new();
    for row in ["Market type", "Market const"] {
        if std::panic::catch_unwind(|| {
            assert!(
                same_type,
                "ApexOmni protocol row {row}: enum alias identity"
            )
        })
        .is_err()
        {
            failures.push(row);
        }
    }
    assert!(failures.is_empty(), "Market alias rows: {failures:?}");
    let members = expected["marketAlias"]["members"].as_array().unwrap();
    assert_eq!(members.len(), 37, "APEX-CARDINALITY Market source total");
    let source_members: serde_json::Map<String, Value> = members
        .iter()
        .map(|pair| (pair[0].as_str().unwrap().to_owned(), pair[1].clone()))
        .collect();
    assert_eq!(
        serde_json::Value::Object(source_members),
        expected["variants"]["ApexMarket"],
        "APEX-CARDINALITY Market independent member inventory"
    );
    for pair in expected["marketAlias"]["members"].as_array().unwrap() {
        let actual: interface::Market =
            serde_json::from_value(pair[1].clone()).expect("Market alias member accepted");
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            pair[1],
            "Market alias member wire value"
        );
    }
}

#[test]
fn changed_typescript_market_type_fails_named_assertion() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "market_alias_matches_live_typescript",
            "--nocapture",
        ])
        .env("APEXOMNI_MUTATE_TS", "market-type")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("test result: FAILED")
            && text
                .contains("ApexOmni protocol row Market type: TypeScript declared alias identity"),
        "{text}"
    );
}
