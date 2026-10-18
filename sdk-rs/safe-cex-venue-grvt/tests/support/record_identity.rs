use super::enum_identity::NativeEnum;
use super::*;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_grvt::types::*;

pub(super) trait NativeIdentity {
    fn assert_native(&self, expected: &Value, path: &str);
    fn assert_retained(&self, expected: &Input, path: &str) {
        let view = expected.clone().decode_native::<Value>(false).unwrap();
        self.assert_native(&view, path);
    }
}
impl NativeIdentity for String {
    fn assert_native(&self, expected: &Value, path: &str) {
        assert_eq!(
            Some(self.as_str()),
            expected.as_str(),
            "GRVT native string identity {path}"
        );
    }
}
impl NativeIdentity for bool {
    fn assert_native(&self, expected: &Value, path: &str) {
        assert_eq!(
            Some(*self),
            expected.as_bool(),
            "GRVT native bool identity {path}"
        );
    }
}
impl NativeIdentity for f64 {
    fn assert_native(&self, expected: &Value, path: &str) {
        let expected = safe_cex_num::numeric_ingress::js_value_as_f64(expected).unwrap();
        assert_eq!(
            self.to_bits(),
            expected.to_bits(),
            "GRVT native number identity {path}"
        );
    }
}
impl<T: NativeIdentity> NativeIdentity for Option<T> {
    fn assert_retained(&self, expected: &Input, path: &str) {
        if let Some(value) = self {
            value.assert_retained(expected, path);
        } else {
            assert!(
                matches!(expected, Input::Null | Input::Undefined),
                "GRVT native absent retained identity {path}"
            );
        }
    }
    fn assert_native(&self, expected: &Value, path: &str) {
        if let Some(value) = self {
            value.assert_native(expected, path);
        } else {
            assert!(expected.is_null(), "GRVT native absent identity {path}");
        }
    }
}
impl<T: NativeIdentity> NativeIdentity for Vec<T> {
    fn assert_retained(&self, expected: &Input, path: &str) {
        let Input::Array(expected) = expected else {
            panic!("GRVT expected retained array {path}")
        };
        assert_eq!(
            self.len(),
            expected.len(),
            "GRVT native retained array length {path}"
        );
        for (i, (actual, expected)) in self.iter().zip(expected).enumerate() {
            actual.assert_retained(expected, &format!("{path}[{i}]"));
        }
    }
    fn assert_native(&self, expected: &Value, path: &str) {
        let expected = expected.as_array().unwrap();
        assert_eq!(
            self.len(),
            expected.len(),
            "GRVT native array length {path}"
        );
        for (i, (actual, expected)) in self.iter().zip(expected).enumerate() {
            actual.assert_native(expected, &format!("{path}[{i}]"));
        }
    }
}
impl NativeIdentity for safe_cex_num::numeric_ingress::JsNumberInput {
    fn assert_retained(&self, expected: &Input, path: &str) {
        let actual = self.clone().decode::<Input>().unwrap();
        assert_retained_data(&actual, expected, path);
    }
    fn assert_native(&self, expected: &Value, path: &str) {
        assert_eq!(
            serde_json::to_value(self).unwrap(),
            *expected,
            "GRVT native opaque field identity {path}"
        );
    }
}

// Inspect native opaque contents before a JSON writer can replace units or
// numeric bits. The old Value-only expectation cannot represent these inputs.
fn assert_retained_data(actual: &Input, expected: &Input, path: &str) {
    let number = |value: &Input| match value {
        Input::Float(value) => Some(*value),
        Input::Lexeme(value) => Some(value.parse::<f64>().unwrap()),
        _ => None,
    };
    if let Some(expected) = number(expected) {
        assert_eq!(
            number(actual).unwrap().to_bits(),
            expected.to_bits(),
            "GRVT retained number bits {path}"
        );
        return;
    }
    match (actual, expected) {
        (Input::Array(actual), Input::Array(expected)) => {
            assert_eq!(
                actual.len(),
                expected.len(),
                "GRVT retained array length {path}"
            );
            for (i, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_retained_data(actual, expected, &format!("{path}[{i}]"));
            }
        }
        (Input::Object(actual), Input::Object(expected)) => {
            assert_eq!(
                actual.len(),
                expected.len(),
                "GRVT retained object length {path}"
            );
            for (i, ((key, actual), (expected_key, expected))) in
                actual.iter().zip(expected).enumerate()
            {
                assert_eq!(key, expected_key, "GRVT retained key units {path}[{i}]");
                assert_retained_data(actual, expected, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(actual, expected, "GRVT retained native contents {path}"),
    }
}
macro_rules! native_record {
    ($ty:ident { $($field:ident => $wire:literal),* $(,)? }) => {
        impl NativeIdentity for $ty {
            fn assert_retained(&self, expected: &Input, path: &str) {
                let Self { $($field),* } = self;
                $($field.assert_retained(expected.get($wire).unwrap_or(&Input::Undefined), &format!("{path}.{}", $wire));)*
            }
            fn assert_native(&self, expected: &Value, path: &str) {
                let Self { $($field),* } = self;
                $($field.assert_native(&expected[$wire], &format!("{path}.{}", $wire));)*
            }
        }
    };
}
macro_rules! native_enum {
    ($($ty:ty),* $(,)?) => {$(impl NativeIdentity for $ty {
        fn assert_native(&self, expected: &Value, path: &str) {
            assert_eq!(Some(self.native_wire()), expected.as_str(), "GRVT native enum identity {path}");
        }
    })*};
}
native_enum!(
    GrvtAssetKind,
    GrvtOrderSide,
    GrvtOrderType,
    GrvtTimeInForce,
    GrvtOrderStatus,
    GrvtLiquidity,
    GrvtJsonRpcVersion,
    GrvtSubscriptionMethod,
    GrvtTriggerKind,
    safe_cex_types::OrderStopTriggerType
);

native_record!(GrvtInstrument {
    instrument_id => "instrument_id",
    instrument_hash => "instrument_hash",
    base => "base",
    quote => "quote",
    kind => "kind",
    venue => "venue",
    settlement_period => "settlement_period",
    tick_size => "tick_size",
    min_size => "min_size",
    max_size => "max_size",
    step_size => "step_size",
    mark_price => "mark_price",
    index_price => "index_price",
    last_trade_price => "last_trade_price",
    last_trade_time => "last_trade_time",
    is_active => "is_active",
});
native_record!(GrvtTicker {
    instrument_id => "instrument_id",
    last_price => "last_price",
    bid_price => "bid_price",
    ask_price => "ask_price",
    volume_24h => "volume_24h",
    price_change_24h => "price_change_24h",
    price_change_pct_24h => "price_change_pct_24h",
    high_24h => "high_24h",
    low_24h => "low_24h",
    funding_rate => "funding_rate",
    next_funding_time => "next_funding_time",
    mark_price => "mark_price",
    index_price => "index_price",
    open_interest => "open_interest",
    timestamp => "timestamp",
});
native_record!(GrvtOrderBookLevel {
    p => "p",
    s => "s",
    n => "n",
});
native_record!(GrvtOrderBook {
    instrument_id => "instrument_id",
    bids => "bids",
    asks => "asks",
    timestamp => "timestamp",
});
native_record!(GrvtTrade {
    trade_id => "trade_id",
    instrument_id => "instrument_id",
    price => "price",
    size => "size",
    side => "side",
    timestamp => "timestamp",
});
native_record!(GrvtKline {
    timestamp => "timestamp",
    open => "open",
    high => "high",
    low => "low",
    close => "close",
    volume => "volume",
});
native_record!(GrvtPosition {
    sub_account_id => "sub_account_id",
    instrument_id => "instrument_id",
    size => "size",
    notional => "notional",
    entry_price => "entry_price",
    mark_price => "mark_price",
    unrealized_pnl => "unrealized_pnl",
    realized_pnl => "realized_pnl",
    total_pnl => "total_pnl",
    roi => "roi",
    leverage => "leverage",
    liquidation_price => "liquidation_price",
    margin => "margin",
    timestamp => "timestamp",
});
native_record!(GrvtBalance {
    currency => "currency",
    total => "total",
    available => "available",
    reserved => "reserved",
});
native_record!(GrvtAccountSummary {
    sub_account_id => "sub_account_id",
    balances => "balances",
    total_collateral => "total_collateral",
    total_notional => "total_notional",
    total_upnl => "total_upnl",
    account_leverage => "account_leverage",
    margin_usage => "margin_usage",
    is_liquidatable => "is_liquidatable",
    timestamp => "timestamp",
});
native_record!(GrvtOrder {
    order_id => "order_id",
    client_order_id => "client_order_id",
    sub_account_id => "sub_account_id",
    instrument_id => "instrument_id",
    side => "side",
    r#type => "type",
    time_in_force => "time_in_force",
    price => "price",
    size => "size",
    filled_size => "filled_size",
    remaining_size => "remaining_size",
    average_fill_price => "average_fill_price",
    status => "status",
    post_only => "post_only",
    reduce_only => "reduce_only",
    created_at => "created_at",
    updated_at => "updated_at",
    expires_at => "expires_at",
});
native_record!(GrvtAuthRequest {
    api_key => "api_key",
    signature => "signature",
    nonce => "nonce",
    expires_at => "expires_at",
});
native_record!(GrvtAuthResponse {
    session_token => "session_token",
    account_id => "account_id",
    sub_account_id => "sub_account_id",
    expires_at => "expires_at",
});
native_record!(GrvtFill {
    fill_id => "fill_id",
    trade_id => "trade_id",
    order_id => "order_id",
    sub_account_id => "sub_account_id",
    instrument_id => "instrument_id",
    side => "side",
    price => "price",
    size => "size",
    fee => "fee",
    fee_currency => "fee_currency",
    liquidity => "liquidity",
    timestamp => "timestamp",
});
native_record!(OrderLeg {
    instrument => "instrument",
    size => "size",
    limit_price => "limitPrice",
    is_buying_asset => "isBuyingAsset",
});
native_record!(GrvtBuilderAuthorizationResult {
    success => "success",
    message => "message",
});
native_record!(GrvtInstrumentLite {
    i => "i",
    ih => "ih",
    b => "b",
    q => "q",
    k => "k",
    v => "v",
    sp1 => "sp1",
    bd => "bd",
    qd => "qd",
    ts => "ts",
    ms => "ms",
    ss => "ss",
    mp => "mp",
    ip => "ip",
    ltp => "ltp",
    ltt => "ltt",
    ia => "ia",
    ct => "ct",
    fi => "fi",
    af => "af",
    af1 => "af1",
});
native_record!(GrvtSignature {
    r => "r",
    s => "s",
    v => "v",
    signer => "signer",
    nonce => "nonce",
    expiration => "expiration",
});
native_record!(OrderPayload {
    sub_account_id => "subAccountId",
    is_market => "isMarket",
    time_in_force => "timeInForce",
    post_only => "postOnly",
    reduce_only => "reduceOnly",
    legs => "legs",
    nonce => "nonce",
    expiration => "expiration",
});
native_record!(GrvtBuilderCheckResult {
    authorized => "authorized",
    max_fee_rate => "maxFeeRate",
    max_fee_rate_raw => "maxFeeRateRaw",
});
native_record!(GrvtBuilderInfo {
    builder => "builder",
    builder_fee => "builderFee",
    builder_fee_int => "builderFeeInt",
});
native_record!(GrvtWebSocketSubscriptionParams {
    channel => "channel",
    instrument_id => "instrument_id",
    interval => "interval",
});
native_record!(GrvtWebSocketSubscription {
    jsonrpc => "jsonrpc",
    id => "id",
    method => "method",
    params => "params",
});
native_record!(GrvtWebSocketError {
    code => "code",
    message => "message",
    data => "data",
});
native_record!(GrvtWebSocketMessage {
    jsonrpc => "jsonrpc",
    method => "method",
    params => "params",
    result => "result",
    error => "error",
});
native_record!(GrvtTriggerMetadata {
    r#type => "type",
    trigger_price => "triggerPrice",
    trigger_type => "triggerType",
    close => "close",
    is_split_position => "isSplitPosition",
});
