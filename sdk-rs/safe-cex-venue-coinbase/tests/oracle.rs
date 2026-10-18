#[path = "support/schema.rs"]
mod schema;
#[path = "support/wire_identity.rs"]
mod wire_identity;
use safe_cex_venue_coinbase::{tables::*, types::*};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
use wire_identity::{
    NativeWireIdentity, assert_native_identity_and_fields, assert_wire_corpus_content,
    complete_identity_calls, counted_identity, reset_complete_identity_calls,
};

struct ConsumingVec<T, const KEEP: usize>(Vec<T>);

impl<'de, T: Deserialize<'de>, const KEEP: usize> Deserialize<'de> for ConsumingVec<T, KEEP> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ConsumeVisitor<T, const KEEP: usize>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const KEEP: usize> serde::de::Visitor<'de>
            for ConsumeVisitor<T, KEEP>
        {
            type Value = ConsumingVec<T, KEEP>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an array")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut retained = Vec::new();
                while let Some(value) = sequence.next_element::<T>()? {
                    if retained.len() < KEEP {
                        retained.push(value);
                    }
                }
                Ok(ConsumingVec(std::mem::take(&mut retained)))
            }
        }
        deserializer.deserialize_seq(ConsumeVisitor::<T, KEEP>(std::marker::PhantomData))
    }
}

fn assert_consuming_collection_mutant_reaches_length_guard<const KEEP: usize>(raw: &Value) {
    let decoded: ConsumingVec<StopDirection, KEEP> = serde_json::from_value(raw.clone()).unwrap();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        decoded
            .0
            .assert_native_wire_identity(raw, "CONSUMING_CONTROL");
    }))
    .expect_err("consuming collection mutant must fail native identity");
    let message = failure
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| failure.downcast_ref::<&str>().copied())
        .unwrap_or("non-string panic");
    assert!(
        message.contains("CONSUMING_CONTROL decoded array length"),
        "mutant must be rejected by the native cardinality assertion, got {message}"
    );
}

#[test]
fn consuming_empty_and_subset_collection_mutants_hit_named_cardinality_guard() {
    let raw = json!(["STOP_DIRECTION_STOP_UP", "STOP_DIRECTION_STOP_DOWN"]);
    assert_consuming_collection_mutant_reaches_length_guard::<0>(&raw);
    assert_consuming_collection_mutant_reaches_length_guard::<1>(&raw);
}

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/oracle.cjs"))
            .output()
            .expect("Node is required for the maintained Coinbase TS oracle");
        assert!(
            output.status.success(),
            "Coinbase TS oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("Coinbase TS oracle JSON")
    })
}

fn check_wire<T: DeserializeOwned + Serialize + NativeWireIdentity>(
    name: &str,
    rust_name: &str,
    identity: impl Fn(&T, &Value),
) {
    let mut rust_schema = schema::wire_schema(include_str!("../src/types.rs"), rust_name);
    let mut source_schema = oracle()["schemas"][name].clone();
    if name == "PLACE_ORDER_REQUEST" {
        assert_eq!(
            source_schema["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|field| field["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "client_order_id",
                "product_id",
                "side",
                "order_configuration",
                "self_trade_prevention_id",
                "leverage",
                "margin_type",
                "retail_portfolio_id"
            ],
            "PLACE_ORDER_REQUEST: source declaration order"
        );
        // Declaration fields retain their complete schemas; formatOrder owns wire order.
        // The production transport regression compares those exact serialized bytes.
        for schema in [&mut rust_schema, &mut source_schema] {
            schema["fields"]
                .as_array_mut()
                .unwrap()
                .sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        }
    }
    assert_eq!(
        rust_schema, source_schema,
        "{name}: complete declaration schema"
    );
    let cases = oracle()["types"][name].as_array().expect("TS type exists");
    let expected_count = match name {
        "BALANCE_RESPONSE" => 244,
        "BALANCE_RESPONSE_CFM" => 363,
        "CANDLES_RESPONSE" => 59,
        "CANDLE_EVENT" => 75,
        "EDIT_ORDER_REQUEST" => 27,
        "EDIT_ORDER_RESPONSE" => 38,
        "INTX_INSTRUMENTS_RESPONSE" => 297,
        "LEVEL2_EVENT" => 63,
        "LIST_ACCOUNTS_RESPONSE" => 122,
        "LIST_ORDERS_RESPONSE" => 365,
        "LIST_PORTFOLIOS_RESPONSE" => 45,
        "LIST_POSITIONS_CFM_RESPONSE" => 75,
        "LIST_POSITIONS_INTX_RESPONSE" => 509,
        "MARKET_RESPONSE" => 392,
        "ORDER_EVENT" => 147,
        "PLACE_ORDER_REQUEST" => 315,
        "PLACE_ORDER_RESPONSE" => 339,
        "PORTFOLIO_BREAKDOWN_RESPONSE" => 540,
        "STOP_DIRECTION" => 8,
        "TICKER_EVENT" => 89,
        "TRADE_EVENT" => 70,
        _ => panic!("unclassified Coinbase wire corpus: {name}"),
    };
    assert_eq!(cases.len(), expected_count, "{name}: source case total");
    assert!(!cases.is_empty(), "{name}: missing oracle cases");
    reset_complete_identity_calls();
    let identity = counted_identity(identity);
    let mut decoded_cases = Vec::new();
    for case in cases {
        let actual = serde_json::from_value::<T>(case["value"].clone());
        assert_eq!(
            actual.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}: {:?}",
            case["name"],
            actual.as_ref().err()
        );
        if let Ok(value) = actual {
            if case["name"].as_str().unwrap().ends_with(": complete") {
                assert_native_identity_and_fields(&value, &case["value"], name, &identity);
            } else {
                value.assert_native_wire_identity(&case["value"], name);
            }
            decoded_cases.push((value, case));
        }
    }
    let expected_identity_calls = match name {
        "EDIT_ORDER_RESPONSE" => 6,
        "CANDLES_RESPONSE" => 9,
        "CANDLE_EVENT" => 11,
        "LEVEL2_EVENT" => 9,
        "EDIT_ORDER_REQUEST" => 4,
        "LIST_ACCOUNTS_RESPONSE" => 18,
        "BALANCE_RESPONSE" => 35,
        "INTX_INSTRUMENTS_RESPONSE" => 43,
        "LIST_PORTFOLIOS_RESPONSE" => 7,
        "BALANCE_RESPONSE_CFM" => 52,
        "LIST_POSITIONS_CFM_RESPONSE" => 11,
        "LIST_ORDERS_RESPONSE" => 47,
        "ORDER_EVENT" => 19,
        "TICKER_EVENT" => 13,
        "TRADE_EVENT" => 10,
        "MARKET_RESPONSE" => 57,
        "PLACE_ORDER_RESPONSE" => 48,
        "LIST_POSITIONS_INTX_RESPONSE" => 73,
        "PORTFOLIO_BREAKDOWN_RESPONSE" => 78,
        "PLACE_ORDER_REQUEST" => 44,
        _ => 1,
    };
    assert_eq!(
        complete_identity_calls(),
        expected_identity_calls,
        "{name}: native identity assertion count"
    );
    // Every native assertion, including the execution floor, precedes wire checks.
    assert_wire_corpus_content(name, &oracle()["types"][name]);
    for (value, case) in decoded_cases {
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            case["value"],
            "{}: roundtrip",
            case["name"]
        );
    }
}
fn table<T: Serialize>(pairs: &[(&str, T)]) -> Value {
    serde_json::to_value(pairs).unwrap()
}

#[test]
fn wire_market_response() {
    check_wire::<MarketResponse>("MARKET_RESPONSE", "MarketResponse", |decoded, input| {
        assert_eq!(
            decoded.products[0].quote_name,
            input["products"][0]["quote_name"]
        );
        assert_eq!(
            decoded.products[0].base_name,
            input["products"][0]["base_name"]
        );
        assert_eq!(
            decoded.products[0].base_display_symbol,
            input["products"][0]["base_display_symbol"]
        );
        assert_eq!(
            decoded.products[0].quote_display_symbol,
            input["products"][0]["quote_display_symbol"]
        );
        assert_ne!(
            decoded.products[0].quote_name,
            decoded.products[0].base_name
        );
    });
}

#[test]
fn wire_candles_response() {
    check_wire::<CandlesResponse>("CANDLES_RESPONSE", "CandlesResponse", |decoded, input| {
        let item = &decoded.candles[0];
        let raw = &input["candles"][0];
        assert_eq!(item.start, raw["start"]);
        assert_eq!(item.low, raw["low"]);
        assert_eq!(item.high, raw["high"]);
        assert_eq!(item.open, raw["open"]);
        assert_eq!(item.close, raw["close"]);
        assert_eq!(item.volume, raw["volume"]);
    });
}

#[test]
fn wire_list_accounts_response() {
    check_wire::<ListAccountsResponse>(
        "LIST_ACCOUNTS_RESPONSE",
        "ListAccountsResponse",
        |decoded, input| {
            assert_eq!(decoded[0].uuid, input[0]["uuid"]);
            assert_eq!(decoded[0].name, input[0]["name"]);
            assert_eq!(decoded[0].currency, input[0]["currency"]);
            assert_eq!(
                decoded[0].available_balance.value,
                input[0]["available_balance"]["value"]
            );
            assert_eq!(decoded[0].hold.value, input[0]["hold"]["value"]);
            assert_ne!(decoded[0].available_balance.value, decoded[0].hold.value);
        },
    );
}

#[test]
fn list_account_flag_field_identity_supplement() {
    let supplement = &oracle()["fieldIdentitySupplements"]["listAccountFlags"];
    assert_eq!(
        supplement["label"],
        "field-identity-supplement: LIST_ACCOUNTS_RESPONSE default and ready"
    );
    let raw = &supplement["value"][0];
    assert_eq!(raw["default"], false);
    assert_eq!(raw["ready"], true);
    let decoded: ListAccountsResponseItem =
        serde_json::from_value(raw.clone()).expect("account identity supplement");
    decoded.assert_native_wire_identity(raw, "LIST_ACCOUNTS_RESPONSE identity supplement");
    assert!(!decoded.r#default, "account.default decoded identity");
    assert!(decoded.ready, "account.ready decoded identity");
}

#[test]
fn wire_list_portfolios_response() {
    check_wire::<ListPortfoliosResponse>(
        "LIST_PORTFOLIOS_RESPONSE",
        "ListPortfoliosResponse",
        |decoded, input| {
            assert_eq!(decoded.portfolios[0].name, input["portfolios"][0]["name"]);
            assert_eq!(decoded.portfolios[0].uuid, input["portfolios"][0]["uuid"]);
            assert_ne!(decoded.portfolios[0].name, decoded.portfolios[0].uuid);
        },
    );
}

#[test]
fn wire_list_positions_intx_response() {
    check_wire::<ListPositionsIntxResponse>(
        "LIST_POSITIONS_INTX_RESPONSE",
        "ListPositionsIntxResponse",
        |decoded, input| {
            assert_eq!(
                decoded.positions[0].product_id,
                input["positions"][0]["product_id"]
            );
            assert_eq!(
                decoded.positions[0].product_uuid,
                input["positions"][0]["product_uuid"]
            );
            assert_eq!(decoded.positions[0].symbol, input["positions"][0]["symbol"]);
        },
    );
}

#[test]
fn wire_list_positions_cfm_response() {
    check_wire::<ListPositionsCfmResponse>(
        "LIST_POSITIONS_CFM_RESPONSE",
        "ListPositionsCfmResponse",
        |decoded, input| {
            assert_eq!(
                decoded.positions[0].product_id,
                input["positions"][0]["product_id"]
            );
            assert_eq!(
                decoded.positions[0].expiration_time,
                input["positions"][0]["expiration_time"]
            );
            assert_eq!(
                decoded.positions[0].side,
                serde_json::from_value(input["positions"][0]["side"].clone()).unwrap()
            );
        },
    );
}

#[test]
fn wire_candle_event() {
    check_wire::<CandleEvent>("CANDLE_EVENT", "CandleEvent", |decoded, input| {
        assert_eq!(
            decoded.candles[0].product_id,
            input["candles"][0]["product_id"]
        );
        assert_eq!(decoded.candles[0].start, input["candles"][0]["start"]);
    });
}

#[test]
fn wire_trade_event() {
    check_wire::<TradeEvent>("TRADE_EVENT", "TradeEvent", |decoded, input| {
        assert_eq!(decoded.trades[0].trade_id, input["trades"][0]["trade_id"]);
        assert_eq!(decoded.trades[0].price, input["trades"][0]["price"]);
        assert_eq!(decoded.trades[0].size, input["trades"][0]["size"]);
        assert_ne!(decoded.trades[0].price, decoded.trades[0].size);
    });
}

#[test]
fn wire_ticker_event() {
    check_wire::<TickerEvent>("TICKER_EVENT", "TickerEvent", |decoded, input| {
        let ticker = &decoded.tickers[0];
        let raw = &input["tickers"][0];
        assert!(matches!(ticker.r#type, TickerEventTickersItemType::Ticker));
        assert_eq!(ticker.product_id, raw["product_id"]);
        assert_eq!(ticker.price, raw["price"]);
        assert_eq!(ticker.volume_24_h, raw["volume_24_h"]);
        assert_eq!(ticker.low_24_h, raw["low_24_h"]);
        assert_eq!(ticker.high_24_h, raw["high_24_h"]);
        assert_eq!(ticker.low_52_w, raw["low_52_w"]);
        assert_eq!(ticker.high_52_w, raw["high_52_w"]);
        assert_eq!(ticker.price_percent_chg_24_h, raw["price_percent_chg_24_h"]);
    });
}

#[test]
fn wire_level2_event() {
    check_wire::<Level2Event>("LEVEL2_EVENT", "Level2Event", |decoded, input| {
        assert_eq!(
            decoded.updates[0].price_level,
            input["updates"][0]["price_level"]
        );
        assert_eq!(
            decoded.updates[0].new_quantity,
            input["updates"][0]["new_quantity"]
        );
        assert_ne!(
            decoded.updates[0].price_level,
            decoded.updates[0].new_quantity
        );
    });
}

#[test]
fn wire_intx_instruments_response() {
    check_wire::<IntxInstrumentsResponse>(
        "INTX_INSTRUMENTS_RESPONSE",
        "IntxInstrumentsResponse",
        |decoded, input| {
            assert_eq!(decoded[0].instrument_id, input[0]["instrument_id"]);
            assert_eq!(decoded[0].base_asset_id, input[0]["base_asset_id"]);
            assert_ne!(decoded[0].instrument_id, decoded[0].base_asset_id);
        },
    );
}

#[test]
fn wire_balance_response() {
    check_wire::<BalanceResponse>("BALANCE_RESPONSE", "BalanceResponse", |decoded, input| {
        assert_eq!(
            decoded.summary.portfolio_uuid,
            input["summary"]["portfolio_uuid"]
        );
        assert_eq!(decoded.summary.collateral, input["summary"]["collateral"]);
    });
}

#[test]
fn wire_order_event() {
    check_wire::<OrderEvent>("ORDER_EVENT", "OrderEvent", |decoded, input| {
        assert_eq!(decoded.orders[0].order_id, input["orders"][0]["order_id"]);
        assert_eq!(
            decoded.orders[0].product_id,
            input["orders"][0]["product_id"]
        );
        assert_ne!(decoded.orders[0].order_id, decoded.orders[0].product_id);
    });
}

#[test]
fn wire_list_orders_response() {
    check_wire::<ListOrdersResponse>(
        "LIST_ORDERS_RESPONSE",
        "ListOrdersResponse",
        |decoded, input| {
            assert_eq!(decoded.orders[0].order_id, input["orders"][0]["order_id"]);
            assert_eq!(
                decoded.orders[0].product_id,
                input["orders"][0]["product_id"]
            );
            assert_ne!(decoded.orders[0].order_id, decoded.orders[0].product_id);
        },
    );
}

#[test]
fn list_orders_response_side_decodes_native_variants() {
    let buy = serde_json::from_value::<ListOrdersResponseOrdersItemSide>(json!("BUY")).unwrap();
    let sell = serde_json::from_value::<ListOrdersResponseOrdersItemSide>(json!("SELL")).unwrap();
    assert!(
        matches!(buy, ListOrdersResponseOrdersItemSide::Buy),
        "LIST_ORDERS_RESPONSE.side native BUY variant"
    );
    assert!(
        matches!(sell, ListOrdersResponseOrdersItemSide::Sell),
        "LIST_ORDERS_RESPONSE.side native SELL variant"
    );
}

#[test]
fn wire_stop_direction() {
    check_wire::<StopDirection>("STOP_DIRECTION", "StopDirection", |decoded, input| {
        match (*decoded, input.as_str().unwrap()) {
            (StopDirection::StopDirectionStopDown, "STOP_DIRECTION_STOP_DOWN")
            | (StopDirection::StopDirectionStopUp, "STOP_DIRECTION_STOP_UP") => (),
            _ => panic!("StopDirection native variant identity"),
        }
    });
}

#[test]
fn wire_place_order_request() {
    check_wire::<PlaceOrderRequest>(
        "PLACE_ORDER_REQUEST",
        "PlaceOrderRequest",
        |decoded, input| {
            assert_eq!(decoded.client_order_id, input["client_order_id"]);
            assert_eq!(decoded.product_id, input["product_id"]);
            assert_ne!(decoded.client_order_id, decoded.product_id);
        },
    );
}

#[test]
fn wire_place_order_response() {
    check_wire::<PlaceOrderResponse>(
        "PLACE_ORDER_RESPONSE",
        "PlaceOrderResponse",
        |decoded, input| {
            assert_eq!(decoded.success, input["success"]);
            assert_eq!(
                decoded.success_response.as_ref().unwrap().order_id,
                input["success_response"]["order_id"]
            );
        },
    );
}

#[test]
fn wire_edit_order_request() {
    check_wire::<EditOrderRequest>(
        "EDIT_ORDER_REQUEST",
        "EditOrderRequest",
        |decoded, input| {
            assert_eq!(decoded.order_id, input["order_id"]);
            assert_eq!(decoded.price, input["price"]);
        },
    );
}

#[test]
fn wire_edit_order_response() {
    check_wire::<EditOrderResponse>(
        "EDIT_ORDER_RESPONSE",
        "EditOrderResponse",
        |decoded, input| {
            let errors = decoded.errors.as_ref().unwrap();
            assert_eq!(
                errors[0].edit_failure_reason,
                input["errors"][0]["edit_failure_reason"]
                    .as_str()
                    .map(str::to_owned)
            );
            assert_eq!(
                errors[0].preview_failure_reason,
                input["errors"][0]["preview_failure_reason"]
                    .as_str()
                    .map(str::to_owned)
            );
        },
    );
}

#[test]
fn wire_balance_response_cfm() {
    check_wire::<BalanceResponseCfm>(
        "BALANCE_RESPONSE_CFM",
        "BalanceResponseCfm",
        |decoded, input| {
            assert_eq!(
                decoded.balance_summary.liquidation_buffer_percentage,
                input["balance_summary"]["liquidation_buffer_percentage"]
            );
            assert_eq!(
                decoded
                    .balance_summary
                    .intraday_margin_window_measure
                    .margin_window_type,
                input["balance_summary"]["intraday_margin_window_measure"]["margin_window_type"]
            );
        },
    );
}

#[test]
fn wire_portfolio_breakdown_response() {
    check_wire::<PortfolioBreakdownResponse>(
        "PORTFOLIO_BREAKDOWN_RESPONSE",
        "PortfolioBreakdownResponse",
        |decoded, input| {
            assert_eq!(
                decoded.breakdown.portfolio.name,
                input["breakdown"]["portfolio"]["name"]
            );
            assert_eq!(
                decoded.breakdown.portfolio.uuid,
                input["breakdown"]["portfolio"]["uuid"]
            );
            assert_ne!(
                decoded.breakdown.portfolio.name,
                decoded.breakdown.portfolio.uuid
            );
        },
    );
}

#[test]
fn table_recv_window() {
    assert_eq!(
        json!(RECV_WINDOW),
        oracle()["tables"]["RECV_WINDOW"],
        "RECV_WINDOW"
    );
}

#[test]
fn table_base_url() {
    assert_eq!(table(BASE_URL), oracle()["tables"]["BASE_URL"], "BASE_URL");
}

#[test]
fn table_base_ws_url() {
    assert_eq!(
        json!(BASE_WS_URL),
        oracle()["tables"]["BASE_WS_URL"],
        "BASE_WS_URL"
    );
}

#[test]
fn table_endpoints() {
    assert_eq!(
        table(ENDPOINTS),
        oracle()["tables"]["ENDPOINTS"],
        "ENDPOINTS"
    );
}

#[test]
fn table_grafana_endpoints_every() {
    assert_eq!(
        json!(GRAFANA_ENDPOINTS_EVERY),
        oracle()["tables"]["GRAFANA_ENDPOINTS_EVERY"],
        "GRAFANA_ENDPOINTS_EVERY"
    );
}

#[test]
fn table_grafana_endpoints_periodic() {
    assert_eq!(
        json!(GRAFANA_ENDPOINTS_PERIODIC),
        oracle()["tables"]["GRAFANA_ENDPOINTS_PERIODIC"],
        "GRAFANA_ENDPOINTS_PERIODIC"
    );
}

#[test]
fn table_public_endpoints() {
    assert_eq!(
        json!(PUBLIC_ENDPOINTS),
        oracle()["tables"]["PUBLIC_ENDPOINTS"],
        "PUBLIC_ENDPOINTS"
    );
}

#[test]
fn table_order_type() {
    assert_eq!(
        table(ORDER_TYPE),
        oracle()["tables"]["ORDER_TYPE"],
        "ORDER_TYPE"
    );
}

#[test]
fn table_order_side() {
    assert_eq!(
        table(ORDER_SIDE),
        oracle()["tables"]["ORDER_SIDE"],
        "ORDER_SIDE"
    );
}

#[test]
fn table_position_side() {
    assert_eq!(
        table(POSITION_SIDE),
        oracle()["tables"]["POSITION_SIDE"],
        "POSITION_SIDE"
    );
}

#[test]
fn table_time_in_force() {
    assert_eq!(
        table(TIME_IN_FORCE),
        oracle()["tables"]["TIME_IN_FORCE"],
        "TIME_IN_FORCE"
    );
}

#[test]
fn oracle_covers_all_exports() {
    let mut actual: Vec<_> = oracle()["tables"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    let mut expected = [
        "RECV_WINDOW",
        "BASE_URL",
        "BASE_WS_URL",
        "ENDPOINTS",
        "GRAFANA_ENDPOINTS_EVERY",
        "GRAFANA_ENDPOINTS_PERIODIC",
        "PUBLIC_ENDPOINTS",
        "ORDER_TYPE",
        "ORDER_SIDE",
        "POSITION_SIDE",
        "TIME_IN_FORCE",
    ];
    expected.sort_unstable();
    assert_eq!(actual, expected, "TS table export set changed");
    let mut actual: Vec<_> = oracle()["types"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    let mut expected = [
        "MARKET_RESPONSE",
        "CANDLES_RESPONSE",
        "LIST_ACCOUNTS_RESPONSE",
        "LIST_PORTFOLIOS_RESPONSE",
        "LIST_POSITIONS_INTX_RESPONSE",
        "LIST_POSITIONS_CFM_RESPONSE",
        "CANDLE_EVENT",
        "TRADE_EVENT",
        "TICKER_EVENT",
        "LEVEL2_EVENT",
        "INTX_INSTRUMENTS_RESPONSE",
        "BALANCE_RESPONSE",
        "ORDER_EVENT",
        "LIST_ORDERS_RESPONSE",
        "STOP_DIRECTION",
        "PLACE_ORDER_REQUEST",
        "PLACE_ORDER_RESPONSE",
        "EDIT_ORDER_REQUEST",
        "EDIT_ORDER_RESPONSE",
        "BALANCE_RESPONSE_CFM",
        "PORTFOLIO_BREAKDOWN_RESPONSE",
    ];
    expected.sort_unstable();
    assert_eq!(actual, expected, "TS type export set changed");
}
