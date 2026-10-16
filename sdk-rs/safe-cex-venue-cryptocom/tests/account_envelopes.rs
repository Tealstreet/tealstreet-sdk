use safe_cex_venue_cryptocom::{account_envelopes::*, ws_payloads::*};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
};
#[derive(Debug)]
enum N {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Vec<Rc<N>>),
    Object(RefCell<BTreeMap<Vec<u16>, Rc<N>>>),
    Capture {
        tag: String,
        classification: String,
        marker: Cell<f64>,
    },
}
type V = Rc<N>;
struct Host;
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn ingress(v: &Value) -> V {
    Rc::new(match v["kind"].as_str().unwrap() {
        "undefined" => N::Undefined,
        "null" => N::Null,
        "boolean" => N::Bool(v["value"].as_bool().unwrap()),
        "number" => N::Number(f64::from_bits(
            u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "string" => N::String(units(&v["units"])),
        "array" => N::Array(
            v["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(ingress)
                .collect(),
        ),
        "object" => N::Object(RefCell::new(
            v["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (units(&p[0]), ingress(&p[1])))
                .collect(),
        )),
        "capture" => N::Capture {
            tag: v["tag"].as_str().unwrap().into(),
            classification: v["classification"].as_str().unwrap().into(),
            marker: Cell::new(42.0),
        },
        _ => panic!("native ingress"),
    })
}
impl AccountEnvelopeHost for Host {
    type Value = V;
    type Error = &'static str;
    fn field(&self, v: &V, k: &'static str) -> Result<V, Self::Error> {
        if let N::Object(fields) = v.as_ref() {
            Ok(fields
                .borrow()
                .get(&k.encode_utf16().collect::<Vec<_>>())
                .cloned()
                .unwrap_or_else(|| Rc::new(N::Undefined)))
        } else {
            Err("object")
        }
    }
    fn string(&self, v: V) -> Result<Vec<u16>, Self::Error> {
        if let N::String(v) = v.as_ref() {
            Ok(v.clone())
        } else {
            Err("string")
        }
    }
    fn number(&self, v: V) -> Result<f64, Self::Error> {
        if let N::Number(v) = v.as_ref() {
            Ok(*v)
        } else {
            Err("number")
        }
    }
    fn boolean(&self, v: V) -> Result<bool, Self::Error> {
        if let N::Bool(v) = v.as_ref() {
            Ok(*v)
        } else {
            Err("boolean")
        }
    }
    fn undefined(&self, v: &V) -> bool {
        matches!(v.as_ref(), N::Undefined)
    }
    fn record(&self, v: V) -> Result<NativeRecord<V>, Self::Error> {
        if matches!(v.as_ref(),N::Capture{classification,..}if classification=="object") {
            Ok(NativeRecord { original: v })
        } else {
            Err("captured record")
        }
    }
    fn array<T>(
        &self,
        v: V,
        decode: impl FnMut(V) -> Result<T, Self::Error>,
    ) -> Result<Vec<T>, Self::Error> {
        if let N::Array(values) = v.as_ref() {
            values.clone().into_iter().map(decode).collect()
        } else {
            Err("array")
        }
    }
    fn invalid_literal(&self, _: &'static str) -> Self::Error {
        "literal"
    }
}
fn string(actual: &[u16], e: &Value, label: &str) {
    assert_eq!(
        actual,
        units(&e["units"]),
        "CRYPTOCOM-ACCOUNT-NATIVE-STRING {label}"
    );
}
fn number(actual: f64, e: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap(),
        "CRYPTOCOM-ACCOUNT-NATIVE-NUMBER {label}"
    );
}
fn boolean(actual: bool, e: &Value, label: &str) {
    assert_eq!(
        actual,
        e["value"].as_bool().unwrap(),
        "CRYPTOCOM-ACCOUNT-NATIVE-BOOLEAN {label}"
    );
}
fn capture(actual: &V, e: &Value, original: &V, label: &str) {
    assert!(
        Rc::ptr_eq(actual, original),
        "CRYPTOCOM-ACCOUNT-NATIVE-ORIGINAL-IDENTITY {label}"
    );
    let N::Capture {
        tag,
        classification,
        ..
    } = actual.as_ref()
    else {
        panic!("capture")
    };
    assert_eq!(
        tag,
        e["tag"].as_str().unwrap(),
        "CRYPTOCOM-ACCOUNT-NATIVE-CAPTURE {label}"
    );
    assert_eq!(
        classification,
        e["classification"].as_str().unwrap(),
        "CRYPTOCOM-ACCOUNT-NATIVE-CLASS {label}"
    );
}
fn optional<T>(actual: &Option<T>, e: &Value, label: &str, check: impl FnOnce(&T)) {
    if e["kind"] == "undefined" {
        assert!(
            actual.is_none(),
            "CRYPTOCOM-ACCOUNT-NATIVE-OPTIONAL {label}"
        );
    } else {
        check(
            actual
                .as_ref()
                .unwrap_or_else(|| panic!("CRYPTOCOM-ACCOUNT-NATIVE-PRESENT {label}")),
        );
    }
}
#[allow(non_snake_case)]
fn check_CryptocomWsSide(actual: CryptocomWsSide, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "BUY".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsSide::Buy,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "SELL".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsSide::Sell,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomRequestType(actual: CryptocomRequestType, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "LIMIT".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestType::Limit,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "MARKET".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestType::Market,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "STOP_LOSS".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestType::StopLoss,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "STOP_LIMIT".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestType::StopLimit,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "TAKE_PROFIT".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestType::TakeProfit,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "TAKE_PROFIT_LIMIT".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestType::TakeProfitLimit,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomRequestTimeInForce(actual: CryptocomRequestTimeInForce, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "GOOD_TILL_CANCEL".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestTimeInForce::GoodTillCancel,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "FILL_OR_KILL".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestTimeInForce::FillOrKill,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "IMMEDIATE_OR_CANCEL".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestTimeInForce::ImmediateOrCancel,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "POST_ONLY".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomRequestTimeInForce::PostOnly,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsRefPriceType(actual: CryptocomWsRefPriceType, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "MARK_PRICE".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsRefPriceType::MarkPrice,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "INDEX_PRICE".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsRefPriceType::IndexPrice,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "LAST_PRICE".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsRefPriceType::LastPrice,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsPositionType(actual: CryptocomWsPositionType, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "PERPETUAL_SWAP".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsPositionType::PerpetualSwap,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "LONG_POSITION".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsPositionType::LongPosition,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    if value == "SHORT_POSITION".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsPositionType::ShortPosition,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomWsIsolationType(actual: CryptocomWsIsolationType, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "ISOLATED_MARGIN".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomWsIsolationType::IsolatedMargin,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomSubscribeMethod(actual: CryptocomSubscribeMethod, e: &Value, label: &str) {
    let value = units(&e["units"]);
    if value == "subscribe".encode_utf16().collect::<Vec<_>>() {
        assert_eq!(
            actual,
            CryptocomSubscribeMethod::Subscribe,
            "CRYPTOCOM-ACCOUNT-NATIVE-LITERAL {label}"
        );
        return;
    }
    panic!("unexpected source literal {label}");
}
#[allow(non_snake_case)]
fn check_CryptocomOrderRequest(actual: &CryptocomOrderRequest, e: &Value, raw: &V, label: &str) {
    let _ = raw;
    string(
        &actual.instrument_name,
        &e["fields"]["instrument_name"],
        &format!("{label}.instrument_name"),
    );
    check_CryptocomWsSide(actual.side, &e["fields"]["side"], &format!("{label}.side"));
    check_CryptocomRequestType(
        actual.r#type,
        &e["fields"]["type"],
        &format!("{label}.type"),
    );
    optional(
        &actual.quantity,
        &e["fields"]["quantity"],
        &format!("{label}.quantity"),
        |a| {
            string(&a, &e["fields"]["quantity"], &format!("{label}.quantity"));
        },
    );
    optional(
        &actual.notional,
        &e["fields"]["notional"],
        &format!("{label}.notional"),
        |a| {
            string(&a, &e["fields"]["notional"], &format!("{label}.notional"));
        },
    );
    optional(
        &actual.price,
        &e["fields"]["price"],
        &format!("{label}.price"),
        |a| {
            string(&a, &e["fields"]["price"], &format!("{label}.price"));
        },
    );
    optional(
        &actual.ref_price,
        &e["fields"]["ref_price"],
        &format!("{label}.ref_price"),
        |a| {
            string(&a, &e["fields"]["ref_price"], &format!("{label}.ref_price"));
        },
    );
    optional(
        &actual.ref_price_type,
        &e["fields"]["ref_price_type"],
        &format!("{label}.ref_price_type"),
        |a| {
            check_CryptocomWsRefPriceType(
                *a,
                &e["fields"]["ref_price_type"],
                &format!("{label}.ref_price_type"),
            );
        },
    );
    optional(
        &actual.time_in_force,
        &e["fields"]["time_in_force"],
        &format!("{label}.time_in_force"),
        |a| {
            check_CryptocomRequestTimeInForce(
                *a,
                &e["fields"]["time_in_force"],
                &format!("{label}.time_in_force"),
            );
        },
    );
    optional(
        &actual.exec_inst,
        &e["fields"]["exec_inst"],
        &format!("{label}.exec_inst"),
        |a| {
            assert_eq!(
                a.len(),
                (&e["fields"]["exec_inst"])["values"]
                    .as_array()
                    .unwrap()
                    .len(),
                "CRYPTOCOM-ACCOUNT-NATIVE-ARRAY-COUNT {label}"
            );
            for (i, a) in a.iter().enumerate() {
                string(
                    &a,
                    &&e["fields"]["exec_inst"]["values"][i],
                    &format!("{label}.exec_inst[{i}]"),
                );
            }
        },
    );
    optional(
        &actual.client_oid,
        &e["fields"]["client_oid"],
        &format!("{label}.client_oid"),
        |a| {
            string(
                &a,
                &e["fields"]["client_oid"],
                &format!("{label}.client_oid"),
            );
        },
    );
    optional(
        &actual.post_only,
        &e["fields"]["post_only"],
        &format!("{label}.post_only"),
        |a| {
            boolean(*a, &e["fields"]["post_only"], &format!("{label}.post_only"));
        },
    );
}
#[allow(non_snake_case)]
fn check_CryptocomPosition(actual: &CryptocomPosition, e: &Value, raw: &V, label: &str) {
    let _ = raw;
    optional(
        &actual.account_id,
        &e["fields"]["account_id"],
        &format!("{label}.account_id"),
        |a| {
            string(
                &a,
                &e["fields"]["account_id"],
                &format!("{label}.account_id"),
            );
        },
    );
    string(
        &actual.quantity,
        &e["fields"]["quantity"],
        &format!("{label}.quantity"),
    );
    string(&actual.cost, &e["fields"]["cost"], &format!("{label}.cost"));
    string(
        &actual.open_position_pnl,
        &e["fields"]["open_position_pnl"],
        &format!("{label}.open_position_pnl"),
    );
    optional(
        &actual.session_pnl,
        &e["fields"]["session_pnl"],
        &format!("{label}.session_pnl"),
        |a| {
            string(
                &a,
                &e["fields"]["session_pnl"],
                &format!("{label}.session_pnl"),
            );
        },
    );
    string(
        &actual.instrument_name,
        &e["fields"]["instrument_name"],
        &format!("{label}.instrument_name"),
    );
    optional(
        &actual.open_pos_cost,
        &e["fields"]["open_pos_cost"],
        &format!("{label}.open_pos_cost"),
        |a| {
            string(
                &a,
                &e["fields"]["open_pos_cost"],
                &format!("{label}.open_pos_cost"),
            );
        },
    );
    optional(
        &actual.update_timestamp_ms,
        &e["fields"]["update_timestamp_ms"],
        &format!("{label}.update_timestamp_ms"),
        |a| {
            number(
                *a,
                &e["fields"]["update_timestamp_ms"],
                &format!("{label}.update_timestamp_ms"),
            );
        },
    );
    check_CryptocomWsPositionType(
        actual.r#type,
        &e["fields"]["type"],
        &format!("{label}.type"),
    );
    optional(
        &actual.position_id,
        &e["fields"]["position_id"],
        &format!("{label}.position_id"),
        |a| {
            string(
                &a,
                &e["fields"]["position_id"],
                &format!("{label}.position_id"),
            );
        },
    );
    optional(
        &actual.leverage,
        &e["fields"]["leverage"],
        &format!("{label}.leverage"),
        |a| {
            string(&a, &e["fields"]["leverage"], &format!("{label}.leverage"));
        },
    );
    optional(
        &actual.liquidation_price,
        &e["fields"]["liquidation_price"],
        &format!("{label}.liquidation_price"),
        |a| {
            string(
                &a,
                &e["fields"]["liquidation_price"],
                &format!("{label}.liquidation_price"),
            );
        },
    );
    optional(
        &actual.mark_price,
        &e["fields"]["mark_price"],
        &format!("{label}.mark_price"),
        |a| {
            string(
                &a,
                &e["fields"]["mark_price"],
                &format!("{label}.mark_price"),
            );
        },
    );
    optional(
        &actual.isolation_id,
        &e["fields"]["isolation_id"],
        &format!("{label}.isolation_id"),
        |a| {
            string(
                &a,
                &e["fields"]["isolation_id"],
                &format!("{label}.isolation_id"),
            );
        },
    );
    optional(
        &actual.isolation_type,
        &e["fields"]["isolation_type"],
        &format!("{label}.isolation_type"),
        |a| {
            check_CryptocomWsIsolationType(
                *a,
                &e["fields"]["isolation_type"],
                &format!("{label}.isolation_type"),
            );
        },
    );
}
#[allow(non_snake_case)]
fn check_CryptocomAccountSummary(
    actual: &CryptocomAccountSummary,
    e: &Value,
    raw: &V,
    label: &str,
) {
    let _ = raw;
    let native_values = Host.field(raw, "accounts").unwrap();
    let N::Array(original_values) = native_values.as_ref() else {
        panic!("array")
    };
    assert_eq!(
        actual.accounts.len(),
        (&e["fields"]["accounts"])["values"]
            .as_array()
            .unwrap()
            .len(),
        "CRYPTOCOM-ACCOUNT-NATIVE-ARRAY-COUNT {label}"
    );
    for (i, a) in actual.accounts.iter().enumerate() {
        check_CryptocomAccount(
            &a,
            &&e["fields"]["accounts"]["values"][i],
            &&original_values[i],
            &format!("{label}.accounts[{i}]"),
        );
    }
}
#[allow(non_snake_case)]
fn check_CryptocomAccount(actual: &CryptocomAccount, e: &Value, raw: &V, label: &str) {
    let _ = raw;
    number(
        actual.balance,
        &e["fields"]["balance"],
        &format!("{label}.balance"),
    );
    number(
        actual.available_balance,
        &e["fields"]["available_balance"],
        &format!("{label}.available_balance"),
    );
    number(
        actual.margin_balance,
        &e["fields"]["margin_balance"],
        &format!("{label}.margin_balance"),
    );
    number(
        actual.order_margin,
        &e["fields"]["order_margin"],
        &format!("{label}.order_margin"),
    );
    number(
        actual.position_margin,
        &e["fields"]["position_margin"],
        &format!("{label}.position_margin"),
    );
    string(
        &actual.currency,
        &e["fields"]["currency"],
        &format!("{label}.currency"),
    );
    boolean(
        actual.is_liquidating,
        &e["fields"]["is_liquidating"],
        &format!("{label}.is_liquidating"),
    );
    number(
        actual.position_limit,
        &e["fields"]["position_limit"],
        &format!("{label}.position_limit"),
    );
    number(
        actual.used_position_limit,
        &e["fields"]["used_position_limit"],
        &format!("{label}.used_position_limit"),
    );
    number(
        actual.position_limit_usd,
        &e["fields"]["position_limit_usd"],
        &format!("{label}.position_limit_usd"),
    );
    string(
        &actual.margin_score,
        &e["fields"]["margin_score"],
        &format!("{label}.margin_score"),
    );
    boolean(
        actual.has_risk,
        &e["fields"]["has_risk"],
        &format!("{label}.has_risk"),
    );
    optional(
        &actual.account_id,
        &e["fields"]["account_id"],
        &format!("{label}.account_id"),
        |a| {
            string(
                &a,
                &e["fields"]["account_id"],
                &format!("{label}.account_id"),
            );
        },
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsRequest(actual: &CryptocomWsRequest<V>, e: &Value, raw: &V, label: &str) {
    let _ = raw;
    number(actual.id, &e["fields"]["id"], &format!("{label}.id"));
    string(
        &actual.method,
        &e["fields"]["method"],
        &format!("{label}.method"),
    );
    optional(
        &actual.params,
        &e["fields"]["params"],
        &format!("{label}.params"),
        |a| {
            capture(
                &a.original,
                &e["fields"]["params"],
                &Host.field(raw, "params").unwrap(),
                &format!("{label}.params"),
            );
        },
    );
    optional(
        &actual.api_key,
        &e["fields"]["api_key"],
        &format!("{label}.api_key"),
        |a| {
            string(&a, &e["fields"]["api_key"], &format!("{label}.api_key"));
        },
    );
    optional(
        &actual.sig,
        &e["fields"]["sig"],
        &format!("{label}.sig"),
        |a| {
            string(&a, &e["fields"]["sig"], &format!("{label}.sig"));
        },
    );
    optional(
        &actual.nonce,
        &e["fields"]["nonce"],
        &format!("{label}.nonce"),
        |a| {
            number(*a, &e["fields"]["nonce"], &format!("{label}.nonce"));
        },
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsResponse(actual: &CryptocomWsResponse<V>, e: &Value, raw: &V, label: &str) {
    let _ = raw;
    optional(
        &actual.id,
        &e["fields"]["id"],
        &format!("{label}.id"),
        |a| {
            number(*a, &e["fields"]["id"], &format!("{label}.id"));
        },
    );
    optional(
        &actual.method,
        &e["fields"]["method"],
        &format!("{label}.method"),
        |a| {
            string(&a, &e["fields"]["method"], &format!("{label}.method"));
        },
    );
    number(actual.code, &e["fields"]["code"], &format!("{label}.code"));
    optional(
        &actual.message,
        &e["fields"]["message"],
        &format!("{label}.message"),
        |a| {
            string(&a, &e["fields"]["message"], &format!("{label}.message"));
        },
    );
    optional(
        &actual.result,
        &e["fields"]["result"],
        &format!("{label}.result"),
        |a| {
            capture(
                &a,
                &e["fields"]["result"],
                &Host.field(raw, "result").unwrap(),
                &format!("{label}.result"),
            );
        },
    );
}
#[allow(non_snake_case)]
fn check_CryptocomWsSubscription(
    actual: &CryptocomWsSubscription,
    e: &Value,
    raw: &V,
    label: &str,
) {
    let _ = raw;
    number(actual.id, &e["fields"]["id"], &format!("{label}.id"));
    check_CryptocomSubscribeMethod(
        actual.method,
        &e["fields"]["method"],
        &format!("{label}.method"),
    );
    check_CryptocomSubscriptionParams(
        &actual.params,
        &e["fields"]["params"],
        &raw,
        &format!("{label}.params"),
    );
    optional(
        &actual.nonce,
        &e["fields"]["nonce"],
        &format!("{label}.nonce"),
        |a| {
            number(*a, &e["fields"]["nonce"], &format!("{label}.nonce"));
        },
    );
}
#[allow(non_snake_case)]
fn check_CryptocomSubscriptionParams(
    actual: &CryptocomSubscriptionParams,
    e: &Value,
    raw: &V,
    label: &str,
) {
    let _ = raw;
    assert_eq!(
        actual.channels.len(),
        (&e["fields"]["channels"])["values"]
            .as_array()
            .unwrap()
            .len(),
        "CRYPTOCOM-ACCOUNT-NATIVE-ARRAY-COUNT {label}"
    );
    for (i, a) in actual.channels.iter().enumerate() {
        string(
            &a,
            &&e["fields"]["channels"]["values"][i],
            &format!("{label}.channels[{i}]"),
        );
    }
}
#[allow(non_snake_case)]
fn check_CryptocomApiResponse(actual: &CryptocomApiResponse<V>, e: &Value, raw: &V, label: &str) {
    let _ = raw;
    optional(
        &actual.id,
        &e["fields"]["id"],
        &format!("{label}.id"),
        |a| {
            number(*a, &e["fields"]["id"], &format!("{label}.id"));
        },
    );
    string(
        &actual.method,
        &e["fields"]["method"],
        &format!("{label}.method"),
    );
    number(actual.code, &e["fields"]["code"], &format!("{label}.code"));
    optional(
        &actual.result,
        &e["fields"]["result"],
        &format!("{label}.result"),
        |a| {
            capture(
                &a,
                &e["fields"]["result"],
                &Host.field(raw, "result").unwrap(),
                &format!("{label}.result"),
            );
        },
    );
    optional(
        &actual.message,
        &e["fields"]["message"],
        &format!("{label}.message"),
        |a| {
            string(&a, &e["fields"]["message"], &format!("{label}.message"));
        },
    );
}

#[test]
fn account_envelopes_match_actual_typescript() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_envelopes_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source schema oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    for row in source["rows"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let native = ingress(&row["input"]);
        let e = &row["observed"];
        let decoded_held = match row["record"].as_str().unwrap() {
            "CryptocomOrderRequest" => {
                let actual = CryptocomOrderRequest::from_native(&Host, &native).unwrap();
                check_CryptocomOrderRequest(&actual, e, &native, name);
                None
            }
            "CryptocomPosition" => {
                let actual = CryptocomPosition::from_native(&Host, &native).unwrap();
                check_CryptocomPosition(&actual, e, &native, name);
                None
            }
            "CryptocomAccountSummary" => {
                let actual = CryptocomAccountSummary::from_native(&Host, &native).unwrap();
                check_CryptocomAccountSummary(&actual, e, &native, name);
                None
            }
            "CryptocomWsRequest" => {
                let actual = CryptocomWsRequest::from_native(&Host, &native).unwrap();
                check_CryptocomWsRequest(&actual, e, &native, name);
                actual.params.as_ref().map(|v| v.original.clone())
            }
            "CryptocomWsResponse" => {
                let actual = CryptocomWsResponse::from_native(&Host, &native).unwrap();
                check_CryptocomWsResponse(&actual, e, &native, name);
                actual.result.clone()
            }
            "CryptocomWsSubscription" => {
                let actual = CryptocomWsSubscription::from_native(&Host, &native).unwrap();
                check_CryptocomWsSubscription(&actual, e, &native, name);
                None
            }
            "CryptocomApiResponse" => {
                let actual = CryptocomApiResponse::from_native(&Host, &native, Ok).unwrap();
                check_CryptocomApiResponse(&actual, e, &native, name);
                actual.result.clone()
            }
            _ => panic!("unknown record"),
        };
        // Replacement while a native field result is alive must preserve old referent.
        if !row["retained"].is_null() {
            let key = if row["record"] == "CryptocomWsRequest" {
                "params"
            } else {
                "result"
            };
            let held = decoded_held.expect("decoded original referent");
            let old = Host.field(&native, key).unwrap();
            let N::Object(fields) = native.as_ref() else {
                panic!("object")
            };
            fields
                .borrow_mut()
                .insert(key.encode_utf16().collect(), Rc::new(N::Null));
            assert!(
                Rc::ptr_eq(&held, &old),
                "CRYPTOCOM-ACCOUNT-NATIVE-RETAINED {name}"
            );
            assert!(
                !Rc::ptr_eq(&held, &Host.field(&native, key).unwrap()),
                "CRYPTOCOM-ACCOUNT-NATIVE-REPLACEMENT {name}"
            );
            if let N::Capture {
                classification,
                marker,
                ..
            } = held.as_ref()
            {
                if classification == "object" {
                    marker.set(99.0);
                    assert_eq!(
                        marker.get(),
                        99.0,
                        "CRYPTOCOM-ACCOUNT-NATIVE-MUTABLE {name}"
                    );
                }
            }
            assert_eq!(row["retained"]["same"], true);
            assert_eq!(row["retained"]["replacementAllowed"], true);
            assert_eq!(row["retained"]["oldRetained"], true);
            assert_eq!(row["retained"]["mutable"], true);
        }
    }
    let expected_schema: Value =
        serde_json::from_str(include_str!("account_envelopes_expected_schema.json")).unwrap();
    assert_eq!(
        source["schemas"], expected_schema,
        "CRYPTOCOM-ACCOUNT-DECLARED-SCHEMA after native assertions"
    );
    assert_eq!(
        source["rows"].as_array().unwrap().len(),
        235,
        "independent source fixture floor"
    );
    assert_eq!(
        serde_json::from_str::<Vec<Value>>(include_str!("account_envelopes_fixtures.json"))
            .unwrap()
            .len(),
        235,
        "independent fixture floor"
    );
    println!(
        "CRYPTOCOM account/envelope native floor235 stops index234 CryptocomAccountSummary:account-undefined-account_id; runtime bindings remain pending"
    );
}
