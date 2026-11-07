//! One agent's nonce sequence is shared by every engine in the process; a lower
//! nonce released after a higher one is still valid, as on Hyperliquid.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Permit, Prepared};
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
fn a_bound_write_survives_a_higher_nonce_released_by_another_engine_on_its_agent() {
    let (mut held, _) = ready("disabled");
    place(&mut held, 1, 2000.);
    let permit = bound_exchange_write(&mut held);
    // A main and a vault on one agent: the vault's write releases first.
    for command in 10..14 {
        release_write(&mut ready("disabled").0, command, 2000.);
    }
    let now = safe_cex_engine::test_support::dispatch_now(&held);
    assert!(matches!(
        held.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
}
