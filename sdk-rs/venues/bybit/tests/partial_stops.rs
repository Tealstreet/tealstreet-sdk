//! Owner-decided Partial TP/SL drag semantics against the shared and inverse TS contracts.
//! Payloads follow Bybit V5 order, position and trading-stop wire records.
mod support;

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
    h.message("/v5/private", &format!(r#"{{"topic":"position","data":[{{"category":"{category}","symbol":"{symbol}","side":"Buy","size":"0","avgPrice":"0","markPrice":"100","leverage":"10","positionIdx":{}}}]}}"#, if hedge {1} else {0}));
    h.message("/v5/private", &format!(r#"{{"topic":"order","data":[{{"category":"{category}","symbol":"{symbol}","orderId":"partial","side":"{side}","orderType":"Market","orderStatus":"Untriggered","stopOrderType":"{kind}","tpslMode":"Partial","qty":"7","cumExecQty":"2","cumExecValue":"180","avgPrice":"90","price":"90","leavesQty":"5","triggerPrice":"90","triggerBy":"{trigger}","reduceOnly":{reduce},"closeOnTrigger":true,"positionIdx":{}}}]}}"#, if hedge {1} else {0}));
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
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#,
    );
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
            h.reply(cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
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
    h.reply(cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
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
                let request = replacement(&mut h, profile);
                assert_eq!(request.category, profile.category());
                assert_eq!(request.symbol, profile.symbol());
                if kind == "StopLoss" {
                    assert_eq!(
                        request.sl_trigger_by.as_deref(),
                        Some(if trigger == "LastPrice" {
                            "LastPrice"
                        } else {
                            "MarkPrice"
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
fn missing_mark_fallback_treats_tpsl_not_modified_as_satisfied() {
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
            "LastPrice",
            false,
            true,
        );
        h.engine.store_mut().set_tickers(Vec::new());
        amend(&mut h, Some(100.049), None);
        assert!(!h.has_request(profile.cancel_path()));
        h.respond(
            "/v5/position/trading-stop",
            200,
            r#"{"retCode":34040,"retMsg":"not modified","result":{}}"#,
        );
        assert_eq!(h.result(91), Some(&Ok("[]".into())), "{profile:?}");
        assert!(!h.unknown.contains(&91));
    }
}

#[test]
fn missing_mark_fallback_retains_raw_fields_original_kind_and_ordinary_position_index() {
    for profile in PROFILES {
        for mark in [Some(0.0), Some(f64::NAN), None] {
            for side in ["Sell", "Buy"] {
                let mut h = ready(profile);
                seed(&mut h, profile, side, "StopLoss", "LastPrice", true, false);
                let mut tickers = h.engine.store().tickers().to_vec();
                match mark {
                    Some(mark) => {
                        for ticker in &mut tickers {
                            ticker.mark = mark;
                        }
                    }
                    None => tickers.clear(),
                }
                h.engine.store_mut().set_tickers(tickers);
                amend(&mut h, Some(100.049), Some(9.234));
                assert!(!h.has_request(profile.cancel_path()));
                assert!(!h.has_request("/v5/order/amend"));
                let request = h.take("/v5/position/trading-stop");
                let request: StopWrite = serde_json::from_str(body(&request)).unwrap();
                assert_eq!(request.stop_loss.as_deref(), Some("100.049"));
                assert!(request.take_profit.is_none());
                assert!(request.sl_trigger_by.is_none());
                assert!(request.tp_trigger_by.is_none());
                assert_eq!(
                    request.position_idx,
                    if profile.variant.is_hyro() {
                        0
                    } else if side == "Sell" {
                        2
                    } else {
                        1
                    }
                );
                assert_eq!(
                    request.tpsl_mode.as_deref(),
                    (profile.variant != Variant::Inverse).then_some("Partial")
                );
                assert_eq!(
                    request.sl_size.as_deref(),
                    (profile.variant != Variant::Inverse).then_some("9.234")
                );
            }
        }
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
        h.respond(profile.cancel_path(), 200, r#"{"retCode":0,"retMsg":"OK","retExtInfo":{"list":[{"code":110001,"msg":"order not exists or too late to cancel"}]}}"#);
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
fn partial_fallback_preserves_optional_raw_size_and_does_not_switch_to_full() {
    for profile in PROFILES {
        for (price, amount, level, size) in [
            (Some(100.049), None, Some("100.049"), "7"),
            (None, Some(3.234), None, "3.234"),
            (Some(100.049), Some(0.0), Some("100.049"), "0"),
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
                assert!(!h.has_request("/v5/position/trading-stop"));
                continue;
            }
            let request = h.take("/v5/position/trading-stop");
            let request: StopWrite = serde_json::from_str(body(&request)).unwrap();
            assert_eq!(request.stop_loss.as_deref(), level);
            assert_eq!(
                request.tpsl_mode.as_deref(),
                (profile.variant != Variant::Inverse).then_some("Partial")
            );
            assert_eq!(
                request.sl_size.as_deref(),
                (profile.variant != Variant::Inverse).then_some(size)
            );
            assert!(!h.has_request(profile.cancel_path()));
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
            h.respond(
                profile.cancel_path(),
                200,
                r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
            );
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
            assert_eq!(
                h.result(91).unwrap().is_ok(),
                matches!(outcome, "not-modified" | "accepted")
            );
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
            let refreshed = matches!(outcome, "not-modified" | "accepted")
                && profile.variant != Variant::Inverse;
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
    h.reply(replaced.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
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
}

/// codex-mxb4mq's P1: the cancel finds the old stop already gone (110001); the
/// replacement is resent after a timestamp refusal (10002) and then refused (10001).
/// The old stop is never recreated: it may have filled or triggered.
#[test]
fn a_stop_the_cancel_found_gone_is_never_recreated_even_after_a_resend() {
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
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
    );
    let replacement = h.take("/v5/position/trading-stop");
    h.reply(
        replacement.id,
        200,
        r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param","result":{},"time":1760352369814}"#,
    );
    let resent = h.take("/v5/position/trading-stop");
    assert_eq!(body(&resent), body(&replacement), "an identical resend");
    h.reply(
        resent.id,
        200,
        r#"{"retCode":10001,"retMsg":"StopLoss would trigger immediately","result":{}}"#,
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
fn review_3018_gone_cancel_survives_replacement_signature_resend() {
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
    h.respond(
        profile.cancel_path(),
        200,
        r#"{"retCode":110001,"retMsg":"order not exists or too late to cancel"}"#,
    );
    let replacement = h.take("/v5/position/trading-stop");
    let original_body = body(&replacement).to_owned();
    h.reply(replacement.id,200,r#"{"retCode":10002,"retMsg":"invalid request, please check your server timestamp or recv_window param","result":{}}"#);
    let resend = h.take("/v5/position/trading-stop");
    assert_eq!(body(&resend), original_body);
    h.reply(
        resend.id,
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    assert!(
        !h.has_request("/v5/position/trading-stop"),
        "an already gone stop must not be recreated after a replacement signature resend; requests: {:?}",
        h.requests
            .iter()
            .filter(|r| r.url.contains("trading-stop"))
            .map(|r| r.body.as_deref())
            .collect::<Vec<_>>()
    );
    assert!(matches!(h.result(91), Some(Err(_))));
}
#[test]
fn review_3018_exact_partial_restore_working_control() {
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
        h.reply(first_cancel.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
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
    use safe_cex_engine::{Action, request_context::HttpAction};
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
                    assert!(request.url.contains("/v5/order/cancel"));
                    assert!(preparation.replace(token).is_none());
                    h.prepared_routes.insert(request.id, (token, request.id.0));
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
    h.reply(request.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"partial"},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
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
