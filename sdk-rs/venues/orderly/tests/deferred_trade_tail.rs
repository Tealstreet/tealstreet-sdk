//! O1 interim (`docs/plans/RUST_ORDERLY_PROVIDER_READS.md`): a managed account
//! delivery cannot complete a legacy trading command, so a pair the snapshot
//! confirms completes on the next tick, exactly once. O2 deletes this.
mod support;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice};
use support::*;

const ALGO_WITH_PAIR: &str = r#"{"success":true,"data":{"rows":[{"algo_order_id":200,"root_algo_order_id":200,"symbol":"PERP_BTC_USDC","algo_type":"POSITIONAL_TP_SL","algo_status":"NEW","margin_mode":"CROSS","child_orders":[{"algo_order_id":201,"root_algo_order_id":200,"algo_type":"STOP_LOSS","side":"SELL","algo_status":"NEW","trigger_price":48000,"type":"CLOSE_POSITION","reduce_only":true,"quantity":0}]}],"meta":{"total":1}}}"#;

fn terminals(out: &[Action]) -> usize {
    out.iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandDone { command: 9, .. })
                    | Action::Notice(Notice::CommandFailed { command: 9, .. })
            )
        })
        .count()
}

fn http(out: &[Action]) -> Vec<HttpRequest> {
    out.iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}

/// Bootstraps like `support::ready`, tracking every armed timer.
fn ready_clocked() -> (Driver, Clock) {
    let mut e = engine(options());
    let mut clock = Clock::new(1000.0);
    e.start(1000.0);
    let mut private = None;
    for _ in 0..10 {
        let out = clock.drain(&mut e);
        if out.is_empty() {
            break;
        }
        for a in out {
            match a {
                Action::Http(r) => reply(
                    &mut e,
                    &r,
                    200,
                    &account_body(&r.url, ORDER, "[]", POSITIONS),
                ),
                Action::WsConnect { socket, url, .. } if url.contains("private") => {
                    private = Some(socket)
                }
                _ => {}
            }
        }
    }
    let socket = private.expect("the private socket");
    let mut pending = clock.handle(&mut e, Event::WsOpened { socket });
    pending.extend(clock.handle(
        &mut e,
        Event::WsMessage {
            socket,
            text: r#"{"event":"auth","success":true}"#,
        },
    ));
    for r in http(&pending) {
        reply(
            &mut e,
            &r,
            200,
            &account_body(&r.url, ORDER, "[]", POSITIONS),
        );
    }
    // Answer the post-auth polls and run their reconcile ticks until no read
    // is outstanding.
    for _ in 0..10 {
        let mut out = clock.drain(&mut e);
        out.extend(clock.advance(&mut e, 1000.0).into_iter().map(|(_, a)| a));
        let reads = http(&out);
        if reads.is_empty() {
            break;
        }
        for r in reads {
            reply(
                &mut e,
                &r,
                200,
                &account_body(&r.url, ORDER, "[]", POSITIONS),
            );
        }
    }
    (e, clock)
}

#[test]
fn a_snapshot_confirmed_pair_completes_on_the_next_tick_exactly_once() {
    let (mut e, mut clock) = ready_clocked();
    let orders = serde_json::from_str(
        r#"[{"symbol":"BTCUSDC","type":"stop_market","side":"sell","amount":1,"price":48000,"reduceOnly":true}]"#,
    )
    .unwrap();
    let out = clock.handle(
        &mut e,
        Event::Command(Command::PlaceOrders { command: 9, orders }),
    );
    let pair = http(&out).pop().expect("the pair write");
    reply(
        &mut e,
        &pair,
        200,
        r#"{"success":true,"data":{"order_id":200,"rows":[{"orderId":201}]}}"#,
    );
    let algo = http(&clock.drain(&mut e))
        .into_iter()
        .find(|r| r.url.contains("/v1/algo/orders"))
        .expect("the confirming snapshot read");
    reply(&mut e, &algo, 200, ALGO_WITH_PAIR);
    // Nothing completes inside the managed delivery itself.
    assert_eq!(terminals(&clock.drain(&mut e)), 0);
    // The zero-delay tick completes it once, with no further write.
    let fired: Vec<_> = clock
        .advance(&mut e, 1000.0)
        .into_iter()
        .map(|(_, a)| a)
        .collect();
    assert_eq!(terminals(&fired), 1);
    assert!(http(&fired).is_empty());
    assert_eq!(terminals(&clock.drain(&mut e)), 0);
}
