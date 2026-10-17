//! Stateless switches from CryptocomPrivateWebSocket; consumers remain separate.
use safe_cex_types::{OrderStatus, OrderTimeInForce, OrderType};

// String switches compare UTF-16 units without coercion. All non-strings share
// the default branch and are represented by None.
fn equals(value: Option<&[u16]>, literal: &str) -> bool {
    value.is_some_and(|value| value.iter().copied().eq(literal.encode_utf16()))
}

pub fn map_order_type(kind: Option<&[u16]>) -> OrderType {
    if equals(kind, "MARKET") {
        return OrderType::Market;
    }
    if equals(kind, "LIMIT") {
        return OrderType::Limit;
    }
    if equals(kind, "STOP_LOSS") || equals(kind, "STOP_LIMIT") {
        return OrderType::StopLoss;
    }
    if equals(kind, "TAKE_PROFIT") || equals(kind, "TAKE_PROFIT_LIMIT") {
        return OrderType::TakeProfit;
    }
    OrderType::Limit
}

pub fn map_order_status(status: Option<&[u16]>) -> OrderStatus {
    if equals(status, "ACTIVE") || equals(status, "PENDING") {
        return OrderStatus::Open;
    }
    if equals(status, "FILLED") {
        return OrderStatus::Closed;
    }
    if equals(status, "CANCELED") || equals(status, "EXPIRED") || equals(status, "REJECTED") {
        return OrderStatus::Canceled;
    }
    OrderStatus::Open
}

pub fn map_time_in_force(tif: Option<&[u16]>) -> OrderTimeInForce {
    if equals(tif, "IMMEDIATE_OR_CANCEL") {
        return OrderTimeInForce::ImmediateOrCancel;
    }
    if equals(tif, "FILL_OR_KILL") {
        return OrderTimeInForce::FillOrKill;
    }
    if equals(tif, "POST_ONLY") {
        return OrderTimeInForce::PostOnly;
    }
    OrderTimeInForce::GoodTillCancel
}

#[cfg(test)]
mod accepted_equals {
    #[test]
    fn native_strict_comparison() {
        let cases:Vec<serde_json::Value>=serde_json::from_str(r###"[{"input":{"value":null,"literal":""},"output":false},{"input":{"value":null,"literal":"market"},"output":false},{"input":{"value":[],"literal":""},"output":true},{"input":{"value":[109,97,114,107,101,116],"literal":"market"},"output":true},{"input":{"value":[77,97,114,107,101,116],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101,116,88],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101],"literal":"market"},"output":false},{"input":{"value":[],"literal":"market"},"output":false},{"input":{"value":[109,97,114,107,101,116],"literal":""},"output":false},{"input":{"value":[109,97,114,107,101,116,0],"literal":"market"},"output":false},{"input":{"value":[0],"literal":"\u0000"},"output":true},{"input":{"value":[97,0,98],"literal":"a\u0000b"},"output":true},{"input":{"value":[937],"literal":"\u03a9"},"output":true},{"input":{"value":[55357,56898],"literal":"\ud83d\ude42"},"output":true},{"input":{"value":[233],"literal":"\u00e9"},"output":true},{"input":{"value":[101,769],"literal":"\u00e9"},"output":false},{"input":{"value":[233],"literal":"e\u0301"},"output":false},{"input":{"value":[55296],"literal":""},"output":false},{"input":{"value":[56320],"literal":""},"output":false},{"input":{"value":[109,97,114,107,101,116,55296],"literal":"market"},"output":false},{"input":{"value":[56320,109,97,114,107,101,116],"literal":"market"},"output":false},{"input":{"value":[55357,56898,120],"literal":"\ud83d\ude42"},"output":false},{"input":{"value":[120,55357,56898],"literal":"\ud83d\ude42"},"output":false},{"input":{"value":[937],"literal":"\u03c9"},"output":false}]"###).unwrap();
        assert_eq!(
            cases.len(),
            24,
            "strict comparison independent fixture count"
        );
        let mut executed = 0usize;
        for case in cases {
            let value: Option<Vec<u16>> =
                serde_json::from_value(case["input"]["value"].clone()).unwrap();
            let literal = case["input"]["literal"].as_str().unwrap();
            assert_eq!(
                super::equals(value.as_deref(), literal),
                case["output"].as_bool().unwrap(),
                "strict comparison native result input {}",
                case["input"]
            );
            executed += 1;
        }
        assert_eq!(executed, 24, "strict comparison executed fixture count");
    }
}
