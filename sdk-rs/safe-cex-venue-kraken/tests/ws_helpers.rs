use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{ProtoMember, TableHit};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{
    Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
};
use safe_cex_venue_kraken::ws_helpers;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/ws-helpers-oracle.cjs"
            ))
            .output()
            .expect("actual public helper source");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!value["header"]["node"].as_str().unwrap().is_empty());
        assert!(!value["header"]["v8"].as_str().unwrap().is_empty());
        assert!(!value["header"]["arch"].as_str().unwrap().is_empty());
        assert_eq!(value["header"]["source"].as_str().unwrap().len(), 64);
        value
    })
}

fn market(symbol: &str, active: bool) -> Market {
    Market {
        runtime: Default::default(),
        id: symbol.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: "BASE".into(),
        quote: "QUOTE".into(),
        active,
        precision: MarketPrecision {
            base: None,
            quote: None,
            amount: 0.125,
            price: 0.25,
        },
        limits: MarketLimits {
            cost: None,
            amount: MarketLimitsAmount {
                min: 1.0,
                max: 10.0,
            },
            leverage: MarketLimitsLeverage {
                min: 1.0,
                max: 20.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
        info: None,
        option: None,
    }
}

#[test]
fn product_ids_live_source() {
    let cases = oracle()["products"].as_array().unwrap();
    assert_eq!(cases.len(), 9, "PUBLIC-HELPER product source cases");
    let counts = [
        Some(0),
        Some(7),
        Some(1),
        Some(2),
        Some(7),
        Some(3),
        Some(5),
        Some(2),
        None,
    ];
    let mut total = 0;
    for index in 0..9 {
        let case = &cases[index];
        let store = DefaultStore::new();
        let markets: Vec<_> = case["symbols"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, symbol)| market(symbol.as_str().unwrap(), i != 3))
            .collect();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let trace = RefCell::new(Vec::new());
        let mut error = ExchangeError::new("active symbols failure");
        error.metadata.store_subscriber_ids = vec![71, 73];
        let pointer = error.metadata.as_ref() as *const _;
        let result = ws_helpers::get_ticker_product_ids(&store, || {
            trace.borrow_mut().push("active");
            if case["replace"] == true {
                Store::update(
                    &store,
                    StoreUpdate {
                        markets: Some(vec![market("CHANGED", true)]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            if case["fail"] == true {
                return Err(error);
            }
            Ok(case["active"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect())
        });
        assert_eq!(
            *trace.borrow(),
            vec!["active"],
            "PUBLIC-HELPER independent native callback count"
        );
        assert_eq!(
            case["trace"],
            json!(["active"]),
            "PUBLIC-HELPER source callback count"
        );
        let after: Vec<_> = store.markets().iter().map(|m| m.symbol.clone()).collect();
        assert_eq!(
            json!(after),
            case["after"],
            "PUBLIC-HELPER continuation Store mutation"
        );
        if let Some(count) = counts[index] {
            let ids = result.unwrap();
            assert_eq!(
                ids.len(),
                count,
                "PUBLIC-HELPER independent native product count"
            );
            assert_eq!(
                case["output"].as_array().unwrap().len(),
                count,
                "PUBLIC-HELPER source product count"
            );
            for row in 0..count {
                assert_eq!(
                    ids[row].encode_utf16().collect::<Vec<_>>(),
                    case["output"][row]
                        .as_str()
                        .unwrap()
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                    "PUBLIC-HELPER native product id case {index} row {row}"
                );
                total += 1;
            }
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.message,
                case["error"]["message"].as_str().unwrap(),
                "PUBLIC-HELPER native callback error"
            );
            assert!(
                std::ptr::eq(pointer, error.metadata.as_ref()),
                "PUBLIC-HELPER native error allocation"
            );
            assert_eq!(
                error.metadata.store_subscriber_ids,
                vec![71, 73],
                "PUBLIC-HELPER native error metadata"
            );
            assert_eq!(
                case["error"]["same"], true,
                "PUBLIC-HELPER source error allocation"
            );
        }
    }
    assert_eq!(total, 27, "PUBLIC-HELPER completed native products");
}

#[test]
fn timeframe_live_source() {
    let cases = oracle()["timeframes"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "PUBLIC-HELPER timeframe source cases");
    let members = [
        ProtoMember::Constructor,
        ProtoMember::DefineGetter,
        ProtoMember::DefineSetter,
        ProtoMember::HasOwnProperty,
        ProtoMember::LookupGetter,
        ProtoMember::LookupSetter,
        ProtoMember::IsPrototypeOf,
        ProtoMember::PropertyIsEnumerable,
        ProtoMember::ToString,
        ProtoMember::ValueOf,
        ProtoMember::Prototype,
        ProtoMember::ToLocaleString,
    ];
    let values: [f64; 12] = [
        1.0, 3.0, 5.0, 15.0, 30.0, 60.0, 120.0, 240.0, 360.0, 720.0, 1440.0, 10080.0,
    ];
    let mut numbers = 0;
    let mut inherited = 0;
    for index in 0..32 {
        let case = &cases[index];
        let units: Vec<u16> = serde_json::from_value(case["input"].clone()).unwrap();
        match ws_helpers::map_timeframe(&units) {
            TableHit::Mapped(value) => {
                let expected = if index < 12 { values[index] } else { 1.0 };
                assert_eq!(
                    value.to_bits(),
                    expected.to_bits(),
                    "PUBLIC-HELPER native timeframe bits case {index}"
                );
                assert_eq!(
                    case["output"]["kind"], "number",
                    "PUBLIC-HELPER source numeric kind"
                );
                assert_eq!(
                    format!("{:016x}", value.to_bits()),
                    case["output"]["bits"].as_str().unwrap(),
                    "PUBLIC-HELPER source/native timeframe bits"
                );
                numbers += 1;
            }
            TableHit::Inherited(member) => {
                assert!(
                    (12..24).contains(&index),
                    "PUBLIC-HELPER unexpected native inherited case"
                );
                assert_eq!(
                    member,
                    members[index - 12],
                    "PUBLIC-HELPER native inherited identity"
                );
                if let Some(length) = member.length() {
                    assert_eq!(case["output"]["kind"], "function");
                    assert_eq!(
                        member.js_string(),
                        case["output"]["text"].as_str().unwrap(),
                        "PUBLIC-HELPER native inherited callable"
                    );
                    assert_eq!(
                        u64::from(length),
                        case["output"]["length"].as_u64().unwrap(),
                        "PUBLIC-HELPER native callable arity"
                    );
                } else {
                    assert_eq!(case["output"]["kind"], "object");
                    assert_eq!(case["output"]["identity"], "Object.prototype");
                }
                inherited += 1;
            }
            TableHit::Missing => panic!("PUBLIC-HELPER native nullish fallback missing"),
        }
    }
    assert_eq!(numbers, 20, "PUBLIC-HELPER completed numeric outputs");
    assert_eq!(inherited, 12, "PUBLIC-HELPER completed inherited outputs");
}
