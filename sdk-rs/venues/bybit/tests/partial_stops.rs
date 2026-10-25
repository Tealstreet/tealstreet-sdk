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
            ("Sell", "TakeProfit", 100.0, "stopLoss", "slTriggerBy"),
            ("Buy", "StopLoss", 100.0, "takeProfit", "tpTriggerBy"),
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
                        Some(if trigger == "MarkPrice" {
                            "MarkPrice"
                        } else {
                            "LastPrice"
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
        for amount in [
            None,
            Some(9.234),
            Some(0.0),
            Some(if profile.inverse() { 0.4 } else { 0.0004 }),
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
fn partial_amount_only_replaces_shared_family_but_inverse_keeps_its_distinct_fallback() {
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
        let request = if profile.variant == Variant::Inverse {
            assert!(!h.has_request(profile.cancel_path()));
            let request = h.take("/v5/position/trading-stop");
            serde_json::from_str::<StopWrite>(body(&request)).unwrap()
        } else {
            replacement(&mut h, profile)
        };
        if profile.variant == Variant::Inverse {
            assert!(request.tpsl_mode.is_none());
            assert!(request.stop_loss.is_none());
            assert!(request.sl_size.is_none());
            assert!(request.sl_trigger_by.is_none());
        } else {
            assert_eq!(request.stop_loss.as_deref(), Some("90"));
            assert_eq!(
                request.sl_size.as_deref(),
                Some(if profile.inverse() { "9" } else { "9.234" })
            );
            assert_eq!(request.sl_trigger_by.as_deref(), Some("LastPrice"));
        }
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
        h.command(Command::AmendOrder {
            command: 91,
            id: id.into(),
            price: Some(110.0),
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
