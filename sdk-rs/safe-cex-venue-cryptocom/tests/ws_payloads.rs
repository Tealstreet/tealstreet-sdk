use safe_cex_venue_cryptocom::ws_payloads::*;
use serde_json::Value;
use std::{collections::BTreeMap, process::Command};
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn ingress(v: &Value) -> PayloadValue {
    match v["kind"].as_str().unwrap() {
        "undefined" => PayloadValue::Undefined,
        "null" => PayloadValue::Null,
        "boolean" => PayloadValue::Bool(v["value"].as_bool().unwrap()),
        "number" => PayloadValue::Number(f64::from_bits(
            u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "string" => PayloadValue::String(units(&v["units"])),
        "array" => PayloadValue::Array(
            v["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(ingress)
                .collect(),
        ),
        "object" => PayloadValue::Object(
            v["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (units(&pair[0]), ingress(&pair[1])))
                .collect::<BTreeMap<_, _>>(),
        ),
        _ => panic!("unknown native ingress"),
    }
}
fn text(actual: &[u16], expected: &Value, label: &str) {
    assert_eq!(
        actual,
        units(&expected["units"]),
        "CRYPTOCOM-PAYLOAD-NATIVE-STRING {label}"
    );
}
fn number(actual: f64, expected: &Value, label: &str) {
    let expected =
        f64::from_bits(u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap());
    assert_eq!(
        actual.to_bits(),
        expected.to_bits(),
        "CRYPTOCOM-PAYLOAD-NATIVE-NUMBER {label}"
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsValuationChannel(
    actual: CryptocomWsValuationChannel,
    expected: &Value,
    label: &str,
) {
    let source = units(&expected["units"]);
    if source == "mark".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsValuationChannel::Mark,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "index".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsValuationChannel::Index,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "funding".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsValuationChannel::Funding,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "estimatedfunding".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsValuationChannel::Estimatedfunding,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unknown source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsSide(actual: CryptocomWsSide, expected: &Value, label: &str) {
    let source = units(&expected["units"]);
    if source == "BUY".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsSide::Buy,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "SELL".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsSide::Sell,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unknown source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsRefPriceType(actual: CryptocomWsRefPriceType, expected: &Value, label: &str) {
    let source = units(&expected["units"]);
    if source == "MARK_PRICE".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsRefPriceType::MarkPrice,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "INDEX_PRICE".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsRefPriceType::IndexPrice,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "LAST_PRICE".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsRefPriceType::LastPrice,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unknown source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsPositionType(actual: CryptocomWsPositionType, expected: &Value, label: &str) {
    let source = units(&expected["units"]);
    if source == "PERPETUAL_SWAP".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsPositionType::PerpetualSwap,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "LONG_POSITION".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsPositionType::LongPosition,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    if source == "SHORT_POSITION".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsPositionType::ShortPosition,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unknown source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsIsolationType(actual: CryptocomWsIsolationType, expected: &Value, label: &str) {
    let source = units(&expected["units"]);
    if source == "ISOLATED_MARGIN".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsIsolationType::IsolatedMargin,
            "CRYPTOCOM-PAYLOAD-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unknown source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsTickerUpdate(actual: &CryptocomWsTickerUpdate, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsTickerData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsTickerData(actual: &CryptocomWsTickerData, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    text(&actual.i, &fields["i"], &format!("{prefix}.i"));
    text(&actual.b, &fields["b"], &format!("{prefix}.b"));
    text(&actual.k, &fields["k"], &format!("{prefix}.k"));
    text(&actual.a, &fields["a"], &format!("{prefix}.a"));
    text(&actual.c, &fields["c"], &format!("{prefix}.c"));
    text(&actual.h, &fields["h"], &format!("{prefix}.h"));
    text(&actual.l, &fields["l"], &format!("{prefix}.l"));
    text(&actual.v, &fields["v"], &format!("{prefix}.v"));
    text(&actual.vv, &fields["vv"], &format!("{prefix}.vv"));
    match &actual.oi {
        Some(value) => {
            assert_ne!(
                (&fields["oi"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.oi")
            );
            text(value, &fields["oi"], &format!("{prefix}.oi"));
        }
        None => assert_eq!(
            (&fields["oi"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.oi")
        ),
    };
    number(actual.t, &fields["t"], &format!("{prefix}.t"));
}
#[allow(non_snake_case)]
fn check_CryptocomWsValuationUpdate(
    actual: &CryptocomWsValuationUpdate,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    check_CryptocomWsValuationChannel(
        actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsValuationData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsValuationData(
    actual: &CryptocomWsValuationData,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(&actual.v, &fields["v"], &format!("{prefix}.v"));
    number(actual.t, &fields["t"], &format!("{prefix}.t"));
}
#[allow(non_snake_case)]
fn check_CryptocomWsTradeUpdate(actual: &CryptocomWsTradeUpdate, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsTradeData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsTradeData(actual: &CryptocomWsTradeData, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    number(actual.d, &fields["d"], &format!("{prefix}.d"));
    number(actual.t, &fields["t"], &format!("{prefix}.t"));
    number(actual.p, &fields["p"], &format!("{prefix}.p"));
    number(actual.q, &fields["q"], &format!("{prefix}.q"));
    check_CryptocomWsSide(actual.s, &fields["s"], &format!("{prefix}.s"));
    text(&actual.i, &fields["i"], &format!("{prefix}.i"));
    number(
        actual.dataTime,
        &fields["dataTime"],
        &format!("{prefix}.dataTime"),
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsBookUpdate(actual: &CryptocomWsBookUpdate, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsBookData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsBookData(actual: &CryptocomWsBookData, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    for (index, (item, source)) in (&actual.bids)
        .iter()
        .zip(fields["bids"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.bids"));
        for (index, field) in item.iter().enumerate() {
            text(
                field,
                &source["values"][index],
                &format!("{item_label}.{index}"),
            );
        }
    }
    assert_eq!(
        (&actual.bids).len(),
        fields["bids"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.bids")
    );
    for (index, (item, source)) in (&actual.asks)
        .iter()
        .zip(fields["asks"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.asks"));
        for (index, field) in item.iter().enumerate() {
            text(
                field,
                &source["values"][index],
                &format!("{item_label}.{index}"),
            );
        }
    }
    assert_eq!(
        (&actual.asks).len(),
        fields["asks"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.asks")
    );
    number(actual.t, &fields["t"], &format!("{prefix}.t"));
    number(actual.tt, &fields["tt"], &format!("{prefix}.tt"));
}
#[allow(non_snake_case)]
fn check_CryptocomWsCandlestickUpdate(
    actual: &CryptocomWsCandlestickUpdate,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsCandlestickData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsCandlestickData(
    actual: &CryptocomWsCandlestickData,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    number(actual.t, &fields["t"], &format!("{prefix}.t"));
    text(&actual.o, &fields["o"], &format!("{prefix}.o"));
    text(&actual.h, &fields["h"], &format!("{prefix}.h"));
    text(&actual.l, &fields["l"], &format!("{prefix}.l"));
    text(&actual.c, &fields["c"], &format!("{prefix}.c"));
    text(&actual.v, &fields["v"], &format!("{prefix}.v"));
}
#[allow(non_snake_case)]
fn check_CryptocomWsOrderUpdate(actual: &CryptocomWsOrderUpdate, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsOrderData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsOrderData(actual: &CryptocomWsOrderData, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    match &actual.account_id {
        Some(value) => {
            assert_ne!(
                (&fields["account_id"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.account_id")
            );
            text(
                value,
                &fields["account_id"],
                &format!("{prefix}.account_id"),
            );
        }
        None => assert_eq!(
            (&fields["account_id"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.account_id")
        ),
    };
    text(
        &actual.order_id,
        &fields["order_id"],
        &format!("{prefix}.order_id"),
    );
    match &actual.client_oid {
        Some(value) => {
            assert_ne!(
                (&fields["client_oid"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.client_oid")
            );
            text(
                value,
                &fields["client_oid"],
                &format!("{prefix}.client_oid"),
            );
        }
        None => assert_eq!(
            (&fields["client_oid"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.client_oid")
        ),
    };
    text(
        &actual.order_type,
        &fields["order_type"],
        &format!("{prefix}.order_type"),
    );
    text(
        &actual.time_in_force,
        &fields["time_in_force"],
        &format!("{prefix}.time_in_force"),
    );
    check_CryptocomWsSide(actual.side, &fields["side"], &format!("{prefix}.side"));
    match &actual.exec_inst {
        Some(value) => {
            assert_ne!(
                (&fields["exec_inst"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.exec_inst")
            );
            for (index, (item, source)) in (value)
                .iter()
                .zip(fields["exec_inst"]["values"].as_array().unwrap())
                .enumerate()
            {
                let item_label = format!("{}.{index}", &format!("{prefix}.exec_inst"));
                text(item, source, &item_label);
            }
            assert_eq!(
                (value).len(),
                fields["exec_inst"]["values"].as_array().unwrap().len(),
                "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
                &format!("{prefix}.exec_inst")
            );
        }
        None => assert_eq!(
            (&fields["exec_inst"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.exec_inst")
        ),
    };
    text(
        &actual.quantity,
        &fields["quantity"],
        &format!("{prefix}.quantity"),
    );
    match &actual.limit_price {
        Some(value) => {
            assert_ne!(
                (&fields["limit_price"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.limit_price")
            );
            text(
                value,
                &fields["limit_price"],
                &format!("{prefix}.limit_price"),
            );
        }
        None => assert_eq!(
            (&fields["limit_price"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.limit_price")
        ),
    };
    match &actual.trigger_price {
        Some(value) => {
            assert_ne!(
                (&fields["trigger_price"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.trigger_price")
            );
            text(
                value,
                &fields["trigger_price"],
                &format!("{prefix}.trigger_price"),
            );
        }
        None => assert_eq!(
            (&fields["trigger_price"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.trigger_price")
        ),
    };
    match &actual.ref_price {
        Some(value) => {
            assert_ne!(
                (&fields["ref_price"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.ref_price")
            );
            text(value, &fields["ref_price"], &format!("{prefix}.ref_price"));
        }
        None => assert_eq!(
            (&fields["ref_price"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.ref_price")
        ),
    };
    match &actual.ref_price_type {
        Some(value) => {
            assert_ne!(
                (&fields["ref_price_type"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.ref_price_type")
            );
            check_CryptocomWsRefPriceType(
                *value,
                &fields["ref_price_type"],
                &format!("{prefix}.ref_price_type"),
            );
        }
        None => assert_eq!(
            (&fields["ref_price_type"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.ref_price_type")
        ),
    };
    match &actual.avg_price {
        Some(value) => {
            assert_ne!(
                (&fields["avg_price"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.avg_price")
            );
            text(value, &fields["avg_price"], &format!("{prefix}.avg_price"));
        }
        None => assert_eq!(
            (&fields["avg_price"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.avg_price")
        ),
    };
    match &actual.cumulative_quantity {
        Some(value) => {
            assert_ne!(
                (&fields["cumulative_quantity"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.cumulative_quantity")
            );
            text(
                value,
                &fields["cumulative_quantity"],
                &format!("{prefix}.cumulative_quantity"),
            );
        }
        None => assert_eq!(
            (&fields["cumulative_quantity"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.cumulative_quantity")
        ),
    };
    match &actual.cumulative_value {
        Some(value) => {
            assert_ne!(
                (&fields["cumulative_value"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.cumulative_value")
            );
            text(
                value,
                &fields["cumulative_value"],
                &format!("{prefix}.cumulative_value"),
            );
        }
        None => assert_eq!(
            (&fields["cumulative_value"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.cumulative_value")
        ),
    };
    match &actual.fee_currency {
        Some(value) => {
            assert_ne!(
                (&fields["fee_currency"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.fee_currency")
            );
            text(
                value,
                &fields["fee_currency"],
                &format!("{prefix}.fee_currency"),
            );
        }
        None => assert_eq!(
            (&fields["fee_currency"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.fee_currency")
        ),
    };
    match &actual.fee_instrument_name {
        Some(value) => {
            assert_ne!(
                (&fields["fee_instrument_name"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.fee_instrument_name")
            );
            text(
                value,
                &fields["fee_instrument_name"],
                &format!("{prefix}.fee_instrument_name"),
            );
        }
        None => assert_eq!(
            (&fields["fee_instrument_name"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.fee_instrument_name")
        ),
    };
    match &actual.create_time {
        Some(value) => {
            assert_ne!(
                (&fields["create_time"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.create_time")
            );
            number(
                *value,
                &fields["create_time"],
                &format!("{prefix}.create_time"),
            );
        }
        None => assert_eq!(
            (&fields["create_time"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.create_time")
        ),
    };
    match &actual.update_time {
        Some(value) => {
            assert_ne!(
                (&fields["update_time"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.update_time")
            );
            number(
                *value,
                &fields["update_time"],
                &format!("{prefix}.update_time"),
            );
        }
        None => assert_eq!(
            (&fields["update_time"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.update_time")
        ),
    };
    match &actual.time_created {
        Some(value) => {
            assert_ne!(
                (&fields["time_created"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.time_created")
            );
            number(
                *value,
                &fields["time_created"],
                &format!("{prefix}.time_created"),
            );
        }
        None => assert_eq!(
            (&fields["time_created"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.time_created")
        ),
    };
    match &actual.time_updated {
        Some(value) => {
            assert_ne!(
                (&fields["time_updated"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.time_updated")
            );
            number(
                *value,
                &fields["time_updated"],
                &format!("{prefix}.time_updated"),
            );
        }
        None => assert_eq!(
            (&fields["time_updated"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.time_updated")
        ),
    };
    match &actual.status {
        Some(value) => {
            assert_ne!(
                (&fields["status"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.status")
            );
            text(value, &fields["status"], &format!("{prefix}.status"));
        }
        None => assert_eq!(
            (&fields["status"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.status")
        ),
    };
    match &actual.order_status {
        Some(value) => {
            assert_ne!(
                (&fields["order_status"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.order_status")
            );
            text(
                value,
                &fields["order_status"],
                &format!("{prefix}.order_status"),
            );
        }
        None => assert_eq!(
            (&fields["order_status"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.order_status")
        ),
    };
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    match &actual.reason {
        Some(value) => {
            assert_ne!(
                (&fields["reason"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.reason")
            );
            number(*value, &fields["reason"], &format!("{prefix}.reason"));
        }
        None => assert_eq!(
            (&fields["reason"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.reason")
        ),
    };
    match &actual.update_user_id {
        Some(value) => {
            assert_ne!(
                (&fields["update_user_id"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.update_user_id")
            );
            text(
                value,
                &fields["update_user_id"],
                &format!("{prefix}.update_user_id"),
            );
        }
        None => assert_eq!(
            (&fields["update_user_id"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.update_user_id")
        ),
    };
}
#[allow(non_snake_case)]
fn check_CryptocomWsPositionUpdate(
    actual: &CryptocomWsPositionUpdate,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsPositionData(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsPositionData(actual: &CryptocomWsPositionData, expected: &Value, prefix: &str) {
    let fields = &expected["fields"];
    match &actual.account_id {
        Some(value) => {
            assert_ne!(
                (&fields["account_id"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.account_id")
            );
            text(
                value,
                &fields["account_id"],
                &format!("{prefix}.account_id"),
            );
        }
        None => assert_eq!(
            (&fields["account_id"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.account_id")
        ),
    };
    text(
        &actual.quantity,
        &fields["quantity"],
        &format!("{prefix}.quantity"),
    );
    text(&actual.cost, &fields["cost"], &format!("{prefix}.cost"));
    text(
        &actual.open_position_pnl,
        &fields["open_position_pnl"],
        &format!("{prefix}.open_position_pnl"),
    );
    match &actual.session_pnl {
        Some(value) => {
            assert_ne!(
                (&fields["session_pnl"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.session_pnl")
            );
            text(
                value,
                &fields["session_pnl"],
                &format!("{prefix}.session_pnl"),
            );
        }
        None => assert_eq!(
            (&fields["session_pnl"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.session_pnl")
        ),
    };
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    match &actual.open_pos_cost {
        Some(value) => {
            assert_ne!(
                (&fields["open_pos_cost"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.open_pos_cost")
            );
            text(
                value,
                &fields["open_pos_cost"],
                &format!("{prefix}.open_pos_cost"),
            );
        }
        None => assert_eq!(
            (&fields["open_pos_cost"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.open_pos_cost")
        ),
    };
    match &actual.update_timestamp_ms {
        Some(value) => {
            assert_ne!(
                (&fields["update_timestamp_ms"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.update_timestamp_ms")
            );
            number(
                *value,
                &fields["update_timestamp_ms"],
                &format!("{prefix}.update_timestamp_ms"),
            );
        }
        None => assert_eq!(
            (&fields["update_timestamp_ms"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.update_timestamp_ms")
        ),
    };
    check_CryptocomWsPositionType(actual.r#type, &fields["type"], &format!("{prefix}.type"));
    match &actual.mark_price {
        Some(value) => {
            assert_ne!(
                (&fields["mark_price"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.mark_price")
            );
            text(
                value,
                &fields["mark_price"],
                &format!("{prefix}.mark_price"),
            );
        }
        None => assert_eq!(
            (&fields["mark_price"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.mark_price")
        ),
    };
    match &actual.target_leverage {
        Some(value) => {
            assert_ne!(
                (&fields["target_leverage"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.target_leverage")
            );
            text(
                value,
                &fields["target_leverage"],
                &format!("{prefix}.target_leverage"),
            );
        }
        None => assert_eq!(
            (&fields["target_leverage"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.target_leverage")
        ),
    };
    match &actual.leverage {
        Some(value) => {
            assert_ne!(
                (&fields["leverage"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.leverage")
            );
            text(value, &fields["leverage"], &format!("{prefix}.leverage"));
        }
        None => assert_eq!(
            (&fields["leverage"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.leverage")
        ),
    };
    match &actual.liquidation_price {
        Some(value) => {
            assert_ne!(
                (&fields["liquidation_price"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.liquidation_price")
            );
            text(
                value,
                &fields["liquidation_price"],
                &format!("{prefix}.liquidation_price"),
            );
        }
        None => assert_eq!(
            (&fields["liquidation_price"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.liquidation_price")
        ),
    };
    match &actual.isolated_margin_balance {
        Some(value) => {
            assert_ne!(
                (&fields["isolated_margin_balance"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.isolated_margin_balance")
            );
            text(
                value,
                &fields["isolated_margin_balance"],
                &format!("{prefix}.isolated_margin_balance"),
            );
        }
        None => assert_eq!(
            (&fields["isolated_margin_balance"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.isolated_margin_balance")
        ),
    };
    match &actual.isolation_id {
        Some(value) => {
            assert_ne!(
                (&fields["isolation_id"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.isolation_id")
            );
            text(
                value,
                &fields["isolation_id"],
                &format!("{prefix}.isolation_id"),
            );
        }
        None => assert_eq!(
            (&fields["isolation_id"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.isolation_id")
        ),
    };
    match &actual.isolation_type {
        Some(value) => {
            assert_ne!(
                (&fields["isolation_type"])["kind"],
                "undefined",
                "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
                &format!("{prefix}.isolation_type")
            );
            check_CryptocomWsIsolationType(
                *value,
                &fields["isolation_type"],
                &format!("{prefix}.isolation_type"),
            );
        }
        None => assert_eq!(
            (&fields["isolation_type"])["kind"],
            "undefined",
            "CRYPTOCOM-PAYLOAD-NATIVE-PRESENCE {}",
            &format!("{prefix}.isolation_type")
        ),
    };
}
#[allow(non_snake_case)]
fn check_CryptocomWsBalanceUpdate(
    actual: &CryptocomWsBalanceUpdate,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(
        &actual.subscription,
        &fields["subscription"],
        &format!("{prefix}.subscription"),
    );
    text(
        &actual.channel,
        &fields["channel"],
        &format!("{prefix}.channel"),
    );
    for (index, (item, source)) in (&actual.data)
        .iter()
        .zip(fields["data"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.data"));
        check_CryptocomWsBalanceSummary(item, source, &item_label);
    }
    assert_eq!(
        (&actual.data).len(),
        fields["data"]["values"].as_array().unwrap().len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.data")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsBalanceSummary(
    actual: &CryptocomWsBalanceSummary,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(
        &actual.total_available_balance,
        &fields["total_available_balance"],
        &format!("{prefix}.total_available_balance"),
    );
    text(
        &actual.total_margin_balance,
        &fields["total_margin_balance"],
        &format!("{prefix}.total_margin_balance"),
    );
    text(
        &actual.total_initial_margin,
        &fields["total_initial_margin"],
        &format!("{prefix}.total_initial_margin"),
    );
    text(
        &actual.total_maintenance_margin,
        &fields["total_maintenance_margin"],
        &format!("{prefix}.total_maintenance_margin"),
    );
    text(
        &actual.total_cash_balance,
        &fields["total_cash_balance"],
        &format!("{prefix}.total_cash_balance"),
    );
    text(
        &actual.total_collateral_value,
        &fields["total_collateral_value"],
        &format!("{prefix}.total_collateral_value"),
    );
    text(
        &actual.total_session_unrealized_pnl,
        &fields["total_session_unrealized_pnl"],
        &format!("{prefix}.total_session_unrealized_pnl"),
    );
    text(
        &actual.total_session_realized_pnl,
        &fields["total_session_realized_pnl"],
        &format!("{prefix}.total_session_realized_pnl"),
    );
    text(
        &actual.total_position_cost,
        &fields["total_position_cost"],
        &format!("{prefix}.total_position_cost"),
    );
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    for (index, (item, source)) in (&actual.position_balances)
        .iter()
        .zip(fields["position_balances"]["values"].as_array().unwrap())
        .enumerate()
    {
        let item_label = format!("{}.{index}", &format!("{prefix}.position_balances"));
        check_CryptocomWsPositionBalance(item, source, &item_label);
    }
    assert_eq!(
        (&actual.position_balances).len(),
        fields["position_balances"]["values"]
            .as_array()
            .unwrap()
            .len(),
        "CRYPTOCOM-PAYLOAD-NATIVE-ARRAY-LENGTH {}",
        &format!("{prefix}.position_balances")
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsPositionBalance(
    actual: &CryptocomWsPositionBalance,
    expected: &Value,
    prefix: &str,
) {
    let fields = &expected["fields"];
    text(
        &actual.instrument_name,
        &fields["instrument_name"],
        &format!("{prefix}.instrument_name"),
    );
    text(
        &actual.quantity,
        &fields["quantity"],
        &format!("{prefix}.quantity"),
    );
    text(
        &actual.reserved_qty,
        &fields["reserved_qty"],
        &format!("{prefix}.reserved_qty"),
    );
    text(
        &actual.collateral_amount,
        &fields["collateral_amount"],
        &format!("{prefix}.collateral_amount"),
    );
    assert_eq!(
        actual.collateral_eligible,
        fields["collateral_eligible"]["value"].as_bool().unwrap(),
        "CRYPTOCOM-PAYLOAD-NATIVE-BOOL {}",
        &format!("{prefix}.collateral_eligible")
    );
    text(
        &actual.haircut,
        &fields["haircut"],
        &format!("{prefix}.haircut"),
    );
    text(
        &actual.market_value,
        &fields["market_value"],
        &format!("{prefix}.market_value"),
    );
    text(
        &actual.max_withdrawal_balance,
        &fields["max_withdrawal_balance"],
        &format!("{prefix}.max_withdrawal_balance"),
    );
    text(
        &actual.hourly_interest_rate,
        &fields["hourly_interest_rate"],
        &format!("{prefix}.hourly_interest_rate"),
    );
}
#[allow(non_snake_case)]
#[test]
fn ws_payloads_match_actual_typescript_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_payloads_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for row in corpus["rows"].as_array().unwrap() {
        let input = ingress(&row["input"]);
        let expected = &row["observed"];
        let label = row["name"].as_str().unwrap();
        match row["record"].as_str().unwrap() {
            "CryptocomWsTickerUpdate" => check_CryptocomWsTickerUpdate(
                &CryptocomWsTickerUpdate::from_value(&input).expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsValuationUpdate" => check_CryptocomWsValuationUpdate(
                &CryptocomWsValuationUpdate::from_value(&input)
                    .expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsTradeUpdate" => check_CryptocomWsTradeUpdate(
                &CryptocomWsTradeUpdate::from_value(&input).expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsBookUpdate" => check_CryptocomWsBookUpdate(
                &CryptocomWsBookUpdate::from_value(&input).expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsCandlestickUpdate" => check_CryptocomWsCandlestickUpdate(
                &CryptocomWsCandlestickUpdate::from_value(&input)
                    .expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsOrderUpdate" => check_CryptocomWsOrderUpdate(
                &CryptocomWsOrderUpdate::from_value(&input).expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsPositionUpdate" => check_CryptocomWsPositionUpdate(
                &CryptocomWsPositionUpdate::from_value(&input)
                    .expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsBalanceUpdate" => check_CryptocomWsBalanceUpdate(
                &CryptocomWsBalanceUpdate::from_value(&input).expect("typed source native ingress"),
                expected,
                label,
            ),
            "CryptocomWsValuationChannel" => check_CryptocomWsValuationChannel(
                CryptocomWsValuationChannel::from_value(&input)
                    .expect("typed source native ingress"),
                expected,
                label,
            ),
            _ => panic!("unknown payload record"),
        }
    }
    // All decoded native fields and numeric bits precede schema/fixture inventories.
    let schema: Value =
        serde_json::from_str(include_str!("ws_payloads_expected_schema.json")).unwrap();
    assert_eq!(corpus["schemas"], schema, "CRYPTOCOM-PAYLOAD-SOURCE-SCHEMA");
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 310, "independent scenario floor");
    assert_eq!(
        rows[309]["name"],
        "CryptocomWsValuationChannel:estimatedfunding"
    );
    let fixtures: Value = serde_json::from_str(include_str!("ws_payloads_fixtures.json")).unwrap();
    assert_eq!(fixtures.as_array().unwrap().len(), 310);
    for (row, input) in rows.iter().zip(fixtures.as_array().unwrap()) {
        assert_eq!(row["name"], input["name"]);
        assert_eq!(row["record"], input["record"]);
    }
}
