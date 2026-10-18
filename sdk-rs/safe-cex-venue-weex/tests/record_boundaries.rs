use safe_cex_num::numeric_ingress::{JsNumberOutput, js_from_str, js_from_value};
use safe_cex_types::{MarginType, Market, PositionMode, PositionSide};
use safe_cex_venue_weex::utils as utility;
use safe_cex_venue_weex::{
    income::normalize_income,
    market_data::normalize_market,
    public_trade::normalize_public_trade,
    response_types::{
        WeexExchangeInfoRecord, WeexExchangeInfoResponse, WeexKlineRow, WeexNumberOrString,
        WeexOrderResult,
    },
    utils::{WeexUtilityInput, fake_positions_for_market, to_integer_string},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[path = "support/source_profile.rs"]
mod source_profile;

const WORDS: [&str; 4] = ["A/Q", "🚀/Q", "�/Q", r"\ud800/Q"];
const SOURCE: &str = "../parity/evidence/okx-kraken-weex-audit/source-observations.mjs";
const FULL_SOURCE: &str = "../parity/evidence/okx-kraken-weex-audit/full-source-observations.mjs";

fn routes<T: DeserializeOwned + Serialize + JsNumberOutput>(text: &str) -> [T; 2] {
    [
        serde_json::from_str(text).unwrap(),
        js_from_str(text).unwrap(),
    ]
}

fn refuse<T: DeserializeOwned + Serialize + JsNumberOutput>(text: &str, label: &str) {
    assert!(
        serde_json::from_str::<T>(text).is_err(),
        "{label} serde whole input"
    );
    assert!(
        js_from_str::<T>(text).is_err(),
        "{label} numeric whole input"
    );
}

fn record_name(record: &WeexExchangeInfoRecord) -> String {
    match record {
        WeexExchangeInfoRecord::Object(fields) => fields
            .iter()
            .find(|(name, _)| name.as_str() == "name")
            .expect("native named record field")
            .1
            .clone()
            .decode::<String>()
            .unwrap(),
        WeexExchangeInfoRecord::Array(_) => panic!("native record must be object"),
    }
}

fn market_value(symbol: &str) -> Value {
    json!({"id":"venue-id", "symbol":symbol, "base":"B", "quote":"Q", "active":true,
        "precision":{"amount":1,"price":1},
        "limits":{"amount":{"min":0,"max":10},"leverage":{"min":1,"max":20}}})
}

#[test]
fn kline_returned_string_is_native_before_whole_tuple_refusal() {
    let mut compared = 0;
    for word in WORDS {
        let text = json!([
            word, "101", "202", "303", "404", "505", 606, "707", 808, "909", "1010"
        ])
        .to_string();
        for native in routes::<WeexKlineRow>(&text) {
            assert!(
                matches!(&native.0, WeexNumberOrString::String(value) if value == word),
                "WEEX native kline timestamp string"
            );
            assert_eq!(
                [
                    native.1.as_str(),
                    native.2.as_str(),
                    native.3.as_str(),
                    native.4.as_str(),
                    native.5.as_str(),
                    native.9.as_str(),
                    native.10.as_str()
                ],
                ["101", "202", "303", "404", "505", "909", "1010"],
                "WEEX native kline string slots"
            );
            assert!(
                matches!(native.6, WeexNumberOrString::Number(value) if value.to_bits() == 606.0_f64.to_bits())
            );
            assert_eq!(native.7, "707", "WEEX native kline quote slot");
            assert!(
                matches!(native.8, WeexNumberOrString::Number(value) if value.to_bits() == 808.0_f64.to_bits())
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX kline native route floor");
    refuse::<WeexKlineRow>(
        r#"["\ud800","1","2","3","4","5",6,"7",8,"9","10"]"#,
        "WEEX returned kline unit",
    );
    for observation in source_profile::observe(
        "tests/divergence-observations.cjs",
        "kline-unit",
        "WEEX SOURCE kline returned unit",
    ) {
        assert_eq!(
            observation["cases"],
            json!([{"id":"tuple-0","units":[0xd800]}])
        );
    }
}

#[test]
fn utility_input_refusals_lose_normal_results() {
    let scalar: WeexUtilityInput = js_from_str(r#""?""#).unwrap();
    assert_eq!(
        utility::to_number(Some(&scalar), None).unwrap().to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        utility::normalize_percentage(Some(&scalar), 12.0, 10.0)
            .unwrap()
            .to_bits(),
        20.0_f64.to_bits()
    );
    assert_eq!(
        utility::normalize_timestamp_to_ms(Some(&scalar), |v| v)
            .unwrap()
            .to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        utility::normalize_timestamp_to_seconds(Some(&scalar), |v| v)
            .unwrap()
            .to_bits(),
        0.0_f64.to_bits()
    );
    assert_eq!(
        utility::normalize_position_mode(Some(&scalar)).unwrap(),
        PositionMode::OneWay
    );
    assert_eq!(
        utility::normalize_margin_type(Some(&scalar)).unwrap(),
        MarginType::Cross
    );
    assert_eq!(
        utility::normalize_position_side(Some(&scalar)).unwrap(),
        PositionSide::Long
    );
    assert_eq!(utility::normalize_order_side(Some(&scalar)).unwrap(), None);
    assert_eq!(
        utility::normalize_order_status(Some(&scalar)).unwrap(),
        safe_cex_types::OrderStatus::Open
    );
    assert_eq!(
        utility::normalize_order_type(Some(&scalar)).unwrap(),
        safe_cex_types::OrderType::Limit
    );
    assert_eq!(
        utility::normalize_trigger_type(Some(&scalar)).unwrap(),
        safe_cex_types::OrderStopTriggerType::LastPrice
    );
    let candle: WeexUtilityInput=js_from_str(r#"{"timestamp":1700000000000,"open":"11","high":"13","low":"7","close":"12","volume":"5"}"#).unwrap();
    let candle = utility::normalize_candle(Some(&candle), |v| v)
        .unwrap()
        .unwrap();
    assert_eq!(
        [
            candle.timestamp,
            candle.open,
            candle.high,
            candle.low,
            candle.close,
            candle.volume
        ]
        .map(f64::to_bits),
        [1700000000.0_f64, 11.0, 13.0, 7.0, 12.0, 5.0].map(f64::to_bits)
    );
    let side: WeexUtilityInput = js_from_str(r#"[["1","2"]]"#).unwrap();
    let side = utility::normalize_order_book_side(Some(&side)).unwrap();
    assert_eq!(side.len(), 1);
    assert_eq!(
        [side[0].price, side[0].amount, side[0].total].map(f64::to_bits),
        [1.0_f64, 2.0, 0.0].map(f64::to_bits)
    );
    let book: WeexUtilityInput = js_from_str(r#"{"bids":[["1","2"]],"asks":[["3","4"]]}"#).unwrap();
    let book = utility::normalize_order_book(Some(&book)).unwrap();
    assert_eq!((book.bids.len(), book.asks.len()), (1, 1));
    assert_eq!(
        [
            book.bids[0].price,
            book.bids[0].amount,
            book.bids[0].total,
            book.asks[0].price,
            book.asks[0].amount,
            book.asks[0].total
        ]
        .map(f64::to_bits),
        [1.0_f64, 2.0, 2.0, 3.0, 4.0, 4.0].map(f64::to_bits)
    );
    let names = [
        "toNumber",
        "normalizePercentage",
        "normalizeTimestampToMs",
        "normalizeTimestampToSeconds",
        "normalizePositionMode",
        "normalizeMarginType",
        "normalizePositionSide",
        "normalizeOrderSide",
        "normalizeOrderStatus",
        "normalizeOrderType",
        "normalizeTriggerType",
        "normalizeCandle",
        "normalizeOrderBookSide",
        "normalizeOrderBook",
    ];
    let deep = format!("{}\".25\"{}", "[".repeat(150), "]".repeat(150));
    let mut compared = 0;
    for name in names {
        for (kind, unread) in [("unit", r#""\ud800""#), ("depth150", deep.as_str())] {
            let raw = match name {
                "normalizeCandle" => format!(
                    r#"{{"timestamp":1700000000000,"open":"11","high":"13","low":"7","close":"12","volume":"5","ignored":{unread}}}"#
                ),
                "normalizeOrderBookSide" => format!(r#"[["1","2",{unread}]]"#),
                "normalizeOrderBook" => {
                    format!(r#"{{"bids":[["1","2"]],"asks":[["3","4"]],"ignored":{unread}}}"#)
                }
                _ => unread.to_owned(),
            };
            refuse::<WeexUtilityInput>(&raw, &format!("WEEX input cost {name}:{kind}"));
            compared += 1;
        }
    }
    assert_eq!(compared, 28, "WEEX native input cost case floor");
    for observation in source_profile::observe(
        "tests/divergence-observations.cjs",
        "input-prices",
        "WEEX SOURCE input price case floor",
    ) {
        let cases = observation["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 28);
        let ids = names
            .into_iter()
            .flat_map(|name| [format!("{name}:unit"), format!("{name}:depth150")])
            .collect::<Vec<_>>();
        assert_eq!(
            cases
                .iter()
                .map(|c| c["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ids
        );
    }
}

#[test]
fn income_range_returns_native_ids_before_whole_page_refusal() {
    use safe_cex_exchange::runtime::ExchangeFuture;
    use safe_cex_types::IncomeRecord;
    use safe_cex_venue_weex::income_reads::{WeexIncomeRangeRuntime, fetch_income_history_range};
    use std::{
        cell::RefCell,
        task::{Context, Poll, Waker},
    };
    let mut compared = 0;
    for word in WORDS {
        let record: IncomeRecord=serde_json::from_str(&json!({"symbol":"B/Q","incomeType":"FUNDING_FEE","income":3,"asset":"Q","timestamp":1,"tranId":word}).to_string()).unwrap();
        let host = RefCell::new((record, 0_usize, Vec::<f64>::new()));
        let runtime = WeexIncomeRangeRuntime {
            fetch_page: |host: &RefCell<(IncomeRecord, usize, Vec<f64>)>, params, _: ()| {
                assert_eq!(params.limit.to_bits(), 100.0_f64.to_bits());
                let mut state = host.borrow_mut();
                state.1 += 1;
                let records = if state.1 == 1 {
                    vec![state.0.clone()]
                } else {
                    vec![]
                };
                Ok(Box::pin(std::future::ready(Ok(records)))
                    as ExchangeFuture<'_, Vec<IncomeRecord>>)
            },
            sleep: |host, ms| {
                host.borrow_mut().2.push(ms);
                Ok(Box::pin(std::future::ready(Ok(()))))
            },
            warning_sink: |_| panic!("WEEX two-point native range must not truncate"),
            warning: |_, _: (), _| panic!("WEEX unexpected native range warning"),
            iso_date: |_, _| panic!("WEEX unexpected native range date"),
        };
        let mut future = fetch_income_history_range(&host, &runtime, 1.0, 2.0, ());
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        let records = (0..32)
            .find_map(|_| match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => Some(value.unwrap()),
                Poll::Pending => None,
            })
            .expect("WEEX native range poll floor");
        assert_eq!(records.len(), 1, "WEEX native range record floor");
        assert_eq!(records[0].tran_id, word, "WEEX native range transaction id");
        assert_eq!(records[0].income.to_bits(), 3.0_f64.to_bits());
        assert_eq!(host.borrow().1, 3, "WEEX native range fetch floor");
        assert_eq!(host.borrow().2, [100.0, 100.0, 100.0]);
        compared += 1;
    }
    assert_eq!(compared, 4, "WEEX native range positive floor");
    assert!(
        serde_json::from_str::<Vec<IncomeRecord>>(r#"[{"symbol":"B/Q","incomeType":"FUNDING_FEE","income":3,"asset":"Q","timestamp":1,"tranId":"\ud800"}]"#).is_err(),
        "WEEX range page input returned unit whole page"
    );
    for observation in source_profile::observe(
        "tests/divergence-observations.cjs",
        "income-range-unit",
        "WEEX SOURCE range returned unit",
    ) {
        assert_eq!(
            observation["cases"],
            json!([{"id":"returned-tran-id","units":[0xd800],"count":1}])
        );
    }
}

#[test]
fn exchange_info_names_are_native_before_whole_record_refusal() {
    let mut compared = 0;
    for word in WORDS {
        let asset = format!("asset:{word}");
        let symbol = format!("symbol:{word}");
        let text = json!({"assets":[{"name":asset}],"symbols":[{"name":symbol}]}).to_string();
        for native in routes::<WeexExchangeInfoResponse>(&text) {
            let assets = native.assets.unwrap();
            let symbols = native.symbols.unwrap();
            assert_eq!(assets.len(), 1, "WEEX native response asset floor");
            assert_eq!(symbols.len(), 1, "WEEX native response symbol floor");
            assert_eq!(
                record_name(&assets[0]),
                asset,
                "WEEX native response asset name"
            );
            assert_eq!(
                record_name(&symbols[0]),
                symbol,
                "WEEX native response symbol name"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX response name route floor");
    refuse::<WeexExchangeInfoResponse>(
        r#"{"assets":[{"name":"\ud800"}],"symbols":[]}"#,
        "WEEX exchange info returned unit",
    );
    for observation in source_profile::observe(
        SOURCE,
        "weex-response-unit",
        "SOURCE-WEEX-RESPONSE-UNIT materialized assets[0].name",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
}

#[test]
fn order_result_ids_are_native_before_whole_record_refusal() {
    let mut compared = 0;
    for word in WORDS {
        let order_id = format!("order:{word}");
        let client_id = format!("client:{word}");
        let error_code = format!("code:{word}");
        let error_message = format!("message:{word}");
        let text = json!({"orderId":order_id,"clientOrderId":client_id,"success":true,
            "errorCode":error_code,"errorMessage":error_message})
        .to_string();
        for native in routes::<WeexOrderResult>(&text) {
            assert_eq!(
                native.order_id.as_deref(),
                Some(order_id.as_str()),
                "WEEX native result order id"
            );
            assert_eq!(
                native.client_order_id.as_deref(),
                Some(client_id.as_str()),
                "WEEX native result client id"
            );
            assert_eq!(native.success, Some(true), "WEEX native result success");
            assert_eq!(
                native.error_code.as_deref(),
                Some(error_code.as_str()),
                "WEEX native result error code"
            );
            assert_eq!(
                native.error_message.as_deref(),
                Some(error_message.as_str()),
                "WEEX native result error message"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX result id route floor");
    refuse::<WeexOrderResult>(
        r#"{"success":true,"orderId":"\ud800","clientOrderId":"ordinary"}"#,
        "WEEX order result returned unit",
    );
    for observation in source_profile::observe(
        FULL_SOURCE,
        "weex-order-result-unit",
        "FULL-WEEX-ORDER-RESULT-UNIT actual consumer returned id",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
}

#[test]
fn integer_string_is_native_before_whole_argument_refusal() {
    let mut compared = 0;
    for word in WORDS {
        for native in routes::<WeexUtilityInput>(&json!(word).to_string()) {
            assert_eq!(
                to_integer_string(Some(&native)).unwrap(),
                word,
                "WEEX native integer string"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX integer string route floor");
    refuse::<WeexUtilityInput>(r#""\ud800""#, "WEEX integer string returned unit");
    for observation in source_profile::observe(
        SOURCE,
        "weex-integer-unit",
        "SOURCE-WEEX-INTEGER-UNIT returned String",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
}

#[test]
fn income_id_is_native_before_whole_record_refusal() {
    let mut compared = 0;
    for word in WORDS {
        let bill_id = format!("bill:{word}");
        let trade_id = format!("trade:{word}");
        let text = json!({"symbol":"B/Q","incomeType":"FUNDING_FEE","income":3,
            "time":1700000000123_u64,"billId":bill_id,"tradeId":trade_id})
        .to_string();
        for raw in routes::<WeexUtilityInput>(&text) {
            let native = normalize_income(&raw, |v| v).unwrap();
            assert_eq!(native.tran_id, bill_id, "WEEX native income transaction id");
            assert_eq!(
                native.trade_id.as_deref(),
                Some(trade_id.as_str()),
                "WEEX native income trade id"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX income id route floor");
    refuse::<WeexUtilityInput>(
        r#"{"billId":"\ud800","income":3,"time":1700000000123}"#,
        "WEEX income returned transaction unit",
    );
    for observation in source_profile::observe(
        FULL_SOURCE,
        "weex-income-unit",
        "FULL-WEEX-INCOME-UNIT returned tranId",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
}

#[test]
fn public_trade_id_is_native_before_whole_record_refusal() {
    let mut compared = 0;
    for word in WORDS {
        let id = format!("public:{word}");
        let text = json!({"id":id,"price":3,"qty":5,"time":1700000000123_u64}).to_string();
        for raw in routes::<WeexUtilityInput>(&text) {
            let native = normalize_public_trade(&raw, "B/Q", |v| v).unwrap().unwrap();
            assert_eq!(native.id, id, "WEEX native public trade id");
            assert_eq!(native.symbol, "B/Q", "WEEX native public trade symbol");
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX public trade route floor");
    refuse::<WeexUtilityInput>(
        r#"{"id":"\ud800","price":3,"qty":5,"time":1700000000123}"#,
        "WEEX public trade returned unit",
    );
    for observation in source_profile::observe(
        FULL_SOURCE,
        "weex-public-trade-unit",
        "FULL-WEEX-PUBLIC-TRADE-UNIT returned id",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
}

#[test]
fn fake_positions_keep_native_symbols_and_price_market_admission() {
    let mut compared = 0;
    for word in WORDS {
        for market in routes::<Market>(&market_value(word).to_string()) {
            for mode in [PositionMode::OneWay, PositionMode::Hedged] {
                let native = fake_positions_for_market(&market, mode, MarginType::Cross, 3.0);
                assert_eq!(native.len(), 2, "WEEX native fake position floor");
                for (position, side, suffix) in [
                    (&native[0], PositionSide::Long, "long"),
                    (&native[1], PositionSide::Short, "short"),
                ] {
                    assert_eq!(position.symbol, word, "WEEX native fake position symbol");
                    let id = if mode == PositionMode::OneWay {
                        word.to_owned()
                    } else {
                        format!("{word}:{suffix}")
                    };
                    assert_eq!(position.id, id, "WEEX native fake position id");
                    assert_eq!(position.side, side, "WEEX native fake position side");
                    assert_eq!(
                        position.info.as_ref().unwrap(),
                        &json!({}).into(),
                        "WEEX native fake fresh info"
                    );
                    compared += 1;
                }
            }
        }
    }
    assert_eq!(compared, 32, "WEEX fake position comparison floor");
    let text = market_value("B/Q").to_string();
    assert_eq!(text.matches("\"base\":\"B\"").count(), 1);
    let unused_unit = text.replace("\"base\":\"B\"", "\"base\":\"\\ud800\"");
    refuse::<Market>(&unused_unit, "WEEX unread Market base admission");
    let symbol_unit = text.replace("\"symbol\":\"B/Q\"", "\"symbol\":\"\\ud800\"");
    refuse::<Market>(&symbol_unit, "WEEX returned fake position symbol unit");
    let deep = format!("{}1{}", "[".repeat(150), "]".repeat(150));
    let deep_market = text.replacen('{', &format!("{{\"info\":{{\"ignored\":{deep}}},"), 1);
    refuse::<Market>(&deep_market, "WEEX unread Market info depth admission");
    for observation in source_profile::observe(
        SOURCE,
        "weex-fake-unit",
        "SOURCE-WEEX-FAKE-UNIT returned symbol",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
    for observation in source_profile::observe(
        "tests/record-boundary-oracle.cjs",
        "fake-input",
        "WEEX SOURCE unused Market input positions",
    ) {
        let cases = observation["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2, "WEEX unused Market input case floor");
        assert_eq!(cases[0]["id"], "unused-base-unit");
        assert_eq!(cases[1]["id"], "unused-info-depth");
        for case in cases {
            assert_eq!(case["count"], 2);
        }
    }
}

#[test]
fn market_strings_and_info_are_native_before_whole_record_refusal() {
    let mut compared = 0;
    for word in WORDS {
        let text = json!({"symbol":word,"baseAsset":"B","quoteAsset":"Q","extra":word}).to_string();
        for raw in routes::<WeexUtilityInput>(&text) {
            let native = normalize_market(&raw).unwrap();
            assert_eq!(native.symbol, word, "WEEX native market symbol");
            assert_eq!(native.id, word, "WEEX native market id");
            assert_eq!(
                native
                    .info
                    .as_ref()
                    .unwrap()
                    .get("extra")
                    .unwrap()
                    .as_text()
                    .unwrap(),
                word.encode_utf16().collect::<Vec<_>>(),
                "WEEX native market retained info"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8, "WEEX market string route floor");
    refuse::<WeexUtilityInput>(
        r#"{"symbol":"B/Q","baseAsset":"B","quoteAsset":"Q","extra":"\ud800"}"#,
        "WEEX market returned info unit",
    );
    for observation in source_profile::observe(
        "tests/record-boundary-oracle.cjs",
        "market-unit",
        "WEEX SOURCE market returned info unit",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
    }
}

#[test]
fn market_numeric_errors_precede_string_admission() {
    let mut compared = 0;
    for field in ["symbol", "baseAsset", "quoteAsset"] {
        for numeric in [
            "maxOrderSize",
            "minLeverage",
            "maxLeverage",
            "makerFeeRate",
            "takerFeeRate",
        ] {
            let mut raw = json!({"symbol":"B/Q","baseAsset":"B","quoteAsset":"Q"});
            raw[field] = Value::Null;
            raw[numeric] = json!({"toString":null});
            let native: WeexUtilityInput = js_from_value(raw).unwrap();
            let error = normalize_market(&native).unwrap_err();
            assert_eq!(
                error.name, "TypeError",
                "WEEX market numeric conversion class"
            );
            assert_eq!(
                error.message, "Cannot convert object to primitive value",
                "WEEX market numeric conversion before string admission {field}:{numeric}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 15, "WEEX market numeric before admission floor");
    for observation in source_profile::observe(
        "tests/record-boundary-oracle.cjs",
        "market-order",
        "WEEX SOURCE market conversion before string admission",
    ) {
        let cases = observation["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 15, "WEEX source market coercion case floor");
        let identities = [
            "symbol:maxOrderSize",
            "symbol:minLeverage",
            "symbol:maxLeverage",
            "symbol:makerFeeRate",
            "symbol:takerFeeRate",
            "baseAsset:maxOrderSize",
            "baseAsset:minLeverage",
            "baseAsset:maxLeverage",
            "baseAsset:makerFeeRate",
            "baseAsset:takerFeeRate",
            "quoteAsset:maxOrderSize",
            "quoteAsset:minLeverage",
            "quoteAsset:maxLeverage",
            "quoteAsset:makerFeeRate",
            "quoteAsset:takerFeeRate",
        ];
        for (case, identity) in cases.iter().zip(identities) {
            assert_eq!(case["id"], identity, "WEEX source coercion identity");
            assert_eq!(case["name"], "TypeError");
            assert_eq!(case["message"], "Cannot convert object to primitive value");
        }
    }
}
