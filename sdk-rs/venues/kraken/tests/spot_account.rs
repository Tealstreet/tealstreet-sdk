mod spot_support;
use safe_cex_engine::{
    Action, Command, Event, Notice,
    model::{MarginType, OrderStatus},
};
use safe_cex_venue_kraken::sign::Signer;
use spot_support::*;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 10_000.0;

#[test]
fn official_spot_auth_vector_and_form_nonce_are_exact() {
    let signer = Signer::new(
        "kQH5HW/8p1uGOVjbgWA7FunAmGO8lsSUXNsu3eow76sz84Q18fWxnyRzBHCd3pd5nE9qa99HAZtuZuj6F1huXg==",
    )
    .unwrap();
    assert_eq!(
        signer
            .spot_signature(
                "nonce=1616492376594&ordertype=limit&pair=XBTUSD&price=37500&type=buy&volume=1.25",
                "1616492376594",
                "/0/private/AddOrder"
            )
            .unwrap(),
        "4/dpxb3iT4tp/ZCVEwSnEsLxx0bqyhLpdfOpc6fn7OR8+UClSV5n9E6aSS8MPtnRfp32bAb0nmbRn6H8ndwLUQ=="
    );
    assert!(
        signer
            .spot_signature("", "1", "/derivatives/api/v3/sendorder")
            .is_err()
    );
}
#[test]
fn multi_asset_balance_open_order_aliases_and_spot_settings() {
    let (e, _, _) = ready(false);
    let b = e.store().balance();
    assert_eq!((b.total, b.free, b.used), (1355., 1355., 0.));
    assert_eq!(e.store().currency_balances().unwrap().len(), 4);
    assert!(e.store().positions().is_empty());
    assert!(e.store().loaded().positions);
    for symbol in ["XBT/USD", "ETH/USD"] {
        let setting = e.store().setting(symbol).unwrap();
        assert_eq!(setting.leverage, 1.);
        assert_eq!(setting.margin_type, Some(MarginType::Isolated));
    }
    let o = e.store().order("O-100").unwrap();
    assert_eq!(o.symbol, "XBT/USD");
    assert_eq!((o.amount, o.filled, o.remaining), (2., 0.5, 1.5));
    assert!(!o.reduce_only);
}
#[test]
fn token_refreshed_per_socket_and_stale_token_never_subscribes_new_socket() {
    let (mut e, _, private) = ready_without_private_open();
    let old = private.unwrap();
    e.handle(NOW, Event::WsOpened { socket: old });
    let request = requests(&mut e).pop().unwrap();
    assert!(request.url.ends_with("/GetWebSocketsToken"));
    e.handle(
        NOW,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    let reconnect = e
        .drain_plain()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (NOW + 750.0..=NOW + 1000.).contains(&at_ms) => {
                Some(timer)
            }
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 1000., Event::TimerFired { timer: reconnect });
    let new = e
        .drain_plain()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 1000., Event::WsOpened { socket: new });
    let request2 = requests(&mut e).pop().unwrap();
    assert_ne!(request.body, request2.body);
    reply_at(
        &mut e,
        NOW + 1000.,
        &request,
        r#"{"error":[],"result":{"token":"stale"}}"#,
    );
    assert!(!e.drain_plain().any(|a| matches!(a, Action::WsSend { .. })));
    reply_at(
        &mut e,
        NOW + 1000.,
        &request2,
        r#"{"error":[],"result":{"token":"fresh"}}"#,
    );
    let messages = e
        .drain_plain()
        .filter_map(|a| match a {
            Action::WsSend { socket, text } => Some((socket, text)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(messages.len(), 2);
    assert!(
        messages
            .iter()
            .all(|(s, t)| *s == new && t.contains("fresh") && !t.contains("stale"))
    );
}
#[test]
fn malformed_snapshots_preserve_state_and_ws_fences_rest() {
    let (mut e, _, private) = ready(false);
    let s = private.unwrap();
    message(
        &mut e,
        s,
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let balance = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &balance,
        r#"{"error":[],"result":{"ZUSD":"invalid","XXBT":"2","XETH":"3","USDT":"5"}}"#,
    );
    assert_eq!(e.store().balance().total, 1355.);
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"snapshot","data":[{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","order_qty":"invalid"}]}"#,
    );
    assert!(e.store().order("O-100").is_some());
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","exec_type":"trade","cum_qty":1.5,"order_qty":2,"last_qty":1,"avg_price":99,"timestamp":"2023-11-14T22:13:21Z"}]}"#,
    );
    assert_eq!(e.store().order("O-100").unwrap().filled, 1.5);
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","exec_type":"new","cum_qty":0.5,"order_qty":2,"timestamp":"2023-11-14T22:13:20Z"}]}"#,
    );
    assert_eq!(e.store().order("O-100").unwrap().filled, 1.5);
}
#[test]
fn bad_balance_row_keeps_its_stored_asset_while_peers_update() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let balance = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &balance,
        r#"{"error":[],"result":{"ZUSD":"invalid","XXBT":"3","XETH":"3","USDT":"5"}}"#,
    );
    assert_eq!(e.store().balance().total, 1455.);
}
#[test]
fn bad_open_order_row_keeps_its_stored_copy_while_peers_commit() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","exec_type":"new","cum_qty":0.5,"order_qty":2,"timestamp":"2023-11-14T22:13:20Z"}]}"#,
    );
    let timer = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .next_back()
        .unwrap();
    e.handle(NOW + 5_000., Event::TimerFired { timer });
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("OpenOrders"))
        .unwrap();
    reply(
        &mut e,
        &read,
        r#"{"error":[],"result":{"open":{"O-100":{"descr":{"pair":"XBTUSD","type":"buy","ordertype":"limit","price":"99"},"vol":"x","vol_exec":"0.5"},"O-200":{"descr":{"pair":"ETHUSD","type":"sell","ordertype":"limit","price":"60"},"vol":"1","vol_exec":"0","opentm":1700000001}}}}"#,
    );
    assert_eq!(e.store().order("O-100").unwrap().amount, 2.);
    assert!(e.store().order("O-200").is_some());
}
#[test]
fn bad_snapshot_execution_row_keeps_its_stored_copy_while_peers_commit() {
    let peer = r#"{"order_id":"O-200","symbol":"ETH/USD","order_type":"limit","side":"sell","order_qty":1,"cum_qty":0,"limit_price":60}"#;
    for (bad, kept) in [
        (
            r#"{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","order_qty":"x"}"#,
            &["O-100", "O-200"][..],
        ),
        (
            r#"{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","order_qty":3,"cum_qty":5}"#,
            &["O-100", "O-200"],
        ),
        (
            r#"{"order_id":"GONE","symbol":"BTC/USD","order_type":"limit","side":"up","order_qty":1}"#,
            &["O-200"],
        ),
        (r#"{"order_qty":"x"}"#, &["O-100", "O-200"]),
    ] {
        let (mut e, _, private) = ready(false);
        let frame =
            format!(r#"{{"channel":"executions","type":"snapshot","data":[{bad},{peer}]}}"#);
        message(&mut e, private.unwrap(), &frame);
        let mut ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, kept, "{bad}");
        if let Some(o) = e.store().order("O-100") {
            assert_eq!(o.amount, 2., "{bad}");
        }
    }
}
#[test]
fn bad_update_execution_row_is_skipped_alone_and_emits_no_fill() {
    let (mut e, _, private) = ready(false);
    e.take_frame(NOW);
    let bad = r#"{"order_id":"O-100","symbol":"BTC/USD","exec_type":"trade","order_qty":"x","cum_qty":2,"last_qty":1.5,"last_price":100,"exec_id":"T-bad","timestamp":"2023-11-14T22:13:21Z"}"#;
    let peer = r#"{"order_id":"O-200","symbol":"ETH/USD","side":"sell","order_type":"limit","exec_type":"trade","order_qty":1,"cum_qty":0.5,"last_qty":0.5,"last_price":60,"exec_id":"T-good","timestamp":"2023-11-14T22:13:21Z"}"#;
    message(
        &mut e,
        private.unwrap(),
        &format!(r#"{{"channel":"executions","type":"update","data":[{bad},{peer}]}}"#),
    );
    assert_eq!(e.store().order("O-100").unwrap().filled, 0.5);
    assert_eq!(e.store().order("O-200").unwrap().filled, 0.5);
    assert_eq!(e.take_frame(NOW).map_or(0, support_fill_count), 1);
}
#[test]
fn snapshot_history_never_emits_and_live_execution_reconnect_duplicates_dedupe() {
    let (mut e, _, private) = ready(false);
    let s = private.unwrap();
    let frame = r#"{"channel":"executions","type":"snapshot","data":[{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","exec_type":"trade","order_qty":2,"cum_qty":0.5,"last_qty":0.5,"avg_price":99,"exec_id":"history","timestamp":"2023-11-14T22:13:20Z"}]}"#;
    message(&mut e, s, frame);
    let _ = e.take_frame(NOW).unwrap_or(&[]).to_vec();
    let live = r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","exec_type":"trade","order_qty":2,"cum_qty":1,"last_qty":0.5,"avg_price":99,"exec_id":"live-1","timestamp":"2023-11-14T22:13:21Z"}]}"#;
    message(&mut e, s, live);
    let frame = e.take_frame(NOW).unwrap_or(&[]).to_vec();
    assert_eq!(support_fill_count(&frame), 1);
    message(&mut e, s, live);
    let frame = e.take_frame(NOW).unwrap_or(&[]).to_vec();
    assert_eq!(support_fill_count(&frame), 0);
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","exec_type":"canceled","timestamp":"2023-11-14T22:13:22Z"}]}"#,
    );
    assert_eq!(
        e.store().order("O-100").unwrap().status,
        OrderStatus::Canceled
    );
}
fn support_fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + size;
    }
    0
}
#[test]
fn balance_read_signature_rejection_resigns_once() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let first = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &first,
        r#"{"error":["EAPI:Invalid signature"],"result":{}}"#,
    );
    let second = requests(&mut e).pop().unwrap();
    assert_ne!(first.body, second.body);
    assert_ne!(first.headers, second.headers);
    reply(
        &mut e,
        &second,
        r#"{"error":["EAPI:Invalid signature"],"result":{}}"#,
    );
    assert!(
        requests(&mut e).is_empty(),
        "a signature rejection resigns once"
    );
}

#[test]
fn balance_read_network_retry_is_bounded() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let request = requests(&mut e).pop().unwrap();
    e.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "timeout",
        },
    );
    let t = e
        .drain_plain()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 400., Event::TimerFired { timer: t });
    assert_eq!(requests(&mut e).len(), 1);
}

#[test]
fn posted_read_network_retry_stops_after_three_resends() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let mut request = requests(&mut e).pop().unwrap();
    let mut lastnonce = 0u64;
    for attempt in 0..4 {
        let nonce = request
            .body
            .as_ref()
            .unwrap()
            .split('&')
            .find_map(|p| p.strip_prefix("nonce="))
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!(nonce > lastnonce);
        lastnonce = nonce;
        e.handle(
            NOW,
            Event::HttpFailed {
                request: request.id,
                error: "connection reset",
            },
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        if attempt < 3 {
            let timer = actions
                .into_iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if at_ms == NOW => Some(timer),
                    _ => None,
                })
                .unwrap();
            e.handle(NOW, Event::TimerFired { timer });
            request = requests(&mut e).pop().unwrap();
        } else {
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
            );
        }
    }
}

#[test]
fn documented_native_trigger_object_maps_effective_price_without_treating_relative_offset_as_absolute()
 {
    let (mut e, _, private) = ready(false);
    let s = private.unwrap();
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"snapshot","data":[{"order_id":"STOP","symbol":"BTC/USD","side":"sell","order_type":"stop-loss","order_qty":2,"cum_qty":0,"triggers":{"price":90,"price_type":"static","actual_price":91}},{"order_id":"TP","symbol":"ETH/USD","side":"sell","order_type":"take-profit","order_qty":1,"cum_qty":0,"triggers":{"price":60,"price_type":"static"}},{"order_id":"TRAIL","symbol":"BTC/USD","side":"sell","order_type":"trailing-stop","order_qty":1,"cum_qty":0,"triggers":{"price":5,"price_type":"pct","actual_price":95}}]}"#,
    );
    assert_eq!(e.store().order("STOP").unwrap().price, 91.);
    assert_eq!(e.store().order("TP").unwrap().price, 60.);
    assert_eq!(e.store().order("TRAIL").unwrap().price, 95.);
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"update","data":[{"order_id":"TRAIL","symbol":"BTC/USD","order_type":"trailing-stop","order_qty":1,"cum_qty":0,"triggers":{"price":5,"price_type":"pct"}}]}"#,
    );
    assert_eq!(e.store().order("TRAIL").unwrap().price, 95.);
}

#[test]
fn externally_placed_iceberg_does_not_poison_mixed_order_snapshot() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"executions","type":"snapshot","data":[{"order_id":"LIMIT","symbol":"BTC/USD","side":"buy","order_type":"limit","order_qty":1,"cum_qty":0,"limit_price":99},{"order_id":"ICEBERG","symbol":"ETH/USD","side":"buy","order_type":"iceberg","order_qty":2,"cum_qty":0,"limit_price":49,"display_qty":0.5},{"order_id":"EXTERNAL-EUR","symbol":"ETH/EUR","side":"buy","order_type":"limit","order_qty":1,"cum_qty":0,"limit_price":40}]}"#,
    );
    assert_eq!(e.store().orders().len(), 2);
    assert_eq!(e.store().order("ICEBERG").unwrap().price, 49.);
    e.drain_plain().for_each(drop);
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 50,
            id: "ICEBERG".into(),
            price: Some(48.),
            amount: None,
        }),
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 50, .. })))
    );
}

#[test]
fn both_legacy_dogecoin_balance_codes_value_against_the_canonical_market_asset() {
    for asset in ["XDG", "XXDG"] {
        let balance = format!(r#"{{"error":[],"result":{{"ZUSD":"50","{asset}":"100"}}}}"#);
        let (e, _, _) = ready_with_payloads(false, DOGE_META, DOGE_TICKERS, &balance);
        assert_eq!(e.store().balance().total, 65.);
        let currency = e
            .store()
            .currency_balances()
            .unwrap()
            .iter()
            .find(|c| c.currency == "DOGE")
            .unwrap();
        assert_eq!(currency.balance.total, 100.);
        assert_eq!(currency.balance.free, 100.);
        assert_eq!(e.store().market("XDG/USD").unwrap().base, "DOGE");
    }
}

// WS v2 executions reports trade execution fields separately from order-status transitions.
// https://docs.kraken.com/exchange/api-reference/spot-websocket-v2/executions
#[test]
fn final_trade_and_filled_lifecycle_in_one_batch_emit_one_execution() {
    let trade = r#"{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"limit","exec_type":"trade","order_qty":2,"cum_qty":2,"last_qty":1.5,"last_price":100,"exec_id":"T-final","timestamp":"2023-11-14T22:13:21Z"}"#;
    let filled = r#"{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"limit","exec_type":"filled","order_qty":2,"cum_qty":2,"avg_price":100,"timestamp":"2023-11-14T22:13:21Z"}"#;
    for (first, last) in [(trade, filled), (filled, trade)] {
        let (mut e, _, private) = ready(false);
        e.take_frame(NOW);
        let frame =
            format!(r#"{{"channel":"executions","type":"update","data":[{first},{last}]}}"#);
        message(&mut e, private.unwrap(), &frame);
        let o = e.store().order("O-100").unwrap();
        assert_eq!(
            (o.filled, o.remaining, o.status),
            (2., 0., OrderStatus::Closed)
        );
        assert_eq!(e.take_frame(NOW).map_or(0, support_fill_count), 1);
        message(&mut e, private.unwrap(), &frame);
        assert_eq!(e.take_frame(NOW).map_or(0, support_fill_count), 0);
    }
}
fn prepared_fill_values(frame: &[u8]) -> Vec<(f64, f64)> {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let payload = &frame[at + 8..at + 8 + size];
            let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
            let values = (4 + 12 * count + 7) & !7;
            return (0..count)
                .map(|i| {
                    let offset = values + i * 16;
                    (
                        f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap()),
                        f64::from_le_bytes(payload[offset + 8..offset + 16].try_into().unwrap()),
                    )
                })
                .collect();
        }
        at += 8 + size;
    }
    Vec::new()
}

fn assert_rejected_price_correction(price: f64) {
    let (mut e, _, private) = ready(false);
    let socket = private.unwrap();
    e.take_frame(NOW);
    let bad = format!(
        r#"{{"channel":"executions","type":"update","data":[{{"order_id":"O-100","exec_type":"trade","cum_qty":1,"last_price":{price},"avg_price":100,"exec_id":"T-correctable","timestamp":"2023-11-14T22:13:21Z"}}]}}"#
    );
    message(&mut e, socket, &bad);
    assert!(prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])).is_empty());
    let corrected = r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","exec_type":"trade","cum_qty":1,"last_price":100,"exec_id":"T-correctable","timestamp":"2023-11-14T22:13:21Z"}]}"#;
    message(&mut e, socket, corrected);
    assert_eq!(
        prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])),
        vec![(100., 0.5)],
        "same-quantity correction after rejected price {price}"
    );
    message(&mut e, socket, corrected);
    assert!(prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])).is_empty());
}

#[test]
fn rejected_zero_price_trade_keeps_same_quantity_correction_eligible() {
    assert_rejected_price_correction(0.);
}

#[test]
fn rejected_negative_price_trade_keeps_same_quantity_correction_eligible() {
    assert_rejected_price_correction(-1.);
}

#[test]
fn rejected_trade_keeps_row_time_terminal_and_provenance_while_peer_commits() {
    let (mut e, _, private) = ready(false);
    let socket = private.unwrap();
    e.take_frame(NOW);
    let bad = r#"{"order_id":"O-100","order_type":"market","exec_type":"trade","cum_qty":2,"last_price":0,"exec_id":"T-bad-final","timestamp":"2023-11-14T22:13:22Z"}"#;
    let peer = r#"{"order_id":"O-200","symbol":"ETH/USD","side":"sell","order_type":"limit","exec_type":"trade","order_qty":1,"cum_qty":0.5,"last_qty":0.5,"last_price":60,"exec_id":"T-good","timestamp":"2023-11-14T22:13:21Z"}"#;
    message(
        &mut e,
        socket,
        &format!(r#"{{"channel":"executions","type":"update","data":[{bad},{peer}]}}"#),
    );
    let order = e.store().order("O-100").unwrap();
    assert_eq!(
        (order.filled, order.status, order.kind),
        (
            0.5,
            OrderStatus::Open,
            safe_cex_engine::model::OrderType::Limit
        )
    );
    assert_eq!(e.store().order("O-200").unwrap().filled, 0.5);
    assert_eq!(
        prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])),
        vec![(60., 0.5)]
    );
    let corrected = r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","exec_type":"trade","cum_qty":1,"last_price":100,"exec_id":"T-bad-final","timestamp":"2023-11-14T22:13:21Z"}]}"#;
    message(&mut e, socket, corrected);
    assert_eq!(
        prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])),
        vec![(100., 0.5)]
    );
    assert_eq!(
        e.store().order("O-100").unwrap().kind,
        safe_cex_engine::model::OrderType::Limit
    );
}

#[test]
fn rejected_trade_same_batch_correction_uses_the_last_accepted_row() {
    let bad = r#"{"order_id":"O-100","order_type":"market","exec_type":"trade","cum_qty":1,"last_price":0,"exec_id":"T-same-batch","timestamp":"2023-11-14T22:13:21Z"}"#;
    let corrected = r#"{"order_id":"O-100","exec_type":"trade","cum_qty":1,"last_price":100,"exec_id":"T-same-batch","timestamp":"2023-11-14T22:13:21Z"}"#;
    let peer = r#"{"order_id":"O-200","symbol":"ETH/USD","side":"sell","order_type":"limit","exec_type":"trade","order_qty":1,"cum_qty":0.5,"last_qty":0.5,"last_price":60,"exec_id":"T-good","timestamp":"2023-11-14T22:13:21Z"}"#;
    for (rows, expected) in [
        (
            format!("{bad},{corrected},{peer}"),
            vec![(100., 0.5), (60., 0.5)],
        ),
        (
            format!("{peer},{bad},{corrected}"),
            vec![(60., 0.5), (100., 0.5)],
        ),
    ] {
        let (mut e, _, private) = ready(false);
        let socket = private.unwrap();
        e.take_frame(NOW);
        let update = format!(r#"{{"channel":"executions","type":"update","data":[{rows}]}}"#);
        message(&mut e, socket, &update);
        assert_eq!(
            prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])),
            expected
        );
        let order = e.store().order("O-100").unwrap();
        assert_eq!(
            (order.filled, order.kind),
            (1., safe_cex_engine::model::OrderType::Limit)
        );
        message(&mut e, socket, &update);
        assert!(prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])).is_empty());
    }
}

#[test]
fn zero_quantity_and_no_growth_keep_native_observation_without_a_backlog() {
    let (mut e, _, private) = ready(false);
    let socket = private.unwrap();
    e.take_frame(NOW);
    for (row, filled) in [
        (
            r#"{"order_id":"O-100","exec_type":"trade","cum_qty":0.5,"last_price":0,"timestamp":"2023-11-14T22:13:21Z"}"#,
            0.5,
        ),
        (
            r#"{"order_id":"O-100","exec_type":"trade","cum_qty":1,"last_qty":0,"last_price":0,"timestamp":"2023-11-14T22:13:22Z"}"#,
            1.,
        ),
    ] {
        message(
            &mut e,
            socket,
            &format!(r#"{{"channel":"executions","type":"update","data":[{row}]}}"#),
        );
        assert!(prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])).is_empty());
        assert_eq!(e.store().order("O-100").unwrap().filled, filled);
    }
    message(
        &mut e,
        socket,
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","exec_type":"trade","cum_qty":1.5,"last_price":100,"timestamp":"2023-11-14T22:13:23Z"}]}"#,
    );
    assert_eq!(
        prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])),
        vec![(100., 0.5)]
    );
}

#[test]
fn unused_snapshot_and_lifecycle_prices_do_not_require_live_delivery_validation() {
    let (mut e, _, private) = ready(false);
    let socket = private.unwrap();
    e.take_frame(NOW);
    message(
        &mut e,
        socket,
        r#"{"channel":"executions","type":"snapshot","data":[{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"limit","order_qty":2,"exec_type":"trade","cum_qty":1,"last_price":0,"timestamp":"2023-11-14T22:13:21Z"}]}"#,
    );
    assert_eq!(e.store().order("O-100").unwrap().filled, 1.);
    assert!(prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])).is_empty());
    message(
        &mut e,
        socket,
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","exec_type":"filled","cum_qty":2,"last_price":0,"timestamp":"2023-11-14T22:13:22Z"}]}"#,
    );
    assert_eq!(
        e.store().order("O-100").unwrap().status,
        OrderStatus::Closed
    );
    assert!(prepared_fill_values(e.take_frame(NOW).unwrap_or(&[])).is_empty());
}

#[test]
fn cumulative_fallback_progress_uses_the_previous_row_in_the_same_batch() {
    let (mut e, _, private) = ready(false);
    e.take_frame(NOW);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","exec_type":"trade","cum_qty":1,"avg_price":100,"timestamp":"2023-11-14T22:13:21Z"},{"order_id":"O-100","exec_type":"trade","cum_qty":2,"avg_price":100,"timestamp":"2023-11-14T22:13:22Z"}]}"#,
    );
    let frame = e.take_frame(NOW).unwrap();
    assert_eq!(support_fill_count(frame), 2);
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let payload = &frame[at + 8..at + 8 + size];
            let n = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
            let offset = (4 + 12 * n + 7) & !7;
            let amounts = (0..n)
                .map(|i| {
                    f64::from_le_bytes(
                        payload[offset + i * 16 + 8..offset + i * 16 + 16]
                            .try_into()
                            .unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(amounts, vec![0.5, 1.]);
            return;
        }
        at += 8 + size;
    }
    panic!("missing fills");
}
#[test]
fn exhausted_signature_rejection_emits_terminal_auth_error_after_one_resend() {
    let (mut e, _, private) = ready(false);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let first = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &first,
        r#"{"error":["EAPI:Invalid signature"],"result":{}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    let retry = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_ne!(first.body, retry.body);
    reply(
        &mut e,
        &retry,
        r#"{"error":["EAPI:Invalid signature"],"result":{}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))if message.contains("Invalid signature"))));
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn malformed_nonempty_private_secret_fails_locally_before_io() {
    let mut opts = options();
    opts.secret = Some("invalid base64!".into());
    let mut e =
        safe_cex_engine::test_support::seeded_engine(safe_cex_venue_kraken::KrakenSpot::new(opts));
    e.start(NOW);
    let actions = e.drain_plain().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
    );
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))if message.contains("invalid secret"))));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

#[test]
fn distinct_same_order_native_trades_in_a_batch_remain_separate_fills() {
    let (mut e, _, private) = ready(false);
    e.take_frame(NOW);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"limit","exec_type":"trade","order_qty":2,"cum_qty":1,"last_qty":0.5,"last_price":99,"exec_id":"T-first","timestamp":"2023-11-14T22:13:21Z"},{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"limit","exec_type":"trade","order_qty":2,"cum_qty":2,"last_qty":1,"last_price":100,"exec_id":"T-second","timestamp":"2023-11-14T22:13:22Z"}]}"#,
    );
    assert_eq!(e.take_frame(NOW).map_or(0, support_fill_count), 2);
    assert_eq!(
        e.store().order("O-100").unwrap().status,
        OrderStatus::Closed
    );
}
#[test]
fn filled_lifecycle_without_trade_does_not_invent_an_execution() {
    let (mut e, _, private) = ready(false);
    e.take_frame(NOW);
    message(
        &mut e,
        private.unwrap(),
        r#"{"channel":"executions","type":"update","data":[{"order_id":"O-100","symbol":"BTC/USD","side":"buy","order_type":"limit","exec_type":"filled","order_qty":2,"cum_qty":2,"avg_price":100,"timestamp":"2023-11-14T22:13:21Z"}]}"#,
    );
    assert_eq!(
        e.store().order("O-100").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(e.take_frame(NOW).map_or(0, support_fill_count), 0);
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _, _) = ready(false);
    let now = NOW + 1000.;
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
        pending.iter().any(|r| r.url.ends_with("Balance")),
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
            let body = if r.url.ends_with("Balance") {
                BALANCE
            } else {
                OPEN
            };
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
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
fn balance_frames_faster_than_the_balance_read_cannot_starve_the_balance() {
    let (mut e, _, private) = ready(false);
    let s = private.unwrap();
    let frame = r#"{"channel":"balances","type":"update","data":[]}"#;
    message(&mut e, s, frame);
    let mut read = requests(&mut e).pop().unwrap();
    for step in 1..=3u32 {
        message(&mut e, s, frame);
        let body = format!(r#"{{"error":[],"result":{{"ZUSD":"{}"}}}}"#, 2000 + step);
        reply(&mut e, &read, &body);
        let expected = if step <= 2 { 1355. } else { 2003. };
        assert_eq!(e.store().balance().total, expected, "step {step}");
        read = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/Balance"))
            .expect("a stale read is re-read");
    }
}

fn spot_row_reports(
    e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>,
) -> Vec<String> {
    e.drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::ROW_SKIPPED =>
            {
                Some(event.message)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn row_report_namespaces_and_empty_order_identity_rearm_independently() {
    let (mut e, _, private) = ready(false);
    let socket = private.unwrap();
    spot_row_reports(&mut e);
    message(
        &mut e,
        socket,
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let balance = requests(&mut e).pop().unwrap();
    reply(&mut e, &balance, r#"{"error":[],"result":{"ZUSD":"bad"}}"#);
    assert_eq!(
        spot_row_reports(&mut e),
        ["Kraken Spot skipped malformed USD balance"]
    );
    let bad = r#"{"order_id":"USD","order_qty":"bad"},{"order_qty":"bad"}"#;
    let send = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>, rows: &str| {
        message(
            e,
            socket,
            &format!(r#"{{"channel":"executions","type":"snapshot","data":[{rows}]}}"#),
        );
        spot_row_reports(e)
    };
    assert_eq!(
        send(&mut e, bad),
        [
            "Kraken Spot skipped unmappable execution for order USD",
            "Kraken Spot skipped execution row without an order id"
        ]
    );
    assert!(send(&mut e, bad).is_empty());
    assert!(send(&mut e, "").is_empty());
    assert_eq!(
        send(&mut e, bad).len(),
        2,
        "clean order snapshot rearms only orders"
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"balances","type":"update","data":[]}"#,
    );
    let balance = requests(&mut e).pop().unwrap();
    reply(&mut e, &balance, r#"{"error":[],"result":{"ZUSD":"bad"}}"#);
    assert!(
        spot_row_reports(&mut e).is_empty(),
        "order cleanup did not forget coin:USD"
    );
}

#[test]
fn row_report_balance_rearm_follows_native_rejected_and_admitted_stale_reads() {
    for stale_reads in [1, 3] {
        let (mut e, _, private) = ready(false);
        let socket = private.unwrap();
        let frame = r#"{"channel":"balances","type":"update","data":[]}"#;
        message(&mut e, socket, frame);
        let read = requests(&mut e).pop().unwrap();
        reply(&mut e, &read, r#"{"error":[],"result":{"ZUSD":"bad"}}"#);
        assert_eq!(spot_row_reports(&mut e).len(), 1);
        message(&mut e, socket, frame);
        let mut read = requests(&mut e).pop().unwrap();
        for _ in 0..stale_reads {
            message(&mut e, socket, frame);
            reply(&mut e, &read, BALANCE);
            read = requests(&mut e)
                .into_iter()
                .find(|read| read.url.ends_with("/Balance"))
                .unwrap();
        }
        reply(&mut e, &read, r#"{"error":[],"result":{"ZUSD":"bad"}}"#);
        assert_eq!(
            spot_row_reports(&mut e).len(),
            usize::from(stale_reads == 3),
            "only the admitted stale clean balance rearms coin:USD"
        );
    }
}

#[test]
fn row_report_rest_accepted_duplicate_rearms_while_ws_rejected_duplicate_does_not() {
    let (mut e, _, private) = ready(false);
    let socket = private.unwrap();
    let bad = r#"{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","order_qty":"bad"}"#;
    let good = r#"{"order_id":"O-100","symbol":"BTC/USD","order_type":"limit","side":"buy","order_qty":2,"cum_qty":0.5,"limit_price":99}"#;
    let send = |e: &mut safe_cex_engine::Engine<safe_cex_venue_kraken::KrakenSpot>, rows: &str| {
        message(
            e,
            socket,
            &format!(r#"{{"channel":"executions","type":"snapshot","data":[{rows}]}}"#),
        );
        spot_row_reports(e)
    };
    assert_eq!(send(&mut e, bad).len(), 1);
    assert!(send(&mut e, &format!("{good},{bad}")).is_empty());
    assert!(
        send(&mut e, bad).is_empty(),
        "WS filters the rejected identity from rearm"
    );
    message(
        &mut e,
        socket,
        &format!(r#"{{"channel":"executions","type":"update","data":[{good},{bad}]}}"#),
    );
    let timer = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .next_back()
        .unwrap();
    e.handle(NOW + 5_000.0, Event::TimerFired { timer });
    let read = requests(&mut e)
        .into_iter()
        .find(|read| read.url.ends_with("OpenOrders"))
        .unwrap();
    reply(
        &mut e,
        &read,
        r#"{"error":[],"result":{"open":{"O-100":{"descr":{"pair":"XBTUSD","type":"buy","ordertype":"limit","price":"99"},"vol":"2","vol_exec":"0.5","opentm":1700000000},"O-100":{"vol":"bad"}}}}"#,
    );
    assert!(spot_row_reports(&mut e).is_empty());
    assert_eq!(
        send(&mut e, bad).len(),
        1,
        "REST preserves its native unfiltered accepted-ID rearm"
    );
}

#[test]
fn definitive_revoked_key_and_read_permission_still_report_auth_rejection() {
    for code in ["EAPI:Invalid key", "EGeneral:Permission denied"] {
        let (mut e, _, private) = ready(false);
        message(
            &mut e,
            private.unwrap(),
            r#"{"channel":"balances","type":"update","data":[]}"#,
        );
        let request = requests(&mut e).pop().unwrap();
        reply(&mut e, &request, &format!(r#"{{"error":["{code}"]}}"#));
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::Deactivate)))
        );
        assert!(actions.iter().any(
            |a| matches!(a, Action::Notice(Notice::Event(event)) if event.message.contains(code))
        ));
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}
