// Explicit native-member contract: do not generate this mapping from serde metadata.
use safe_cex_venue_bitget::types::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
trait NativeIdentity {
    fn identity(&self, expected: &Value, path: &str, bits: &Value);
}
impl NativeIdentity for String {
    fn identity(&self, expected: &Value, path: &str, _: &Value) {
        assert_eq!(
            Some(self.as_str()),
            expected.as_str(),
            "native field identity {path}"
        );
    }
}
impl NativeIdentity for f64 {
    fn identity(&self, _: &Value, path: &str, bits: &Value) {
        assert_eq!(
            format!("{:016x}", self.to_bits()),
            bits[path].as_str().unwrap(),
            "native numeric field identity {path}"
        );
    }
}
impl<T: NativeIdentity> NativeIdentity for Option<T> {
    fn identity(&self, expected: &Value, path: &str, bits: &Value) {
        match self {
            Some(value) => {
                assert!(!expected.is_null(), "native optional presence {path}");
                value.identity(expected, path, bits);
            }
            None => assert!(expected.is_null(), "native optional absence {path}"),
        }
    }
}
impl<T: NativeIdentity> NativeIdentity for Vec<T> {
    fn identity(&self, expected: &Value, path: &str, bits: &Value) {
        let expected = expected.as_array().unwrap();
        assert_eq!(self.len(), expected.len(), "native array length {path}");
        assert_eq!(
            self.len(),
            2,
            "identity corpus needs two array elements {path}"
        );
        for (i, (actual, expected)) in self.iter().zip(expected).enumerate() {
            actual.identity(expected, &format!("{path}/{i}"), bits);
        }
    }
}
trait RecordIdentity: NativeIdentity + DeserializeOwned {
    const FIELD_COUNT: usize;
}
macro_rules! record {
    ($ty:ident { $($member:ident => $wire:literal),* $(,)? }) => {
        impl NativeIdentity for $ty {
            fn identity(&self, expected: &Value, path: &str, bits: &Value) {
                // Exhaustive destructuring makes a new Rust field a compile failure.
                let Self { $($member,)* extra } = self;
                let keys: std::collections::BTreeSet<&str> = expected.as_object().unwrap().keys().map(String::as_str).collect();
                let expected_keys: std::collections::BTreeSet<&str> = [$($wire,)* "__identityExtra"].into_iter().filter(|key| expected.get(*key).is_some()).collect();
                assert_eq!(keys,expected_keys,"native member inventory {path}");
                $($member.identity(&expected[$wire], &format!("{path}/{}",$wire),bits);)*
                assert_eq!(extra.len(),1,"native extras inventory {path}");
                assert_eq!(extra["__identityExtra"],expected["__identityExtra"],"native extras identity {path}");
            }
        }
        impl RecordIdentity for $ty {
            const FIELD_COUNT: usize = [$($wire),*].len();
        }
    };
}
record!(FetchSingleAccountResponseData {
    margin_coin => "marginCoin",
    locked => "locked",
    available => "available",
    crossed_max_available => "crossedMaxAvailable",
    isolated_max_available => "isolatedMaxAvailable",
    max_transfer_out => "maxTransferOut",
    account_equity => "accountEquity",
    usdt_equity => "usdtEquity",
    btc_equity => "btcEquity",
    crossed_risk_rate => "crossedRiskRate",
    crossed_margin_leverage => "crossedMarginLeverage",
    isolated_long_lever => "isolatedLongLever",
    isolated_short_lever => "isolatedShortLever",
    margin_mode => "marginMode",
    pos_mode => "posMode",
    unrealized_pl => "unrealizedPL",
    coupon => "coupon",
    asset_mode => "assetMode",
});
record!(FetchSingleAccountResponse {
    code => "code",
    data => "data",
    msg => "msg",
    request_time => "requestTime",
});
record!(FetchUtaAccountAssetsResponseDataAssetsItem {
    coin => "coin",
    equity => "equity",
    usd_value => "usdValue",
    balance => "balance",
    available => "available",
    debt => "debt",
    locked => "locked",
    bonus => "bonus",
});
record!(FetchUtaAccountAssetsResponseData {
    account_equity => "accountEquity",
    usdt_equity => "usdtEquity",
    btc_equity => "btcEquity",
    unrealised_pnl => "unrealisedPnl",
    usdt_unrealised_pnl => "usdtUnrealisedPnl",
    btc_unrealized_pnl => "btcUnrealizedPnl",
    eff_equity => "effEquity",
    mmr => "mmr",
    imr => "imr",
    mgn_ratio => "mgnRatio",
    position_mgn_ratio => "positionMgnRatio",
    position_value => "positionValue",
    leverage => "leverage",
    assets => "assets",
});
record!(FetchUtaAccountAssetsResponse {
    code => "code",
    msg => "msg",
    request_time => "requestTime",
    data => "data",
});
record!(FetchUtaAccountSettingsResponseDataSymbolConfigListItem {
    category => "category",
    symbol => "symbol",
    margin_mode => "marginMode",
    leverage => "leverage",
    long_leverage => "longLeverage",
    short_leverage => "shortLeverage",
});
record!(FetchUtaAccountSettingsResponseData {
    account_mode => "accountMode",
    account_level => "accountLevel",
    asset_mode => "assetMode",
    hold_mode => "holdMode",
    symbol_config_list => "symbolConfigList",
});
record!(FetchUtaAccountSettingsResponse {
    code => "code",
    msg => "msg",
    request_time => "requestTime",
    data => "data",
});
record!(BitgetUtaPosition {
    category => "category",
    symbol => "symbol",
    margin_coin => "marginCoin",
    hold_mode => "holdMode",
    pos_side => "posSide",
    hold_side => "holdSide",
    margin_mode => "marginMode",
    position_balance => "positionBalance",
    margin_size => "marginSize",
    available => "available",
    frozen => "frozen",
    total => "total",
    size => "size",
    leverage => "leverage",
    cur_realised_pnl => "curRealisedPnl",
    avg_price => "avgPrice",
    open_price_avg => "openPriceAvg",
    position_status => "positionStatus",
    unrealised_pnl => "unrealisedPnl",
    unrealized_pl => "unrealizedPL",
    liquidation_price => "liquidationPrice",
    liq_price => "liqPrice",
    mmr => "mmr",
    profit_rate => "profitRate",
    mark_price => "markPrice",
    break_even_price => "breakEvenPrice",
    total_funding => "totalFunding",
    total_fee => "totalFee",
    open_fee_total => "openFeeTotal",
    close_fee_total => "closeFeeTotal",
    deducted_fee => "deductedFee",
    cash_dividend => "cashDividend",
    created_time => "createdTime",
    updated_time => "updatedTime",
});
record!(FetchUtaPositionsResponseData {
    list => "list",
});
record!(FetchUtaPositionsResponse {
    code => "code",
    msg => "msg",
    request_time => "requestTime",
    data => "data",
});
record!(BitgetUtaOrderFeeDetailItem {
    fee_coin => "feeCoin",
    fee => "fee",
});
record!(BitgetUtaOrder {
    category => "category",
    symbol => "symbol",
    order_id => "orderId",
    client_oid => "clientOid",
    request_id => "requestId",
    price => "price",
    qty => "qty",
    amount => "amount",
    hold_mode => "holdMode",
    hold_side => "holdSide",
    pos_side => "posSide",
    delegate_type => "delegateType",
    trade_side => "tradeSide",
    order_type => "orderType",
    time_in_force => "timeInForce",
    side => "side",
    margin_mode => "marginMode",
    margin_coin => "marginCoin",
    reduce_only => "reduceOnly",
    cum_exec_qty => "cumExecQty",
    cum_exec_value => "cumExecValue",
    avg_price => "avgPrice",
    order_status => "orderStatus",
    cancel_reason => "cancelReason",
    leverage => "leverage",
    fee_detail => "feeDetail",
    created_time => "createdTime",
    updated_time => "updatedTime",
    take_profit => "takeProfit",
    stop_loss => "stopLoss",
    tp_trigger_by => "tpTriggerBy",
    sl_trigger_by => "slTriggerBy",
    tp_order_type => "tpOrderType",
    sl_order_type => "slOrderType",
    tp_limit_price => "tpLimitPrice",
    sl_limit_price => "slLimitPrice",
});
record!(FetchUtaOrdersResponseData {
    list => "list",
    cursor => "cursor",
});
record!(FetchUtaOrdersResponse {
    code => "code",
    msg => "msg",
    request_time => "requestTime",
    data => "data",
});
record!(FetchPositionsV2ResponseDataItem {
    margin_coin => "marginCoin",
    symbol => "symbol",
    hold_side => "holdSide",
    open_delegate_size => "openDelegateSize",
    margin_size => "marginSize",
    available => "available",
    locked => "locked",
    total => "total",
    leverage => "leverage",
    achieved_profits => "achievedProfits",
    open_price_avg => "openPriceAvg",
    margin_mode => "marginMode",
    pos_mode => "posMode",
    unrealized_pl => "unrealizedPL",
    liquidation_price => "liquidationPrice",
    keep_margin_rate => "keepMarginRate",
    mark_price => "markPrice",
    break_even_price => "breakEvenPrice",
    total_fee => "totalFee",
    deducted_fee => "deductedFee",
    take_profit => "takeProfit",
    stop_loss => "stopLoss",
    take_profit_id => "takeProfitId",
    stop_loss_id => "stopLossId",
    margin_ratio => "marginRatio",
    asset_mode => "assetMode",
    c_time => "cTime",
    u_time => "uTime",
});
record!(FetchPositionsV2Response {
    code => "code",
    msg => "msg",
    request_time => "requestTime",
    data => "data",
});

fn probe<T: RecordIdentity>(oracle: &Value, name: &str, pointer: &str) -> usize {
    let value = oracle["samples"][name].pointer(pointer).unwrap();
    assert_eq!(
        value.as_object().unwrap().len(),
        T::FIELD_COUNT + 1,
        "full native field inventory {name}{pointer}"
    );
    let path = format!("{name}{pointer}");
    let bits = &oracle["numericBits"];
    // Raw, owned Value and borrowed Value all reach the public decoder.
    let raw: T = serde_json::from_str(&value.to_string()).unwrap();
    raw.identity(value, &path, bits);
    let owned: T = serde_json::from_value(value.clone()).unwrap();
    owned.identity(value, &path, bits);
    let borrowed = T::deserialize(value).unwrap();
    borrowed.identity(value, &path, bits);
    // Optional syntax and nullable unions are independent TS source facts.
    let policies = oracle["fieldPolicies"][&path].as_object().unwrap();
    assert_eq!(
        policies.len(),
        T::FIELD_COUNT,
        "source field policy inventory {path}"
    );
    for (key, policy) in policies {
        let mut absent = value.clone();
        assert!(absent.as_object_mut().unwrap().remove(key).is_some());
        let decoded = serde_json::from_value::<T>(absent.clone());
        assert_eq!(
            decoded.is_ok(),
            policy["optional"].as_bool().unwrap(),
            "source optional field identity {path}/{key}"
        );
        if let Ok(decoded) = decoded {
            decoded.identity(&absent, &path, bits);
        }
        let mut null = value.clone();
        null[key] = Value::Null;
        let decoded = serde_json::from_value::<T>(null.clone());
        assert_eq!(
            decoded.is_ok(),
            policy["nullable"].as_bool().unwrap(),
            "source nullable field identity {path}/{key}"
        );
        if let Ok(decoded) = decoded {
            decoded.identity(&null, &path, bits);
        }
    }
    T::FIELD_COUNT
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_native_wire_field_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/types-oracle.cjs"
        ))
        .arg("--field-identity")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "native identity source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["samples"].as_object().unwrap().len(),
        8,
        "native wire root inventory"
    );
    let categories = [
        ("USDT-FUTURES", BitgetUtaCategory::Usdt),
        ("USDC-FUTURES", BitgetUtaCategory::Usdc),
        ("COIN-FUTURES", BitgetUtaCategory::Coin),
    ];
    assert_eq!(
        oracle["categories"],
        serde_json::json!(["USDT-FUTURES", "USDC-FUTURES", "COIN-FUTURES"]),
        "native category source inventory"
    );
    for (wire, expected) in categories {
        let value = Value::String(wire.into());
        let owned: BitgetUtaCategory = serde_json::from_value(value.clone()).unwrap();
        let raw: BitgetUtaCategory = serde_json::from_str(&value.to_string()).unwrap();
        let borrowed = <BitgetUtaCategory as serde::Deserialize>::deserialize(&value).unwrap();
        for actual in [owned, raw, borrowed] {
            assert_eq!(actual, expected, "native category identity {wire}");
        }
    }
    let mut fields = 0;
    fields +=
        probe::<FetchSingleAccountResponseData>(&oracle, "FETCH_SINGLE_ACCOUNT_RESPONSE", "/data");
    fields += probe::<FetchSingleAccountResponse>(&oracle, "FETCH_SINGLE_ACCOUNT_RESPONSE", "");
    fields += probe::<FetchUtaAccountAssetsResponseDataAssetsItem>(
        &oracle,
        "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE",
        "/data/assets/0",
    );
    fields += probe::<FetchUtaAccountAssetsResponseData>(
        &oracle,
        "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE",
        "/data",
    );
    fields +=
        probe::<FetchUtaAccountAssetsResponse>(&oracle, "FETCH_UTA_ACCOUNT_ASSETS_RESPONSE", "");
    fields += probe::<FetchUtaAccountSettingsResponseDataSymbolConfigListItem>(
        &oracle,
        "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE",
        "/data/symbolConfigList/0",
    );
    fields += probe::<FetchUtaAccountSettingsResponseData>(
        &oracle,
        "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE",
        "/data",
    );
    fields += probe::<FetchUtaAccountSettingsResponse>(
        &oracle,
        "FETCH_UTA_ACCOUNT_SETTINGS_RESPONSE",
        "",
    );
    fields += probe::<BitgetUtaPosition>(&oracle, "BITGET_UTA_POSITION", "");
    fields +=
        probe::<FetchUtaPositionsResponseData>(&oracle, "FETCH_UTA_POSITIONS_RESPONSE", "/data");
    fields += probe::<FetchUtaPositionsResponse>(&oracle, "FETCH_UTA_POSITIONS_RESPONSE", "");
    fields += probe::<BitgetUtaOrderFeeDetailItem>(&oracle, "BITGET_UTA_ORDER", "/feeDetail/0");
    fields += probe::<BitgetUtaOrder>(&oracle, "BITGET_UTA_ORDER", "");
    fields += probe::<FetchUtaOrdersResponseData>(&oracle, "FETCH_UTA_ORDERS_RESPONSE", "/data");
    fields += probe::<FetchUtaOrdersResponse>(&oracle, "FETCH_UTA_ORDERS_RESPONSE", "");
    fields += probe::<FetchPositionsV2ResponseDataItem>(
        &oracle,
        "FETCH_POSITIONS_V2_RESPONSE",
        "/data/0",
    );
    fields += probe::<FetchPositionsV2Response>(&oracle, "FETCH_POSITIONS_V2_RESPONSE", "");
    assert_eq!(
        fields, 178,
        "native declared wire field inventory (17 records)"
    );
}
