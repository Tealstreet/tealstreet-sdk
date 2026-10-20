mod spot_support;
use safe_cex_engine::{
    Action, Command, Event, Notice,
    model::{OrderStatus, PositionSide},
};
use safe_cex_venue_kraken::sign::Signer;
use spot_support::*;
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
fn multi_asset_balance_open_order_aliases_and_fake_positions() {
    let (e, _, _) = ready(false);
    let b = e.store().balance();
    assert_eq!((b.total, b.free, b.used), (1355., 1355., 0.));
    assert_eq!(e.store().currency_balances().unwrap().len(), 4);
    assert_eq!(e.store().positions().len(), 4);
    assert!(e.store().positions().iter().any(|p| p.id == "XBT/USD:long"
        && p.side == PositionSide::Long
        && p.contracts == 0.
        && p.leverage == 1.));
    let o = e.store().order("O-100").unwrap();
    assert_eq!(o.symbol, "XBT/USD");
    assert_eq!((o.amount, o.filled, o.remaining), (2., 0.5, 1.5));
    assert!(!o.reduce_only);
}
#[test]
fn token_refreshed_per_socket_and_stale_token_never_subscribes_new_socket() {
    let (mut e, _, private) = ready(false);
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
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == NOW + 1000. => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 1000., Event::TimerFired { timer: reconnect });
    let new = e
        .drain_actions()
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
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    reply_at(
        &mut e,
        NOW + 1000.,
        &request2,
        r#"{"error":[],"result":{"token":"fresh"}}"#,
    );
    let messages = e
        .drain_actions()
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
        r#"{"error":[],"result":{"ZUSD":"invalid"}}"#,
    );
    assert_eq!(e.store().balance().total, 1355.);
    message(
        &mut e,
        s,
        r#"{"channel":"executions","type":"snapshot","data":[{"order_id":"bad","symbol":"BTC/USD","order_type":"limit","side":"buy","order_qty":"invalid"}]}"#,
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
fn balance_read_signature_rejection_resigns_once_and_network_retry_is_bounded() {
    let (mut e, _, _) = ready(false);
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 10 }));
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
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 10,
            result: Err(_)
        })
    )));
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 11 }));
    let request = requests(&mut e).pop().unwrap();
    e.handle(
        NOW,
        Event::HttpFailed {
            request: request.id,
            error: "timeout",
        },
    );
    let t = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 400., Event::TimerFired { timer: t });
    assert_eq!(requests(&mut e).len(), 1);
}
