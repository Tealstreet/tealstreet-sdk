//! Documented PerpetualV2 envelopes and aop_p execution/position payloads.
mod support;
use safe_cex_engine::model::{MarginType, OrderStatus, PositionMode};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, RequestId, Response, SocketId, TimerId,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

const NOW: f64 = 1_700_000_000_000.;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"},{"symbol":"ETHUSDC","type":"PerpetualV2","settleCurrency":"USDC","baseCurrency":"ETH","quoteCurrency":"USDC","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1000","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"result":[{"symbol":"BTCUSDT","bidRp":"69999","askRp":"70001","lastRp":"70000","markRp":"70000","indexRp":"70000","openRp":"69000","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"},{"symbol":"ETHUSDC","bidRp":"1999","askRp":"2001","lastRp":"2000","markRp":"2000","indexRp":"2000","openRp":"1900","openInterestRv":"100","fundingRateRr":"0","volumeRq":"10"}]}"#;
const BTC: &str = r#"{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","side":"Buy","size":"0.01","leverageRr":"-20","avgEntryPriceRp":"69000","liquidationPriceRp":"50000","transactTimeNs":1700000000000000000}"#;
const ETH: &str = r#"{"symbol":"ETHUSDC","currency":"USDC","posMode":"Hedged","posSide":"Long","side":"Buy","size":"1","leverageRr":"10","avgEntryPriceRp":"1900","liquidationPriceRp":"1000","transactTimeNs":1700000000000000000}"#;
const FLAT: &str = r#"{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","size":"0","leverageRr":"-30"}"#;
fn execution_rows(frame: &[u8], expected_count: usize) -> &[u8] {
    let mut at = 16;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let length = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == safe_cex_engine::wire::section::FILLS {
            assert_eq!(
                u16::from_le_bytes(frame[at + 2..at + 4].try_into().unwrap()),
                2
            );
            let payload = &frame[at + 8..at + 8 + length];
            let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
            assert_eq!(count, expected_count);
            let rows = (4 + 12 * count).next_multiple_of(8);
            return &payload[rows..rows + 16 * count];
        }
        at += 8 + length;
    }
    panic!("missing execution fills");
}
fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}
fn response(e: &mut Engine<PhemexPerpetuals>, now: f64, id: RequestId, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn request(actions: &[Action], query: &str) -> HttpRequest {
    requests(actions)
        .into_iter()
        .find(|r| r.url.contains(query))
        .unwrap()
}
fn envelope(currency: &str, total: f64, used: f64, rows: &str) -> String {
    format!(
        r#"{{"code":0,"data":{{"account":{{"currency":"{currency}","accountBalanceRv":"{total}","totalUsedBalanceRv":"{used}"}},"positions":[{rows}]}}}}"#
    )
}
fn started() -> (Engine<PhemexPerpetuals>, Vec<Action>, SocketId) {
    let mut e = support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    e.start(NOW);
    let a: Vec<_> = e.drain_actions().collect();
    response(&mut e, NOW, request(&a, "products").id, 200, PRODUCTS);
    let mut a: Vec<_> = e.drain_actions().collect();
    response(&mut e, NOW, request(&a, "24hr/all").id, 200, DAILY);
    a.extend(e.drain_actions());
    let socket = a
        .iter()
        .filter_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    (e, a, socket)
}
fn seed(e: &mut Engine<PhemexPerpetuals>, actions: &[Action]) {
    response(
        e,
        NOW,
        request(actions, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., BTC),
    );
    response(
        e,
        NOW,
        request(actions, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
}
fn authenticate(e: &mut Engine<PhemexPerpetuals>, socket: SocketId) {
    e.handle(NOW, Event::WsOpened { socket });
    e.handle(
        NOW,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"error":null,"result":{"status":"success"}}"#,
        },
    );
}
fn ws(e: &mut Engine<PhemexPerpetuals>, socket: SocketId, text: &str) {
    e.handle(NOW + 100., Event::WsMessage { socket, text });
}
fn timer(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (*at_ms == at).then_some(*timer)
            } else {
                None
            }
        })
        .next_back()
        .unwrap()
}
fn order(status: &str, time: u64) -> String {
    format!(
        r#"{{"orderID":"one","symbol":"BTCUSDT","side":"Buy","posSide":"Merged","ordType":"Limit","ordStatus":"{status}","orderQty":"0.3","cumQty":"0","leavesQty":"0.3","priceRp":"69000","timeInForce":"PostOnly","transactTimeNs":{time}}}"#
    )
}
fn push_orders(
    e: &mut Engine<PhemexPerpetuals>,
    socket: SocketId,
    kind: &str,
    sequence: u64,
    rows: &str,
) {
    ws(
        e,
        socket,
        &format!(r#"{{"type":"{kind}","sequence":{sequence},"orders_p":[{rows}]}}"#),
    );
}
#[test]
fn rest_and_ws_balance_conventions_match_across_currencies_and_pnl_signs() {
    use safe_cex_engine::model::Balance;
    for (btc_pnl, eth_pnl) in [(10., 100.), (-10., -100.), (10., -100.), (-10., 100.)] {
        let (mut e, a, socket) = started();
        let btc = BTC.replace("69000", if btc_pnl > 0. { "69000" } else { "71000" });
        let eth = ETH.replace("1900", if eth_pnl > 0. { "1900" } else { "2100" });
        response(
            &mut e,
            NOW,
            request(&a, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 69., &btc),
        );
        assert_eq!(
            e.store().balance(),
            &Balance {
                total: 1000. - btc_pnl,
                free: 931.,
                used: 69.,
                upnl: btc_pnl,
            }
        );
        response(
            &mut e,
            NOW,
            request(&a, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 19., &eth),
        );
        let expected = Balance {
            total: 1200. - btc_pnl - eth_pnl,
            free: 1112.,
            used: 88.,
            upnl: btc_pnl + eth_pnl,
        };
        assert_eq!(e.store().balance(), &expected);
        authenticate(&mut e, socket);
        ws(
            &mut e,
            socket,
            r#"{"timestamp":1700000001000000000,"accounts_p":[{"currency":"USDT","accountBalanceRv":"1000","totalUsedBalanceRv":"69"},{"currency":"USDC","accountBalanceRv":"200","totalUsedBalanceRv":"19"}]}"#,
        );
        assert_eq!(e.store().balance(), &expected);
        // A fresh periodic REST snapshot after that push has equivalent values.
        let a = refresh(&mut e, NOW + 2000.);
        for (currency, total, used, row) in [("USDT", 1000., 69., &btc), ("USDC", 200., 19., &eth)]
        {
            response(
                &mut e,
                NOW + 2000.,
                request(&a, &format!("currency={currency}")).id,
                200,
                &envelope(currency, total, used, row),
            );
        }
        assert_eq!(e.store().balance(), &expected);
        let a = refresh(&mut e, NOW + 4000.);
        response(
            &mut e,
            NOW + 4000.,
            request(&a, "currency=USDT").id,
            200,
            &envelope("USDT", 1100., 69., &btc),
        );
        response(
            &mut e,
            NOW + 4000.,
            request(&a, "currency=USDC").id,
            503,
            "unavailable",
        );
        assert_eq!(
            e.store().balance(),
            &Balance {
                total: expected.total + 100.,
                free: expected.free + 100.,
                ..expected
            }
        );
        assert_eq!(e.store().positions().len(), 2);
    }
}

#[test]
fn rest_and_account_push_publish_per_currency_balances() {
    use safe_cex_engine::model::{Balance, CurrencyBalance};
    let currency = |currency: &str, total: f64, used: f64, free: f64, upnl: f64| CurrencyBalance {
        currency: currency.into(),
        balance: Balance {
            total,
            used,
            free,
            upnl,
        },
    };
    let (mut e, a, socket) = started();
    seed(&mut e, &a);
    let usdc = currency("USDC", 100., 19., 181., 100.);
    assert_eq!(
        e.store().currency_balances().unwrap(),
        &[usdc.clone(), currency("USDT", 990., 69., 931., 10.)]
    );
    authenticate(&mut e, socket);
    // The negative-used USDC row is rejected; only USDT moves.
    ws(
        &mut e,
        socket,
        r#"{"timestamp":1700000001000000000,"accounts_p":[{"currency":"USDT","accountBalanceRv":"1100","totalUsedBalanceRv":"70"},{"currency":"USDC","accountBalanceRv":"5","totalUsedBalanceRv":"-1"}]}"#,
    );
    assert_eq!(
        e.store().currency_balances().unwrap(),
        &[usdc, currency("USDT", 1090., 70., 1030., 10.)]
    );
    assert_eq!(e.store().balance().total, 1190.);
}

#[test]
fn independent_currency_failure_keeps_prior_slice_and_flat_settings() {
    let (mut e, a, _) = started();
    seed(&mut e, &a);
    assert_eq!(e.store().balance().total, 1090.);
    assert_eq!(e.store().balance().upnl, 110.);
    assert_eq!(e.store().positions().len(), 2);
    assert_eq!(
        e.store().position("BTCUSDT").unwrap().position_mode,
        Some(PositionMode::Oneway)
    );
    assert_eq!(
        e.store().position("ETHUSDC:long").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
    let a: Vec<_> = e.drain_actions().collect();
    e.handle(
        NOW + 2000.,
        Event::TimerFired {
            timer: timer(&a, NOW + 2000.),
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    response(
        &mut e,
        NOW + 2000.,
        request(&a, "currency=USDT").id,
        200,
        &envelope("USDT", 500., 0., FLAT),
    );
    response(
        &mut e,
        NOW + 2000.,
        request(&a, "currency=USDC").id,
        503,
        "unavailable",
    );
    assert_eq!(e.store().balance().total, 600.);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "ETHUSDC");
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 30.);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(MarginType::Cross)
    );
}
#[test]
fn streamed_partial_success_never_publishes_before_valid_envelope() {
    let (mut e, a, _) = started();
    let r = request(&a, "currency=USDT");
    e.handle(
        NOW,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW,
        Event::HttpChunk {
            request: r.id,
            data: format!(r#"{{"data":{{"positions":[{BTC}]}}"#).as_bytes(),
        },
    );
    assert!(e.store().positions().is_empty());
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "broken stream",
        },
    );
    assert!(e.store().positions().is_empty());
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
    assert_eq!(e.store().balance().total, 100.);
    assert_eq!(e.store().positions()[0].symbol, "ETHUSDC");
}
#[test]
fn missing_positions_is_failure_but_a_bad_row_keeps_the_slice_balance() {
    let (mut e, a, _) = started();
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDT").id,
        200,
        r#"{"code":0,"data":{"account":{"currency":"USDT","accountBalanceRv":"1000","totalUsedBalanceRv":"0"}}}"#,
    );
    assert!(!e.store().loaded().positions);
    assert_eq!(e.store().balance().total, 0.);
    // A NaN row skips only that symbol; the USDC balance still updates.
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., &ETH.replace("\"1\"", "\"NaN\"")),
    );
    assert!(e.store().position("ETHUSDC").is_none());
    assert_eq!(e.store().balance().used, 19.);
}
#[test]
fn bad_position_row_keeps_the_stored_position_and_still_refreshes_the_balance() {
    let (mut e, a, _) = started();
    seed(&mut e, &a);
    let held = e.store().position("BTCUSDT").unwrap().contracts;
    let refresh = refresh(&mut e, NOW + 2_000.);
    response(
        &mut e,
        NOW + 2_000.,
        request(&refresh, "currency=USDT").id,
        200,
        &envelope("USDT", 5000., 77., &BTC.replace("69000", "0")),
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, held);
    assert_eq!(e.store().balance().used, 77. + 19.);
}
#[test]
fn private_auth_snapshot_and_triggered_are_nonterminal() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        a.iter()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("aop_p.subscribe")))
    );
    push_orders(
        &mut e,
        s,
        "snapshot",
        1,
        &order("New", 1_700_000_000_000_000_001),
    );
    let loaded = e.store().loaded();
    assert!(
        loaded.orders && loaded.balance && loaded.positions && loaded.markets && loaded.tickers
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    push_orders(
        &mut e,
        s,
        "incremental",
        2,
        &order("Triggered", 1_700_000_000_000_000_002),
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    push_orders(
        &mut e,
        s,
        "incremental",
        3,
        &order("New", 1_700_000_000_000_000_003),
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    push_orders(
        &mut e,
        s,
        "incremental",
        4,
        &order("Filled", 1_700_000_000_000_000_004),
    );
    push_orders(
        &mut e,
        s,
        "incremental",
        5,
        &order("New", 1_700_000_000_000_000_005),
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
}
#[test]
fn execution_fills_deduplicate_individually_even_after_terminal_row() {
    let (mut e, _, s) = started();
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, "");
    let first=order("Filled",1_700_000_000_000_000_001).replace("\"cumQty\":\"0\"",r#""cumQty":"0.3","execID":"exec-1","execQty":"0.1","execPriceRp":"69000","tradeType":"Trade""#);
    let second = first
        .replace("exec-1", "exec-2")
        .replace("\"execQty\":\"0.1\"", "\"execQty\":\"0.2\"")
        .replace("\"execPriceRp\":\"69000\"", "\"execPriceRp\":\"69100\"");
    push_orders(&mut e, s, "incremental", 2, &format!("{first},{second}"));
    push_orders(&mut e, s, "incremental", 3, &format!("{first},{second}"));
    // Fills travel with their orders: the book-cadence flush carries both.
    let frame = e.take_market_frame(NOW).unwrap();
    let data = execution_rows(frame, 2);
    assert_eq!(f64::from_le_bytes(data[..8].try_into().unwrap()), 69000.);
    assert_eq!(f64::from_le_bytes(data[8..16].try_into().unwrap()), 0.1);
    assert_eq!(f64::from_le_bytes(data[16..24].try_into().unwrap()), 69100.);
    assert_eq!(f64::from_le_bytes(data[24..].try_into().unwrap()), 0.2);
}
#[test]
fn newer_private_position_prevents_inflight_rest_rollback() {
    let (mut e, a, s) = started();
    authenticate(&mut e, s);
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"sequence":1,"positions_p":[{}]}}"#,
            BTC.replace("0.01", "0.02")
        ),
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.02);
    seed(&mut e, &a);
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.02);
    assert_eq!(e.store().position("ETHUSDC:long").unwrap().contracts, 1.);
}
#[test]
fn older_private_zero_cannot_close_fresher_rest_position() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"sequence":1,"positions_p":[{}]}}"#,
            BTC.replace("0.01", "0")
                .replace("1700000000000000000", "1699999999999999999")
        ),
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.01);
}
#[test]
fn deadline_revokes_late_generation_and_starts_fresh_reads() {
    let (mut e, a, _) = started();
    let expired = request(&a, "currency=USDC");
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., BTC),
    );
    e.handle(
        NOW + 20000.,
        Event::TimerFired {
            timer: timer(&a, NOW + 20000.),
        },
    );
    response(
        &mut e,
        NOW + 20001.,
        expired.id,
        200,
        &envelope("USDC", 999., 0., ETH),
    );
    assert_eq!(e.store().balance().total, 990.);
    let a: Vec<_> = e.drain_actions().collect();
    e.handle(
        NOW + 22000.,
        Event::TimerFired {
            timer: timer(&a, NOW + 22000.),
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(requests(&a).len(), 2);
    response(
        &mut e,
        NOW + 22000.,
        request(&a, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
    assert_eq!(e.store().balance().total, 1090.);
}
#[test]
fn dead_private_socket_reconnects_and_auth_uses_fresh_expiry() {
    let (mut e, _, s) = started();
    e.handle(NOW, Event::WsOpened { socket: s });
    e.handle(
        NOW,
        Event::WsMessage {
            socket: s,
            text: r#"{"id":999,"error":null,"result":{"status":"success"}}"#,
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    let mut next = timer(&a, NOW + 5000.);
    for delta in [5000., 10000.] {
        e.handle(NOW + delta, Event::TimerFired { timer: next });
        let a: Vec<_> = e.drain_actions().collect();
        assert!(
            a.iter()
                .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("server.ping")))
        );
        next = timer(
            &a,
            NOW + if delta < 10000. {
                delta + 5000.
            } else {
                12500.
            },
        );
    }
    e.handle(NOW + 12500., Event::TimerFired { timer: next });
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        a.iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==s))
    );
    let (reconnect, at) = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (NOW + 13250. ..=NOW + 13500.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer: reconnect });
    let a: Vec<_> = e.drain_actions().collect();
    let fresh = a
        .iter()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    assert_ne!(s, fresh);
    e.handle(at, Event::WsOpened { socket: fresh });
    let expiry = (((at + 60_000.) / 1000.) + 0.5).floor().to_string();
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains(&expiry)))
    );
}

#[test]
fn documented_dual_qty_bootstrap_loads_all_private_flags() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    let row = order("New", 4_000_000_000_000_000_000).replace(
        "\"orderQty\":\"0.3\"",
        r#""orderQty":"0.3","orderQtyRq":"0.3","cumQtyRq":"0","leavesQtyRq":"0.3""#,
    );
    push_orders(&mut e, s, "snapshot", 1, &row);
    assert_eq!(e.store().orders().len(), 1);
    let loaded = e.store().loaded();
    assert!(
        loaded.orders && loaded.positions && loaded.balance && loaded.markets && loaded.tickers
    );
}
#[test]
fn malformed_snapshot_cannot_poison_terminal_cache_or_sequence() {
    let (mut e, _, s) = started();
    authenticate(&mut e, s);
    let malformed = order("New", 100)
        .replace("\"one\"", "\"bad\"")
        .replace("\"orderQty\":\"0.3\"", "\"orderQty\":\"-1\"");
    push_orders(
        &mut e,
        s,
        "snapshot",
        1,
        &format!("{},{}", order("Filled", 100), malformed),
    );
    assert!(!e.store().loaded().orders);
    push_orders(&mut e, s, "snapshot", 1, &order("New", 101));
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}
#[test]
fn stale_balance_without_sequence_does_not_undo_newer_account_frame() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    ws(
        &mut e,
        s,
        r#"{"timestamp":1700000000000000100,"accounts_p":[{"currency":"USDT","accountBalanceRv":"2000","totalUsedBalanceRv":"10"}]}"#,
    );
    let newer = e.store().balance().clone();
    ws(
        &mut e,
        s,
        r#"{"timestamp":1700000000000000099,"accounts_p":[{"currency":"USDT","accountBalanceRv":"100","totalUsedBalanceRv":"1"}]}"#,
    );
    assert_eq!(e.store().balance(), &newer);
}
fn amend(e: &mut Engine<PhemexPerpetuals>) -> Vec<Action> {
    e.drain_actions().for_each(drop);
    e.handle(
        NOW + 200.,
        Event::Command(Command::AmendOrder {
            command: 99,
            id: "one".into(),
            price: Some(69010.),
            amount: None,
        }),
    );
    e.drain_actions().collect()
}
#[test]
fn complete_fill_row_reconciles_order_and_keeps_future_amends_available() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, &order("New", 1));
    let filled=order("PartiallyFilled",2).replace("\"cumQty\":\"0\"",r#""cumQty":"0.1","execID":"fill-one","execQty":"0.1","execPriceRp":"69000","tradeType":"Trade""#).replace("\"leavesQty\":\"0.3\"","\"leavesQty\":\"0.2\"");
    push_orders(&mut e, s, "incremental", 2, &filled);
    assert_eq!(e.store().order("one").unwrap().filled, 0.1);
    assert!(
        requests(&amend(&mut e))
            .iter()
            .any(|r| r.method != safe_cex_engine::Method::Get)
    );
}
#[test]
fn incomplete_fill_blocks_amend_until_same_order_cumulative_update() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, &order("New", 1));
    let filled = order("PartiallyFilled", 2)
        .replace(
            "\"cumQty\":\"0\",",
            r#""execID":"fill-one","execQty":"0.1","execPriceRp":"69000","tradeType":"Trade","#,
        )
        .replace("\"leavesQty\":\"0.3\",", "");
    push_orders(&mut e, s, "incremental", 2, &filled);
    let actions = amend(&mut e);
    assert!(requests(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { error, .. })
            if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
    )));
    let cumulative = order("PartiallyFilled", 3)
        .replace("\"cumQty\":\"0\"", "\"cumQty\":\"0.1\"")
        .replace("\"leavesQty\":\"0.3\"", "\"leavesQty\":\"0.2\"");
    push_orders(&mut e, s, "incremental", 3, &cumulative);
    assert!(
        requests(&amend(&mut e))
            .iter()
            .any(|r| r.method != safe_cex_engine::Method::Get)
    );
}

#[test]
fn native_versions_take_precedence_over_fresh_outer_timestamp() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, &order("New", 100));
    push_orders(
        &mut e,
        s,
        "incremental",
        2,
        &order("New", 200).replace("69000", "70000"),
    );
    let stale_position = BTC
        .replace("0.01", "0")
        .replace("1700000000000000000", "1699999999999999999");
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"sequence":3,"timestamp":1700000000000000300,"orders_p":[{}],"positions_p":[{stale_position}]}}"#,
            order("New", 100)
        ),
    );
    assert_eq!(e.store().order("one").unwrap().price, 70000.);
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.01);
}

#[test]
fn stale_hedge_leg_cannot_supply_context_to_a_fresh_opposite_leg() {
    let base = 1_700_000_000_000_000_000u64;
    let long = |leverage: u32, version: u64| {
        BTC.replace("OneWay", "Hedged")
            .replace("Merged", "Long")
            .replace("\"-20\"", &format!("\"{leverage}\""))
            .replace(&base.to_string(), &version.to_string())
    };
    // A row may be stale against a previous frame or against an earlier row
    // in this same frame. Neither can change the accepted opposite-side rail.
    for same_frame in [false, true] {
        let (mut e, a, socket) = started();
        let seed_long = long(
            if same_frame { 10 } else { 20 },
            base + if same_frame { 50 } else { 300 },
        );
        response(
            &mut e,
            NOW,
            request(&a, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 69., &seed_long),
        );
        response(
            &mut e,
            NOW,
            request(&a, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 19., ETH),
        );
        authenticate(&mut e, socket);
        let stale_long = long(5, base + 100);
        let fresh_short = long(15, base + 400)
            .replace("\"Long\"", "\"Short\"")
            .replace("\"Buy\"", "\"Sell\"");
        let rows = if same_frame {
            format!("{},{stale_long},{fresh_short}", long(20, base + 300))
        } else {
            format!("{stale_long},{fresh_short}")
        };
        ws(
            &mut e,
            socket,
            &format!(r#"{{"sequence":1,"positions_p":[{rows}]}}"#),
        );
        assert_eq!(e.store().position("BTCUSDT:long").unwrap().leverage, 20.);
        assert_eq!(e.store().position("BTCUSDT:short").unwrap().leverage, 15.);
        assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.);
        assert_eq!(
            e.store().setting("BTCUSDT").unwrap().margin_type,
            Some(MarginType::Isolated)
        );
    }
}

#[test]
fn untimestamped_flat_cannot_erase_a_new_live_row_in_the_same_frame() {
    let (mut e, actions, socket) = started();
    response(
        &mut e,
        NOW,
        request(&actions, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 0., ""),
    );
    response(
        &mut e,
        NOW,
        request(&actions, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
    authenticate(&mut e, socket);
    ws(
        &mut e,
        socket,
        &format!(r#"{{"sequence":1,"positions_p":[{BTC},{FLAT}]}}"#),
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.01);
    assert_eq!(e.store().position("BTCUSDT").unwrap().leverage, 20.);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.);
}

#[test]
fn rejected_position_cannot_supply_the_only_mode_for_snapshot_authority() {
    let (mut e, _, socket) = started();
    authenticate(&mut e, socket);
    let long = BTC
        .replace("OneWay", "Hedged")
        .replace("Merged", "Long")
        .replace("1700000000000000000", "0");
    let short = long
        .replace("\"posMode\":\"Hedged\",", "")
        .replace("\"Long\"", "\"Short\"")
        .replace("\"Buy\"", "\"Sell\"")
        .replace("\"transactTimeNs\":0", "\"transactTimeNs\":2");
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"type":"snapshot","sequence":1,"orders_p":[],"positions_p":[{long},{short}]}}"#
        ),
    );
    // The rest of the frame applies, but the unmodelled leg borrows no mode.
    assert!(e.store().loaded().orders);
    assert!(e.store().positions().is_empty());
    assert!(e.store().setting("BTCUSDT").is_none());
    let corrected_long = long.replace("\"transactTimeNs\":0", "\"transactTimeNs\":1");
    let corrected_short = short.replace("\"posSide\"", "\"posMode\":\"Hedged\",\"posSide\"");
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"type":"incremental","sequence":2,"positions_p":[{corrected_long},{corrected_short}]}}"#
        ),
    );
    assert_eq!(e.store().positions().len(), 2);
}

fn refresh(e: &mut Engine<PhemexPerpetuals>, at: f64) -> Vec<Action> {
    let actions: Vec<_> = e.drain_actions().collect();
    e.handle(
        at,
        Event::TimerFired {
            timer: timer(&actions, at),
        },
    );
    e.drain_actions().collect()
}

#[test]
fn complete_rest_omission_fences_same_version_ws_but_allows_old_active_rest() {
    // Native exchange timestamps need not agree with the local clock. Both an
    // ordinary clock and a future exchange clock must fence omitted positions.
    for timestamp in ["1700000000000000000", "4000000000000000000"] {
        let (mut e, a, s) = started();
        let btc = BTC.replace("1700000000000000000", timestamp);
        response(
            &mut e,
            NOW,
            request(&a, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 69., &btc),
        );
        response(
            &mut e,
            NOW,
            request(&a, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 19., ETH),
        );
        authenticate(&mut e, s);
        let a = refresh(&mut e, NOW + 2000.);
        response(
            &mut e,
            NOW + 2000.,
            request(&a, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 0., ""),
        );
        response(
            &mut e,
            NOW + 2000.,
            request(&a, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 19., ETH),
        );
        assert!(e.store().position("BTCUSDT").is_none());
        ws(
            &mut e,
            s,
            &format!(r#"{{"sequence":1,"positions_p":[{btc}]}}"#),
        );
        assert!(
            e.store().position("BTCUSDT").is_none(),
            "stale omitted position reopened"
        );
        let a = refresh(&mut e, NOW + 4000.);
        response(
            &mut e,
            NOW + 4000.,
            request(&a, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 69., &btc),
        );
        response(
            &mut e,
            NOW + 4000.,
            request(&a, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 19., ETH),
        );
        assert_eq!(
            e.store().position("BTCUSDT").unwrap().contracts,
            0.01,
            "authoritative REST must accept genuinely active old native timestamps"
        );
    }
}

#[test]
fn invalid_semantic_rows_are_skipped_without_blocking_the_frame() {
    for bad_row in [
        format!(r#""positions_p":[{}]"#, BTC.replace("69000", "0")),
        r#""accounts_p":[{"currency":"USDT","accountBalanceRv":"9000","totalUsedBalanceRv":"-1"}]"#
            .into(),
    ] {
        let (mut e, a, s) = started();
        seed(&mut e, &a);
        authenticate(&mut e, s);
        let balance = e.store().balance().clone();
        ws(
            &mut e,
            s,
            &format!(r#"{{"type":"snapshot","sequence":1,"orders_p":[],{bad_row}}}"#),
        );
        // The bad row is skipped; the frame (here the order snapshot) still applies.
        assert!(e.store().loaded().orders);
        assert_eq!(e.store().balance(), &balance);
        assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.01);
        ws(
            &mut e,
            s,
            &format!(
                r#"{{"type":"incremental","sequence":2,"orders_p":[{}],"positions_p":[{}]}}"#,
                order("New", 1),
                BTC.replace("0.01", "0.02")
                    .replace("1700000000000000000", "1700000000000000009")
            ),
        );
        assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
        assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.02);
    }
}

#[test]
fn stale_cumulative_row_with_new_outer_time_does_not_clear_fill_uncertainty() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, &order("New", 1));
    let sparse = order("PartiallyFilled", 20)
        .replace(
            "\"cumQty\":\"0\",",
            r#""execID":"sparse","execQty":"0.1","execPriceRp":"69000","tradeType":"Trade","#,
        )
        .replace("\"leavesQty\":\"0.3\",", "");
    push_orders(&mut e, s, "incremental", 2, &sparse);
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"sequence":3,"timestamp":30,"orders_p":[{}]}}"#,
            order("New", 10)
        ),
    );
    assert!(requests(&amend(&mut e)).is_empty());
    let cumulative = order("PartiallyFilled", 20)
        .replace("\"cumQty\":\"0\"", "\"cumQty\":\"0.1\"")
        .replace("\"leavesQty\":\"0.3\"", "\"leavesQty\":\"0.2\"");
    push_orders(&mut e, s, "incremental", 4, &cumulative);
    assert!(!requests(&amend(&mut e)).is_empty());
}

#[test]
fn late_unique_execution_after_terminal_emits_fill_without_reopening_order() {
    let (mut e, _, s) = started();
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, "");
    let terminal = order("Filled", 20).replace("\"cumQty\":\"0\"",
        r#""cumQty":"0.3","execID":"second","execQty":"0.2","execPriceRp":"69100","tradeType":"Trade""#);
    push_orders(&mut e, s, "incremental", 2, &terminal);
    let late = terminal
        .replace("second", "first")
        .replace("Filled", "PartiallyFilled")
        .replace("\"execQty\":\"0.2\"", "\"execQty\":\"0.1\"")
        .replace("\"execPriceRp\":\"69100\"", "\"execPriceRp\":\"69000\"")
        .replace("\"transactTimeNs\":20", "\"transactTimeNs\":19");
    push_orders(&mut e, s, "incremental", 3, &late);
    push_orders(&mut e, s, "incremental", 4, &late);
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    // Fills travel with their orders: the book-cadence flush carries both.
    let frame = e.take_market_frame(NOW).unwrap();
    let data = execution_rows(frame, 2);
    assert_eq!(f64::from_le_bytes(data[..8].try_into().unwrap()), 69100.);
    assert_eq!(f64::from_le_bytes(data[8..16].try_into().unwrap()), 0.2);
    assert_eq!(f64::from_le_bytes(data[16..24].try_into().unwrap()), 69000.);
    assert_eq!(f64::from_le_bytes(data[24..].try_into().unwrap()), 0.1);
}

#[test]
fn rest_omission_fence_accepts_new_native_opening_when_exchange_clock_lags() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    let a = refresh(&mut e, NOW + 2000.);
    response(
        &mut e,
        NOW + 2000.,
        request(&a, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 0., ""),
    );
    response(
        &mut e,
        NOW + 2000.,
        request(&a, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
    assert!(e.store().position("BTCUSDT").is_none());
    let opening = BTC.replace("1700000000000000000", "1700000001500000000");
    e.handle(
        NOW + 2000.,
        Event::WsMessage {
            socket: s,
            text: &format!(r#"{{"sequence":1,"positions_p":[{opening}]}}"#),
        },
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.01);
}

#[test]
fn rest_balance_accepts_new_native_version_behind_client_clock_and_fences_replay() {
    let (mut e, a, socket) = started();
    seed(&mut e, &a);
    authenticate(&mut e, socket);
    let initial = r#"{"timestamp":1700000001000000000,"accounts_p":[{"currency":"USDT","accountBalanceRv":"2000","totalUsedBalanceRv":"10"}]}"#;
    e.handle(
        NOW + 1000.,
        Event::WsMessage {
            socket,
            text: initial,
        },
    );
    let a = refresh(&mut e, NOW + 2000.);
    response(
        &mut e,
        NOW + 2000.,
        request(&a, "currency=USDT").id,
        200,
        &envelope("USDT", 3000., 20., BTC),
    );
    response(
        &mut e,
        NOW + 2000.,
        request(&a, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., ETH),
    );
    let authoritative = e.store().balance().clone();
    assert_eq!(authoritative.total, 3090.);
    e.handle(
        NOW + 2000.,
        Event::WsMessage {
            socket,
            text: initial,
        },
    );
    assert_eq!(
        e.store().balance(),
        &authoritative,
        "equal native replay undid REST"
    );
    e.handle(NOW + 2000., Event::WsMessage { socket, text:
        r#"{"timestamp":1700000000999999999,"accounts_p":[{"currency":"USDT","accountBalanceRv":"100","totalUsedBalanceRv":"1"}]}"# });
    assert_eq!(
        e.store().balance(),
        &authoritative,
        "older native replay undid REST"
    );
    e.handle(NOW + 2000., Event::WsMessage { socket, text:
        r#"{"timestamp":1700000001500000000,"accounts_p":[{"currency":"USDT","accountBalanceRv":"4000","totalUsedBalanceRv":"30"}]}"# });
    assert_eq!(e.store().balance().used, 49.);
    assert_eq!(e.store().balance().total, 4090.);
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) => panic!("policy updates must not issue account reads"),
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1_501.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_500.0);
    engine.handle(
        now + 91_500.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    let reads: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_501.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let (mut e, initial, _) = started();
    seed(&mut e, &initial);
    let engine = &mut e;
    let (now, mut reads) = policy_poll_read(engine, NOW, "accountPositions");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.contains("USDT") {
                envelope("USDT", 1000., 69., BTC)
            } else {
                envelope("USDC", 200., 19., ETH)
            };
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body: &body,
                    },
                },
            );
        }
        reads = engine
            .drain_actions()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}

#[test]
fn failure_poll_floor_survives_fast_policy_and_pause_resume() {
    let (mut e, initial, _) = started();
    seed(&mut e, &initial);
    let engine = &mut e;
    let now = NOW;
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let timer = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == now + 1_500.0 => Some(timer),
            _ => None,
        })
        .expect("fast account poll");
    engine.handle(now + 1_500.0, Event::TimerFired { timer });
    let output: Vec<_> = engine.drain_actions().collect();
    let reads: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert!(!reads.is_empty());
    // Keep any next periodic timer for a loop that rearms before HTTP completes.
    for r in reads {
        engine.handle(
            now + 1_500.0,
            Event::HttpFailed {
                request: r.id,
                error: "account unavailable",
            },
        );
    }
    engine.drain_actions().for_each(drop);
    // The jittered failure floor is fixed once and must survive every policy change.
    let mut floor = None;
    for (offset, interval_ms) in [(1.0, 1_000.0), (2.0, f64::INFINITY), (3.0, 1_200.0)] {
        engine.handle(
            now + 1_500.0 + offset,
            Event::PollScaleChanged {
                scale: interval_ms / POLL_MS,
            },
        );
        let times: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        if interval_ms.is_infinite() {
            assert!(times.is_empty());
        } else {
            let at = *times.first().expect("one rearmed poll");
            assert_eq!(times, vec![at]);
            let delay = at - (now + 1_500.0);
            assert!(
                (0.75 * 2_000.0..=2_000.0).contains(&delay),
                "jittered failure backoff {delay}"
            );
            assert_eq!(*floor.get_or_insert(at), at);
        }
    }
}
#[test]
fn owned_position_outside_partial_catalogue_is_not_an_empty_success() {
    let mut e = support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    e.start(NOW);
    let a: Vec<_> = e.drain_actions().collect();
    let partial = PRODUCTS.replace(
        r#"{"symbol":"ETHUSDC""#,
        r#"{"symbol":"MISSINGUSDT","type":"PerpetualV2","settleCurrency":"USDT"},{"symbol":"ETHUSDC""#,
    );
    response(&mut e, NOW, request(&a, "products").id, 200, &partial);
    let a: Vec<_> = e.drain_actions().collect();
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., &BTC.replace("BTCUSDT", "MISSINGUSDT")),
    );
    assert!(!e.store().loaded().positions);
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("owned position market unavailable"))));
}

#[test]
fn owned_position_outside_settled_catalogue_is_skipped_as_an_error() {
    let (mut e, actions, _) = started();
    response(
        &mut e,
        NOW,
        request(&actions, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., &BTC.replace("BTCUSDT", "MISSINGUSDT")),
    );
    assert!(e.store().positions().is_empty());
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
            if message == "Phemex skipped MISSINGUSDT position on an unavailable market")));
}

#[test]
fn unknown_owned_live_position_cannot_grant_snapshot_authority_or_consume_sequence() {
    let (mut e, actions, socket) = started();
    seed(&mut e, &actions);
    authenticate(&mut e, socket);
    let before = e.store().positions().to_vec();
    let balance = e.store().balance().clone();
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"type":"snapshot","sequence":1,"orders_p":[],"positions_p":[{}]}}"#,
            BTC.replace("BTCUSDT", "MISSINGUSDT")
        ),
    );
    assert_eq!(e.store().positions(), before);
    assert_eq!(*e.store().balance(), balance);
    assert!(!e.store().loaded().orders);
    ws(
        &mut e,
        socket,
        &format!(r#"{{"type":"snapshot","sequence":1,"orders_p":[],"positions_p":[{BTC}]}}"#),
    );
    assert!(e.store().loaded().orders);
}

#[test]
fn unknown_working_live_order_cannot_clear_previous_snapshot_or_consume_sequence() {
    let (mut e, actions, socket) = started();
    seed(&mut e, &actions);
    authenticate(&mut e, socket);
    push_orders(&mut e, socket, "incremental", 1, &order("New", 1));
    let before = e.store().orders().to_vec();
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"type":"snapshot","sequence":2,"orders_p":[{}]}}"#,
            order("New", 2).replace("BTCUSDT", "MISSINGUSDT")
        ),
    );
    assert_eq!(e.store().orders(), before);
    assert!(!e.store().loaded().orders);
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("working order market unavailable"))));
    ws(
        &mut e,
        socket,
        &format!(
            r#"{{"type":"snapshot","sequence":2,"orders_p":[{}]}}"#,
            order("New", 2).replace("69000", "70000")
        ),
    );
    // The corrected same-sequence snapshot is accepted, while the newer
    // incremental row remains protected by the existing snapshot race fence.
    assert!(e.store().loaded().orders);
    assert_eq!(e.store().order("one").unwrap().price, 69000.0);
}

#[test]
fn unsupported_order_type_or_status_skips_only_that_row() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    let site = order("New", 100)
        .replace("\"one\"", "\"site\"")
        .replace("\"Limit\"", "\"Bracket\"");
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"type":"snapshot","sequence":1,"orders_p":[{},{site}],"positions_p":[{}]}}"#,
            order("New", 100),
            BTC.replace("0.01", "0.02")
        ),
    );
    assert!(e.store().loaded().orders);
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    assert!(e.store().order("site").is_none());
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.02);
    let filled = order("Mystery", 200)
        .replace("\"cumQty\":\"0\"", r#""cumQty":"0.1","execID":"exec-x","execQty":"0.1","execPriceRp":"69000","tradeType":"Trade""#);
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"type":"incremental","sequence":2,"orders_p":[{filled}],"positions_p":[{}]}}"#,
            BTC.replace("0.01", "0.03")
                .replace("1700000000000000000", "1700000000000000005")
        ),
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.03);
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    let frame = e.take_frame(NOW).unwrap();
    let data = execution_rows(frame, 1);
    assert_eq!(f64::from_le_bytes(data[8..16].try_into().unwrap()), 0.1);
}
#[test]
fn deactivated_or_expired_row_cancels_the_open_order_and_a_later_snapshot_cannot_revive_it() {
    for status in ["Deactivated", "Expired"] {
        let (mut e, a, s) = started();
        seed(&mut e, &a);
        authenticate(&mut e, s);
        push_orders(&mut e, s, "snapshot", 1, &order("New", 100));
        assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
        push_orders(&mut e, s, "incremental", 2, &order(status, 200));
        assert_eq!(
            e.store().order("one").unwrap().status,
            OrderStatus::Canceled,
            "{status} left the order open"
        );
        e.drain_actions().for_each(drop);
        e.handle(
            NOW + 200.,
            Event::WsClosed {
                socket: s,
                code: 1006,
            },
        );
        let (retry, at) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .expect("reconnect timer");
        e.handle(at, Event::TimerFired { timer: retry });
        let fresh = e
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => Some(socket),
                _ => None,
            })
            .expect("private reconnect");
        authenticate(&mut e, fresh);
        let other = order("New", 300).replace("\"one\"", "\"two\"");
        let rows = format!("{},{other}", order("New", 100));
        push_orders(&mut e, fresh, "snapshot", 1, &rows);
        assert!(e.store().order("two").is_some(), "snapshot did not apply");
        assert!(
            e.store()
                .order("one")
                .is_none_or(|o| o.status != OrderStatus::Open),
            "stale snapshot revived a {status} order"
        );
    }
}
#[test]
fn bad_stream_position_row_still_applies_the_frame_balance() {
    let (mut e, a, s) = started();
    seed(&mut e, &a);
    authenticate(&mut e, s);
    push_orders(&mut e, s, "snapshot", 1, "");
    let before = e.store().balance().used;
    ws(
        &mut e,
        s,
        &format!(
            r#"{{"type":"incremental","sequence":2,"timestamp":1700000000000000100,"positions_p":[{}],"accounts_p":[{{"currency":"USDT","accountBalanceRv":"1000","totalUsedBalanceRv":"80"}}]}}"#,
            BTC.replace("69000", "0")
        ),
    );
    assert_eq!(e.store().position("BTCUSDT").unwrap().contracts, 0.01);
    assert_ne!(e.store().balance().used, before);
    assert_eq!(e.store().balance().used, 80. + 19.);
}
#[test]
fn one_malformed_row_keeps_its_stored_leg_while_valid_peers_commit() {
    let short = ETH
        .replace("\"Long\"", "\"Short\"")
        .replace("\"Buy\"", "\"Sell\"")
        .replace("\"size\":\"1\"", "\"size\":\"2\"");
    let cycle = |e: &mut Engine<PhemexPerpetuals>, prior: &[Action], at: f64, usdc: &str| {
        e.handle(
            at,
            Event::TimerFired {
                timer: timer(prior, at),
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        response(
            e,
            at,
            request(&actions, "currency=USDT").id,
            200,
            &envelope("USDT", 1000., 69., BTC),
        );
        response(
            e,
            at,
            request(&actions, "currency=USDC").id,
            200,
            &envelope("USDC", 200., 25., usdc),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let logs = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) => Some((event.level, event.message.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        (actions, logs)
    };
    let (mut e, a, socket) = started();
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., BTC),
    );
    response(
        &mut e,
        NOW,
        request(&a, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 19., &format!("{ETH},{short}")),
    );
    assert_eq!(e.store().position("ETHUSDC:short").unwrap().contracts, 2.);
    // BTC +10, ETH long +100, ETH short -200 at the seeded marks.
    assert_eq!(e.store().balance().upnl, -90.);
    authenticate(&mut e, socket);
    let seeded: Vec<_> = e.drain_actions().collect();

    // A zero entry is unmappable; an owned row off the market list has no stored copy.
    let long = ETH.replace("\"size\":\"1\"", "\"size\":\"3\"");
    let bad_short = short.replace("1900", "0");
    let missing = ETH.replace("ETHUSDC", "MISSINGUSDC");
    let (actions, logs) = cycle(
        &mut e,
        &seeded,
        NOW + 2_000.,
        &format!("{long},{bad_short},{missing}"),
    );
    assert_eq!(e.store().position("ETHUSDC:long").unwrap().contracts, 3.);
    assert_eq!(e.store().position("ETHUSDC:short").unwrap().contracts, 2.);
    assert_eq!(e.store().balance().used, 69. + 25.);
    // UPnL derives from the committed rows: the fresh long and the retained short.
    let derived = 10. + 300. - 200.;
    assert_eq!(e.store().balance().upnl, derived);
    assert_eq!(e.store().balance().total, 1000. + 200. - derived);
    for (level, message) in [
        (
            safe_cex_engine::Level::Warn,
            "Phemex skipped unmappable ETHUSDC position row",
        ),
        (
            safe_cex_engine::Level::Error,
            "Phemex skipped MISSINGUSDC position on an unavailable market",
        ),
    ] {
        assert!(
            logs.iter().any(|(l, m)| *l == level && m == message),
            "{message}"
        );
    }
    assert!(
        !logs
            .iter()
            .any(|(_, l)| l.contains('{') || l.contains("1900"))
    );
    // A WS account push recomputes the same rule over the same rows.
    e.handle(NOW + 2_100., Event::WsMessage { socket, text:
        r#"{"timestamp":1700000002100000000,"accounts_p":[{"currency":"USDC","accountBalanceRv":"200","totalUsedBalanceRv":"25"}]}"# });
    assert_eq!(e.store().balance().upnl, derived);
    assert_eq!(e.store().balance().total, 1000. + 200. - derived);

    // The same skips on the next poll are not reported again.
    let skipped = |logs: &[(safe_cex_engine::Level, String)]| {
        logs.iter()
            .filter(|(_, m)| m.starts_with("Phemex skipped"))
            .count()
    };
    let rows = format!("{long},{bad_short},{missing}");
    let (actions, logs) = cycle(&mut e, &actions, NOW + 4_000., &rows);
    assert_eq!(skipped(&logs), 0);

    let (actions, _) = cycle(&mut e, &actions, NOW + 6_000., &long);
    assert!(e.store().position("ETHUSDC:short").is_none());
    assert_eq!(e.store().position("ETHUSDC:long").unwrap().contracts, 3.);
    // A clean slice re-arms them: a recurrence is reported again.
    let (_, logs) = cycle(&mut e, &actions, NOW + 8_000., &rows);
    assert_eq!(skipped(&logs), 2);
}

fn row_report_skips(actions: &[Action]) -> Vec<(safe_cex_engine::Level, String)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::ROW_SKIPPED =>
            {
                Some((event.level, event.message.clone()))
            }
            _ => None,
        })
        .collect()
}
fn row_report_cycle(
    e: &mut Engine<PhemexPerpetuals>,
    prior: &[Action],
    at: f64,
    usdt: &str,
    usdc: &str,
) -> Vec<Action> {
    e.handle(
        at,
        Event::TimerFired {
            timer: timer(prior, at),
        },
    );
    let reads: Vec<_> = e.drain_actions().collect();
    response(
        e,
        at,
        request(&reads, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., usdt),
    );
    response(
        e,
        at,
        request(&reads, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 25., usdc),
    );
    e.drain_actions().collect()
}

#[test]
fn row_report_clean_currency_rearms_only_its_own_anonymous_key() {
    use safe_cex_engine::Level;
    let (mut e, reads, _) = started();
    seed(&mut e, &reads);
    let actions: Vec<_> = e.drain_actions().collect();
    let warning = (
        Level::Warn,
        "Phemex skipped an unreadable position row".to_owned(),
    );

    let actions = row_report_cycle(&mut e, &actions, NOW + 2_000., "{}", "{}");
    assert_eq!(
        row_report_skips(&actions),
        [warning.clone(), warning.clone()]
    );
    assert_eq!(e.store().positions().len(), 2);
    let actions = row_report_cycle(&mut e, &actions, NOW + 4_000., BTC, "{}");
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 6_000., "{}", "{}");
    // The USDT clean read must not forget the identically labelled USDC row.
    assert_eq!(
        row_report_skips(&actions).as_slice(),
        std::slice::from_ref(&warning)
    );
    let actions = row_report_cycle(&mut e, &actions, NOW + 8_000., "{}", ETH);
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 10_000., "{}", "{}");
    assert_eq!(row_report_skips(&actions), [warning]);
}

#[test]
fn row_report_duplicate_symbol_blocks_rearm_but_unrelated_rejection_does_not() {
    use safe_cex_engine::Level;
    let (mut e, reads, _) = started();
    seed(&mut e, &reads);
    let actions: Vec<_> = e.drain_actions().collect();
    let bad = ETH.replace("1900", "0");
    let foreign = ETH.replace("ETHUSDC", "MISSINGUSDC");
    let rejected = format!("{bad},{foreign}");
    let warning = (
        Level::Warn,
        "Phemex skipped unmappable ETHUSDC position row".to_owned(),
    );
    let missing = (
        Level::Error,
        "Phemex skipped MISSINGUSDC position on an unavailable market".to_owned(),
    );
    let actions = row_report_cycle(&mut e, &actions, NOW + 2_000., BTC, &rejected);
    assert_eq!(row_report_skips(&actions), [warning.clone(), missing]);
    let actions = row_report_cycle(
        &mut e,
        &actions,
        NOW + 4_000.,
        BTC,
        &format!("{bad},{ETH},{foreign}"),
    );
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 6_000., BTC, &rejected);
    assert!(row_report_skips(&actions).is_empty());
    assert_eq!(e.store().position("ETHUSDC:long").unwrap().contracts, 1.);

    // A named unrelated rejection keeps the slice partial while allowing the
    // mapped ETH leg to forget its symbol key. An anonymous row would not.
    let actions = row_report_cycle(
        &mut e,
        &actions,
        NOW + 8_000.,
        BTC,
        &format!("{ETH},{foreign}"),
    );
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 10_000., BTC, &rejected);
    assert_eq!(row_report_skips(&actions), [warning]);
}

#[test]
fn row_report_stale_slice_does_not_rearm_but_a_current_clean_slice_does() {
    use safe_cex_engine::Level;
    let (mut e, reads, socket) = started();
    seed(&mut e, &reads);
    authenticate(&mut e, socket);
    let actions: Vec<_> = e.drain_actions().collect();
    let bad = format!("{},{{}}", ETH.replace("1900", "0"));
    let actions = row_report_cycle(&mut e, &actions, NOW + 2_000., BTC, &bad);
    let expected = [
        (
            Level::Warn,
            "Phemex skipped unmappable ETHUSDC position row".to_owned(),
        ),
        (
            Level::Warn,
            "Phemex skipped an unreadable position row".to_owned(),
        ),
    ];
    assert_eq!(row_report_skips(&actions), expected);

    e.handle(
        NOW + 4_000.,
        Event::TimerFired {
            timer: timer(&actions, NOW + 4_000.),
        },
    );
    let reads: Vec<_> = e.drain_actions().collect();
    // This accepted native push makes only the in-flight USDC slice stale.
    e.handle(NOW + 4_000., Event::WsMessage { socket, text:
        r#"{"timestamp":1700000004000000000,"accounts_p":[{"currency":"USDC","accountBalanceRv":"333","totalUsedBalanceRv":"25"}]}"# });
    response(
        &mut e,
        NOW + 4_000.,
        request(&reads, "currency=USDT").id,
        200,
        &envelope("USDT", 1000., 69., BTC),
    );
    response(
        &mut e,
        NOW + 4_000.,
        request(&reads, "currency=USDC").id,
        200,
        &envelope("USDC", 200., 25., ETH),
    );
    assert_eq!(
        e.store()
            .currency_balances()
            .unwrap()
            .iter()
            .find(|c| c.currency == "USDC")
            .unwrap()
            .balance
            .free,
        308.
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 6_000., BTC, &bad);
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 8_000., BTC, ETH);
    assert!(row_report_skips(&actions).is_empty());
    let actions = row_report_cycle(&mut e, &actions, NOW + 10_000., BTC, &bad);
    assert_eq!(row_report_skips(&actions), expected);
}
