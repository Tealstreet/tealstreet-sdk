mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Notice, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_woo::{Options, Woo};
use support::*;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

const INFO: &str = r#"{"success":true,"data":{"accountMode":"FUTURES","positionMode":"ONE_WAY","freeCollateral":"900.125","totalCollateral":"1000.125","totalAccountValue":"1100.125"}}"#;
const POS: &str = r#"{"success":true,"data":{"positions":[{"symbol":"PERP_BTC_USDT","holding":0.01,"averageOpenPrice":60000,"leverage":10,"marginMode":"CROSS","estLiqPrice":50000}]}}"#;
const ORDER: &str = r#"{"symbol":"PERP_BTC_USDT","order_id":9007199254740993,"status":"NEW","side":"BUY","type":"LIMIT","quantity":0.02,"executed":0.005,"price":59000,"reduce_only":false,"client_order_id":123}"#;
const ALGO: &str = r#"{"symbol":"PERP_BTC_USDT","rootAlgoOrderId":42,"algoType":"POSITIONAL_TP_SL","childOrders":[{"algoOrderId":43,"algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","quantity":0.01,"triggerPrice":58000,"triggerPriceType":"MARK_PRICE","totalExecutedQuantity":0,"reduceOnly":true},{"algoOrderId":44,"algoType":"TAKE_PROFIT","algoStatus":"NEW","side":"SELL","quantity":0.01,"triggerPrice":65000,"triggerPriceType":"MARKET_PRICE","totalExecutedQuantity":0,"reduceOnly":true}]}"#;
fn page(row: &str) -> String {
    format!(
        r#"{{"success":true,"meta":{{"total":1,"records_per_page":100,"current_page":1}},"rows":[{row}]}}"#
    )
}
fn algos(row: &str) -> String {
    format!(
        r#"{{"success":true,"data":{{"meta":{{"total":1,"records_per_page":100,"current_page":1}},"rows":[{row}]}}}}"#
    )
}
fn start() -> (TestEngine, SocketId, Vec<HttpRequest>, TimerId) {
    start_with(MARKETS)
}
fn start_with(markets: &str) -> (TestEngine, SocketId, Vec<HttpRequest>, TimerId) {
    let (e, socket, reads, timer, _) = start_with_retry(markets);
    (e, socket, reads, timer)
}

fn established_stream() -> (TestEngine, SocketId, Vec<HttpRequest>, TimerId) {
    let (mut engine, socket, reads, timer) = start();
    engine.handle(20.25, Event::WsOpened { socket });
    ws(
        &mut engine,
        socket,
        20.5,
        r#"{"event":"auth","success":true}"#,
    );
    engine.drain_actions().for_each(drop);
    (engine, socket, reads, timer)
}
/// Also returns the catalogue re-read timer a partial catalogue schedules.
fn start_with_retry(
    markets: &str,
) -> (
    TestEngine,
    SocketId,
    Vec<HttpRequest>,
    TimerId,
    Option<TimerId>,
) {
    start_with_retry_at(markets, 0.0)
}

fn start_with_retry_at(
    markets: &str,
    at: f64,
) -> (
    TestEngine,
    SocketId,
    Vec<HttpRequest>,
    TimerId,
    Option<TimerId>,
) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        application_id: Some("app".into()),
        ..Default::default()
    }));
    e.start(at);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, at + 10.0, markets);
    let mut early = e.drain_actions().collect::<Vec<_>>();
    let ticker = early
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/public/futures") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    response(&mut e, &ticker, at + 20.0, TICKERS);
    early.extend(e.drain_actions());
    let actions = early;
    let socket = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, url, .. } = a {
                url.contains("/private/").then_some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (*at_ms == at + 2010.0).then_some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    let retry = actions.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if jittered(*at_ms - (at + 10.0), 1000.0) => Some(*timer),
        _ => None,
    });
    let reads = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                r.url
                    .contains("/public/")
                    .then_some(())
                    .is_none()
                    .then_some(r)
            } else {
                None
            }
        })
        .collect();
    (e, socket, reads, timer, retry)
}
fn take(reads: &mut Vec<HttpRequest>, path: &str) -> HttpRequest {
    let i = reads
        .iter()
        .position(|r| r.url.contains(path))
        .unwrap_or_else(|| {
            panic!(
                "Missing {path}; {:?}",
                reads.iter().map(|r| &r.url).collect::<Vec<_>>()
            )
        });
    reads.remove(i)
}
fn load(e: &mut TestEngine, reads: &mut Vec<HttpRequest>) {
    let info = take(reads, "accountinfo");
    response(e, &info, 30.0, INFO);
    reads.extend(requests(e));
    let pos = take(reads, "/v3/positions");
    response(e, &pos, 40.0, POS);
    let regular = take(reads, "/v1/orders?");
    response(e, &regular, 50.0, &page(ORDER));
    let algo = take(reads, "/v3/algo/orders?");
    response(e, &algo, 60.0, &algos(ALGO));
}
fn count_fills(e: &mut TestEngine) -> u32 {
    let Some(frame) = e.take_frame(100.0) else {
        return 0;
    };
    let mut cursor = 16;
    for _ in 0..u16::from_le_bytes(frame[6..8].try_into().unwrap()) {
        let kind = u16::from_le_bytes(frame[cursor..cursor + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[cursor + 8..cursor + 12].try_into().unwrap());
        }
        cursor += 8 + size;
    }
    0
}
fn fill_amounts(e: &mut TestEngine) -> Vec<f64> {
    fill_values(e)
        .into_iter()
        .map(|(_, amount)| amount)
        .collect()
}
fn fill_values(e: &mut TestEngine) -> Vec<(f64, f64)> {
    let Some(frame) = e.take_frame(100.0) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let rows = (4 + 12 * count + 7) & !7;
    (0..count)
        .map(|index| {
            let at = rows + 16 * index;
            (
                f64::from_le_bytes(payload[at..at + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[at + 8..at + 16].try_into().unwrap()),
            )
        })
        .collect()
}
#[test]
fn startup_auth_snapshot_settings_and_exact_ids() {
    let (mut e, socket, mut reads, _) = start();
    assert_eq!(reads.len(), 3);
    assert!(reads.iter().all(|r| {
        r.headers
            .iter()
            .any(|(k, v)| *k == "x-api-key" && v == "replay-key")
    }));
    e.handle(25.0, Event::WsOpened { socket });
    let sent: Vec<_> = e.drain_actions().collect();
    assert!(sent.iter().any(
        |a| matches!(a,Action::WsSend{text,..}if text.contains("auth")&&text.contains("timestamp"))
    ));
    ws(&mut e, socket, 26.0, r#"{"event":"auth","success":true}"#);
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a,Action::WsSend{text,..}if text.contains("subscribe")))
            .count(),
        2
    );
    load(&mut e, &mut reads);
    assert_eq!(e.store().orders().len(), 3);
    assert_eq!(
        e.store()
            .order("9007199254740993")
            .unwrap()
            .client_order_id
            .as_deref(),
        Some("123")
    );
    assert_eq!(
        e.store().order("43").unwrap().parent_id.as_deref(),
        Some("42")
    );
    assert_eq!(e.store().positions()[0].id, "BTCUSDT");
    assert_eq!(e.store().positions()[0].contracts, 0.01);
    assert_eq!(e.store().balance().free, 900.13);
    assert_eq!(e.store().balance().upnl, 0.01);
    assert!((e.store().balance().total - 1100.12).abs() < 1e-10);
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(10.0)
    );
    assert!(e.store().loaded().orders);
}
#[test]
fn failed_algo_snapshot_preserves_complete_known_orders() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let regular = take(&mut reads, "/v1/orders?");
    response(&mut e, &regular, 80.0, &page(""));
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(
        &mut e,
        &algo,
        90.0,
        r#"{"success":false,"message":"failed","data":{"rows":[]}}"#,
    );
    assert_eq!(e.store().orders().len(), 3);
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.005);
}
#[test]
fn stream_terminal_and_max_filled_survive_older_snapshot() {
    let (mut e, socket, mut reads, _) = established_stream();
    ws(
        &mut e,
        socket,
        21.0,
        r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.02,"executedQuantity":0.02,"executedPrice":59000,"tradeId":101}}"#,
    );
    load(&mut e, &mut reads);
    let order = e.store().order("9007199254740993").unwrap();
    assert_eq!(order.status, OrderStatus::Closed);
    assert_eq!(order.filled, 0.02);
    assert_eq!(e.store().positions()[0].contracts, 0.02);
    assert_eq!(count_fills(&mut e), 1);
}
#[test]
fn repeated_execution_deduplicates_and_distinct_partial_fills_adjust_position_once() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    let frame = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIAL_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedQuantity":0.001,"executedPrice":59000,"tradeId":101}}"#;
    ws(&mut e, socket, 70.0, frame);
    ws(&mut e, socket, 71.0, frame);
    ws(
        &mut e,
        socket,
        72.0,
        &frame.replace("0.006", "0.007").replace("101", "102"),
    );
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.007);
    assert!((e.store().positions()[0].contracts - 0.012).abs() < 1e-10);
    assert_eq!(count_fills(&mut e), 2);
}
#[test]
fn cumulative_only_reports_emit_deltas_not_repeated_totals() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    let frame = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedPrice":59000}}"#;
    ws(&mut e, socket, 70.0, frame);
    ws(&mut e, socket, 71.0, frame);
    ws(&mut e, socket, 72.0, &frame.replace("0.006", "0.007"));
    assert_eq!(count_fills(&mut e), 2);
    assert!((e.store().positions()[0].contracts - 0.012).abs() < 1e-10);
}
#[test]
fn flat_leverage_uses_legacy_oneway_cross_and_updates_both_rails() {
    let (mut e, _, _, _) = start();
    e.handle(
        30.0,
        Event::Command(Command::FetchLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(
        r.url
            .contains("margin_mode=CROSS&position_mode=ONE_WAY&symbol=PERP_BTC_USDT")
    );
    response(
        &mut e,
        &r,
        40.0,
        r#"{"success":true,"data":{"symbol":"PERP_BTC_USDT","margin_mode":"CROSS","position_mode":"ONE_WAY","details":[{"position_side":"BOTH","leverage":"12"}]}}"#,
    );
    let s = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(s.buy_leverage, Some(12.0));
    assert_eq!(s.sell_leverage, Some(12.0));
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 7,
            result: _
        })
    )));
}
#[test]
fn non_futures_account_never_marks_balance_or_positions_loaded() {
    let (mut e, _, mut reads, _) = start();
    let r = take(&mut reads, "accountinfo");
    response(&mut e, &r, 30.0, &INFO.replace("FUTURES", "SPOT"));
    assert!(!e.store().loaded().balance);
    assert!(!e.store().loaded().positions);
    assert!(requests(&mut e).is_empty());
}
#[test]
fn private_watchdog_closes_then_reconnects_with_capped_backoff() {
    let (mut e, socket, _, _) = start();
    e.handle(30.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    ws(&mut e, socket, 31.0, r#"{"event":"auth","success":true}"#);
    let watchdog = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(21_000.0, Event::TimerFired { timer: watchdog });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    let reconnect = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (21_750.0..=22_000.0).contains(&at_ms) => {
                Some(timer)
            }
            _ => None,
        })
        .unwrap();
    e.handle(22_000.0, Event::TimerFired { timer: reconnect });
    assert!(e.drain_actions().any(|a| matches!(a,Action::WsConnect{url,channel:Some(SocketChannel::Private),context:None,..}if url == "wss://wss.woox.io/v2/ws/private/stream/app")));
}

#[test]
fn two_hedge_rails_in_one_snapshot_preserve_distinct_settings() {
    let (mut e, _, mut reads, _) = start();
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 30.0, &INFO.replace("ONE_WAY", "HEDGE_MODE"));
    reads.extend(requests(&mut e));
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        40.0,
        r#"{"success":true,"data":{"positions":[{"symbol":"PERP_BTC_USDT","holding":0.01,"averageOpenPrice":60000,"leverage":10,"marginMode":"CROSS","estLiqPrice":50000},{"symbol":"PERP_BTC_USDT","holding":-0.02,"averageOpenPrice":61000,"leverage":15,"marginMode":"CROSS","estLiqPrice":70000}]}}"#,
    );
    assert!(e.store().hedged());
    assert_eq!(e.store().positions()[0].id, "BTCUSDT:long");
    assert_eq!(e.store().positions()[1].id, "BTCUSDT:short");
    let s = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(s.buy_leverage, Some(10.0));
    assert_eq!(s.sell_leverage, Some(15.0));
    assert!(requests(&mut e).is_empty());
}
#[test]
fn malformed_positions_read_preserves_known_positions() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        80.0,
        &POS.replace("\"holding\":0.01", "\"holding\":\"bad\""),
    );
    assert_eq!(e.store().positions()[0].contracts, 0.01);
}
#[test]
fn regular_pages_join_only_after_both_complete() {
    let (mut e, _, mut reads, _) = start();
    let regular = take(&mut reads, "/v1/orders?");
    let first = page(ORDER)
        .replace("\"total\":1", "\"total\":2")
        .replace("\"records_per_page\":100", "\"records_per_page\":1");
    response(&mut e, &regular, 30.0, &first);
    assert!(!e.store().loaded().orders);
    let mut more = requests(&mut e);
    let second = take(&mut more, "/v1/orders?");
    assert!(second.url.contains("page=2"));
    response(
        &mut e,
        &second,
        40.0,
        &first
            .replace("9007199254740993", "9007199254740994")
            .replace("\"current_page\":1", "\"current_page\":2"),
    );
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 50.0, &algos(""));
    assert_eq!(e.store().orders().len(), 2);
    assert!(e.store().loaded().orders);
}
#[test]
fn minimal_terminal_event_shields_unknown_order_from_later_snapshot() {
    let (mut e, socket, mut reads, _) = established_stream();
    ws(
        &mut e,
        socket,
        25.0,
        r#"{"topic":"executionreport","data":{"orderId":9007199254740993,"status":"CANCELLED"}}"#,
    );
    load(&mut e, &mut reads);
    assert_eq!(
        e.store().order("9007199254740993").unwrap().status,
        OrderStatus::Canceled
    );
}
#[test]
fn minimal_parent_cancel_closes_both_existing_bracket_children() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    ws(
        &mut e,
        socket,
        70.0,
        r#"{"topic":"algoexecutionreportv2","data":{"algoOrderId":42,"algoStatus":"CANCELLED"}}"#,
    );
    assert_eq!(e.store().order("43").unwrap().status, OrderStatus::Canceled);
    assert_eq!(e.store().order("44").unwrap().status, OrderStatus::Canceled);
}
#[test]
fn auth_rejection_does_not_subscribe() {
    let (mut e, socket, _, _) = start();
    ws(
        &mut e,
        socket,
        25.0,
        r#"{"event":"auth","success":false,"message":"bad key"}"#,
    );
    assert!(
        e.drain_actions()
            .all(|a| !matches!(a, Action::WsSend { .. }))
    );
}
#[test]
fn malformed_known_order_keeps_its_stored_copy_while_the_rest_commits() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let regular = take(&mut reads, "/v1/orders?");
    response(
        &mut e,
        &regular,
        80.0,
        &page(&ORDER.replace("\"quantity\":0.02,", "")),
    );
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 90.0, &algos(""));
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().order("9007199254740993").unwrap().amount, 0.02);
}
#[test]
fn account_equity_always_excludes_current_unrealized_pnl() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 80.0, INFO);
    assert_eq!(e.store().balance().upnl, 0.01);
    assert!((e.store().balance().total - 1100.12).abs() < 1e-10);
}
#[test]
fn single_object_algo_parent_updates_each_child() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    let data = ALGO.replace("58000", "58500").replace("65000", "65500");
    ws(
        &mut e,
        socket,
        70.0,
        &format!(r#"{{"topic":"algoexecutionreportv2","data":{data}}}"#),
    );
    assert_eq!(e.store().order("43").unwrap().price, 58500.0);
    assert_eq!(e.store().order("44").unwrap().price, 65500.0);
}
#[test]
fn closing_fill_keeps_flat_position_across_multiple_stale_refreshes() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        70.0,
        r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":77,"status":"FILLED","side":"SELL","type":"MARKET","quantity":0.01,"totalExecutedQuantity":0.01,"executedQuantity":0.01,"executedPrice":60001,"tradeId":102}}"#,
    );
    assert!(e.store().positions().is_empty());
    let mut actions: Vec<_> = e.drain_actions().collect();
    for _ in 0..2 {
        let (timer, now) = actions
            .iter()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((*timer, *at_ms))
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(now, Event::TimerFired { timer });
        let r = requests(&mut e).remove(0);
        response(&mut e, &r, now + 1.0, POS);
        assert!(e.store().positions().is_empty());
        actions = e.drain_actions().collect();
    }
}
#[test]
fn old_position_read_cannot_restore_preconfirmation_leverage() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let old = take(&mut reads, "/v3/positions");
    e.handle(
        80.0,
        Event::Command(Command::SetLeverage {
            command: 9,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 90.0, r#"{"success":true}"#);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        100.0,
        r#"{"success":true,"data":{"symbol":"PERP_BTC_USDT","margin_mode":"CROSS","position_mode":"ONE_WAY","details":[{"position_side":"BOTH","leverage":"20"}]}}"#,
    );
    response(&mut e, &old, 110.0, POS);
    assert_eq!(e.store().positions()[0].leverage, 20.0);
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}
#[test]
fn old_position_read_cannot_restore_preconfirmation_mode() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let old = take(&mut reads, "/v3/positions");
    e.handle(
        80.0,
        Event::Command(Command::SetPositionMode {
            command: 9,
            hedged: true,
            symbol: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 90.0, r#"{"success":true}"#);
    let r = requests(&mut e).remove(0);
    response(
        &mut e,
        &r,
        100.0,
        r#"{"success":true,"data":{"accountMode":"FUTURES","positionMode":"HEDGE_MODE"}}"#,
    );
    response(
        &mut e,
        &old,
        110.0,
        &POS.replace("\"holding\":0.01", "\"holding\":0.02"),
    );
    assert!(e.store().hedged());
    assert_eq!(e.store().positions()[0].contracts, 0.01);
    assert!(
        e.store().positions()[0].position_mode
            != Some(safe_cex_engine::model::PositionMode::Hedged)
            || e.store().positions()[0].id.contains(':')
    );
}
#[test]
fn sized_stop_kind_uses_closing_side_and_selected_source_then_preserves_identity() {
    let (mut e, socket, mut reads, _) = start();
    let regular = take(&mut reads, "/v1/orders?");
    response(&mut e, &regular, 30.0, &page(""));
    let algo = take(&mut reads, "/v3/algo/orders?");
    let rows = r#"{"symbol":"PERP_BTC_USDT","algoOrderId":51,"algoType":"STOP","side":"SELL","quantity":0.01,"triggerPrice":60000.5,"triggerPriceType":"MARK_PRICE","totalExecutedQuantity":0,"reduceOnly":true},{"symbol":"PERP_BTC_USDT","algoOrderId":52,"algoType":"STOP","side":"SELL","quantity":0.01,"triggerPrice":60000.5,"triggerPriceType":"MARKET_PRICE","totalExecutedQuantity":0,"reduceOnly":true},{"symbol":"PERP_BTC_USDT","algoOrderId":53,"algoType":"STOP","side":"BUY","quantity":0.01,"triggerPrice":60000.5,"triggerPriceType":"MARK_PRICE","totalExecutedQuantity":0,"reduceOnly":true},{"symbol":"PERP_BTC_USDT","algoOrderId":54,"algoType":"STOP","side":"BUY","quantity":0.01,"triggerPrice":60000.5,"triggerPriceType":"MARKET_PRICE","totalExecutedQuantity":0,"reduceOnly":true}"#;
    response(&mut e, &algo, 40.0, &algos(rows));
    use safe_cex_engine::model::OrderType;
    assert_eq!(e.store().order("51").unwrap().kind, OrderType::StopLoss);
    assert_eq!(e.store().order("52").unwrap().kind, OrderType::TakeProfit);
    assert_eq!(e.store().order("53").unwrap().kind, OrderType::TakeProfit);
    assert_eq!(e.store().order("54").unwrap().kind, OrderType::StopLoss);
    ws(
        &mut e,
        socket,
        50.0,
        r#"{"topic":"algoexecutionreportv2","data":{"symbol":"PERP_BTC_USDT","algoOrderId":52,"algoType":"STOP","algoStatus":"REPLACED","side":"SELL","quantity":0.01,"triggerPrice":59000,"triggerPriceType":"MARKET_PRICE"}}"#,
    );
    assert_eq!(e.store().order("52").unwrap().kind, OrderType::TakeProfit);
}
#[test]
fn sparse_algo_update_retains_parent_reduce_flag_and_trigger_source() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    ws(
        &mut e,
        socket,
        70.0,
        r#"{"topic":"algoexecutionreportv2","data":{"algoOrderId":43,"algoStatus":"REPLACED","triggerPrice":58500}}"#,
    );
    let o = e.store().order("43").unwrap();
    assert_eq!(o.parent_id.as_deref(), Some("42"));
    assert!(o.reduce_only);
    assert_eq!(
        o.trigger_type,
        Some(safe_cex_engine::model::StopTriggerType::MarkPrice)
    );
    assert_eq!(o.price, 58500.0);
}
#[test]
fn execution_id_only_increment_updates_order_cumulative_once() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    let data = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIAL_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"executedQuantity":0.001,"executedPrice":59000,"tradeId":101}}"#;
    ws(&mut e, socket, 70.0, data);
    ws(&mut e, socket, 71.0, data);
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.006);
    assert_eq!(
        e.store()
            .order("9007199254740993")
            .unwrap()
            .client_order_id
            .as_deref(),
        Some("123")
    );
    assert_eq!(count_fills(&mut e), 1);
}
#[test]
fn malformed_executable_algo_leaf_keeps_its_stored_copy_while_siblings_commit() {
    for malformed in [
        ALGO.replace("\"triggerPrice\":58000,", ""),
        ALGO.replace("\"triggerPrice\":58000", "\"triggerPrice\":0"),
    ] {
        let (mut e, socket, mut reads, _) = start();
        load(&mut e, &mut reads);
        e.drain_actions().for_each(drop);
        e.handle(70.0, Event::WsOpened { socket });
        let mut reads = requests(&mut e);
        let r = take(&mut reads, "/v1/orders?");
        response(&mut e, &r, 80.0, &page(""));
        let r = take(&mut reads, "/v3/algo/orders?");
        response(
            &mut e,
            &r,
            90.0,
            &algos(&malformed.replace("65000", "65500")),
        );
        assert_eq!(e.store().orders().len(), 2);
        assert_eq!(e.store().order("43").unwrap().price, 58000.0);
        assert_eq!(e.store().order("44").unwrap().price, 65500.0);
    }
}
#[test]
fn regular_dual_id_spellings_agree_and_preserve_exact_native_id() {
    let (mut e, _, mut reads, _) = start();
    let r = take(&mut reads, "/v1/orders?");
    response(
        &mut e,
        &r,
        30.0,
        &page(&ORDER.replace(
            "\"order_id\":9007199254740993",
            "\"order_id\":9007199254740993,\"orderId\":9007199254740993",
        )),
    );
    let r = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &r, 40.0, &algos(""));
    assert!(e.store().order("9007199254740993").is_some());
}
#[test]
fn buy_trailing_display_is_extreme_plus_callback_and_sell_is_minus() {
    let (mut e, socket, mut reads, _) = established_stream();
    let regular = take(&mut reads, "/v1/orders?");
    response(&mut e, &regular, 30.0, &page(""));
    let r = take(&mut reads, "/v3/algo/orders?");
    let rows = r#"{"symbol":"PERP_BTC_USDT","algoOrderId":61,"algoType":"TRAILING_STOP","side":"BUY","quantity":0.01,"extremePrice":60000,"callbackValue":61000,"totalExecutedQuantity":0,"reduceOnly":true},{"symbol":"PERP_BTC_USDT","algoOrderId":62,"algoType":"TRAILING_STOP","side":"SELL","quantity":0.01,"extremePrice":60000,"callbackValue":1000,"totalExecutedQuantity":0,"reduceOnly":true}"#;
    response(&mut e, &r, 40.0, &algos(rows));
    assert_eq!(e.store().order("61").unwrap().price, 121000.0);
    assert_eq!(e.store().order("62").unwrap().price, 59000.0);
    ws(
        &mut e,
        socket,
        50.0,
        r#"{"topic":"algoexecutionreportv2","data":{"symbol":"PERP_BTC_USDT","algoOrderId":61,"algoType":"TRAILING_STOP","algoStatus":"REPLACED","side":"BUY","quantity":0.01,"extremePrice":59000,"callbackValue":1000}}"#,
    );
    assert_eq!(e.store().order("61").unwrap().price, 60000.0);
}

#[test]
fn first_camel_case_order_and_execution_are_not_dropped() {
    let (mut e, socket, _, _) = established_stream();
    ws(
        &mut e,
        socket,
        25.0,
        r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":92,"status":"NEW","side":"BUY","orderType":"LIMIT","orderQuantity":0.01,"orderPrice":59000,"totalExecutedQuantity":0}}"#,
    );
    assert_eq!(
        e.store().order("92").unwrap().kind,
        safe_cex_engine::model::OrderType::Limit
    );
    ws(
        &mut e,
        socket,
        30.0,
        r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":91,"status":"PARTIAL_FILLED","side":"BUY","orderType":"LIMIT","orderQuantity":0.02,"orderPrice":59000,"totalExecutedQuantity":0.001,"executedQuantity":0.001,"executedPrice":59000,"tradeId":9101}}"#,
    );
    let order = e.store().order("91").unwrap();
    assert_eq!(order.kind, safe_cex_engine::model::OrderType::Limit);
    assert_eq!(
        (order.amount, order.filled, order.price),
        (0.02, 0.001, 59000.0)
    );
    assert_eq!(e.store().positions()[0].contracts, 0.001);
    assert_eq!(count_fills(&mut e), 1);
}

#[test]
fn unseen_execution_already_in_rest_emits_once_without_increasing_position() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    let frame = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIAL_FILLED","side":"BUY","orderType":"LIMIT","orderQuantity":0.02,"totalExecutedQuantity":0.005,"executedQuantity":0.005,"executedPrice":59000,"tradeId":101}}"#;
    ws(&mut e, socket, 70.0, frame);
    ws(&mut e, socket, 71.0, frame);
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.005);
    assert_eq!(e.store().positions()[0].contracts, 0.01);
    assert_eq!(fill_amounts(&mut e), vec![0.005]);
}

#[test]
fn partially_reconciled_execution_applies_only_new_cumulative_position_quantity() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    ws(
        &mut e,
        socket,
        70.0,
        r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIAL_FILLED","side":"BUY","orderType":"LIMIT","orderQuantity":0.02,"totalExecutedQuantity":0.006,"executedQuantity":0.003,"executedPrice":59000,"tradeId":101}}"#,
    );
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.006);
    assert!((e.store().positions()[0].contracts - 0.011).abs() < 1e-10);
    assert_eq!(fill_amounts(&mut e), vec![0.003]);
}

#[test]
fn invalid_regular_or_algo_page_metadata_preserves_complete_known_orders() {
    for invalid in ["missing", "zero", "wrong-page", "missing-page"] {
        for algo in [false, true] {
            let (mut e, socket, mut reads, _) = start();
            load(&mut e, &mut reads);
            e.drain_actions().for_each(drop);
            e.handle(70.0, Event::WsOpened { socket });
            let mut reads = requests(&mut e);
            let regular = take(&mut reads, "/v1/orders?");
            let plan = take(&mut reads, "/v3/algo/orders?");
            let good = if algo { page("") } else { algos("") };
            let bad = match (algo, invalid) {
                (false, "missing") => r#"{"success":true,"rows":[]}"#.into(),
                (true, "missing") => r#"{"success":true,"data":{"rows":[]}}"#.into(),
                (_, kind) => {
                    let body = if algo { algos("") } else { page("") };
                    match kind {
                        "zero" => {
                            body.replace("\"records_per_page\":100", "\"records_per_page\":0")
                        }
                        "wrong-page" => body.replace("\"current_page\":1", "\"current_page\":2"),
                        "missing-page" => body.replace(",\"current_page\":1", ""),
                        _ => unreachable!(),
                    }
                }
            };
            if algo {
                response(&mut e, &regular, 80.0, &good);
                response(&mut e, &plan, 90.0, &bad);
            } else {
                response(&mut e, &plan, 80.0, &good);
                response(&mut e, &regular, 90.0, &bad);
            }
            assert_eq!(e.store().orders().len(), 3, "{invalid}, algo={algo}");
            assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.005);
        }
    }
}

#[test]
fn invalid_second_page_metadata_cannot_publish_a_successful_prefix() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let regular = take(&mut reads, "/v1/orders?");
    let first = page(&ORDER.replace("9007199254740993", "81"))
        .replace("\"total\":1", "\"total\":2")
        .replace("\"records_per_page\":100", "\"records_per_page\":1");
    response(&mut e, &regular, 80.0, &first);
    let mut next = requests(&mut e);
    let second = take(&mut next, "/v1/orders?");
    response(&mut e, &second, 90.0, &page(""));
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 100.0, &algos(""));
    assert_eq!(e.store().orders().len(), 3);
    assert!(e.store().order("81").is_none());
}

#[test]
fn rejected_algo_snapshot_cannot_change_later_amend_classification() {
    for failed_second_page in [false, true] {
        let (mut e, socket, mut reads, _) = established_stream();
        load(&mut e, &mut reads);
        ws(
            &mut e,
            socket,
            65.0,
            r#"{"topic":"algoexecutionreportv2","data":{"symbol":"PERP_BTC_USDT","algoOrderId":92,"rootAlgoOrderId":92,"algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","quantity":0.01,"triggerPrice":58000,"totalExecutedQuantity":0,"reduceOnly":true}}"#,
        );
        e.drain_actions().for_each(drop);
        e.handle(66., Event::WsClosed { socket, code: 1006 });
        let (timer, retry_at) = e
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if (816. ..=1_066.).contains(&at_ms) => {
                    Some((timer, at_ms))
                }
                _ => None,
            })
            .expect("private reconnect timer");
        e.handle(retry_at, Event::TimerFired { timer });
        let replacement = e
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => Some(socket),
                _ => None,
            })
            .expect("replacement private socket");
        e.handle(
            retry_at + 1.,
            Event::WsOpened {
                socket: replacement,
            },
        );
        let mut reads = requests(&mut e);
        let regular = take(&mut reads, "/v1/orders?");
        response(&mut e, &regular, retry_at + 10., &page(""));
        let algo = take(&mut reads, "/v3/algo/orders?");
        let reclassified = ALGO.replace("\"algoOrderId\":43", "\"algoOrderId\":92");
        let body = if failed_second_page {
            algos(&reclassified)
                .replace("\"total\":1", "\"total\":2")
                .replace("\"records_per_page\":100", "\"records_per_page\":1")
        } else {
            algos(&reclassified).replace("\"current_page\":1", "\"current_page\":2")
        };
        response(&mut e, &algo, retry_at + 20., &body);
        if failed_second_page {
            let mut reads = requests(&mut e);
            let second = take(&mut reads, "/v3/algo/orders?");
            response(&mut e, &second, retry_at + 30., &algos(""));
        }
        assert_eq!(
            e.store().order("92").unwrap().parent_id.as_deref(),
            Some("92")
        );
        e.drain_actions().for_each(drop);
        e.handle(
            retry_at + 40.,
            Event::Command(Command::AmendOrder {
                command: 92,
                id: "92".into(),
                price: Some(58500.0),
                amount: None,
            }),
        );
        let mut writes = requests(&mut e);
        let amend = take(&mut writes, "/v3/algo/order/92");
        assert_eq!(
            amend.body.as_deref(),
            Some(r#"{"triggerPrice":"58500"}"#),
            "failed_second_page={failed_second_page}"
        );
    }
}
const SPARSE_CHILDREN: &str = r#"{"symbol":"PERP_BTC_USDT","algoOrderId":60,"rootAlgoOrderId":60,"algoType":"POSITIONAL_TP_SL","algoStatus":"NEW","side":"SELL","reduceOnly":true,"childOrders":[{"algoOrderId":61,"algoType":"STOP_LOSS","quantity":0.01,"triggerPrice":58000,"triggerPriceType":"MARK_PRICE","totalExecutedQuantity":0},{"algoOrderId":62,"algoType":"TAKE_PROFIT","quantity":0.01,"triggerPrice":65000,"triggerPriceType":"MARKET_PRICE","totalExecutedQuantity":0}]}"#;
#[test]
fn snapshot_children_inherit_parent_side_status_and_reduce_only() {
    let (mut e, _, mut reads, _) = start();
    let regular = take(&mut reads, "/v1/orders?");
    response(&mut e, &regular, 30.0, &page(""));
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 40.0, &algos(SPARSE_CHILDREN));
    for id in ["61", "62"] {
        let o = e.store().order(id).unwrap();
        assert_eq!(o.side, safe_cex_engine::model::OrderSide::Sell);
        assert_eq!(o.status, OrderStatus::Open);
        assert!(o.reduce_only);
        assert_eq!(o.parent_id.as_deref(), Some("60"));
    }
}
#[test]
fn stream_child_fill_without_own_side_or_status_is_not_lost() {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    e.take_frame(90.0);
    let data = SPARSE_CHILDREN
        .replace(r#""algoStatus":"NEW""#, r#""algoStatus":"FILLED""#)
        .replace(
            r#""totalExecutedQuantity":0}]"#,
            r#""totalExecutedQuantity":0.01,"averageExecutedPrice":65010}]"#,
        );
    ws(
        &mut e,
        socket,
        95.0,
        &format!(r#"{{"topic":"algoexecutionreportv2","data":[{data}]}}"#),
    );
    assert_eq!(fill_amounts(&mut e), vec![0.01]);
    let o = e.store().order("62").unwrap();
    assert_eq!(o.status, OrderStatus::Closed);
    assert_eq!(o.side, safe_cex_engine::model::OrderSide::Sell);
    assert!(o.reduce_only);
    assert_eq!(e.store().order("61").unwrap().status, OrderStatus::Canceled);
}
#[test]
fn one_unmappable_position_row_neither_freezes_nor_drops_the_rest() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        80.0,
        &POS.replace("\"holding\":0.01", "\"holding\":0.02").replace(
            "]}}",
            r#",{"symbol":"PERP_ETH_USDT","holding":"bad"},{"holding":1}]}}"#,
        ),
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 0.02);
    assert!(e.drain_actions().any(|a| matches!(a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. }))
            if message.contains("WOO positions: skipped 1")
    )));
}
#[test]
fn unidentified_position_row_keeps_every_stored_position() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        80.0,
        r#"{"success":true,"data":{"positions":[{"holding":1}]}}"#,
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 0.01);
}
#[test]
fn unidentified_order_row_keeps_every_stored_order() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    assert!(e.store().order("9007199254740993").is_some());
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let regular = take(&mut reads, "/v1/orders?");
    response(&mut e, &regular, 80.0, &page(r#"{"order_id":[1]}"#));
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 90.0, &algos(ALGO));
    assert!(e.store().order("9007199254740993").is_some());
    assert!(e.store().order("43").is_some());
}
#[test]
fn invalid_known_position_row_keeps_its_stored_copy() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        80.0,
        &POS.replace("\"holding\":0.01", "\"holding\":0.05")
            .replace("\"leverage\":10", "\"leverage\":0"),
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 0.01);
    assert!(e.drain_actions().any(|a| matches!(a,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. }))
            if message.contains("BTCUSDT")
    )));
}
#[test]
fn hedge_skip_keeps_only_the_unreadable_leg_and_drops_a_closed_sibling() {
    let (mut e, _, mut reads, timer) = start();
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 30.0, &INFO.replace("ONE_WAY", "HEDGE_MODE"));
    reads.extend(requests(&mut e));
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        40.0,
        r#"{"success":true,"data":{"positions":[{"symbol":"PERP_BTC_USDT","holding":0.01,"averageOpenPrice":60000,"leverage":10,"marginMode":"CROSS"},{"symbol":"PERP_BTC_USDT","holding":-0.02,"averageOpenPrice":61000,"leverage":15,"marginMode":"CROSS"}]}}"#,
    );
    assert_eq!(e.store().positions().len(), 2);
    e.drain_actions().for_each(drop);
    e.handle(2020.0, Event::TimerFired { timer });
    let p = take(&mut requests(&mut e), "/v3/positions");
    response(
        &mut e,
        &p,
        2030.0,
        r#"{"success":true,"data":{"positions":[{"symbol":"PERP_BTC_USDT","holding":0.03,"averageOpenPrice":60000,"leverage":0,"marginMode":"CROSS"}]}}"#,
    );
    let ids: Vec<_> = e.store().positions().iter().map(|p| p.id.clone()).collect();
    assert_eq!(ids, vec!["BTCUSDT:long"]);
    assert_eq!(e.store().positions()[0].contracts, 0.01);
}
#[test]
fn steady_hedged_fills_cannot_starve_position_snapshots() {
    let (mut e, socket, mut reads, mut timer) = established_stream();
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 30.0, &INFO.replace("ONE_WAY", "HEDGE_MODE"));
    reads.extend(requests(&mut e));
    let p = take(&mut reads, "/v3/positions");
    response(
        &mut e,
        &p,
        40.0,
        r#"{"success":true,"data":{"positions":[{"symbol":"PERP_BTC_USDT","holding":0.01,"averageOpenPrice":60000,"leverage":10,"marginMode":"CROSS"},{"symbol":"PERP_BTC_USDT","holding":-0.02,"averageOpenPrice":61000,"leverage":10,"marginMode":"CROSS"}]}}"#,
    );
    e.drain_actions().for_each(drop);
    let mut now = 2020.0;
    for i in 0..3 {
        e.handle(now, Event::TimerFired { timer });
        let mut read = None;
        for a in e.drain_actions() {
            match a {
                Action::Http(r) if r.url.contains("/v3/positions") => read = Some(r),
                Action::SetTimer { timer: t, at_ms } if at_ms == now + 2000.0 => timer = t,
                _ => {}
            }
        }
        ws(
            &mut e,
            socket,
            now + 1.0,
            &format!(
                r#"{{"topic":"executionreport","data":{{"symbol":"PERP_BTC_USDT","orderId":{o},"status":"FILLED","side":"BUY","type":"MARKET","quantity":0.01,"totalExecutedQuantity":0.01,"executedQuantity":0.01,"executedPrice":60001,"tradeId":{t}}}}}"#,
                o = 300 + i,
                t = 400 + i
            ),
        );
        e.drain_actions().for_each(drop);
        response(
            &mut e,
            &read.unwrap(),
            now + 2.0,
            r#"{"success":true,"data":{"positions":[{"symbol":"PERP_BTC_USDT","holding":0.05,"averageOpenPrice":60000,"leverage":10,"marginMode":"CROSS"},{"symbol":"PERP_BTC_USDT","holding":-0.02,"averageOpenPrice":61000,"leverage":10,"marginMode":"CROSS"}]}}"#,
        );
        let rereads = requests(&mut e)
            .into_iter()
            .filter(|r| r.url.contains("/v3/positions"))
            .count();
        let long = e
            .store()
            .positions()
            .iter()
            .find(|p| p.id == "BTCUSDT:long")
            .unwrap()
            .contracts;
        assert_eq!(long, if i < 2 { 0.01 } else { 0.05 });
        assert_eq!(rereads, if i < 2 { 0 } else { 1 });
        now += 2000.0;
    }
}
#[test]
fn repeated_skips_and_hidden_exposure_warn_once_per_change() {
    let (mut e, _, mut reads, timer) = start();
    load(&mut e, &mut reads);
    let mut timer = timer;
    let body = POS.replace("\"leverage\":10", "\"leverage\":0").replace(
        "]}}",
        r#",{"symbol":"PERP_ETH_USDT","holding":1,"averageOpenPrice":3000,"leverage":5,"marginMode":"CROSS"}]}}"#,
    );
    let mut skipped = 0;
    let mut hidden = 0;
    for tick in 1..=3 {
        e.drain_actions().for_each(drop);
        let at = 2020.0 * f64::from(tick);
        e.handle(at, Event::TimerFired { timer });
        let actions: Vec<_> = e.drain_actions().collect();
        timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(*timer),
                _ => None,
            })
            .unwrap();
        let mut http: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        let p = take(&mut http, "/v3/positions");
        response(&mut e, &p, at + 5.0, &body);
        let actions: Vec<_> = e.drain_actions().collect();
        for a in &actions {
            if let Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) = a
            {
                skipped += usize::from(message.contains("WOO positions: skipped"));
                hidden += usize::from(message.contains("PERP_ETH_USDT cannot be shown"));
            }
        }
    }
    assert_eq!((skipped, hidden), (1, 1));
    assert_eq!(e.store().positions()[0].contracts, 0.01);
}

#[test]
fn unknown_working_regular_or_protective_order_retains_the_whole_previous_snapshot() {
    // The catalogue skipped PERP_MISSING_USDT, so its working orders are ambiguous.
    let catalogue = MARKETS.replace("]}", r#",{"symbol":"PERP_MISSING_USDT"}]}"#);
    for (algo_bad, terminal_children) in [(false, false), (true, false), (true, true)] {
        let (mut e, socket, mut reads, _) = start_with(&catalogue);
        load(&mut e, &mut reads);
        let before = e.store().orders().to_vec();
        e.drain_actions().for_each(drop);
        e.handle(70.0, Event::WsOpened { socket });
        let mut reads = requests(&mut e);
        let regular = take(&mut reads, "/v1/orders?");
        let unknown_regular = ORDER.replace("PERP_BTC_USDT", "PERP_MISSING_USDT");
        response(
            &mut e,
            &regular,
            80.0,
            &page(if algo_bad { "" } else { &unknown_regular }),
        );
        let algo = take(&mut reads, "/v3/algo/orders?");
        let mut unknown_algo = ALGO.replace("PERP_BTC_USDT", "PERP_MISSING_USDT").replacen(
            "\"rootAlgoOrderId\"",
            "\"algoStatus\":\"FILLED\",\"rootAlgoOrderId\"",
            1,
        );
        if terminal_children {
            unknown_algo =
                unknown_algo.replace("\"algoStatus\":\"NEW\"", "\"algoStatus\":\"FILLED\"");
        }
        response(
            &mut e,
            &algo,
            90.0,
            &algos(if algo_bad { &unknown_algo } else { "" }),
        );
        if terminal_children {
            assert!(e.store().orders().is_empty());
        } else {
            assert_eq!(e.store().orders(), before);
            assert!(e.drain_actions().any(|action| matches!(action,
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("working order market unavailable"))));
        }
    }
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _, mut reads, _) = start();
    load(&mut e, &mut reads);
    let now = 1000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let mut obsolete = stale;
    let mut current = None;
    for interval in [100_000_f64, 1_500.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(0.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending.iter().any(|r| r.url.contains("/v3/positions")),
        "actual native account request"
    );
    // Pause while REST is in flight. Completion may finish the current read
    // chain, but must not recreate an active recurring host timer.
    e.handle(
        due,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    e.drain_actions().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = if r.url.contains("accountinfo") {
                INFO.to_owned()
            } else if r.url.contains("/v3/positions") {
                POS.to_owned()
            } else if r.url.contains("algo") {
                algos(ALGO)
            } else {
                page(ORDER)
            };
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body: &body,
                    },
                },
            );
            pending.extend(e.drain_actions().filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            }));
        }
    }
    assert!(pending.is_empty(), "all fixture account reads complete");
    e.handle(
        due + 2.,
        Event::PollScaleChanged {
            scale: 10_000. / POLL_MS,
        },
    );
    let resumed: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(0.0));
}

#[test]
fn rest_ahead_of_no_id_execution_emits_once_without_moving_position_twice() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.take_frame(60.0);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let regular = take(&mut reads, "/v1/orders?");
    response(
        &mut e,
        &regular,
        80.0,
        &page(&ORDER.replace("0.005", "0.006")),
    );
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 90.0, &algos(ALGO));
    let frame = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedPrice":59000}}"#;
    ws(&mut e, socket, 100.0, frame);
    ws(&mut e, socket, 101.0, frame);
    let fills = fill_amounts(&mut e);
    assert_eq!(fills.len(), 1);
    assert!((fills[0] - 0.001).abs() < 1e-12);
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.006);
    assert_eq!(e.store().positions()[0].contracts, 0.01);
}

#[test]
fn no_id_execution_after_blind_rest_interval_does_not_reprice_the_gap() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.take_frame(60.0);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let regular = take(&mut reads, "/v1/orders?");
    response(
        &mut e,
        &regular,
        80.0,
        &page(&ORDER.replace("0.005", "0.010")),
    );
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 90.0, &algos(ALGO));
    let frame = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.011,"executedPrice":59000}}"#;
    ws(&mut e, socket, 100.0, frame);
    ws(&mut e, socket, 101.0, frame);
    let fills = fill_amounts(&mut e);
    assert_eq!(fills.len(), 1);
    assert!((fills[0] - 0.001).abs() < 1e-12);
    assert!((e.store().positions()[0].contracts - 0.011).abs() < 1e-12);
}

fn invalid_last_does_not_consume_cumulative_correction(last: f64) {
    let (mut e, socket, mut reads, _) = established_stream();
    load(&mut e, &mut reads);
    e.take_frame(60.0);
    let invalid = format!(
        r#"{{"topic":"executionreport","data":{{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedQuantity":{last},"executedPrice":59000,"tradeId":"correctable-last"}}}}"#
    );
    ws(&mut e, socket, 70.0, &invalid);
    assert!(fill_values(&mut e).is_empty());
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.006);
    assert_eq!(e.store().positions()[0].contracts, 0.01);

    // The correction retains the native identity and supplies cumulative-only
    // quantity evidence. Its valid order observation already reached display.
    let correction = r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedPrice":59000,"tradeId":"correctable-last"}}"#;
    ws(&mut e, socket, 71.0, correction);
    let fills = fill_values(&mut e);
    assert_eq!(
        fills.len(),
        1,
        "valid correction must deliver its execution"
    );
    assert_eq!(fills[0].0, 59000.0);
    assert!((fills[0].1 - 0.001).abs() < 1e-12);
    assert_eq!(e.store().positions()[0].contracts, 0.01);
    ws(&mut e, socket, 72.0, correction);
    assert!(fill_values(&mut e).is_empty(), "same native ID emits once");
}

#[test]
fn zero_last_quantity_keeps_cumulative_correction_eligible() {
    invalid_last_does_not_consume_cumulative_correction(0.0);
}

#[test]
fn negative_last_quantity_keeps_cumulative_correction_eligible() {
    invalid_last_does_not_consume_cumulative_correction(-0.001);
}

#[test]
fn unpriced_last_without_cumulative_does_not_double_count_the_correction() {
    for price in [0.0, -1.0] {
        let (mut e, socket, mut reads, _) = established_stream();
        load(&mut e, &mut reads);
        e.take_frame(60.0);
        let report = |price| {
            format!(
                r#"{{"topic":"executionreport","data":{{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"executedQuantity":0.001,"executedPrice":{price},"tradeId":"correctable-price"}}}}"#
            )
        };
        ws(&mut e, socket, 70.0, &report(price));
        assert!(fill_values(&mut e).is_empty());
        assert_eq!(e.store().positions()[0].contracts, 0.01);
        ws(&mut e, socket, 71.0, &report(59000.0));
        assert_eq!(fill_values(&mut e), [(59000.0, 0.001)]);
        assert!(
            (e.store().order("9007199254740993").unwrap().filled - 0.006).abs() < 1e-12,
            "the unpriced report and its correction describe one execution"
        );
        assert!((e.store().positions()[0].contracts - 0.011).abs() < 1e-12);
        ws(&mut e, socket, 72.0, &report(59000.0));
        assert!(fill_values(&mut e).is_empty());
        assert!((e.store().order("9007199254740993").unwrap().filled - 0.006).abs() < 1e-12);
        assert!((e.store().positions()[0].contracts - 0.011).abs() < 1e-12);
    }
}

// Private reads start before the ticker seed. A stop read then cannot be
// classified; when the seed lands mid-read, the stop is re-read at once
// instead of staying invisible until the next poll.
#[test]
fn stop_read_before_the_ticker_seed_is_reread_as_soon_as_prices_exist() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        application_id: Some("app".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let mut early = requests(&mut e);
    let ticker = take(&mut early, "/public/futures");
    let regular = take(&mut early, "/v1/orders?");
    let algo = take(&mut early, "/v3/algo/orders?");
    response(&mut e, &regular, 15.0, &page(""));
    let stop = r#"{"symbol":"PERP_BTC_USDT","algoOrderId":51,"algoType":"STOP","side":"SELL","quantity":0.01,"triggerPrice":60000.5,"triggerPriceType":"MARK_PRICE","totalExecutedQuantity":0,"reduceOnly":true}"#;
    // The stop's rows stream in before the seed; the read completes after it.
    let body = algos(stop);
    e.handle(
        16.0,
        Event::HttpStart {
            request: algo.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        16.0,
        Event::HttpChunk {
            request: algo.id,
            data: body.as_bytes(),
        },
    );
    assert!(e.store().order("51").is_none());
    response(&mut e, &ticker, 20.0, TICKERS);
    e.handle(25.0, Event::HttpEnd { request: algo.id });
    let mut reads = requests(&mut e);
    let reread = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &take(&mut reads, "/v1/orders?"), 30.0, &page(""));
    response(&mut e, &reread, 30.0, &algos(stop));
    assert_eq!(
        e.store()
            .order("51")
            .expect("stop visible after the re-read")
            .kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
}

const ETH_POSITION: &str = r#"{"symbol":"PERP_ETH_USDT","holding":1,"averageOpenPrice":3000,"markPrice":3100,"leverage":5,"marginMode":"CROSS"}"#;
const ETH_MARKET: &str = r#"{"symbol":"PERP_ETH_USDT","base_tick":0.01,"quote_tick":0.01,"base_min":0.01,"base_max":1000,"base_imr":0.05}"#;

fn positions_with_eth() -> String {
    POS.replace("]}}", &format!(",{ETH_POSITION}]}}}}"))
}

#[test]
fn an_anonymous_catalogue_row_never_holds_exposure_on_other_unlisted_markets() {
    let catalogue = MARKETS.replace("]}", r#",{"symbol":3}]}"#);
    let (mut e, _, mut reads, _) = start_with(&catalogue);
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 30.0, INFO);
    reads.extend(requests(&mut e));
    let pos = take(&mut reads, "/v3/positions");
    response(&mut e, &pos, 40.0, &positions_with_eth());
    let regular = take(&mut reads, "/v1/orders?");
    response(
        &mut e,
        &regular,
        50.0,
        &page(&ORDER.replace("PERP_BTC_USDT", "PERP_ETH_USDT")),
    );
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &algo, 60.0, &algos(ALGO));
    let loaded = e.store().loaded();
    assert!(loaded.balance && loaded.positions && loaded.orders);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "BTCUSDT");
    assert!(e.store().order("9007199254740993").is_none());
    assert!(e.store().orders().iter().all(|o| o.symbol == "BTCUSDT"));
    assert!(!e.store().orders().is_empty());
}

#[test]
fn a_position_on_a_named_malformed_catalogue_row_holds_the_snapshot() {
    let catalogue = MARKETS.replace("]}", r#",{"symbol":"PERP_ETH_USDT"}]}"#);
    let (mut e, _, mut reads, _) = start_with(&catalogue);
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 30.0, INFO);
    reads.extend(requests(&mut e));
    let pos = take(&mut reads, "/v3/positions");
    response(&mut e, &pos, 40.0, &positions_with_eth());
    assert!(!e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
}

#[test]
fn partial_catalogue_is_reread_and_releases_the_repaired_market() {
    let catalogue = MARKETS.replace("]}", r#",{"symbol":"PERP_ETH_USDT"}]}"#);
    let (mut e, _, mut reads, poll, retry) = start_with_retry(&catalogue);
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    let retry = retry.expect("partial catalogue schedules a re-read");
    e.handle(1010.0, Event::TimerFired { timer: retry });
    let reread = requests(&mut e);
    assert_eq!(reread.len(), 1);
    assert!(reread[0].url.ends_with("/v1/public/info"));
    response(
        &mut e,
        &reread[0],
        1020.0,
        &MARKETS.replace("]}", &format!(",{ETH_MARKET}]}}")),
    );
    assert!(e.store().market("ETHUSDT").is_some());
    let after: Vec<_> = e.drain_actions().collect();
    assert!(
        !after.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::WsConnect { .. } | Action::SetTimer { .. }
        )),
        "the refresh neither restarts the account nor schedules another re-read"
    );
    e.handle(2010.0, Event::TimerFired { timer: poll });
    let mut reads = requests(&mut e);
    let pos = take(&mut reads, "/v3/positions");
    response(&mut e, &pos, 2020.0, &positions_with_eth());
    assert!(e.store().positions().iter().any(|p| p.symbol == "ETHUSDT"));
}

#[test]
fn isolated_exposure_stays_visible_and_warns_once_per_symbol() {
    let (mut e, _, mut reads, timer) = start();
    let info = take(&mut reads, "accountinfo");
    response(&mut e, &info, 30.0, INFO);
    reads.extend(requests(&mut e));
    let pos = take(&mut reads, "/v3/positions");
    let isolated = POS.replace("CROSS", "ISOLATED");
    response(&mut e, &pos, 40.0, &isolated);
    let out = e.drain_actions().collect::<Vec<_>>();
    let warnings=out.iter().filter(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::CONFIG_INVALID&&event.message.contains("isolated")&&event.message.contains("BTCUSDT"))).count();
    assert_eq!(warnings, 1);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(
        e.store().positions()[0].margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    e.handle(2010.0, Event::TimerFired { timer });
    let mut reads = requests(&mut e);
    let pos = take(&mut reads, "/v3/positions");
    response(&mut e, &pos, 2020.0, &isolated);
    assert!(!e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::CONFIG_INVALID)));
    assert_eq!(e.store().positions().len(), 1);
}

#[test]
fn fractional_socket_open_uses_integer_auth_stamp_and_matching_signature() {
    let (mut e, socket, _, _, _) = start_with_retry_at(MARKETS, 1_699_999_999_970.0);
    e.drain_actions().for_each(drop);
    e.handle(1700000000000.875, Event::WsOpened { socket });
    let auth = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsSend { text, .. } if text.contains("timestamp") => {
                Some(serde_json::from_str::<serde_json::Value>(&text).unwrap())
            }
            _ => None,
        })
        .expect("auth frame");
    assert_eq!(auth["params"]["timestamp"], 1700000000000.0);
    assert_eq!(
        auth["params"]["sign"],
        safe_cex_crypto::hmac_sha256(
            b"replay-secret",
            b"|1700000000000",
            safe_cex_crypto::Encoding::Hex
        )
    );
}

#[test]
fn skipped_summary_repeats_on_text_change_and_rearms_on_clean_positions() {
    let (mut e, _, mut reads, mut timer) = start();
    load(&mut e, &mut reads);
    let a = POS.replace("\"leverage\":10", "\"leverage\":0");
    let b = a.replace("]}}", ",{}]}}");
    let mut summaries = Vec::new();
    for (i, body) in [&a, &a, &b, &a, POS, &a].into_iter().enumerate() {
        e.drain_actions().for_each(drop);
        let at = 2020.0 * (i + 1) as f64;
        e.handle(at, Event::TimerFired { timer });
        let actions: Vec<_> = e.drain_actions().collect();
        timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(*timer),
                _ => None,
            })
            .unwrap();
        let mut http: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        let p = take(&mut http, "/v3/positions");
        response(&mut e, &p, at + 5.0, body);
        summaries.extend(e.drain_actions().filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.message.starts_with("WOO positions: skipped") =>
            {
                Some(event.message)
            }
            _ => None,
        }));
    }
    assert_eq!(
        summaries,
        [
            "WOO positions: skipped 1 unmappable row(s) [BTCUSDT]",
            "WOO positions: skipped 2 unmappable row(s) [BTCUSDT]",
            "WOO positions: skipped 1 unmappable row(s) [BTCUSDT]",
            "WOO positions: skipped 1 unmappable row(s) [BTCUSDT]",
        ]
    );
}

#[test]
fn grouped_positions_count_unique_ids_and_each_anonymous_row_keep_cached_exposure() {
    let mut markets: serde_json::Value = serde_json::from_str(MARKETS).unwrap();
    let mut eth = markets["rows"][1].clone();
    eth["symbol"] = serde_json::json!("PERP_ETH_USDT");
    markets["rows"].as_array_mut().unwrap().push(eth);
    let (mut e, _, mut reads, timer) = start_with(&markets.to_string());
    assert!(e.store().market("ETHUSDT").is_some());
    load(&mut e, &mut reads);
    let before = e.store().positions().to_vec();
    assert_eq!(before.len(), 1);
    e.drain_actions().for_each(drop);
    let health = e.health().rows_skipped;
    e.handle(2020.0, Event::TimerFired { timer });
    let mut reads = requests(&mut e);
    let r = take(&mut reads, "/v3/positions");
    let bad = POS.replace("\"leverage\":10", "\"leverage\":0");
    let row = bad
        .split("\"positions\":[")
        .nth(1)
        .unwrap()
        .strip_suffix("]}}")
        .unwrap();
    let eth = row.replace("PERP_BTC_USDT", "PERP_ETH_USDT");
    response(
        &mut e,
        &r,
        2025.0,
        &format!(
            r#"{{"success":true,"data":{{"positions":[{eth},{row},{eth},{{}},{{"symbol":""}}]}}}}"#
        ),
    );
    let reports: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.message.starts_with("WOO positions: skipped") =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].level, safe_cex_engine::Level::Warn);
    assert_eq!(reports[0].code, safe_cex_engine::log::code::ROW_SKIPPED);
    assert_eq!(
        reports[0].message,
        "WOO positions: skipped 4 unmappable row(s) [ETHUSDT, BTCUSDT]"
    );
    assert_eq!(e.health().rows_skipped - health, 1);
    assert_eq!(e.store().positions(), before);
}

// Snapshot authority is distinct from the per-ID WS clocks.
#[test]
fn snapshot_token_rejects_both_join_orders_after_explicit_invalidation() {
    for algo_first in [false, true] {
        let (mut e, socket, mut reads, _) = start();
        load(&mut e, &mut reads);
        let before = e.store().orders().to_vec();
        e.drain_actions().for_each(drop);
        e.handle(70.0, Event::WsOpened { socket });
        let mut reads = requests(&mut e);
        let normal = take(&mut reads, "/v1/orders?");
        let algo = take(&mut reads, "/v3/algo/orders?");
        if algo_first {
            response(&mut e, &algo, 80.0, &algos(""));
        } else {
            response(&mut e, &normal, 80.0, &page(""));
        }
        e.store_mut().invalidate_order_snapshots();
        if algo_first {
            response(&mut e, &normal, 90.0, &page(""));
        } else {
            response(&mut e, &algo, 90.0, &algos(""));
        }
        assert_eq!(e.store().orders(), before, "algo_first={algo_first}");
        assert!(!requests(&mut e).iter().any(|r| r.url.contains("/orders?")));
    }
}

#[test]
fn snapshot_token_accepted_cancel_ack_fences_an_older_join() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    let before = e.store().orders().to_vec();
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let normal = take(&mut reads, "/v1/orders?");
    let algo = take(&mut reads, "/v3/algo/orders?");
    e.handle(
        75.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 907,
            symbol: "BTCUSDT".into(),
        }),
    );
    let writes = requests(&mut e);
    let cancel = writes
        .iter()
        .find(|r| r.method == safe_cex_engine::Method::Delete)
        .unwrap();
    response(&mut e, cancel, 76.0, r#"{"success":true}"#);
    response(&mut e, &normal, 80.0, &page(""));
    response(&mut e, &algo, 90.0, &algos(""));
    assert_eq!(e.store().orders(), before);
}

#[test]
fn snapshot_token_capture_and_late_exhaustion_leave_orders_and_polling_owned() {
    for late in [false, true] {
        let (mut e, socket, mut reads, timer) = start();
        load(&mut e, &mut reads);
        let before = e.store().orders().to_vec();
        e.drain_actions().for_each(drop);
        if !late {
            safe_cex_engine::test_support::exhaust_order_snapshots(e.store_mut());
        }
        e.handle(70.0, Event::WsOpened { socket });
        let mut reads = requests(&mut e);
        if late {
            let normal = take(&mut reads, "/v1/orders?");
            let algo = take(&mut reads, "/v3/algo/orders?");
            response(&mut e, &normal, 80.0, &page(""));
            safe_cex_engine::test_support::exhaust_order_snapshots(e.store_mut());
            response(&mut e, &algo, 90.0, &algos(""));
        } else {
            assert!(!reads.iter().any(|r| r.url.contains("/orders?")));
        }
        assert_eq!(e.store().orders(), before);
        e.drain_actions().for_each(drop);
        e.handle(2010.0, Event::TimerFired { timer });
        assert!(
            e.drain_actions()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
}

#[test]
fn snapshot_token_ws_peer_stays_current_and_healthy_algo_rows_commit() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let normal = take(&mut reads, "/v1/orders?");
    let algo = take(&mut reads, "/v3/algo/orders?");
    let sibling = e.store_mut().begin_orders_snapshot().unwrap();
    response(&mut e, &normal, 80.0, &page(ORDER));
    ws(
        &mut e,
        socket,
        85.0,
        r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedPrice":59000}}"#,
    );
    assert!(e.store().orders_snapshot_current(&sibling));
    response(&mut e, &algo, 90.0, &algos(&ALGO.replace("65000", "65500")));
    assert_eq!(e.store().order("9007199254740993").unwrap().filled, 0.006);
    assert_eq!(e.store().order("44").unwrap().price, 65500.0);
    assert!(e.store().orders_snapshot_current(&sibling));
}

#[test]
fn snapshot_token_invalid_placement_ack_does_not_invalidate_but_valid_id_does() {
    for (body, accepted) in [
        (r#"{"success":false,"message":"refused"}"#, false),
        (r#"{"success":true,"order_id":"bad"}"#, false),
        (r#"{"success":true,"order_id":123}"#, true),
    ] {
        let (mut e, _, mut reads, _) = start();
        load(&mut e, &mut reads);
        e.drain_actions().for_each(drop);
        let token = e.store_mut().begin_orders_snapshot().unwrap();
        let order = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":59000}"#,
        )
        .unwrap();
        e.handle(
            100.0,
            Event::Command(Command::PlaceOrders {
                command: 910,
                orders: vec![order],
            }),
        );
        let writes = requests(&mut e);
        let write = writes
            .iter()
            .find(|r| r.method == safe_cex_engine::Method::Post && r.url.ends_with("/v1/order"))
            .unwrap();
        response(&mut e, write, 110.0, body);
        assert_eq!(
            e.store().orders_snapshot_current(&token),
            !accepted,
            "{body}"
        );
    }
}

#[test]
fn snapshot_token_rejected_algo_join_does_not_poison_later_amend_routing() {
    let (mut e, socket, mut reads, _) = start();
    load(&mut e, &mut reads);
    ws(
        &mut e,
        socket,
        65.0,
        r#"{"topic":"algoexecutionreportv2","data":{"symbol":"PERP_BTC_USDT","algoOrderId":92,"rootAlgoOrderId":92,"algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","quantity":0.01,"triggerPrice":58000,"totalExecutedQuantity":0,"reduceOnly":true}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(70.0, Event::WsOpened { socket });
    let mut reads = requests(&mut e);
    let normal = take(&mut reads, "/v1/orders?");
    let algo = take(&mut reads, "/v3/algo/orders?");
    response(&mut e, &normal, 80.0, &page(""));
    e.store_mut().invalidate_order_snapshots();
    response(
        &mut e,
        &algo,
        90.0,
        &algos(&ALGO.replace("\"algoOrderId\":43", "\"algoOrderId\":92")),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        110.0,
        Event::Command(Command::AmendOrder {
            command: 92,
            id: "92".into(),
            price: Some(58500.0),
            amount: None,
        }),
    );
    let writes = requests(&mut e);
    let amend = writes
        .iter()
        .find(|r| r.url.ends_with("/v3/algo/order/92"))
        .unwrap();
    assert_eq!(amend.body.as_deref(), Some(r#"{"triggerPrice":"58500"}"#));
}

#[test]
fn snapshot_token_discarded_initial_join_cannot_seed_execution_prefix_or_readiness() {
    for fresh_read in [false, true] {
        let (mut e, socket, mut reads, _) = start();
        let info = take(&mut reads, "accountinfo");
        response(&mut e, &info, 30.0, INFO);
        reads.extend(requests(&mut e));
        let positions = take(&mut reads, "/v3/positions");
        response(&mut e, &positions, 40.0, POS);
        let normal = take(&mut reads, "/v1/orders?");
        let algo = take(&mut reads, "/v3/algo/orders?");
        response(&mut e, &normal, 50.0, &page(ORDER));
        e.store_mut().invalidate_order_snapshots();
        response(&mut e, &algo, 60.0, &algos(""));
        assert!(!e.store().loaded().orders);
        assert!(e.store().orders().is_empty());
        e.drain_actions().for_each(drop);
        if fresh_read {
            e.handle(70.0, Event::WsOpened { socket });
            let mut reads = requests(&mut e);
            let normal = take(&mut reads, "/v1/orders?");
            let algo = take(&mut reads, "/v3/algo/orders?");
            response(
                &mut e,
                &normal,
                75.0,
                &page(&ORDER.replace("0.005", "0.004")),
            );
            response(&mut e, &algo, 80.0, &algos(""));
            assert!(e.store().loaded().orders);
        }
        e.take_frame(85.0);
        ws(
            &mut e,
            socket,
            90.0,
            r#"{"topic":"executionreport","data":{"symbol":"PERP_BTC_USDT","orderId":9007199254740993,"status":"PARTIALLY_FILLED","side":"BUY","type":"LIMIT","quantity":0.02,"totalExecutedQuantity":0.006,"executedPrice":59000}}"#,
        );
        let amounts = fill_amounts(&mut e);
        assert_eq!(amounts.len(), 1);
        let expected = if fresh_read { 0.002 } else { 0.006 };
        assert!(
            (amounts[0] - expected).abs() < 1e-12,
            "fresh_read={fresh_read}: {amounts:?}"
        );
    }
}
