//! Owner-decided Partial TP/SL drag semantics against the shared and inverse TS contracts.
//! Payloads follow Bybit V5 order, position and trading-stop wire records.
mod support;

#[path = "partial_stops/proof_terminal.rs"]
mod proof_terminal;

use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::{Command, Event, HttpRequest};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::Harness;
use support::Options;

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const OK: &str = r#"{"retCode":0,"retMsg":"OK","result":{}}"#;
const WALLET: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"accountType":"UNIFIED","totalEquity":"1000","totalWalletBalance":"1000","totalMarginBalance":"1000","totalAvailableBalance":"1000","totalInitialMargin":"0","totalMaintenanceMargin":"0","totalPerpUPL":"0","coin":[{"coin":"USDT","walletBalance":"1000","usdValue":"1000","equity":"1000","totalPositionIM":"0","totalOrderIM":"0","locked":"0"}]}]}}"#;

#[derive(Clone, Copy, Debug)]
struct Profile {
    variant: Variant,
    combined_inverse: bool,
}

const PROFILES: [Profile; 5] = [
    Profile {
        variant: Variant::Linear,
        combined_inverse: false,
    },
    Profile {
        variant: Variant::Demo,
        combined_inverse: false,
    },
    Profile {
        variant: Variant::Hyrotrader,
        combined_inverse: false,
    },
    Profile {
        variant: Variant::Inverse,
        combined_inverse: false,
    },
    Profile {
        variant: Variant::Linear,
        combined_inverse: true,
    },
];

impl Profile {
    fn inverse(self) -> bool {
        self.variant == Variant::Inverse || self.combined_inverse
    }
    fn symbol(self) -> &'static str {
        if self.inverse() { "BTCUSD" } else { "BTCUSDT" }
    }
    fn category(self) -> &'static str {
        if self.inverse() { "inverse" } else { "linear" }
    }
    fn cancel_path(self) -> &'static str {
        if self.variant.is_hyro() {
            "/v5/order/cancel"
        } else {
            "/v5/order/cancel-batch"
        }
    }
}

fn ready(profile: Profile) -> Harness {
    let mut h = Harness::started(Options {
        variant: profile.variant,
        enable_inverse_markets: profile.combined_inverse,
        key: Some("key".into()),
        secret: Some("secret".into()),
        endpoints: profile.variant.is_hyro().then(|| Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    });
    if h.has_request("/v5/account/info") {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    while h.has_request("/v5/account/fee-rate") {
        h.respond("/v5/account/fee-rate", 200, EMPTY);
    }
    while h.has_request("status=PreLaunch") {
        h.respond("status=PreLaunch", 200, EMPTY);
    }
    for category in if profile.combined_inverse {
        vec!["linear", "inverse"]
    } else {
        vec![profile.category()]
    } {
        let inverse = category == "inverse";
        let instruments = if inverse {
            support::INSTRUMENTS
                .replace("BTCUSDT", "BTCUSD")
                .replace("ETHUSDT", "ETHUSD")
                .replace("LinearPerpetual", "InversePerpetual")
                .replace("\"USDT\"", "\"USD\"")
                .replace("1190.000", "10")
                .replace("0.001", "1")
        } else {
            support::INSTRUMENTS.to_owned()
        };
        h.respond(
            &format!("/v5/market/instruments-info?category={category}"),
            200,
            &instruments,
        );
        let tickers = if inverse {
            support::TICKERS
                .replace("BTCUSDT", "BTCUSD")
                .replace("ETHUSDT", "ETHUSD")
        } else {
            support::TICKERS.to_owned()
        };
        h.respond(
            &format!("/v5/market/tickers?category={category}"),
            200,
            &tickers,
        );
    }
    if profile.variant.is_hyro() {
        let socket = h.socket("/v5/private").unwrap();
        h.event(Event::WsOpened { socket });
        for op in ["auth", "subscribe"] {
            #[derive(serde::Deserialize)]
            struct Sent {
                req_id: String,
            }
            let message = h
                .sent_on("/v5/private")
                .into_iter()
                .rev()
                .find(|message| message.contains(&format!("\"op\":\"{op}\"")))
                .unwrap();
            let sent: Sent = serde_json::from_str(&message).unwrap();
            h.message(
                "/v5/private",
                &format!(
                    r#"{{"op":"{op}","success":true,"req_id":"{}"}}"#,
                    sent.req_id
                ),
            );
        }
    } else {
        // Managed sockets deliver nothing on an unopened socket; Bybit's own auth ACK then
        // marks the private session live, as in a real session.
        let socket = h.socket("/v5/private").unwrap();
        h.event(Event::WsOpened { socket });
        h.message(
            "/v5/private",
            r#"{"success":true,"ret_msg":"","op":"auth","conn_id":"cejreaspqfh3sjdnldmg-p"}"#,
        );
    }
    while h.has_request("/v5/order/realtime") {
        h.respond("/v5/order/realtime", 200, EMPTY);
    }
    while h.has_request("/v5/position/list") {
        h.respond("/v5/position/list", 200, EMPTY);
    }
    if h.has_request("/v5/account/wallet-balance") {
        h.respond("/v5/account/wallet-balance", 200, WALLET);
    }
    let mut tickers = h.engine.store().tickers().to_vec();
    for ticker in &mut tickers {
        ticker.mark = 100.0;
        ticker.last = 100.0;
        ticker.index = 100.0;
    }
    h.engine.store_mut().set_tickers(tickers);
    h.requests.clear();
    h
}

fn body(request: &HttpRequest) -> &str {
    request.body.as_deref().unwrap()
}

fn seed(
    h: &mut Harness,
    profile: Profile,
    side: &str,
    kind: &str,
    trigger: &str,
    hedge: bool,
    reduce: bool,
) {
    let symbol = profile.symbol();
    let category = profile.category();
    let hedge = hedge && !profile.variant.is_hyro();
    let protected_side = if side == "Sell" { "Buy" } else { "Sell" };
    let idx = if hedge {
        if side == "Sell" { 1 } else { 2 }
    } else {
        0
    };
    let price = if (side == "Sell") == (kind == "StopLoss") {
        "90"
    } else {
        "110"
    };
    h.message("/v5/private", &format!(r#"{{"topic":"position","data":[{{"category":"{category}","symbol":"{symbol}","side":"{protected_side}","size":"20","avgPrice":"100","markPrice":"100","leverage":"10","unrealisedPnl":"0","positionIdx":{idx}}}]}}"#));
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"{category}","symbol":"{symbol}","orderId":"partial","side":"{side}","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"{kind}","tpslMode":"Partial","qty":"7","cumExecQty":"0","cumExecValue":"0","avgPrice":"0","price":"0","leavesQty":"7","triggerPrice":"{price}","triggerBy":"{trigger}","reduceOnly":{reduce},"closeOnTrigger":true,"positionIdx":{}}}]}}"#, idx));
    assert!(
        h.engine.store().order("partial").is_some(),
        "{profile:?}: {:?}",
        h.logs
    );
}

fn amend(h: &mut Harness, price: Option<f64>, amount: Option<f64>) {
    h.command(Command::AmendOrder {
        command: 91,
        id: "partial".into(),
        price,
        amount,
    });
}

fn accept_cancel(h: &mut Harness, profile: Profile) {
    let original = h.engine.store().order("partial").unwrap().clone();
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
    confirm_partial_reads(h, profile, &original);
}

fn confirm_partial_reads(
    h: &mut Harness,
    profile: Profile,
    original: &safe_cex_engine::model::Order,
) {
    confirm_partial_reads_with(h, profile, original, |_| Vec::new());
}

fn confirm_partial_reads_with(
    h: &mut Harness,
    profile: Profile,
    original: &safe_cex_engine::model::Order,
    mutate: impl FnOnce(&mut serde_json::Value) -> Vec<serde_json::Value>,
) {
    let mut target = native_partial_target(h, profile, original);
    let peers = mutate(&mut target);
    for (path, rows) in [
        ("&orderId=partial", vec![target]),
        ("&openOnly=0&limit=50", peers),
        ("/v5/order/history", vec![]),
    ] {
        h.respond(path, 200, &serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":rows,"nextPageCursor":""}}).to_string());
    }
}

fn native_partial_target(
    h: &Harness,
    profile: Profile,
    original: &safe_cex_engine::model::Order,
) -> serde_json::Value {
    let by = match original.trigger_type.unwrap() {
        safe_cex_engine::model::StopTriggerType::LastPrice => "LastPrice",
        safe_cex_engine::model::StopTriggerType::IndexPrice => "IndexPrice",
        safe_cex_engine::model::StopTriggerType::MarkPrice => "MarkPrice",
    };
    let limit = original
        .extra
        .as_ref()
        .and_then(|e| e.conditional_order_price);
    let idx = if h.engine.store().hedged() && !profile.variant.is_hyro() {
        if original.side == safe_cex_engine::model::OrderSide::Sell {
            1
        } else {
            2
        }
    } else {
        0
    };
    serde_json::json!({
        "orderId":original.id,"symbol":original.symbol,
        "side":if original.side == safe_cex_engine::model::OrderSide::Sell {"Sell"} else {"Buy"},
        "orderStatus":"Cancelled","qty":original.amount.to_string(),"cumExecQty":"0",
        "triggerPrice":original.price.to_string(),"triggerBy":by,
        "orderType":if limit.is_some() {"Limit"} else {"Market"},"price":limit.unwrap_or(0.0).to_string(),
        "positionIdx":idx,"stopOrderType":if original.kind == OrderType::StopLoss {"StopLoss"} else {"TakeProfit"},
        "tpslMode":"Partial","parentOrderLinkId":"","orderLinkId":"standalone-link","createdTime":(h.now as u64).to_string()
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopWrite {
    category: String,
    symbol: String,
    position_idx: i64,
    tpsl_mode: Option<String>,
    stop_loss: Option<String>,
    take_profit: Option<String>,
    sl_size: Option<String>,
    tp_size: Option<String>,
    sl_trigger_by: Option<String>,
    tp_trigger_by: Option<String>,
}

fn replacement(h: &mut Harness, profile: Profile) -> StopWrite {
    accept_cancel(h, profile);
    let request = h.take("/v5/position/trading-stop");
    serde_json::from_str(body(&request)).unwrap()
}

#[test]
fn partial_drag_cancels_before_replacing_and_projects_raw_price_without_changing_side() {
    for profile in PROFILES {
        for (side, kind, price, field, by) in [
            ("Sell", "StopLoss", 100.049, "takeProfit", "tpTriggerBy"),
            ("Sell", "TakeProfit", 99.95, "stopLoss", "slTriggerBy"),
            ("Buy", "StopLoss", 99.95, "takeProfit", "tpTriggerBy"),
            ("Buy", "TakeProfit", 100.049, "stopLoss", "slTriggerBy"),
        ] {
            let mut h = ready(profile);
            seed(&mut h, profile, side, kind, "IndexPrice", false, true);
            amend(&mut h, Some(price), None);
            let cancel = h.take(profile.cancel_path());
            assert!(body(&cancel).contains(&format!(r#""category":"{}""#, profile.category())));
            assert!(!h.has_request("/v5/position/trading-stop"));
            assert!(!h.has_request("/v5/order/amend"));
            let original = h.engine.store().order("partial").unwrap().clone();
            h.reply(cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
            confirm_partial_reads(&mut h, profile, &original);
            let replacement = h.take("/v5/position/trading-stop");
            let size = if field == "stopLoss" {
                "slSize"
            } else {
                "tpSize"
            };
            let reference = if field == "stopLoss" {
                "MarkPrice"
            } else {
                "LastPrice"
            };
            assert_eq!(
                body(&replacement),
                format!(
                    r#"{{"category":"{}","symbol":"{}","positionIdx":0,"tpslMode":"Partial","{field}":"{price}","{by}":"{reference}","{size}":"7"}}"#,
                    profile.category(),
                    profile.symbol()
                )
            );
            h.reply(replacement.id, 200, OK);
            assert_eq!(h.result(91), Some(&Ok("[]".into())));
        }
    }
}

#[test]
fn a_refused_replacement_names_the_lost_stop() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "IndexPrice",
        false,
        true,
    );
    amend(&mut h, Some(95.0), None);
    let cancel = h.take(profile.cancel_path());
    let original = h.engine.store().order("partial").unwrap().clone();
    h.reply(cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    confirm_partial_reads(&mut h, profile, &original);
    let replacement = h.take("/v5/position/trading-stop");
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10001,"retMsg":"StopLoss would trigger immediately","result":{}}"#,
    );
    // Putting the old stop back is refused too.
    let restore = h.take("/v5/position/trading-stop");
    h.reply(
        restore.id,
        200,
        r#"{"retCode":10001,"retMsg":"StopLoss would trigger immediately","result":{}}"#,
    );
    let error = h.result(91).unwrap().as_ref().unwrap_err();
    assert!(
        error.contains(profile.symbol()) && error.contains("long position has no stop"),
        "{error}"
    );
}

#[test]
fn partial_same_kind_keeps_supported_trigger_and_uses_destination_defaults_otherwise() {
    for profile in PROFILES {
        for kind in ["StopLoss", "TakeProfit"] {
            for trigger in ["LastPrice", "MarkPrice", "IndexPrice", "UNKNOWN"] {
                let mut h = ready(profile);
                seed(&mut h, profile, "Sell", kind, trigger, false, true);
                amend(
                    &mut h,
                    Some(if kind == "StopLoss" { 90.0 } else { 110.0 }),
                    None,
                );
                if h.engine
                    .store()
                    .order("partial")
                    .unwrap()
                    .trigger_type
                    .is_none()
                {
                    assert!(!h.has_request(profile.cancel_path()));
                    assert!(h.result(91).is_some_and(|r| r.is_err()));
                    continue;
                }
                let request = replacement(&mut h, profile);
                assert_eq!(request.category, profile.category());
                assert_eq!(request.symbol, profile.symbol());
                if kind == "StopLoss" {
                    assert_eq!(
                        request.sl_trigger_by.as_deref(),
                        Some(match trigger {
                            "LastPrice" => "LastPrice",
                            "IndexPrice" => "IndexPrice",
                            _ => "MarkPrice",
                        })
                    );
                    assert!(request.tp_trigger_by.is_none());
                } else {
                    assert_eq!(
                        request.tp_trigger_by.as_deref(),
                        Some(match trigger {
                            "MarkPrice" => "MarkPrice",
                            "IndexPrice" => "IndexPrice",
                            _ => "LastPrice",
                        })
                    );
                    assert!(request.sl_trigger_by.is_none());
                }
            }
        }
    }
}

#[test]
fn partial_replacement_closes_the_original_side_in_hedge_mode_even_without_reduce_only() {
    for profile in PROFILES {
        for side in ["Sell", "Buy"] {
            for reduce in [false, true] {
                let mut h = ready(profile);
                seed(&mut h, profile, side, "StopLoss", "MarkPrice", true, reduce);
                amend(&mut h, Some(90.0), None);
                let request = replacement(&mut h, profile);
                assert_eq!(
                    request.position_idx,
                    if profile.variant.is_hyro() {
                        0
                    } else if side == "Sell" {
                        1
                    } else {
                        2
                    },
                    "{profile:?}/{side}/{reduce}"
                );
            }
        }
    }
}

#[test]
fn partial_replacement_uses_total_native_amount_and_zero_converts_to_full() {
    for profile in PROFILES {
        for amount in [None, Some(9.234), Some(0.0)] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            if profile.inverse() {
                assert!(h.engine.store().order("partial").unwrap().remaining < 1.0);
            }
            amend(&mut h, Some(90.0), amount);
            let request = replacement(&mut h, profile);
            let expected = match amount {
                None => Some("7"),
                Some(9.234) => Some(if profile.inverse() { "9" } else { "9.234" }),
                _ => None,
            };
            assert_eq!(
                request.tpsl_mode.as_deref(),
                Some(if expected.is_some() {
                    "Partial"
                } else {
                    "Full"
                })
            );
            assert_eq!(request.sl_size.as_deref(), expected);
            assert!(request.tp_size.is_none());
        }
    }
}

#[test]
fn a_replacement_quantity_rounding_to_zero_is_refused_not_turned_into_full() {
    for profile in PROFILES {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        amend(
            &mut h,
            Some(90.0),
            Some(if profile.inverse() { 0.4 } else { 0.0004 }),
        );
        assert!(h.result(91).unwrap().is_err(), "{profile:?}");
        assert!(!h.has_request("/v5/position/trading-stop"), "{profile:?}");
        assert!(!h.has_request(profile.cancel_path()), "{profile:?}");
    }
}

#[test]
fn partial_amount_only_replaces_shared_family_but_inverse_refuses() {
    for profile in PROFILES {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "LastPrice",
            false,
            true,
        );
        amend(&mut h, None, Some(9.234));
        // Live: inverse sent a body with no fields and got 10001; nothing is sent now.
        if profile.variant == Variant::Inverse {
            assert!(!h.has_request("/v5/position/trading-stop"));
            assert!(h.result(91).unwrap().is_err());
            continue;
        }
        let request = replacement(&mut h, profile);
        assert_eq!(request.stop_loss.as_deref(), Some("90"));
        assert_eq!(
            request.sl_size.as_deref(),
            Some(if profile.inverse() { "9" } else { "9.234" })
        );
        assert_eq!(request.sl_trigger_by.as_deref(), Some("LastPrice"));
    }
}

#[test]
fn cancel_rejection_or_unknown_transport_outcome_never_recreates_a_partial_stop() {
    for profile in PROFILES {
        for outcome in ["top", "row", "transport", "unknown"] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            amend(&mut h, Some(110.0), None);
            let cancel = h.take(profile.cancel_path());
            match outcome {
                "transport" => h.event(Event::HttpFailed { request: cancel.id, error: "lost cancellation outcome" }),
                "top" => h.reply(cancel.id, 200, r#"{"retCode":10001,"retMsg":"cancel refused"}"#),
                "unknown" => h.reply(cancel.id, 200, r#"{"retCode":10000,"retMsg":"unknown cancellation outcome"}"#),
                _ => h.reply(cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{"code":10001,"msg":"cancel refused"}]}}"#),
            }
            assert!(h.result(91).unwrap().is_err(), "{profile:?}/{outcome}");
            assert!(!h.has_request("/v5/position/trading-stop"));
            assert!(!h.has_request("/v5/order/create"));
        }
    }
}

#[test]
fn partial_replacement_requires_complete_native_cancellation_acknowledgement() {
    for profile in PROFILES {
        let replies = if profile.variant.is_hyro() {
            vec![
                r#"{"retCode":0,"retMsg":"OK","result":{}}"#,
                r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"other"}}"#,
                r#"{"retCode":0,"retMsg":"OK","result":{"orderId":""}}"#,
                r#"{"retCode":0,"retMsg":"OK","result":{"orderId":123}}"#,
            ]
        } else {
            vec![
                r#"{"retCode":0,"retMsg":"OK"}"#,
                r#"{"retCode":0,"retMsg":"OK","retExtInfo":{}}"#,
                r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[]}}"#,
                r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{}]}}"#,
                r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{"code":0},{"code":0}]}}"#,
            ]
        };
        for reply in replies {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            amend(&mut h, Some(110.0), None);
            h.respond(profile.cancel_path(), 200, reply);
            assert!(h.result(91).unwrap().is_err(), "{profile:?}/{reply}");
            assert!(!h.has_request("/v5/position/trading-stop"));
            assert_eq!(
                h.engine.store().order("partial").unwrap().status,
                OrderStatus::Open
            );
        }
    }
}

#[test]
fn documented_already_gone_batch_status_still_allows_partial_replacement() {
    for profile in PROFILES
        .into_iter()
        .filter(|profile| !profile.variant.is_hyro())
    {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        amend(&mut h, Some(110.0), None);
        let original = h.engine.store().order("partial").unwrap().clone();
        h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{"code":110001,"msg":"order not exists or too late to cancel"}]}}"#);
        confirm_partial_reads(&mut h, profile, &original);
        assert!(h.has_request("/v5/position/trading-stop"));
    }
}

#[test]
fn managed_partial_cancel_unresolved_code_cannot_borrow_benign_text_or_start_replacement() {
    for profile in PROFILES
        .into_iter()
        .filter(|profile| !profile.variant.is_hyro())
    {
        for body in [
            r#"{"retCode":10016,"retMsg":"order not exists or too late to cancel"}"#,
            r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{"code":10016,"msg":"order not exists or too late to cancel"}]}}"#,
        ] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            let before = h.engine.store().order("partial").unwrap().clone();
            amend(&mut h, Some(110.0), None);
            h.respond(profile.cancel_path(), 200, body);
            assert!(
                !h.has_request("/v5/position/trading-stop"),
                "{profile:?} {body}"
            );
            assert!(!h.has_request("/v5/order/create"));
            assert_eq!(h.engine.store().order("partial"), Some(&before));
            assert!(h.result(91).unwrap().is_err());
            assert!(h.unknown.contains(&91));
        }
    }
}

#[test]
fn managed_partial_replacement_unresolved_noop_text_does_not_schedule_success_refresh() {
    for profile in PROFILES
        .into_iter()
        .filter(|profile| !profile.variant.is_hyro())
    {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        amend(&mut h, Some(110.0), None);
        accept_cancel(&mut h, profile);
        h.respond(
            "/v5/position/trading-stop",
            200,
            r#"{"retCode":10016,"retMsg":"order not modified"}"#,
        );
        assert!(h.result(91).unwrap().is_err());
        assert!(h.unknown.contains(&91));
        // The Unknown seal reconciles once; that read is not a success refresh.
        h.reconcile_once(EMPTY);
        h.advance(250.0);
        for path in [
            "/v5/position/trading-stop",
            "/v5/order/create",
            "/v5/order/realtime",
            "/v5/position/list",
            "/v5/account/wallet-balance",
        ] {
            assert!(!h.has_request(path), "{profile:?} {path}");
        }
    }
}

#[test]
fn partial_invalid_updates_and_terminal_rows_fail_before_any_write() {
    for (price, amount, terminal) in [
        (Some(f64::NAN), None, false),
        (Some(0.0), None, false),
        (Some(-1.0), None, false),
        (Some(f64::INFINITY), None, false),
        (Some(110.0), Some(-1.0), false),
        (Some(110.0), Some(f64::NAN), false),
        (Some(110.0), Some(f64::INFINITY), false),
        (Some(110.0), None, true),
    ] {
        let profile = PROFILES[0];
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        if terminal {
            h.engine
                .store_mut()
                .set_order_status("partial", OrderStatus::Closed);
        }
        amend(&mut h, price, amount);
        assert!(h.result(91).unwrap().is_err());
        assert!(h.requests.is_empty());
    }
}

#[test]
fn missing_mark_refuses_price_size_and_full_conversion_edits() {
    for profile in PROFILES {
        for (price, amount) in [
            (Some(100.049), None),
            (None, Some(3.234)),
            (Some(100.049), Some(0.0)),
        ] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            let mut tickers = h.engine.store().tickers().to_vec();
            for ticker in &mut tickers {
                ticker.mark = 0.0;
            }
            h.engine.store_mut().set_tickers(tickers);
            amend(&mut h, price, amount);
            if profile.variant == Variant::Inverse && price.is_none() {
                assert!(
                    h.result(91)
                        .unwrap()
                        .as_ref()
                        .unwrap_err()
                        .contains("cannot be edited")
                );
                assert!(h.requests.is_empty());
            } else {
                assert_guard_refused(&h, "fresh mark");
            }
        }
    }
}

#[test]
fn accepted_partial_cancel_filters_replica_stale_identity_but_refresh_discovers_new_stop() {
    for profile in [PROFILES[1], PROFILES[2]] {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        amend(&mut h, Some(110.0), None);
        accept_cancel(&mut h, profile);
        h.respond("/v5/position/trading-stop", 200, OK);
        assert!(h.engine.store().order("partial").is_none());
        h.advance(250.0);
        let old = r#"{"category":"linear","symbol":"BTCUSDT","orderId":"partial","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"7","price":"90","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}"#;
        let new = old
            .replace("partial", "replacement")
            .replace("StopLoss", "TakeProfit")
            .replace("90", "110");
        h.respond("/v5/order/realtime", 200, &format!(r#"{{"retCode":0,"retMsg":"OK","result":{{"list":[{old},{new}],"nextPageCursor":""}}}}"#));
        if profile.variant.is_hyro() {
            h.respond("/v5/position/list", 200, EMPTY);
            h.respond("/v5/account/wallet-balance", 200, WALLET);
        } else {
            h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
        }
        assert!(h.engine.store().order("partial").is_none());
        let row = h.engine.store().order("replacement").unwrap();
        assert_eq!(row.status, OrderStatus::Open);
        assert_eq!(row.kind, OrderType::TakeProfit);
        assert_eq!(row.amount, 7.0);
    }
}

#[test]
fn linear_partial_refresh_cannot_reopen_the_canceled_native_identity() {
    partial_refresh_cannot_reopen_the_canceled_native_identity(PROFILES[0]);
}

#[test]
fn combined_inverse_partial_refresh_cannot_reopen_the_canceled_native_identity() {
    partial_refresh_cannot_reopen_the_canceled_native_identity(PROFILES[4]);
}

fn partial_refresh_cannot_reopen_the_canceled_native_identity(profile: Profile) {
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(110.0), None);
    accept_cancel(&mut h, profile);
    h.respond("/v5/position/trading-stop", 200, OK);
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Canceled
    );
    h.advance(250.0);
    let old = format!(
        r#"{{"category":"{}","symbol":"{}","orderId":"partial","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"7","price":"90","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}}"#,
        profile.category(),
        profile.symbol()
    );
    let new = old
        .replace("partial", "replacement")
        .replace("StopLoss", "TakeProfit")
        .replace("90", "110");
    // The venue has the replacement, but its read replica still includes the old stop.
    // No private terminal event arrives to repair a healthy-but-degraded socket.
    h.respond(
        "/v5/order/realtime",
        200,
        &format!(
            r#"{{"retCode":0,"retMsg":"OK","result":{{"list":[{old},{new}],"nextPageCursor":""}}}}"#
        ),
    );
    if !profile.inverse() {
        h.respond("settleCoin=USDC&limit=50", 200, EMPTY);
    }
    let open: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .filter(|order| order.symbol == profile.symbol() && order.status == OrderStatus::Open)
        .map(|order| order.id.as_str())
        .collect();
    assert_eq!(
        open,
        vec!["replacement"],
        "{profile:?}: the refresh must show only the genuine replacement, never the canceled chart row"
    );
}

#[test]
fn canceled_partial_stop_ignores_delayed_private_open_rows_but_accepts_late_terminal_updates() {
    for profile in PROFILES {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        amend(&mut h, Some(110.0), None);
        accept_cancel(&mut h, profile);
        h.respond("/v5/position/trading-stop", 200, OK);
        let old = format!(
            r#"{{"category":"{}","symbol":"{}","orderId":"partial","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"7","cumExecQty":"2","cumExecValue":"180","avgPrice":"90","price":"90","leavesQty":"5","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}}"#,
            profile.category(),
            profile.symbol()
        );
        let new = old
            .replace("partial", "replacement")
            .replace("StopLoss", "TakeProfit")
            .replace("\"price\":\"90\"", "\"price\":\"110\"")
            .replace("\"triggerPrice\":\"90\"", "\"triggerPrice\":\"110\"")
            .replace("\"cumExecQty\":\"2\"", "\"cumExecQty\":\"0\"")
            .replace("\"cumExecValue\":\"180\"", "\"cumExecValue\":\"0\"")
            .replace("\"leavesQty\":\"5\"", "\"leavesQty\":\"7\"");
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{new}]}}"#),
        );
        // The older open event was in flight independently of the REST cancel ACK.
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{old}]}}"#),
        );
        let open: Vec<_> = h
            .engine
            .store()
            .orders()
            .iter()
            .filter(|order| order.symbol == profile.symbol() && order.status == OrderStatus::Open)
            .map(|order| order.id.as_str())
            .collect();
        assert_eq!(
            open,
            vec!["replacement"],
            "{profile:?}: delayed private state must not reopen the canceled chart row"
        );
        // A cancellation can race an execution; its final economics must still arrive.
        let filled = old
            .replace("Untriggered", "Filled")
            .replace("\"cumExecQty\":\"2\"", "\"cumExecQty\":\"7\"")
            .replace("\"cumExecValue\":\"180\"", "\"cumExecValue\":\"630\"")
            .replace("\"leavesQty\":\"5\"", "\"leavesQty\":\"0\"");
        h.message(
            "/v5/private",
            &format!(r#"{{"topic":"order","data":[{filled}]}}"#),
        );
        let terminal = h.engine.store().order("partial").unwrap();
        assert_eq!(terminal.status, OrderStatus::Closed, "{profile:?}");
        assert_eq!(terminal.filled, 7.0, "{profile:?}");
        assert_eq!(
            h.engine.store().order("replacement").unwrap().status,
            OrderStatus::Open,
            "{profile:?}"
        );
    }
}

#[test]
fn hyro_partial_refresh_reuses_managed_snapshot_recovery_instead_of_starting_an_unjoined_read() {
    let profile = PROFILES[2];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(110.0), None);
    // A managed poll overlaps both writes and must finish/retry as one snapshot.
    h.advance(5000.0);
    let orders = h.take("/v5/order/realtime");
    let positions = h.take("/v5/position/list");
    let wallet = h.take("/v5/account/wallet-balance");
    accept_cancel(&mut h, profile);
    h.respond("/v5/position/trading-stop", 200, OK);
    h.advance(250.0);
    assert!(!h.has_request("/v5/order/realtime"));
    h.reply(orders.id, 200, EMPTY);
    h.reply(positions.id, 200, EMPTY);
    h.reply(wallet.id, 200, WALLET);
    assert!(h.has_request("/v5/order/realtime"));
    assert!(h.has_request("/v5/position/list"));
    assert!(h.has_request("/v5/account/wallet-balance"));
    h.respond("/v5/order/realtime", 200, EMPTY);
    h.respond("/v5/position/list", 200, EMPTY);
    h.respond("/v5/account/wallet-balance", 200, WALLET);
    assert!(h.engine.store().order("partial").is_none());
}

#[test]
fn already_gone_cancel_allows_replacement_but_replacement_failure_never_restores_old_identity() {
    for profile in PROFILES {
        for outcome in ["rejected", "transport", "not-modified", "accepted"] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            amend(&mut h, Some(110.0), None);
            let original = h.engine.store().order("partial").unwrap().clone();
            h.respond(
                profile.cancel_path(),
                200,
                r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
            );
            confirm_partial_reads(&mut h, profile, &original);
            let replacement = h.take("/v5/position/trading-stop");
            match outcome {
                "transport" => h.event(Event::HttpFailed {
                    request: replacement.id,
                    error: "lost replacement outcome",
                }),
                "rejected" => h.reply(
                    replacement.id,
                    200,
                    r#"{"retCode":10001,"retMsg":"replacement refused"}"#,
                ),
                "not-modified" => h.reply(
                    replacement.id,
                    200,
                    r#"{"retCode":34040,"retMsg":"order not modified"}"#,
                ),
                _ => h.reply(replacement.id, 200, OK),
            }
            assert_eq!(h.result(91).unwrap().is_ok(), outcome == "accepted");
            // D2: the absent old leg is retired by the reconcile read, not by
            // the answer; a replacement failure never restores it. Hyro's
            // reconcile read is its full atomic snapshot.
            assert!(h.has_request("/v5/order/realtime"), "{profile:?}/{outcome}");
            while h.has_request("/v5/order/realtime") {
                h.respond("/v5/order/realtime", 200, EMPTY);
                if profile.variant.is_hyro() {
                    h.respond("/v5/position/list", 200, EMPTY);
                    h.respond("/v5/account/wallet-balance", 200, WALLET);
                }
            }
            assert!(
                h.engine
                    .store()
                    .order("partial")
                    .is_none_or(|row| row.status == OrderStatus::Canceled)
            );
            assert!(!h.has_request("/v5/position/trading-stop"));
            h.advance(249.0);
            assert!(!h.has_request("/v5/order/realtime"));
            h.advance(1.0);
            let refreshed = outcome == "accepted" && profile.variant != Variant::Inverse;
            assert_eq!(
                h.has_request("/v5/order/realtime"),
                refreshed,
                "{profile:?}/{outcome}"
            );
            if profile.variant.is_hyro() && refreshed {
                assert!(h.has_request("/v5/position/list"));
                assert!(h.has_request("/v5/account/wallet-balance"));
            }
        }
    }
}

#[test]
fn free_conditional_virtual_full_trailing_and_limit_paths_do_not_cancel_recreate() {
    let profile = PROFILES[0];
    for (kind, bound, amount, id, path, field) in [
        (
            OrderType::StopLoss,
            false,
            7.0,
            "partial",
            "/v5/order/amend",
            "triggerPrice",
        ),
        (
            OrderType::Conditional,
            false,
            7.0,
            "partial",
            "/v5/order/amend",
            "triggerPrice",
        ),
        (
            OrderType::StopLoss,
            true,
            0.0,
            "partial",
            "/v5/position/trading-stop",
            "stopLoss",
        ),
        (
            OrderType::StopLoss,
            true,
            7.0,
            "parent__stop_loss",
            "/v5/order/amend",
            "stopLoss",
        ),
        (
            OrderType::TrailingStopLoss,
            true,
            7.0,
            "partial",
            "/v5/position/trading-stop",
            "trailingStop",
        ),
        (
            OrderType::Limit,
            false,
            7.0,
            "partial",
            "/v5/order/amend",
            "price",
        ),
    ] {
        let mut h = ready(profile);
        if bound {
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
        } else {
            h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"0","positionIdx":0}]}"#);
            h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"partial","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","qty":"7","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true}]}"#);
        }
        let mut row = h.engine.store().order("partial").unwrap().clone();
        row.id = id.into();
        row.kind = kind;
        row.amount = amount;
        if id != "partial" {
            row.parent_id = Some("parent".into());
        }
        h.engine.store_mut().upsert_order(row);
        // A sell stop below the price, so the trigger check passes and only routing differs.
        h.command(Command::AmendOrder {
            command: 91,
            id: id.into(),
            price: Some(95.0),
            amount: None,
        });
        assert!(!h.has_request(profile.cancel_path()));
        let request = h.take(path);
        assert!(
            body(&request).contains(&format!(r#""{field}":""#)),
            "{}",
            body(&request)
        );
    }
}

/// ADAUSD: a stop amended to 1880, a price from another symbol, is refused before its cancel.
#[test]
fn a_partial_stop_moved_to_another_symbols_price_is_refused_before_its_cancel() {
    for profile in PROFILES {
        let mut h = ready(profile);
        seed(&mut h, profile, "Buy", "StopLoss", "MarkPrice", false, true);
        amend(&mut h, Some(1880.0), None);
        assert!(!h.has_request(profile.cancel_path()), "{profile:?}");
        assert!(!h.has_request("/v5/position/trading-stop"));
        let error = h.result(91).unwrap().as_ref().unwrap_err();
        assert!(error.contains("another symbol"), "{error}");
    }
}

/// Each kind is judged against the price that fires it: a buy between last and mark
/// is neither a stop (above mark) nor a take profit (below last), so nothing is canceled.
#[test]
fn a_partial_drag_that_stands_as_neither_kind_is_refused_before_its_cancel() {
    for profile in PROFILES {
        let mut h = ready(profile);
        h.engine
            .store_mut()
            .update_ticker(profile.symbol(), |ticker| ticker.last = 98.0);
        seed(&mut h, profile, "Buy", "StopLoss", "MarkPrice", false, true);
        amend(&mut h, Some(99.0), None);
        assert!(!h.has_request(profile.cancel_path()), "{profile:?}");
        let error = h.result(91).unwrap().as_ref().unwrap_err();
        assert!(error.contains("would trigger at once"), "{error}");
    }
}

#[test]
fn a_refused_partial_replacement_puts_the_exact_old_stop_back() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(95.0), None);
    let replaced = h.take(profile.cancel_path());
    let original = h.engine.store().order("partial").unwrap().clone();
    h.reply(replaced.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    confirm_partial_reads(&mut h, profile, &original);
    let replacement = h.take("/v5/position/trading-stop");
    assert!(body(&replacement).contains(r#""stopLoss":"95""#));
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10001,"retMsg":"StopLoss would trigger immediately","result":{}}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    assert_eq!(
        body(&restore),
        r#"{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Partial","stopLoss":"90","slTriggerBy":"MarkPrice","slSize":"7"}"#
    );
    h.reply(restore.id, 200, OK);
    let error = h.result(91).unwrap().as_ref().unwrap_err();
    assert!(error.contains("put back"), "{error}");
    assert!(!h.has_request("/v5/position/trading-stop"), "never loops");
    assert_applied_restored_partial_failure(&h);
}

/// codex-mxb4mq's P1: the cancel finds the old stop already gone (110001); the
/// replacement receives a timestamp refusal (10002). The replacement is not
/// replayed, and the old stop is never recreated: it may have filled or triggered.
#[test]
fn a_stop_the_cancel_found_gone_is_never_recreated_or_resent() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(95.0), None);
    let original = h.engine.store().order("partial").unwrap().clone();
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
    );
    confirm_partial_reads(&mut h, profile, &original);
    let replacement = h.take("/v5/position/trading-stop");
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param","result":{},"time":1760352369814}"#,
    );
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "never recreates a gone stop"
    );
    let error = h.result(91).unwrap().as_ref().unwrap_err();
    assert!(!error.contains("put back"), "{error}");
}

// codex-mxb4mq's #3018 reproducers (review_3018_*), kept as regression tests.

fn review_3018_standalone_mark_stop() -> Harness {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let mut row = h.engine.store().order("partial").unwrap().clone();
    row.id = "standalone".into();
    row.parent_id = None;
    h.engine.store_mut().upsert_order(row);
    let socket = h.socket("/v5/public/linear").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/public/linear",r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","markPrice":"100","lastPrice":"100","indexPrice":"100"}}"#);
    h
}
#[test]
fn review_3018_same_price_observation_keeps_guard_fresh() {
    let mut h = review_3018_standalone_mark_stop();
    h.now += 10_000.;
    h.message("/v5/public/linear",r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","markPrice":"100","lastPrice":"100","indexPrice":"100"}}"#);
    h.command(Command::AmendOrder {
        command: 93,
        id: "standalone".into(),
        price: Some(110.),
        amount: None,
    });
    assert!(
        !h.has_request("/v5/order/amend"),
        "unchanged but freshly observed mark should refuse crossed stop"
    );
    assert!(h.result(93).is_some_and(|r| r.is_err()));
}
#[test]
fn review_3018_stale_reference_working_control() {
    let mut h = review_3018_standalone_mark_stop();
    h.now += 10_000.;
    h.command(Command::AmendOrder {
        command: 94,
        id: "standalone".into(),
        price: Some(110.),
        amount: None,
    });
    assert!(
        h.has_request("/v5/order/amend"),
        "stale reference should admit"
    );
}

#[test]
fn review_3018_gone_cancel_does_not_replay_a_refused_replacement() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(110.0), None);
    let original = h.engine.store().order("partial").unwrap().clone();
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
    );
    confirm_partial_reads(&mut h, profile, &original);
    let replacement = h.take("/v5/position/trading-stop");
    h.reply(replacement.id,200,r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param","result":{}}"#);
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "an already gone stop must not be recreated after a replacement signature refusal; requests: {:?}",
        h.requests
            .iter()
            .filter(|r| r.url.contains("trading-stop"))
            .map(|r| r.body.as_deref())
            .collect::<Vec<_>>()
    );
    assert!(matches!(h.result(91), Some(Err(_))));
}
#[test]
fn pinfix_exact_partial_restore_working_control() {
    // TS cancel/replace context: reference/bybit/bybit.exchange.ts:3221-3224.
    // Preserve master's exact old-stop restore per brief-pinfix-3342.md.
    a_refused_partial_replacement_puts_the_exact_old_stop_back();
}

#[test]
fn review_3018_limit_partial_restore_preserves_execution_type_and_price() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"partial","side":"Sell","orderType":"Limit","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"7","cumExecQty":"0","price":"89","leavesQty":"7","triggerPrice":"90","triggerBy":"MarkPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}]}"#);
    assert_eq!(h.engine.store().order("partial").unwrap().price, 90.0);
    amend(&mut h, Some(95.0), None);
    accept_cancel(&mut h, profile);
    let replacement = h.take("/v5/position/trading-stop");
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    assert!(
        body(&restore).contains(r#""slOrderType":"Limit""#)
            && body(&restore).contains(r#""slLimitPrice":"89""#),
        "restoring a native Limit Partial stop must preserve its execution type and limit price, got {}",
        body(&restore)
    );
}

// codex-mxb4mq #3018 recheck: a put-back of unknown outcome claims no protection.

fn review_recheck_restore_failure(code: i64) -> safe_cex_engine::VenueError {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(95.), None);
    accept_cancel(&mut h, profile);
    let replacement = h.take("/v5/position/trading-stop");
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    let payload = format!(r#"{{"retCode":{code},"retMsg":"put back failed","result":{{}}}}"#);
    h.reply(restore.id, 200, &payload);
    let errors = h
        .notices
        .iter()
        .filter_map(|a| match a {
            safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "{:?}", h.notices);
    assert!(!h.has_request("/v5/position/trading-stop"), "no loops");
    errors.into_iter().next().unwrap()
}
#[test]
fn review_recheck_unknown_restore_does_not_claim_proven_unprotected() {
    let error = review_recheck_restore_failure(10000);
    assert_eq!(
        error.protection, None,
        "an uncertain restoration may be live; {error:?}"
    );
}
#[test]
fn review_recheck_definitive_restore_refusal_is_unprotected_control() {
    let error = review_recheck_restore_failure(10001);
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Unprotected),
        "{error:?}"
    );
}

// DK2: a long's Partial LastPrice stop at 79124.9 exceeds last 79110.8, below mark.
// Fresh unsafe or stale evidence now refuses before cancel.
// A later move after valid cancellation still exercises exact old-stop restore.
fn dk2_long_partial_last_stop() -> (Harness, Profile) {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "LastPrice",
        false,
        true,
    );
    h.message("/v5/private", r#"{"topic":"order","data":[{"category":"linear","symbol":"BTCUSDT","orderId":"partial","side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"StopLoss","tpslMode":"Partial","qty":"7","cumExecQty":"0","price":"0","leavesQty":"7","triggerPrice":"78900","triggerBy":"LastPrice","reduceOnly":true,"closeOnTrigger":true,"positionIdx":0}]}"#);
    let socket = h.socket("/v5/public/linear").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/v5/public/linear", r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","markPrice":"79130","lastPrice":"79110.8","indexPrice":"79115.0"}}"#);
    (h, profile)
}

#[test]
fn dk2_stop_above_a_fresh_last_is_refused_before_its_cancel() {
    let (mut h, profile) = dk2_long_partial_last_stop();
    amend(&mut h, Some(79124.9), None);
    assert!(!h.has_request(profile.cancel_path()));
    assert!(!h.has_request("/v5/position/trading-stop"));
    let error = h.result(91).unwrap().as_ref().unwrap_err();
    assert!(error.contains("would trigger at once"), "{error}");
}

#[test]
fn dk2_venue_refusal_after_the_cancel_puts_the_exact_old_stop_back() {
    let (mut h, profile) = dk2_long_partial_last_stop();
    set_reference_prices(&mut h, 79130.0, 79130.0);
    amend(&mut h, Some(79124.9), None);
    accept_cancel(&mut h, profile);
    set_reference_prices(&mut h, 79130.0, 79110.8);
    let replacement = h.take("/v5/position/trading-stop");
    assert!(
        body(&replacement).contains(r#""stopLoss":"79124.9""#),
        "{}",
        body(&replacement)
    );
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10001,"retMsg":"PartialStopLoss:791249000 set for Buy position should lower than base_price:791108000 LastPrice","result":{}}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    assert_eq!(
        body(&restore),
        r#"{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Partial","stopLoss":"78900","slTriggerBy":"LastPrice","slSize":"7"}"#
    );
    h.reply(restore.id, 200, OK);
    let error = h.result(91).unwrap().as_ref().unwrap_err();
    assert!(error.contains("put back"), "{error}");
    assert!(!h.has_request("/v5/position/trading-stop"), "never loops");
}

// codex-mxb4mq #3018 DK2 review probes.
#[test]
fn review_dk2_concurrent_same_stop_amend_does_not_restore_over_the_newer_edit() {
    let (mut h, profile) = dk2_long_partial_last_stop();
    set_reference_prices(&mut h, 79130.0, 79130.0);
    amend(&mut h, Some(79124.9), None);
    let first_cancel = h.take(profile.cancel_path());
    h.command(Command::AmendOrder {
        command: 92,
        id: "partial".into(),
        price: Some(78000.),
        amount: None,
    });
    if !h.has_request(profile.cancel_path()) {
        assert!(matches!(h.result(92), Some(Err(_))));
        return;
    }
    let second_cancel = h.take(profile.cancel_path());
    let cancel_ok = r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#;
    h.reply(first_cancel.id, 200, cancel_ok);
    let older_replacement = h.take("/v5/position/trading-stop");
    h.reply(second_cancel.id, 200, cancel_ok);
    let newer_replacement = h.take("/v5/position/trading-stop");
    assert!(body(&newer_replacement).contains(r#""stopLoss":"78000""#));
    h.reply(newer_replacement.id, 200, OK);
    assert!(matches!(h.result(92), Some(Ok(_))), "newer edit completed");
    h.reply(older_replacement.id, 200, r#"{"retCode":10001,"retMsg":"PartialStopLoss should lower than base_price LastPrice","result":{}}"#);
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "older failed edit must not now create an extra 78900 stop alongside the successful newer 78000 stop; actual {:?}",
        h.requests.iter().map(body).collect::<Vec<_>>()
    );
}

#[test]
fn review_dk2_duplicate_refusal_and_restore_ack_never_place_twice() {
    let (mut h, profile) = dk2_long_partial_last_stop();
    set_reference_prices(&mut h, 79130.0, 79130.0);
    amend(&mut h, Some(79124.9), None);
    accept_cancel(&mut h, profile);
    let replacement = h.take("/v5/position/trading-stop");
    let refused = r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#;
    h.reply(replacement.id, 200, refused);
    let restore = h.take("/v5/position/trading-stop");
    h.reply(replacement.id, 200, refused);
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "duplicate replacement response must not issue a second restore"
    );
    h.reply(restore.id, 200, OK);
    h.reply(restore.id, 200, OK);
    assert!(!h.has_request("/v5/position/trading-stop"));
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 91).count(), 1);
}

#[test]
fn review_dk2_fresh_last_boundary_and_stale_last_both_refuse_before_cancel() {
    for (age, cause) in [(5000., "would trigger at once"), (5001., "fresh last")] {
        let (mut h, profile) = dk2_long_partial_last_stop();
        h.now += age;
        h.message("/v5/public/linear", r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","markPrice":"79130"}}"#);
        amend(&mut h, Some(79124.9), None);
        assert!(!h.has_request(profile.cancel_path()), "age {age}");
        let error = h.result(91).unwrap().as_ref().unwrap_err();
        assert!(error.contains(cause), "{error}");
        assert_eq!(
            h.engine.store().order("partial").unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn review_dk2_definitively_refused_restore_is_serialized_as_unprotected_and_does_not_loop() {
    let error = review_recheck_restore_failure(10001);
    assert_eq!(
        error.protection,
        Some(safe_cex_engine::error::Protection::Unprotected)
    );
    assert!(
        error.message.contains("long position has no stop")
            && error.message.contains("putting it back failed too"),
        "{error:?}"
    );
    let wire = serde_json::to_string(&error).unwrap();
    assert!(wire.contains(r#""protection":"unprotected""#), "{wire}");
    assert!(!wire.contains("presents"));
}

/// While an edit of a Partial stop is in flight, a cancel of that stop (or of every
/// order on its symbol) waits its turn as refused, so a later put-back cannot undo it.
#[test]
fn a_cancel_of_a_stop_whose_edit_is_in_flight_is_refused_before_it_is_sent() {
    for cancel in [
        Command::CancelOrders {
            command: 92,
            ids: vec!["partial".into()],
        },
        Command::CancelSymbolOrders {
            command: 92,
            symbol: "BTCUSDT".into(),
        },
    ] {
        let (mut h, profile) = dk2_long_partial_last_stop();
        set_reference_prices(&mut h, 79130.0, 79130.0);
        amend(&mut h, Some(79124.9), None);
        let first_cancel = h.take(profile.cancel_path());
        h.command(cancel);
        assert!(
            !h.requests
                .iter()
                .any(|r| r.url.contains("/v5/order/cancel")),
            "{:?}",
            h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
        assert!(matches!(h.result(92), Some(Err(_))));
        let original = h.engine.store().order("partial").unwrap().clone();
        h.reply(first_cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
        confirm_partial_reads(&mut h, profile, &original);
        let replacement = h.take("/v5/position/trading-stop");
        h.reply(replacement.id, 200, OK);
        assert!(matches!(h.result(91), Some(Ok(_))));
    }
}

fn set_reference_prices(h: &mut Harness, mark: f64, last: f64) {
    let mut tickers = h.engine.store().tickers().to_vec();
    for ticker in &mut tickers {
        ticker.mark = mark;
        ticker.last = last;
    }
    h.message("/v5/private", r#"{"op":"pong"}"#);
    h.engine.store_mut().set_tickers(tickers);
}

fn defer_partial_cancel(h: &mut Harness) -> safe_cex_engine::request_context::Preparation {
    h.engine
        .handle_command(
            h.now,
            Command::AmendOrder {
                command: 91,
                id: "partial".into(),
                price: Some(95.0),
                amount: None,
            },
            None,
        )
        .unwrap();
    take_partial_preparation(h, "/v5/order/cancel")
}

fn take_partial_preparation(
    h: &mut Harness,
    path: &str,
) -> safe_cex_engine::request_context::Preparation {
    use safe_cex_engine::{Action, request_context::HttpAction};
    h.engine.prepare_dispatch(h.now);
    let mut preparation = None;
    for action in h.engine.drain_actions() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation: token,
                    request,
                    ..
                } => {
                    assert!(request.url.contains(path), "{}", request.url);
                    assert!(preparation.replace(token).is_none());
                    h.prepared_routes.insert(request.id, (token, request.id.0));
                }
                HttpAction::Cancel { preparation, .. } => {
                    h.prepared_routes
                        .retain(|_, (token, _)| *token != preparation);
                }
                other => panic!("unexpected managed action {other:?}"),
            },
            Action::SetTimer { timer, at_ms } => h.timers.push((timer, at_ms)),
            Action::ClearTimer { timer } => h.timers.retain(|(id, _)| *id != timer),
            Action::Http(_) => panic!("cancel released before preparation"),
            _ => {}
        }
    }
    preparation.expect("genuine cancel preparation")
}

fn assert_guard_refused(h: &Harness, cause: &str) {
    assert!(
        !h.requests
            .iter()
            .any(|request| request.url.contains("/v5/order/cancel")
                || request.url.contains("/v5/position/trading-stop")),
        "nothing destructive or replacement released"
    );
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Open
    );
    let error = h
        .result(91)
        .expect("visible command outcome")
        .as_ref()
        .unwrap_err();
    assert!(error.contains(cause), "{error}");
    assert!(error.contains("unchanged"), "{error}");
    assert!(
        h.outcomes
            .contains(&(91, Some(safe_cex_engine::Outcome::NotSent)))
    );
}

#[test]
fn partial_stop_uses_its_actual_last_reference_before_cancelling_live_long_protection() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "LastPrice",
        false,
        true,
    );
    h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"0.005","avgPrice":"76200","markPrice":"76260","leverage":"10","positionIdx":0}]}"#);
    set_reference_prices(&mut h, 76260.0, 76250.0);
    amend(&mut h, Some(76250.2), None);
    assert_guard_refused(&h, "last price 76250");
}

#[test]
fn partial_stop_refuses_stale_reference_before_cancelling() {
    let mut h = ready(PROFILES[0]);
    seed(
        &mut h,
        PROFILES[0],
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    h.now += 5001.0;
    amend(&mut h, Some(95.0), None);
    assert_guard_refused(&h, "fresh mark");
}

#[test]
fn partial_stop_rechecks_moved_reference_at_physical_release_and_unwinds_for_next_amend() {
    use safe_cex_engine::request_context::Prepared;
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "LastPrice",
        false,
        true,
    );
    let preparation = defer_partial_cancel(&mut h);
    h.now += 100.0;
    set_reference_prices(&mut h, 100.0, 94.0);
    h.engine.http_prepared(
        h.now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    h.collect();
    assert_guard_refused(&h, "last price 94");
    h.command(Command::AmendOrder {
        command: 92,
        id: "partial".into(),
        price: Some(90.0),
        amount: None,
    });
    let request = h.take(profile.cancel_path());
    let original = h.engine.store().order("partial").unwrap().clone();
    h.reply(request.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    confirm_partial_reads(&mut h, profile, &original);
    h.respond("/v5/position/trading-stop", 200, OK);
    assert!(
        matches!(h.result(92), Some(Ok(_))),
        "follow-up releases normally: {:?}",
        h.result(92)
    );
}

#[test]
fn partial_stop_rechecks_reference_age_after_delayed_signing_preparation() {
    use safe_cex_engine::request_context::Prepared;
    let mut h = ready(PROFILES[0]);
    seed(
        &mut h,
        PROFILES[0],
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let preparation = defer_partial_cancel(&mut h);
    h.now += 5001.0;
    h.engine.http_prepared(
        h.now,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    h.collect();
    assert_guard_refused(&h, "fresh mark");
}

#[test]
fn partial_stop_missing_own_reference_refuses_without_borrowing_mark() {
    let mut h = ready(PROFILES[0]);
    seed(
        &mut h,
        PROFILES[0],
        "Sell",
        "StopLoss",
        "LastPrice",
        false,
        true,
    );
    set_reference_prices(&mut h, 100.0, 0.0);
    amend(&mut h, Some(95.0), None);
    assert_guard_refused(&h, "fresh last");
}

#[test]
fn partial_stop_fresh_reference_boundary_is_inclusive_for_every_profile() {
    for profile in PROFILES {
        for age in [5000.0, 5001.0] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            h.now += age;
            amend(&mut h, Some(95.0), None);
            if age == 5000.0 {
                assert!(h.has_request(profile.cancel_path()), "{profile:?}");
                assert!(h.result(91).is_none());
            } else {
                assert_guard_refused(&h, "fresh mark");
            }
        }
    }
}

#[test]
fn tp_amend_native_rows_keep_real_reference_or_use_last_when_unknown() {
    use safe_cex_engine::model::StopTriggerType;
    for profile in PROFILES {
        for (native, expected, typed) in [
            ("MarkPrice", "MarkPrice", Some(StopTriggerType::MarkPrice)),
            ("LastPrice", "LastPrice", Some(StopTriggerType::LastPrice)),
            (
                "IndexPrice",
                "IndexPrice",
                Some(StopTriggerType::IndexPrice),
            ),
            ("UNKNOWN", "LastPrice", None),
            ("NEW_NATIVE_TRIGGER", "LastPrice", None),
            ("", "LastPrice", None),
        ] {
            for path in ["attached", "full", "missing-mark"] {
                let mut h = ready(profile);
                h.message("/v5/private", &serde_json::json!({"topic":"position","data":[{
                    "category":profile.category(),"symbol":profile.symbol(),"side":"Buy","size":"0","avgPrice":"0","markPrice":"100","leverage":"10","positionIdx":0
                }]}).to_string());
                let mut row = serde_json::json!({
                    "category":profile.category(),"symbol":profile.symbol(),"orderId":"tp-native",
                    "side":"Sell","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"TakeProfit",
                    "tpslMode":if path=="full" {"Full"} else {"Partial"},
                    "qty":if path=="full" {"0"} else {"7"},"cumExecQty":"0","price":"110","leavesQty":"7",
                    "triggerPrice":"110","triggerBy":native,"reduceOnly":true,"closeOnTrigger":true,"positionIdx":0
                });
                if path == "attached" {
                    row["orderId"] = "parent".into();
                    row["side"] = "Buy".into();
                    row["orderType"] = "Limit".into();
                    row["orderStatus"] = "New".into();
                    row["stopOrderType"] = "UNKNOWN".into();
                    row["tpslMode"] = "".into();
                    row["triggerPrice"] = "0".into();
                    row["triggerBy"] = "MarkPrice".into();
                    row["takeProfit"] = "110".into();
                    row["tpTriggerBy"] = native.into();
                }
                h.message(
                    "/v5/private",
                    &serde_json::json!({"topic":"order","data":[row]}).to_string(),
                );
                let id = if path == "attached" {
                    "parent__take_profit"
                } else {
                    "tp-native"
                };
                let order = h.engine.store().order(id).unwrap_or_else(|| {
                    panic!(
                        "native TP row missing {profile:?}/{path}/{native}: {:?}",
                        h.logs
                    )
                });
                assert_eq!(order.trigger_type, typed, "{profile:?}/{path}/{native}");
                if !native.is_empty() && typed.is_none() {
                    assert!(
                        h.logs
                            .iter()
                            .any(|log| log.contains("unrecognized trigger reference")),
                        "unknown must be observable"
                    );
                    assert!(!h.logs.iter().any(|log| log.contains("NEW_NATIVE_TRIGGER")));
                }
                if path == "missing-mark" {
                    h.engine.store_mut().set_tickers(Vec::new());
                }
                h.command(Command::AmendOrder {
                    command: 93,
                    id: id.into(),
                    price: Some(115.0),
                    amount: None,
                });
                if path == "missing-mark" {
                    assert!(
                        h.requests
                            .iter()
                            .all(|r| r.method == safe_cex_engine::Method::Get)
                    );
                    assert!(
                        h.result(93)
                            .unwrap()
                            .as_ref()
                            .unwrap_err()
                            .contains("fresh mark")
                    );
                    assert!(
                        h.outcomes
                            .contains(&(93, Some(safe_cex_engine::Outcome::NotSent)))
                    );
                    assert_eq!(
                        h.engine.store().order(id).unwrap().status,
                        OrderStatus::Open
                    );
                    continue;
                }
                assert!(!h.has_request(profile.cancel_path()));
                let request = h.take(if path == "attached" {
                    "/v5/order/amend"
                } else {
                    "/v5/position/trading-stop"
                });
                let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
                assert_eq!(payload["category"], profile.category());
                assert_eq!(payload["takeProfit"], "115");
                assert_eq!(
                    payload["tpTriggerBy"], expected,
                    "{profile:?}/{path}/{native}"
                );
            }
        }
    }
}

#[test]
fn new_sl_placements_keep_the_existing_index_to_mark_wire_policy() {
    for profile in PROFILES {
        for path in ["standalone", "attached", "position"] {
            let mut h = ready(profile);
            h.message("/v5/private", &serde_json::json!({"topic":"position","data":[{
                "category":profile.category(),"symbol":profile.symbol(),"side":"Buy","size":if path == "position" {"20"} else {"0"},"avgPrice":"100","markPrice":"100","leverage":"10","unrealisedPnl":"0","positionIdx":0
            }]}).to_string());
            let mut order = serde_json::json!({
                "symbol":profile.symbol(),"type":"stop_market","side":"sell",
                "amount":7,"price":90,"slTriggerType":"index_price"
            });
            if path == "attached" {
                order["type"] = "limit".into();
                order["stopLoss"] = 85.into();
            }
            h.command(Command::PlaceOrders {
                command: 95,
                orders: vec![serde_json::from_value(order).unwrap()],
            });
            let request = if path == "position" {
                h.take("/v5/position/trading-stop")
            } else if h.has_request("/v5/order/create-batch") {
                h.take("/v5/order/create-batch")
            } else {
                h.take("/v5/order/create")
            };
            let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
            let payload = payload.get("request").map_or(&payload, |rows| &rows[0]);
            let field = if path == "standalone" {
                "triggerBy"
            } else {
                "slTriggerBy"
            };
            assert_eq!(payload[field], "MarkPrice", "{profile:?}/{path}");
        }
    }
}

#[test]
fn partial_cancel_ack_waits_for_a_target_terminal_read_before_any_create() {
    let (mut h, profile) = dk2_long_partial_last_stop();
    h.message("/v5/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"20","avgPrice":"79000","markPrice":"79130","leverage":"10","positionIdx":0}]}"#);
    amend(&mut h, Some(79000.0), None);
    h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "a cancel ACK is not terminal evidence"
    );
    let proof = h.take("/v5/order/realtime");
    assert_eq!(proof.method, safe_cex_engine::Method::Get);
    assert!(proof.url.contains("orderId=partial"), "{}", proof.url);
}

#[test]
fn partial_absent_live_filled_or_mismatched_native_rows_never_prove_cancellation() {
    for refusal in [
        "absent",
        "open",
        "filled",
        "wrong-id",
        "wrong-symbol",
        "wrong-category",
        "wrong-trigger",
        "wrong-size",
    ] {
        let profile = PROFILES[0];
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        amend(&mut h, Some(95.0), None);
        h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
        let mut row = serde_json::json!({
            "orderId":"partial","symbol":"BTCUSDT","side":"Sell","orderStatus":"Cancelled",
            "qty":"7","cumExecQty":"0","triggerPrice":"90","triggerBy":"MarkPrice",
            "orderType":"Market","price":"0","positionIdx":0,"tpslMode":"Partial","stopOrderType":"StopLoss"
        });
        match refusal {
            "open" => row["orderStatus"] = "Untriggered".into(),
            "filled" => row["cumExecQty"] = "1".into(),
            "wrong-id" => row["orderId"] = "other".into(),
            "wrong-symbol" => row["symbol"] = "ETHUSDT".into(),
            "wrong-trigger" => row["triggerBy"] = "LastPrice".into(),
            "wrong-size" => row["qty"] = "8".into(),
            _ => {}
        }
        let rows = if refusal == "absent" {
            vec![]
        } else {
            vec![row]
        };
        let answer = serde_json::json!({"retCode":0,"result":{
            "category":if refusal == "wrong-category" {"inverse"} else {"linear"},
            "list":rows,"nextPageCursor":""
        }})
        .to_string();
        h.respond("&orderId=partial", 200, &answer);
        // Three realtime reads, then the bounded history polls for an absent stop.
        for _ in 0..8 {
            if h.result(91).is_some() {
                break;
            }
            assert!(!h.has_request("/v5/position/trading-stop"));
            h.advance(1_500.0);
            h.respond("&orderId=partial", 200, &answer);
        }
        assert!(!h.has_request("/v5/position/trading-stop"), "{refusal}");
        assert!(h.unknown.contains(&91), "{refusal}: {:?}", h.notices);
    }
}

#[test]
fn partial_cancellation_poll_waits_for_native_terminal_evidence() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let original = h.engine.store().order("partial").unwrap().clone();
    let terminal = native_partial_target(&h, profile, &original);
    let mut live = terminal.clone();
    live["orderStatus"] = "Untriggered".into();
    amend(&mut h, Some(95.0), None);
    h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    h.respond("&orderId=partial", 200, &serde_json::json!({"retCode":0,"result":{"category":"linear","list":[live],"nextPageCursor":""}}).to_string());
    assert!(!h.has_request("/v5/position/trading-stop"));
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Open
    );
    h.advance(100.0);
    h.respond("&orderId=partial", 200, &serde_json::json!({"retCode":0,"result":{"category":"linear","list":[terminal],"nextPageCursor":""}}).to_string());
    let empty = r#"{"retCode":0,"result":{"category":"linear","list":[],"nextPageCursor":""}}"#;
    h.respond("&openOnly=0&limit=50", 200, empty);
    h.respond("/v5/order/history", 200, empty);
    assert!(h.has_request("/v5/position/trading-stop"));
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Canceled
    );
    h.respond("/v5/position/trading-stop", 200, OK);
    assert!(h.result(91).is_some_and(|result| result.is_ok()));
}

#[test]
fn partial_proof_transport_loss_never_creates_or_claims_protection() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(95.0), None);
    h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    let request = h.take("&orderId=partial");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "lost native cancellation proof",
    });
    assert!(!h.has_request("/v5/position/trading-stop"));
    let error = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        error.outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "{error:?}"
    );
    assert_eq!(error.protection, None, "{error:?}");
    assert!(error.message.contains("Open Orders"), "{error:?}");
}

#[test]
fn partial_edits_refuse_filled_or_unprotected_orders_before_cancel() {
    for failure in ["filled", "no-position", "wrong-side"] {
        let (mut h, profile) = dk2_long_partial_last_stop();
        h.message("/v5/private", &serde_json::json!({"topic":"position","data":[{
            "category":"linear","symbol":"BTCUSDT","side":if failure == "wrong-side" {"Sell"} else {"Buy"},"size":if failure == "no-position" {"0"} else {"20"},"avgPrice":"79000","markPrice":"79130","leverage":"10","positionIdx":0
        }]}).to_string());
        if failure == "filled" {
            let mut order = h.engine.store().order("partial").unwrap().clone();
            order.filled = 1.0;
            order.remaining = 6.0;
            h.engine.store_mut().upsert_order(order);
        }
        amend(&mut h, Some(79000.0), None);
        assert!(!h.has_request(profile.cancel_path()), "{failure}");
        assert!(
            h.outcomes
                .contains(&(91, Some(safe_cex_engine::Outcome::NotSent))),
            "{failure}: {:?}",
            h.notices
        );
    }
}

#[test]
fn pinfix_partial_restore_requires_positive_standalone_provenance() {
    // TS cancel/replace context: reference/bybit/bybit.exchange.ts:3221-3224.
    // Lead preservation rule keeps master's native-lineage guarded put-back beyond this TS flow.
    for shape in [
        "standalone",
        "attached",
        "paired-lost-binding",
        "missing-parent",
        "missing-link",
    ] {
        let profile = PROFILES[0];
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        let original = h.engine.store().order("partial").unwrap().clone();
        amend(&mut h, Some(95.0), None);
        h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
        confirm_partial_reads_with(&mut h, profile, &original, |target| {
            match shape {
                "attached" => target["parentOrderLinkId"] = "parent".into(),
                "missing-parent" => {
                    target.as_object_mut().unwrap().remove("parentOrderLinkId");
                }
                "missing-link" => {
                    target.as_object_mut().unwrap().remove("orderLinkId");
                }
                "paired-lost-binding" => {
                    let mut peer = target.clone();
                    peer["orderId"] = "opposite-leg".into();
                    peer["stopOrderType"] = "TakeProfit".into();
                    peer["orderLinkId"] = "lost-binding-link".into();
                    peer["orderStatus"] = "Untriggered".into();
                    peer["triggerPrice"] = "110".into();
                    return vec![peer];
                }
                _ => {}
            }
            Vec::new()
        });
        h.respond(
            "/v5/position/trading-stop",
            200,
            r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
        );
        if shape == "standalone" {
            assert!(h.has_request("/v5/position/trading-stop"));
            h.respond("/v5/position/trading-stop", 200, OK);
        } else {
            assert!(!h.has_request("/v5/position/trading-stop"), "{shape}");
        }
        let error = h
            .notices
            .iter()
            .find_map(|notice| match notice {
                safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            error.protection,
            Some(if shape == "standalone" {
                safe_cex_engine::error::Protection::Restored
            } else {
                safe_cex_engine::error::Protection::Unprotected
            }),
            "{shape}: {error:?}"
        );
        if shape != "standalone" {
            assert!(error.message.contains("re-place"), "{error:?}");
        }
    }
}

#[test]
fn partial_put_back_never_rounds_the_original_native_size_to_current_precision() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let mut original = h.engine.store().order("partial").unwrap().clone();
    original.amount = 7.0004;
    original.remaining = 7.0004;
    h.engine.store_mut().upsert_order(original);
    amend(&mut h, Some(95.0), None);
    accept_cancel(&mut h, profile);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    let payload: serde_json::Value = serde_json::from_str(body(&restore)).unwrap();
    assert_eq!(payload["slSize"], "7.0004");
}

#[test]
fn partial_history_census_uses_proved_creation_time_and_keeps_incomplete_or_paired_rows_unproven() {
    for profile in PROFILES {
        for census in [
            "complete",
            "history-incomplete",
            "live-incomplete",
            "paired",
            "nearby-unrelated-partial",
            "outside-window",
            "missing-time",
        ] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            let original = h.engine.store().order("partial").unwrap().clone();
            let created = h.now as u64 - 100;
            let mut target = native_partial_target(&h, profile, &original);
            target["createdTime"] = created.to_string().into();
            amend(&mut h, Some(95.0), None);
            h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
            h.respond("&orderId=partial", 200, &serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":[target.clone()],"nextPageCursor":""}}).to_string());
            h.respond("&openOnly=0&limit=50", 200, &serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":[],"nextPageCursor":if census == "live-incomplete" {"more-live"} else {""}}}).to_string());
            let history = h.take("/v5/order/history");
            let parameter = |name: &str| {
                history
                    .url
                    .split('&')
                    .filter_map(|part| part.split_once('='))
                    .find(|(key, _)| *key == name)
                    .and_then(|(_, value)| value.parse::<u64>().ok())
            };
            let bounded = parameter("startTime") == Some(created.saturating_sub(5_000).max(1))
                && parameter("endTime") == Some(created + 5_000);
            assert_eq!(history.method, safe_cex_engine::Method::Get);
            let peers = if matches!(
                census,
                "paired" | "nearby-unrelated-partial" | "outside-window" | "missing-time"
            ) {
                let mut peer = target;
                peer["orderId"] = "paired-leg".into();
                peer["stopOrderType"] = "TakeProfit".into();
                peer["triggerPrice"] = "110".into();
                if census != "paired" {
                    peer["orderLinkId"] = "unrelated".into();
                }
                if census == "nearby-unrelated-partial" {
                    peer["createdTime"] = (created + 1_000).to_string().into();
                }
                if census == "outside-window" {
                    peer["createdTime"] = (created + 5_001).to_string().into();
                }
                if census == "missing-time" {
                    peer.as_object_mut().unwrap().remove("createdTime");
                }
                vec![peer]
            } else {
                Vec::new()
            };
            // The unbounded seven-day census on a busy account has another page.
            h.reply(history.id, 200, &serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":peers,"nextPageCursor":if !bounded || census == "history-incomplete" {"busy-account-next-page"} else {""}}}).to_string());
            h.respond(
                "/v5/position/trading-stop",
                200,
                r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
            );
            assert_eq!(
                h.has_request("/v5/position/trading-stop"),
                census == "complete",
                "{profile:?}/{census}: only proved standalone lineage may put back"
            );
            if census == "complete" && h.has_request("/v5/position/trading-stop") {
                h.respond("/v5/position/trading-stop", 200, OK);
            }
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                    _ => None,
                })
                .unwrap();
            let expected = if census == "complete" {
                safe_cex_engine::error::Protection::Restored
            } else {
                safe_cex_engine::error::Protection::Unprotected
            };
            assert_eq!(
                error.protection,
                Some(expected),
                "{profile:?}/{census}: {error:?}"
            );
            assert!(
                bounded,
                "history must use native createdTime, not Store last_update: {}",
                history.url
            );
            assert!(!h.has_request("/v5/position/trading-stop"));
        }
    }
}

#[test]
fn partial_initial_cancel_prearms_open_orders_words_for_engine_owned_failures() {
    for profile in PROFILES {
        for failure in ["transport", "timeout", "refused"] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            amend(&mut h, Some(95.0), None);
            let cancel = h.take(profile.cancel_path());
            match failure {
                "transport" => h.event(Event::HttpFailed {
                    request: cancel.id,
                    error: "cancel transport lost",
                }),
                "timeout" => h.advance(20_001.0),
                _ => h.reply(
                    cancel.id,
                    200,
                    r#"{"retCode":10001,"retMsg":"cancel refused","result":{}}"#,
                ),
            }
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                    _ => None,
                })
                .unwrap();
            if failure == "refused" {
                assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Rejected));
                assert!(error.message.contains("cancel refused"), "{error:?}");
                assert!(!error.message.contains("unknown outcome"), "{error:?}");
            } else {
                assert_eq!(
                    error.outcome,
                    Some(safe_cex_engine::Outcome::Unknown),
                    "{error:?}"
                );
                assert!(
                    error.message.contains("unknown outcome")
                        && error.message.contains("Open Orders"),
                    "{profile:?}/{failure}: {error:?}"
                );
            }
            assert_eq!(error.protection, None);
            assert!(!h.has_request("/v5/position/trading-stop"));
            assert!(!h.has_request(profile.cancel_path()));
        }
    }
}

#[test]
fn partial_initial_cancel_keeps_a_known_unsent_preparation_refusal() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let preparation = defer_partial_cancel(&mut h);
    h.engine.http_preparation_failed(
        h.now,
        preparation,
        safe_cex_engine::request_context::Error::Invalid,
        safe_cex_engine::VenueError::new("cancel preparation refused")
            .kind(safe_cex_engine::ErrorKind::InvalidInput),
    );
    h.collect();
    let error = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        error.outcome,
        Some(safe_cex_engine::Outcome::NotSent),
        "{error:?}"
    );
    assert_eq!(error.message, "cancel preparation refused");
    assert_eq!(error.protection, None);
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Open
    );
    assert!(!h.has_request(profile.cancel_path()));
    assert!(!h.has_request("/v5/position/trading-stop"));
}

#[test]
fn partial_initial_cancel_preparation_timeout_does_not_claim_an_unknown_write() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let _preparation = defer_partial_cancel(&mut h);
    h.advance(20_001.0);
    let error = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        error.outcome,
        Some(safe_cex_engine::Outcome::NotSent),
        "{error:?}"
    );
    assert!(error.message.contains("timed out"), "{error:?}");
    assert!(!error.message.contains("unknown outcome"), "{error:?}");
    assert_eq!(error.protection, None);
    assert_eq!(
        h.engine.store().order("partial").unwrap().status,
        OrderStatus::Open
    );
    assert!(!h.has_request(profile.cancel_path()));
    assert!(!h.has_request("/v5/position/trading-stop"));
}

#[test]
fn partial_unproved_creation_time_allows_replacement_without_history_or_put_back() {
    for profile in PROFILES {
        for (created, succeeds) in [
            "missing",
            "",
            "0",
            "-1",
            "NaN",
            "18446744073709551616",
            "1001",
        ]
        .into_iter()
        .flat_map(|created| [(created, true), (created, false)])
        {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            let original = h.engine.store().order("partial").unwrap().clone();
            let mut target = native_partial_target(&h, profile, &original);
            if created == "missing" {
                target.as_object_mut().unwrap().remove("createdTime");
            } else {
                target["createdTime"] = created.into();
            }
            amend(&mut h, Some(95.0), None);
            h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
            h.respond("&orderId=partial", 200, &serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":[target],"nextPageCursor":""}}).to_string());
            h.respond("&openOnly=0&limit=50", 200, &serde_json::json!({"retCode":0,"result":{"category":profile.category(),"list":[],"nextPageCursor":""}}).to_string());
            assert!(!h.has_request("/v5/order/history"), "{created}");
            assert!(h.has_request("/v5/position/trading-stop"), "{created}");
            let replacement = h.take("/v5/position/trading-stop");
            let body: serde_json::Value =
                serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["stopLoss"], "95");
            h.reply(
                replacement.id,
                200,
                if succeeds {
                    OK
                } else {
                    r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#
                },
            );
            assert!(!h.has_request("/v5/order/history"), "{created}");
            assert!(!h.has_request("/v5/position/trading-stop"), "{created}");
            if succeeds {
                assert!(
                    !h.notices.iter().any(|notice| matches!(
                        notice,
                        safe_cex_engine::Notice::CommandFailed { command: 91, .. }
                    )),
                    "{created}"
                );
                assert!(
                    h.notices.iter().any(|notice| matches!(
                        notice,
                        safe_cex_engine::Notice::CommandDone { command: 91, .. }
                    )),
                    "{created}"
                );
                continue;
            }
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                error.protection,
                Some(safe_cex_engine::error::Protection::Unprotected),
                "{created}: {error:?}"
            );
            assert!(
                error.message.contains("replacement refused") && error.message.contains("re-place"),
                "{created}: {error:?}"
            );
        }
    }
}

#[test]
fn partial_unknown_replacement_or_put_back_never_restores_or_resends() {
    for leg in ["replacement", "put-back"] {
        for (status, reply) in [
            (429, OK),
            (408, OK),
            (503, OK),
            (
                200,
                r#"{"retCode":10006,"retMsg":"rate limited","result":{}}"#,
            ),
            (
                200,
                r#"{"retCode":10018,"retMsg":"rate limited","result":{}}"#,
            ),
            (200, r#"{"retCode":999999,"retMsg":"unlisted","result":{}}"#),
            (
                200,
                r#"{"retCode":34040,"retMsg":"not modified","result":{}}"#,
            ),
            (200, r#"{"retCode":0,"result":{"orderId":"foreign"}}"#),
            (200, r#"{"retCode":0}"#),
            (200, r#"{"retCode":0,"result":null} trailing"#),
        ] {
            let profile = PROFILES[0];
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            amend(&mut h, Some(95.0), None);
            accept_cancel(&mut h, profile);
            if leg == "put-back" {
                h.respond(
                    "/v5/position/trading-stop",
                    200,
                    r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
                );
            }
            h.respond("/v5/position/trading-stop", status, reply);
            assert!(
                !h.has_request("/v5/position/trading-stop"),
                "{leg}/{status}/{reply}"
            );
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    panic!(
                        "{leg}/{status}/{reply}: {:?}, result={:?}",
                        h.notices,
                        h.result(91)
                    )
                });
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "{leg}/{status}/{reply}: {error:?}"
            );
            assert_eq!(error.protection, None, "{error:?}");
            assert!(error.message.contains("Open Orders"), "{error:?}");
        }
    }
}

#[test]
fn applied_batch_cancel_partial_stop_does_not_report_a_created_order_on_unknown_replacement() {
    for profile in PROFILES {
        for (status, response) in [
            (
                200,
                r#"{"retCode":999999,"retMsg":"unknown replacement","result":{}}"#,
            ),
            (503, "replacement acknowledgement lost"),
        ] {
            let mut h = ready(profile);
            seed(
                &mut h,
                profile,
                "Sell",
                "StopLoss",
                "MarkPrice",
                false,
                true,
            );
            amend(&mut h, Some(95.0), None);
            accept_cancel(&mut h, profile);
            h.respond("/v5/position/trading-stop", status, response);
            assert!(!h.has_request(profile.cancel_path()));
            assert!(
                !h.has_request("/v5/position/trading-stop"),
                "{profile:?}/{status}: no replacement replay or put-back"
            );
            let errors: Vec<_> = h
                .notices
                .iter()
                .filter_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1, "{profile:?}/{status}");
            let error = errors[0];
            assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
            assert_eq!(error.protection, None);
            assert!(
                error
                    .partial
                    .as_ref()
                    .is_none_or(|partial| partial.order_ids.is_empty()),
                "{profile:?}/{status}: the cancelled stop is not a created order: {error:?}"
            );
        }
    }
}

#[test]
fn partial_final_cancel_veto_rechecks_fill_identity_and_protected_position() {
    use safe_cex_engine::request_context::Prepared;
    for change in ["fill", "price", "amount", "position"] {
        let profile = PROFILES[0];
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        let preparation = defer_partial_cancel(&mut h);
        if change == "position" {
            h.engine.store_mut().update_positions(|p| p.contracts = 0.0);
        } else {
            let mut order = h.engine.store().order("partial").unwrap().clone();
            match change {
                "fill" => {
                    order.filled = 1.0;
                    order.remaining = 6.0;
                }
                "price" => order.price = 89.0,
                _ => order.amount = 8.0,
            }
            h.engine.store_mut().upsert_order(order);
        }
        h.engine.http_prepared(
            h.now,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        h.collect();
        assert!(!h.has_request(profile.cancel_path()), "{change}");
        assert!(
            h.outcomes
                .contains(&(91, Some(safe_cex_engine::Outcome::NotSent))),
            "{change}: {:?}",
            h.notices
        );
    }
}

#[test]
fn partial_final_veto_covers_a_resigned_cancel_replacement_and_put_back() {
    use safe_cex_engine::{Response, request_context::Prepared};
    for leg in ["resigned-cancel", "replacement", "put-back"] {
        let profile = PROFILES[0];
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        let original = h.engine.store().order("partial").unwrap().clone();
        amend(&mut h, Some(95.0), None);
        let (request, answer, path) = if leg == "resigned-cancel" {
            (
                h.take(profile.cancel_path()),
                r#"{"retCode":10002,"retMsg":"recv window refusal","result":{}}"#.to_owned(),
                "/v5/order/cancel",
            )
        } else if leg == "put-back" {
            accept_cancel(&mut h, profile);
            (
                h.take("/v5/position/trading-stop"),
                r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#.to_owned(),
                "/v5/position/trading-stop",
            )
        } else {
            h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
            let target = native_partial_target(&h, profile, &original);
            h.respond("&orderId=partial", 200, &serde_json::json!({"retCode":0,"result":{"category":"linear","list":[target],"nextPageCursor":""}}).to_string());
            let answer =
                r#"{"retCode":0,"result":{"category":"linear","list":[],"nextPageCursor":""}}"#;
            h.respond("&openOnly=0&limit=50", 200, answer);
            (
                h.take("/v5/order/history"),
                answer.to_owned(),
                "/v5/position/trading-stop",
            )
        };
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &answer,
                },
            },
        );
        let preparation = take_partial_preparation(&mut h, path);
        h.engine
            .store_mut()
            .update_positions(|position| position.contracts = 0.0);
        h.engine.http_prepared(
            h.now,
            preparation,
            Prepared {
                id: preparation.attempt().request.0,
                expires_at: None,
            },
        );
        h.collect();
        assert!(!h.has_request("/v5/position/trading-stop"), "{leg}");
        assert!(!h.has_request(profile.cancel_path()), "{leg}");
        assert!(h.result(91).is_some_and(|result| result.is_err()), "{leg}");
        if leg != "resigned-cancel" {
            let error = h
                .notices
                .iter()
                .find_map(|notice| match notice {
                    safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                error.protection,
                Some(safe_cex_engine::error::Protection::Unprotected),
                "{leg}: {error:?}"
            );
        }
    }
}

#[test]
fn tp_add_uses_a_known_requested_reference_for_linear_and_inverse() {
    for profile in PROFILES {
        for (typed, expected) in [
            ("mark_price", "MarkPrice"),
            ("last_price", "LastPrice"),
            ("index_price", "IndexPrice"),
        ] {
            let mut h = ready(profile);
            h.message("/v5/private", &serde_json::json!({"topic":"position","data":[{
                "category":profile.category(),"symbol":profile.symbol(),"side":"Buy","size":"20","avgPrice":"100","markPrice":"100","leverage":"10","unrealisedPnl":"0","positionIdx":0
            }]}).to_string());
            h.command(Command::PlaceOrders {command:94, orders:vec![serde_json::from_value(serde_json::json!({"symbol":profile.symbol(),"type":"take_profit_market","side":"sell","amount":0,"price":115,"tpTriggerType":typed})).unwrap()]});
            assert!(
                h.has_request("/v5/position/trading-stop"),
                "{profile:?}/{typed}: result={:?}, requests={:?}",
                h.result(94),
                h.requests.iter().map(|r| &r.url).collect::<Vec<_>>()
            );
            let request = h.take("/v5/position/trading-stop");
            let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
            assert_eq!(payload["tpTriggerBy"], expected, "{profile:?}");
        }
    }
}

#[test]
fn attached_sl_amend_preserves_own_native_reference_or_existing_mark_default() {
    use safe_cex_engine::model::StopTriggerType;
    for profile in PROFILES {
        for (native, expected, typed) in [
            (
                Some("MarkPrice"),
                "MarkPrice",
                Some(StopTriggerType::MarkPrice),
            ),
            (
                Some("LastPrice"),
                "LastPrice",
                Some(StopTriggerType::LastPrice),
            ),
            (
                Some("IndexPrice"),
                "IndexPrice",
                Some(StopTriggerType::IndexPrice),
            ),
            (Some("UNKNOWN"), "MarkPrice", None),
            (Some("NEW_NATIVE_TRIGGER"), "MarkPrice", None),
            (Some(""), "MarkPrice", None),
            (None, "MarkPrice", None),
        ] {
            let mut h = ready(profile);
            h.message(
                "/v5/private",
                &serde_json::json!({"topic":"position","data":[{
                    "category":profile.category(),"symbol":profile.symbol(),"side":"Buy","size":"0",
                    "avgPrice":"0","markPrice":"100","leverage":"10","positionIdx":0
                }]})
                .to_string(),
            );
            let mut row = serde_json::json!({
                "category":profile.category(),"symbol":profile.symbol(),"orderId":"sl-parent",
                "side":"Buy","orderType":"Limit","orderStatus":"New","stopOrderType":"Stop",
                "qty":"7","cumExecQty":"0","price":"110","leavesQty":"7",
                "triggerPrice":"110","triggerBy":"LastPrice","stopLoss":"90","positionIdx":0
            });
            if let Some(native) = native {
                row["slTriggerBy"] = native.into();
            }
            h.message(
                "/v5/private",
                &serde_json::json!({"topic":"order","data":[row]}).to_string(),
            );
            assert_eq!(
                h.engine.store().order("sl-parent").unwrap().trigger_type,
                Some(StopTriggerType::LastPrice)
            );
            assert_eq!(
                h.engine
                    .store()
                    .order("sl-parent__stop_loss")
                    .unwrap()
                    .trigger_type,
                typed,
                "{profile:?}/{native:?}"
            );
            h.command(Command::AmendOrder {
                command: 95,
                id: "sl-parent__stop_loss".into(),
                price: Some(85.0),
                amount: None,
            });
            let request = h.take("/v5/order/amend");
            let payload: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
            assert_eq!(payload["category"], profile.category());
            assert_eq!(payload["orderId"], "sl-parent");
            assert_eq!(payload["stopLoss"], "85");
            assert_eq!(payload["slTriggerBy"], expected, "{profile:?}/{native:?}");
            assert!(payload.get("tpTriggerBy").is_none());
            assert!(!h.has_request(profile.cancel_path()));
            assert!(!h.has_request("/v5/position/trading-stop"));
        }
    }
}

#[test]
fn partial_stop_without_mark_refuses_without_adding_another_stop() {
    for profile in PROFILES {
        for kind in ["StopLoss", "TakeProfit"] {
            for mark in [None, Some(0.0), Some(f64::NAN)] {
                let mut h = ready(profile);
                seed(&mut h, profile, "Sell", kind, "LastPrice", false, true);
                h.engine.store_mut().update_order("partial", |order| {
                    order.filled = 0.;
                    order.remaining = order.amount;
                });
                let original = h.engine.store().order("partial").unwrap().clone();
                let mut tickers = h.engine.store().tickers().to_vec();
                if let Some(mark) = mark {
                    for ticker in &mut tickers {
                        ticker.mark = mark;
                    }
                } else {
                    tickers.clear();
                }
                h.engine.store_mut().set_tickers(tickers);
                amend(
                    &mut h,
                    Some(if kind == "StopLoss" { 95. } else { 105. }),
                    None,
                );
                assert_guard_refused(&h, "fresh mark");
                assert!(
                    h.requests
                        .iter()
                        .all(|r| r.method == safe_cex_engine::Method::Get)
                );
                assert_eq!(h.engine.store().order("partial"), Some(&original));
            }
        }
    }
}

#[test]
fn partial_stop_stale_mark_is_not_renewed_by_its_fresh_last_source() {
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "LastPrice",
        false,
        true,
    );
    let public = h.socket("/v5/public/linear").unwrap();
    h.event(Event::WsOpened { socket: public });
    h.now += 5001.;
    h.message("/v5/public/linear", r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","lastPrice":"100"}}"#);
    assert_eq!(
        h.engine.store().reference_seen_at(
            "BTCUSDT",
            safe_cex_engine::model::StopTriggerType::LastPrice
        ),
        Some(h.now)
    );
    amend(&mut h, Some(95.), None);
    assert_guard_refused(&h, "fresh mark");
}

#[test]
fn partial_replacement_and_put_back_preserve_each_native_trigger_source() {
    for profile in PROFILES {
        for kind in ["StopLoss", "TakeProfit"] {
            for source in ["LastPrice", "MarkPrice", "IndexPrice"] {
                let mut h = ready(profile);
                seed(&mut h, profile, "Sell", kind, source, false, true);
                h.engine.store_mut().update_order("partial", |order| {
                    order.filled = 0.;
                    order.remaining = order.amount;
                });
                let field = if kind == "StopLoss" {
                    "slTriggerBy"
                } else {
                    "tpTriggerBy"
                };
                amend(
                    &mut h,
                    Some(if kind == "StopLoss" { 95. } else { 105. }),
                    None,
                );
                accept_cancel(&mut h, profile);
                let replacement = h.take("/v5/position/trading-stop");
                let leg: StopWrite = serde_json::from_str(body(&replacement)).unwrap();
                if kind == "StopLoss" {
                    assert_eq!(leg.stop_loss.as_deref(), Some("95"));
                    assert!(leg.take_profit.is_none());
                } else {
                    assert_eq!(leg.take_profit.as_deref(), Some("105"));
                    assert!(leg.stop_loss.is_none());
                }
                let payload: serde_json::Value = serde_json::from_str(body(&replacement)).unwrap();
                assert_eq!(payload[field], source, "{profile:?}/{kind}");
                h.reply(
                    replacement.id,
                    200,
                    r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
                );
                let restore = h.take("/v5/position/trading-stop");
                let payload: serde_json::Value = serde_json::from_str(body(&restore)).unwrap();
                assert_eq!(payload[field], source, "put-back {profile:?}/{kind}");
            }
        }
    }
}

fn assert_applied_restored_partial_failure(h: &Harness) {
    let error = h
        .notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: 91, error } => Some(error),
            _ => None,
        })
        .expect("partial edit failure");
    let elements = error
        .elements
        .as_ref()
        .expect("partial edit indexed outcomes");
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    assert_eq!(elements.len(), 3);
    assert!(
        matches!(&elements[0].result, ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), .. } if id == "partial")
    );
    assert!(matches!(
        &elements[1].result,
        ElementResult::NotSent { error: Some(_) }
    ));
    assert!(matches!(
        &elements[2].result,
        ElementResult::Applied {
            effect: AppliedEffect::TradingStop,
            ..
        }
    ));
    assert!(
        !elements
            .iter()
            .any(|row| matches!(row.result, ElementResult::Placed { .. }))
    );
    assert!(
        !h.requests
            .iter()
            .any(|request| request.method == safe_cex_engine::Method::Post),
        "no suffix, resend or rollback write"
    );
}

#[test]
fn legacy_partial_replacement_preserves_raw_trigger_without_a_new_tick_gate() {
    // TS cancel then default-Market replacement: reference/bybit/bybit.exchange.ts:3191-3225,2883-2891.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | Partial TP/SL cancel-replace, Full conversion and protective put-back
    let profile = PROFILES[0];
    let mut h = ready(profile);
    seed(
        &mut h,
        profile,
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    amend(&mut h, Some(95.049), None);
    accept_cancel(&mut h, profile);
    let request = h.take("/v5/position/trading-stop");
    assert!(body(&request).contains("95.049"));
    h.reply(
        request.id,
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused"}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    h.reply(restore.id, 200, OK);
    assert_applied_restored_partial_failure(&h);
}

fn seed_limit_partial(h: &mut Harness, profile: Profile, kind: &str) {
    seed(h, profile, "Sell", kind, "MarkPrice", false, true);
    let (price, trigger) = if kind == "StopLoss" {
        ("89", "90")
    } else {
        ("111", "110")
    };
    h.message(
        "/v5/private",
        &serde_json::json!({"topic":"order","data":[{
            "category":profile.category(),"symbol":profile.symbol(),"orderId":"partial",
            "side":"Sell","orderType":"Limit","orderStatus":"Untriggered",
            "stopOrderType":kind,"tpslMode":"Partial","qty":"7","cumExecQty":"0",
            "price":price,"leavesQty":"7","triggerPrice":trigger,"triggerBy":"MarkPrice",
            "reduceOnly":true,"closeOnTrigger":true,"positionIdx":0
        }]})
        .to_string(),
    );
}

#[test]
fn legacy_limit_partial_replacement_uses_default_market_execution() {
    // TS cancel then default-Market replacement: reference/bybit/bybit.exchange.ts:3191-3225,2883-2891.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bybit | Partial TP/SL cancel-replace, Full conversion and protective put-back
    for profile in PROFILES {
        for kind in ["StopLoss", "TakeProfit"] {
            for amount in [0.0, 8.0] {
                let mut h = ready(profile);
                seed_limit_partial(&mut h, profile, kind);
                amend(
                    &mut h,
                    Some(if kind == "StopLoss" { 95.0 } else { 105.0 }),
                    Some(amount),
                );
                accept_cancel(&mut h, profile);
                let request = h.take("/v5/position/trading-stop");
                let wire: serde_json::Value = serde_json::from_str(body(&request)).unwrap();
                assert_eq!(
                    wire["tpslMode"],
                    if amount == 0.0 { "Full" } else { "Partial" }
                );
                for field in ["slOrderType", "tpOrderType", "slLimitPrice", "tpLimitPrice"] {
                    assert!(wire.get(field).is_none(), "legacy TS omits {field}");
                }
                h.reply(request.id, 200, OK);
                assert!(matches!(h.result(91), Some(Ok(_))));
            }
        }
    }
}

fn check_unknown_notional_stop(profile: Profile) {
    for value in [20.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = ready(profile);
        seed(
            &mut h,
            profile,
            "Sell",
            "StopLoss",
            "MarkPrice",
            false,
            true,
        );
        h.engine
            .store_mut()
            .update_positions(|p| p.notional = value);
        h.requests.clear();
        h.command(Command::PlaceOrders {
                command: 92,
                orders: vec![serde_json::from_value(serde_json::json!({
                    "symbol":profile.symbol(),"type":"stop_market","side":"sell","amount":7,"price":90,"reduceOnly":true
                })).unwrap()],
            });
        if profile.inverse() && !value.is_finite() {
            assert!(
                h.requests.is_empty(),
                "unknown inverse capacity must refuse: {:?}",
                h.requests
            );
            assert!(h.result(92).unwrap().is_err());
        } else {
            assert!(
                h.requests
                    .iter()
                    .any(|r| r.url.contains("/v5/position/trading-stop"))
            );
        }
    }
}

#[test]
fn unknown_notional_refuses_inverse_stop_selection_before_http() {
    check_unknown_notional_stop(PROFILES[3]);
}
#[test]
fn unknown_notional_refuses_combined_inverse_stop_selection_before_http() {
    check_unknown_notional_stop(PROFILES[4]);
}
#[test]
fn unknown_notional_keeps_linear_stop_selection_in_known_base_units() {
    check_unknown_notional_stop(PROFILES[0]);
}
