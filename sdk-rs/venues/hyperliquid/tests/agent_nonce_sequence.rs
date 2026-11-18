//! The engine keeps one nonce sequence per agent address for the whole process.
mod support;
use safe_cex_engine::request_context::{Claim, Error, HttpAction, Permit, Prepared};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest};
use safe_cex_venue_hyperliquid::Hyperliquid;
use support::*;

fn place(e: &mut Engine<Hyperliquid>, command: u32, now: f64) {
    e.handle(
        now,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![
                serde_json::from_str(
                    r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","amount":1,"price":100}"#,
                )
                .unwrap(),
            ],
        }),
    );
}

/// Prepares the next exchange write up to its Bind, leaving the Claim to the caller.
fn bound_exchange_write(e: &mut Engine<Hyperliquid>) -> Permit {
    let now = safe_cex_engine::test_support::dispatch_now(e);
    e.prepare_dispatch(now);
    let mut queue = std::collections::VecDeque::from(e.drain_actions().collect::<Vec<_>>());
    while let Some(action) = queue.pop_front() {
        if let Action::ManagedHttp(action) = action {
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if e.http_preparation_pending(preparation) => e.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id: request.id.0,
                        expires_at: None,
                    },
                ),
                HttpAction::Bind {
                    permit, request, ..
                } if request.url.ends_with("/exchange") => {
                    return permit;
                }
                _ => {}
            }
        }
        queue.extend(e.drain_actions());
    }
    panic!("no exchange write reached Bind");
}

/// Sends one write and settles it, releasing a nonce above every earlier reservation.
fn release_write(e: &mut Engine<Hyperliquid>, command: u32, now: f64) -> HttpRequest {
    place(e, command, now);
    let actions = e.drain_plain().collect();
    let request = advance_packet_timers(e, actions, now + 334.)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/exchange") => Some(request),
            _ => None,
        })
        .expect("exchange write");
    reply(
        e,
        &request,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":1}}]}}}"#,
    );
    e.drain_plain().for_each(drop);
    request
}

#[test]
fn same_agent_releases_supersede_a_bound_write_only_past_the_venues_window() {
    let (mut held, _) = ready("disabled");
    place(&mut held, 1, 2000.);
    let permit = bound_exchange_write(&mut held);
    // Hyperliquid keeps a signer's 100 highest nonces: 99 higher releases leave
    // this write valid; the 100th puts it below the lowest kept one.
    for _ in 0..100 {
        release_write(&mut ready("disabled").0, 10, 2000.);
    }
    let now = safe_cex_engine::test_support::dispatch_now(&held);
    assert!(matches!(held.claim_http(now, permit), Err(Error::Stale)));
    let permit = bound_exchange_write(&mut held);
    let now = safe_cex_engine::test_support::dispatch_now(&held);
    assert!(matches!(
        held.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
}

#[test]
fn ninety_nine_same_agent_releases_leave_a_bound_write_valid() {
    let (mut held, _) = ready("disabled");
    place(&mut held, 1, 2000.);
    let permit = bound_exchange_write(&mut held);
    for _ in 0..99 {
        release_write(&mut ready("disabled").0, 10, 2000.);
    }
    let now = safe_cex_engine::test_support::dispatch_now(&held);
    assert!(matches!(
        held.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
}

#[test]
fn another_tests_ready_engine_cannot_supersede_this_tests_bound_write() {
    let (mut held, _) = ready("disabled");
    place(&mut held, 1, 2000.);
    let permit = bound_exchange_write(&mut held);
    std::thread::Builder::new()
        .name("another_test".into())
        .spawn(|| {
            let (mut other, _) = ready("disabled");
            release_write(&mut other, 10, 2000.);
        })
        .unwrap()
        .join()
        .unwrap();
    let now = safe_cex_engine::test_support::dispatch_now(&held);
    assert!(matches!(
        held.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
}
