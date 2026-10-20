mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_woo::{Options, Woo};
use support::*;
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
    let mut e = Engine::new(Woo::new(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        application_id: Some("app".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let r = requests(&mut e).remove(0);
    response(&mut e, &r, 20.0, TICKERS);
    let actions: Vec<_> = e.drain_actions().collect();
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
                (*at_ms == 2020.0).then_some(*timer)
            } else {
                None
            }
        })
        .unwrap();
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
    (e, socket, reads, timer)
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
            f64::from_le_bytes(
                payload[rows + 16 * index + 8..rows + 16 * index + 16]
                    .try_into()
                    .unwrap(),
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
            result: Ok(_)
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
    let watchdog = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 6030.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(21000.0, Event::TimerFired { timer: watchdog });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    );
    let reconnect = actions
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 22000.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(22000.0, Event::TimerFired { timer: reconnect });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsConnect{url,channel:Some(SocketChannel::Private),context:None,..}if url == "wss://wss.woox.io/v2/ws/private/stream/app"))
    );
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
fn malformed_known_order_cannot_turn_successful_snapshot_into_empty() {
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
    assert_eq!(e.store().orders().len(), 3);
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
fn malformed_executable_algo_leaf_never_clears_known_orders() {
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
        response(&mut e, &r, 90.0, &algos(&malformed));
        assert_eq!(e.store().orders().len(), 3);
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, _, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
    let (mut e, socket, mut reads, _) = start();
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
        let regular = take(&mut reads, "/v1/orders?");
        response(&mut e, &regular, 80.0, &page(""));
        let algo = take(&mut reads, "/v3/algo/orders?");
        let reclassified = ALGO.replace("\"algoOrderId\":43", "\"algoOrderId\":92");
        let body = if failed_second_page {
            algos(&reclassified)
                .replace("\"total\":1", "\"total\":2")
                .replace("\"records_per_page\":100", "\"records_per_page\":1")
        } else {
            algos(&reclassified).replace("\"current_page\":1", "\"current_page\":2")
        };
        response(&mut e, &algo, 90.0, &body);
        if failed_second_page {
            let mut reads = requests(&mut e);
            let second = take(&mut reads, "/v3/algo/orders?");
            response(&mut e, &second, 100.0, &algos(""));
        }
        assert_eq!(
            e.store().order("92").unwrap().parent_id.as_deref(),
            Some("92")
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
        let mut writes = requests(&mut e);
        let amend = take(&mut writes, "/v3/algo/order/92");
        assert_eq!(
            amend.body.as_deref(),
            Some(r#"{"triggerPrice":"58500"}"#),
            "failed_second_page={failed_second_page}"
        );
    }
}
