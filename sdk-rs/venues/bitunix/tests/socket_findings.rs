//! A socket order row that misnames the TIF, and an unanswered private login.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

fn frame(event: &str, effect: &str, client: &str) -> String {
    format!(
        r#"{{"ch":"order","ts":1,"data":{{"event":"{event}","symbol":"BTCUSDT","orderId":"777","positionMode":"HEDGE","side":"Buy","effect":"{effect}","type":"Limit","qty":"0.001","price":"90000","orderStatus":"NEW","averagePrice":"0","dealAmount":"0","clientId":"{client}"}}}}"#
    )
}

// Captured live frames carry "effect":"SHORT"; legacy caches only a recognised TIF.
#[test]
fn a_socket_row_naming_no_tif_keeps_post_only() {
    let (mut e, socket) = startup();
    prepared_now(&mut e);
    let mut o: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90000,"amount":0.001}"#,
    )
    .unwrap();
    o.time_in_force = Some(TimeInForce::PostOnly);
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 5,
            orders: vec![o],
        }),
    );
    let place = prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("place_order") => Some(r),
            _ => None,
        })
        .unwrap();
    let client = serde_json::from_str::<serde_json::Value>(place.body.as_deref().unwrap()).unwrap()
        ["clientId"]
        .as_str()
        .unwrap()
        .to_owned();
    // The venue's REST snapshot reports a POST_ONLY order as GTC.
    let rows = r#"{"code":0,"data":{"orderList":[{"symbol":"BTCUSDT","orderId":"777","side":"BUY","orderType":"LIMIT","qty":"0.001","tradeQty":"0","price":"90000","clientId":"CID","effect":"GTC","reduceOnly":false}]}}"#.replace("CID", &client);
    let mut timers = Vec::new();
    let absorb = |e: &mut TestEngine,
                  actions: Vec<Action>,
                  timers: &mut Vec<(safe_cex_engine::TimerId, f64)>,
                  at: f64| {
        let mut pending = actions;
        for _ in 0..20 {
            let mut next = Vec::new();
            for a in pending {
                match a {
                    Action::SetTimer { timer, at_ms } => timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => timers.retain(|(t, _)| *t != timer),
                    Action::Http(r) => {
                        let body = if r.url.contains("/trade/get_pending_orders") {
                            rows.as_str()
                        } else if r.url.contains("/tpsl/") {
                            r#"{"code":0,"data":[]}"#
                        } else if r.url.contains("get_pending_positions") {
                            POSITIONS
                        } else {
                            ACCOUNT
                        };
                        e.handle(
                            at,
                            Event::HttpResponse {
                                request: r.id,
                                response: safe_cex_engine::Response {
                                    status: 200,
                                    headers: "",
                                    body,
                                },
                            },
                        );
                        next.extend(prepared_now(e));
                    }
                    _ => {}
                }
            }
            if next.is_empty() {
                break;
            }
            pending = next;
        }
    };
    reply(
        &mut e,
        place.id,
        r#"{"code":0,"data":{"orderId":"777","clientId":null}}"#,
    );
    let a = prepared_now(&mut e);
    absorb(&mut e, a, &mut timers, 1.0);
    let mut now = 100.0;
    for event in ["CREATE", "UPDATE"] {
        e.handle(
            now,
            Event::WsMessage {
                socket,
                text: &frame(event, "SHORT", &client),
            },
        );
        let a = prepared_now(&mut e);
        absorb(&mut e, a, &mut timers, now);
    }
    // A private notification debounces the account re-read that carries the snapshot.
    now = 1_100.0;
    e.handle(
        now,
        Event::WsMessage {
            socket,
            text: r#"{"ch":"balance","data":{"coin":"USDT"}}"#,
        },
    );
    let a = prepared_now(&mut e);
    absorb(&mut e, a, &mut timers, now);
    while let Some(i) = (0..timers.len()).min_by(|a, b| timers[*a].1.total_cmp(&timers[*b].1)) {
        let (timer, at) = timers.remove(i);
        if at > 15_000.0 {
            break;
        }
        now = now.max(at);
        e.handle(now, Event::TimerFired { timer });
        let a = prepared_now(&mut e);
        absorb(&mut e, a, &mut timers, now);
    }
    let order = e.store().order("777").expect("the resting order");
    assert_eq!(order.time_in_force, Some(TimeInForce::PostOnly));
}

// auth.rejected switches the account to the refused-key path; silence is not a refusal.
#[test]
fn an_unanswered_login_is_not_an_auth_rejection() {
    let e = engine(Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Options::default()
    });
    let (mut e, _socket, _sent, mut queue) = startup_recording(e, PAIRS, TICKERS);
    let mut codes = Vec::new();
    for _ in 0..200 {
        queue.sort_by(|a, b| b.1.total_cmp(&a.1));
        let Some((timer, at)) = queue.pop() else {
            break;
        };
        if at > 30_000.0 {
            break;
        }
        e.handle(at, Event::TimerFired { timer });
        for a in prepared_now(&mut e) {
            match a {
                Action::Notice(Notice::Event(ev)) => codes.push(ev.code.to_string()),
                Action::SetTimer { timer, at_ms } => queue.push((timer, at_ms)),
                Action::ClearTimer { timer } => queue.retain(|(t, _)| *t != timer),
                Action::WsConnect { socket, .. } => e.handle(at, Event::WsOpened { socket }),
                _ => {}
            }
        }
    }
    assert!(codes.iter().any(|c| c == "stream.unavailable"), "{codes:?}");
    assert!(!codes.iter().any(|c| c == "auth.rejected"), "{codes:?}");
}
