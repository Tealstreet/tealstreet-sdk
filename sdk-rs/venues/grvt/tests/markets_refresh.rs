use alloy_dyn_abi::TypedData;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response, TimerId};
use safe_cex_venue_grvt::{Grvt, Options};
use serde_json::{Value, json};
use std::collections::VecDeque;
const NOW: f64 = 1_700_000_000_000.;
const KEY: &str = "0x0101010101010101010101010101010101010101010101010101010101010101";
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
const REFRESH: &str = r#"{"r":[{"i":"ETH_USDT_Perp","ih":"0x030502","b":"ETH","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6},{"i":"BTC_USDT_Perp","ih":"0x030599","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.2","ms":"0.001","mp":"1000","bd":3,"qd":7}]}"#;
fn respond(e: &mut Engine<Grvt>, request: &HttpRequest, body: &str, headers: &str) -> Vec<Action> {
    e.handle(
        NOW + 2.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers,
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, NOW + 2.)
        .into_iter()
        .collect()
}
fn boot() -> (Engine<Grvt>, TimerId) {
    let (e, refresh, _) = boot_with(r#"{"r":[]}"#);
    (e, refresh)
}
/// Boots with a positions snapshot; also returns the private socket.
fn boot_with(positions: &str) -> (Engine<Grvt>, TimerId, Option<safe_cex_engine::SocketId>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
        key: Some("fixture-key".into()),
        secret: Some(KEY.into()),
        trading_account_id: Some("777".into()),
        ..Options::default()
    }));
    e.start(NOW);
    let mut pending: VecDeque<_> = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
        .into_iter()
        .collect();
    let mut refresh = None;
    let mut private = None;
    while let Some(action) = pending.pop_front() {
        if let Action::WsConnect {
            socket, headers, ..
        } = &action
            && !headers.is_empty()
        {
            private = Some(*socket);
            continue;
        }
        if let Action::SetTimer { timer, at_ms } = action {
            if at_ms == NOW + 300_002. {
                refresh = Some(timer);
            }
            continue;
        }
        let Action::Http(request) = action else {
            continue;
        };
        let (body, headers) = if request.url.ends_with("all_instruments") {
            (CATALOG, "")
        } else if request.url.ends_with("/login") {
            (
                r#"{"status":"success"}"#,
                "X-Ts-Grvt-Session: session\nX-Grvt-Account-Id: account",
            )
        } else if request.url.ends_with("aggregated_account_summary") {
            (
                r#"{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"777"}]}}"#,
                "",
            )
        } else if request.url.ends_with("get_all_initial_leverage") {
            (
                r#"{"r":[{"i":"BTC_USDT_Perp","l":"10","ml":"2","ml1":"50"}]}"#,
                "",
            )
        } else if request.url.ends_with("/positions") {
            (positions, "")
        } else if request.url.ends_with("get_authorized_builders") {
            (
                r#"{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}"#,
                "",
            )
        } else {
            (r#"{"r":[]}"#, "")
        };
        pending.extend(respond(&mut e, &request, body, headers));
    }
    (e, refresh.expect("catalog refresh timer"), private)
}
fn refresh(e: &mut Engine<Grvt>, timer: TimerId, body: &str) -> Vec<Action> {
    e.handle(NOW + 300_002., Event::TimerFired { timer });
    let request = safe_cex_engine::test_support::prepared_actions(e, NOW + 300_002.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("all_instruments") => Some(r),
            _ => None,
        })
        .unwrap();
    respond(e, &request, body, "")
}
#[test]
fn refresh_cannot_enable_retired_product_kinds_from_an_unexpected_catalog() {
    let (mut e, timer) = boot();
    let mut catalog = CATALOG.strip_suffix("]}").unwrap().to_owned();
    for (kind, id) in [
        ("FUTURE", "BTC_USDT_Fut_20Oct23"),
        ("CALL", "BTC_USDT_Call_20Oct23_2800"),
        ("PUT", "BTC_USDT_Put_20Oct23_2800"),
    ] {
        catalog.push_str(&format!(r#",{{"i":"{id}","ih":"0x030502","b":"BTC","q":"USDT","k":"{kind}","ts":"0.1","ms":"0.001","bd":9,"qd":6}}"#));
    }
    catalog.push_str("]}");
    let actions = refresh(&mut e, timer, &catalog);
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].id, "BTC_USDT_Perp");
    assert_eq!(e.store().tickers().len(), 1);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("get_all_initial_leverage")))
    );
}
#[test]
fn catalog_refresh_preserves_authenticated_rails_and_fetches_new_market_limits() {
    let (mut e, timer) = boot();
    assert_eq!(
        e.store().market("BTC_USDT_PERP").unwrap().limits.leverage,
        MinMax { min: 2., max: 50. }
    );
    let actions = refresh(&mut e, timer, REFRESH);
    let market = e.store().market("BTC_USDT_PERP").unwrap();
    assert_eq!(market.limits.leverage, MinMax { min: 2., max: 50. });
    assert_eq!(market.precision.price, 0.2); // public metadata still refreshes
    let request = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("get_all_initial_leverage") => Some(r),
            _ => None,
        })
        .expect("new instruments require an account-specific limits read");
    assert_eq!(request.body.as_deref(), Some(r#"{"sa":"777"}"#));
    respond(
        &mut e,
        &request,
        r#"{"r":[{"i":"BTC_USDT_Perp","l":"10","ml":"3","ml1":"40"},{"i":"ETH_USDT_Perp","l":"5","ml":"1","ml1":"25"}]}"#,
        "",
    );
    assert_eq!(
        e.store().market("BTC_USDT_PERP").unwrap().limits.leverage,
        MinMax { min: 3., max: 40. }
    );
    assert_eq!(
        e.store()
            .market("ETH_USDT_PERP")
            .unwrap()
            .limits
            .leverage
            .max,
        25.
    );
}
#[test]
fn catalog_only_refresh_keeps_rails_and_updates_signing_hash_and_decimal_metadata() {
    let (mut e, timer) = boot();
    let metadata_only = REFRESH.replace(r#"{"i":"ETH_USDT_Perp","ih":"0x030502","b":"ETH","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6},"#, "");
    let actions = refresh(&mut e, timer, &metadata_only);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("get_all_initial_leverage")))
    );
    assert_eq!(
        e.store()
            .market("BTC_USDT_PERP")
            .unwrap()
            .limits
            .leverage
            .max,
        50.
    );
    e.handle(
        NOW + 300_003.,
        Event::Command(Command::PlaceOrders {
            command: 100,
            orders: vec![PlaceOrderOpts {
                symbol: "BTC_USDT_PERP".into(),
                kind: OrderType::Limit,
                side: OrderSide::Buy,
                amount: 1.,
                price: Some(100.),
                trigger_price: None,
                stop_loss: None,
                take_profit: None,
                reduce_only: Some(false),
                time_in_force: Some(TimeInForce::GoodTillCancel),
                trigger_type: None,
                tp_trigger_type: None,
                sl_trigger_type: None,
                client_order_id: Some("101".into()),
                close: None,
                stp_mode: None,
                max_slippage_pct: None,
                bybit_bbo: None,
                bybit_slippage_tolerance_ticks: None,
                implied_volatility: None,
            }],
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 300_003.)
        .into_iter()
        .collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    let request = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("create_order") => Some(r),
            _ => None,
        })
        .unwrap();
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let o = &body["o"];
    let s = &o["s"];
    let typed = json!({
        "domain": {"name":"GRVT Exchange","version":"0","chainId":325},
        "primaryType":"OrderWithBuilderFee",
        "types": {
            "EIP712Domain":[{"name":"name","type":"string"},{"name":"version","type":"string"},{"name":"chainId","type":"uint256"}],
            "OrderWithBuilderFee":[{"name":"subAccountID","type":"uint64"},{"name":"isMarket","type":"bool"},{"name":"timeInForce","type":"uint8"},{"name":"postOnly","type":"bool"},{"name":"reduceOnly","type":"bool"},{"name":"legs","type":"OrderLeg[]"},{"name":"builder","type":"address"},{"name":"builderFee","type":"uint32"},{"name":"nonce","type":"uint32"},{"name":"expiration","type":"int64"}],
            "OrderLeg":[{"name":"assetID","type":"uint256"},{"name":"contractSize","type":"uint64"},{"name":"limitPrice","type":"uint64"},{"name":"isBuyingContract","type":"bool"}]
        },
        "message": {"subAccountID":"777","isMarket":false,"timeInForce":1,"postOnly":false,"reduceOnly":false,
            "legs":[{"assetID":"0x030599","contractSize":"1000","limitPrice":"100000000000","isBuyingContract":true}],
            "builder":o["b"],"builderFee":300,"nonce":s["n"],"expiration":s["e"]}
    });
    let typed: TypedData = serde_json::from_value(typed).unwrap();
    let signer: PrivateKeySigner = KEY.parse().unwrap();
    let expected = signer
        .sign_dynamic_typed_data_sync(&typed)
        .unwrap()
        .to_string();
    let actual = format!(
        "0x{}{}{:02x}",
        &s["r"].as_str().unwrap()[2..],
        &s["s1"].as_str().unwrap()[2..],
        s["v"].as_u64().unwrap()
    );
    assert_eq!(
        actual, expected,
        "metadata-only refresh must use the new ih/bd signing units"
    );
}

#[test]
fn newly_listed_market_limits_wait_for_an_inflight_session_refresh() {
    let (mut e, timer) = boot();
    e.handle(
        NOW + 3.,
        Event::Command(Command::Query {
            command: 200,
            query: safe_cex_engine::Query::OrderByClientId {
                symbol: "BTC_USDT_PERP".into(),
                client_order_id: "101".into(),
            },
        }),
    );
    let query = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 3.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/order") => Some(request),
            _ => None,
        })
        .unwrap();
    let rejected = respond(&mut e, &query, r#"{"c":1000,"m":"Session expired"}"#, "");
    let login = rejected
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/login") => Some(request),
            _ => None,
        })
        .expect("confirmed rejection starts a session refresh");
    let actions = refresh(&mut e, timer, REFRESH);
    assert!(e.store().market("ETH_USDT_PERP").is_some());
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(request)
        if request.url.ends_with("get_all_initial_leverage"))),
        "account reads must wait for the refreshed session"
    );
    let actions = respond(
        &mut e,
        &login,
        r#"{"status":"success"}"#,
        "X-Ts-Grvt-Session: refreshed\nX-Grvt-Account-Id: account",
    );
    let leverage = actions
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("get_all_initial_leverage") => {
                Some(request)
            }
            _ => None,
        })
        .expect("the new-market limits read must survive the overlapping login");
    assert!(
        leverage
            .headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("X-Ts-Grvt-Session")
                && value == "refreshed")
    );
    respond(
        &mut e,
        &leverage,
        r#"{"r":[{"i":"BTC_USDT_Perp","l":"10","ml":"2","ml1":"40"},{"i":"ETH_USDT_Perp","l":"5","ml":"1","ml1":"25"}]}"#,
        "",
    );
    assert_eq!(
        e.store()
            .market("BTC_USDT_PERP")
            .unwrap()
            .limits
            .leverage
            .max,
        40.
    );
    assert_eq!(
        e.store()
            .market("ETH_USDT_PERP")
            .unwrap()
            .limits
            .leverage
            .max,
        25.
    );
}
#[test]
fn refresh_with_a_malformed_row_for_a_held_market_keeps_the_market_and_position() {
    let position = r#"{"r":[{"i":"BTC_USDT_Perp","s":"1","ep":"100","n":"100","l":"20","el":"50","up":"0","mt":"CROSS","et":"1700000000000000000"}]}"#;
    // A zero tick fails validation; an object tick fails decoding with the id intact.
    for bad_tick in [r#""ts":"0""#, r#""ts":{}"#] {
        let (mut e, timer, private) = boot_with(position);
        assert_eq!(e.store().positions().len(), 1);
        let before = e.store().market("BTC_USDT_PERP").unwrap().clone();
        let catalog = REFRESH.replace(r#""ts":"0.2""#, bad_tick);
        assert_ne!(catalog, REFRESH);
        refresh(&mut e, timer, &catalog);
        assert_eq!(
            e.store().market("BTC_USDT_PERP"),
            Some(&before),
            "{bad_tick}"
        );
        assert!(e.store().market("ETH_USDT_PERP").is_some());
        assert_eq!(e.store().positions().len(), 1);
        assert_eq!(e.store().positions()[0].symbol, "BTC_USDT_PERP");
        // The next positions snapshot still maps the held market.
        e.handle(
            NOW + 300_010.,
            Event::WsOpened {
                socket: private.expect("private socket"),
            },
        );
        let read = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 300_010.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("/positions") => Some(r),
                _ => None,
            })
            .expect("private open reconciles positions");
        respond(&mut e, &read, position, "");
        assert_eq!(e.store().positions().len(), 1, "{bad_tick}");
    }
}
