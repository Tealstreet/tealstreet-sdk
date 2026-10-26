//! Absolute account-wide deadlines are signed controls, never local cancels.
use safe_cex_engine::{
    Action, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome, Response, Venue,
};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
const NOW: f64 = 1_800_000_000_000.0;
const OK: &str = r#"{"status":"ok","response":{"type":"default"}}"#;
fn options() -> Options {
    Options {
        secret: Some(format!("0x{}", "11".repeat(32))),
        ..Default::default()
    }
}
fn engine() -> Engine<Hyperliquid> {
    safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(options()))
}
fn command(e: &mut Engine<Hyperliquid>, owner: u32, extra: &str, now: f64) -> Vec<Action> {
    let json = format!(r#"{{"op":"scheduleCancel","command":{owner}{extra}}}"#);
    let command = safe_cex_engine::command::decode_command(&json)
        .expect("standalone scheduleCancel JSON admission");
    e.handle(now, Event::Command(command));
    e.drain_actions().collect()
}
fn write(actions: Vec<Action>) -> HttpRequest {
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1
    );
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn reply(
    e: &mut Engine<Hyperliquid>,
    request: &HttpRequest,
    status: u16,
    body: &str,
    now: f64,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn failed(actions: &[Action], owner: u32, outcome: Outcome) {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) => {
                assert_eq!(*command, owner);
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(outcome));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}
#[test]
fn perp_schedule_capability_is_enabled_while_spot_is_refused() {
    let perp = serde_json::to_value(Hyperliquid::new(options()).capabilities()).unwrap();
    assert_eq!(perp["scheduleCancel"], true);
    assert!(perp.get("hostClock").is_none());
    let spot = serde_json::to_value(Hyperliquid::new_spot(options()).capabilities()).unwrap();
    assert_eq!(spot["scheduleCancel"], false);
    assert!(spot.get("hostClock").is_none());
}
#[test]
fn arm_clear_and_null_are_signed_without_catalogue_or_order_side_effects() {
    for extra in [",\"time\":1800000030000", "", ",\"time\":null"] {
        let mut e = engine();
        let request = write(command(&mut e, 7, extra, NOW));
        let body: serde_json::Value = serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
        assert_eq!(request.url, "https://api.hyperliquid.xyz/exchange");
        assert_eq!(body["action"]["type"], "scheduleCancel");
        if extra.contains("1800000030000") {
            assert_eq!(body["action"]["time"], 1_800_000_030_000u64);
        } else {
            assert!(body["action"].get("time").is_none());
        }
        assert!(body["signature"]["r"].as_str().unwrap().starts_with("0x"));
        assert!(body.get("expiresAfter").is_none());
        assert!(body.get("vaultAddress").is_none());
        let actions = reply(&mut e, &request, 200, OK, NOW + 1.0);
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. })))
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
        );
        assert!(e.store().markets().is_empty());
        assert!(e.store().orders().is_empty());
    }
}
#[test]
fn unsafe_or_near_deadlines_refuse_before_http_and_exact_five_seconds_is_accepted() {
    for extra in [
        ",\"time\":1800000004999",
        ",\"time\":1800000005000.5",
        ",\"time\":9007199254740992",
        ",\"time\":-1",
    ] {
        let mut e = engine();
        let actions = command(&mut e, 7, extra, NOW);
        failed(&actions, 7, Outcome::NotSent);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
    let mut e = engine();
    write(command(&mut e, 7, ",\"time\":1800000005000", NOW));
}
#[test]
fn public_only_spot_invalid_signer_and_invalid_vault_cannot_send_even_clear() {
    let mut scenarios = vec![
        Hyperliquid::new(Options {
            public_data_only: true,
            ..options()
        }),
        Hyperliquid::new_spot(options()),
        Hyperliquid::new(Options {
            secret: Some("invalid".into()),
            ..Default::default()
        }),
        Hyperliquid::new(Options {
            vault: Some("invalid".into()),
            ..options()
        }),
    ];
    for venue in scenarios.drain(..) {
        let mut e = safe_cex_engine::test_support::seeded_engine(venue);
        let actions = command(&mut e, 7, "", NOW);
        failed(&actions, 7, Outcome::NotSent);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}
#[test]
fn malformed_acks_and_timeouts_never_prove_arming_or_trigger_reconciliation() {
    for (status, body) in [
        (200, r#"{"status":"ok"}"#),
        (200, r#"["ok",{"type":"default"}]"#),
        (200, r#"{"status":"ok","response":["default"]}"#),
        (200, r#"{"status":"ok","response":{"type":"order"}}"#),
        (
            200,
            r#"{"status":"ok","response":{"type":"default","data":{"statuses":["success"]}}}"#,
        ),
        (408, r#"{"status":"err","response":"refused"}"#),
        (503, r#"{"status":"err","response":"refused"}"#),
    ] {
        let mut e = engine();
        let r = write(command(&mut e, 7, "", NOW));
        let actions = reply(&mut e, &r, status, body, NOW + 1.0);
        failed(&actions, 7, Outcome::Unknown);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
        );
    }
}
#[test]
fn proven_refusal_and_lost_response_have_distinct_owned_outcomes_without_retry() {
    let mut e = engine();
    let r = write(command(&mut e, 7, "", NOW));
    let actions = reply(
        &mut e,
        &r,
        200,
        r#"{"status":"err","response":"native denied"}"#,
        NOW + 1.0,
    );
    failed(&actions, 7, Outcome::Rejected);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    let mut e = engine();
    let r = write(command(&mut e, 8, "", NOW));
    e.handle(
        NOW + 1.0,
        Event::HttpFailed {
            request: r.id,
            error: "lost response",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    failed(&actions, 8, Outcome::Unknown);
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed {error,..}) if error.kind==Some(ErrorKind::Network))));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
    );
    assert!(reply(&mut e, &r, 200, OK, NOW + 2.0).is_empty());
}
#[test]
fn queued_deadline_is_rechecked_before_signing_and_next_owner_progresses() {
    let mut e = engine();
    let first = write(command(&mut e, 7, "", NOW));
    assert!(command(&mut e, 8, ",\"time\":1800000005000", NOW).is_empty());
    assert!(command(&mut e, 9, "", NOW).is_empty());
    let actions = reply(&mut e, &first, 200, OK, NOW + 1.0);
    let (timer, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    assert_eq!(at, NOW + 334.0);
    e.handle(at, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    failed(&actions, 8, Outcome::NotSent);
    let next = write(actions);
    let actions = reply(&mut e, &next, 200, OK, at + 1.0);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 9, .. })))
    );
}

#[test]
fn nonfinite_typed_deadlines_cannot_turn_into_clear_actions() {
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine();
        e.handle(
            NOW,
            Event::Command(safe_cex_engine::Command::Venue {
                command: 7,
                action: safe_cex_engine::VenueAction::ScheduleCancel {
                    time_ms: Some(time),
                },
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        failed(&actions, 7, Outcome::NotSent);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn refused_malformed_and_lost_replies_release_the_next_owner_once() {
    for scenario in [0, 1, 2] {
        let mut e = engine();
        let r = write(command(&mut e, 7, "", NOW));
        assert!(command(&mut e, 8, "", NOW).is_empty());
        let actions = if scenario == 2 {
            e.handle(
                NOW + 1.0,
                Event::HttpFailed {
                    request: r.id,
                    error: "lost",
                },
            );
            e.drain_actions().collect()
        } else {
            reply(
                &mut e,
                &r,
                200,
                if scenario == 0 {
                    r#"{"status":"err","response":"denied"}"#
                } else {
                    "[]"
                },
                NOW + 1.0,
            )
        };
        failed(
            &actions,
            7,
            if scenario == 0 {
                Outcome::Rejected
            } else {
                Outcome::Unknown
            },
        );
        let (timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap();
        e.handle(at, Event::TimerFired { timer });
        let next = write(e.drain_actions().collect());
        assert!(reply(&mut e, &r, 200, OK, at + 1.0).is_empty());
        let actions = reply(&mut e, &next, 200, OK, at + 2.0);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. })))
                .count(),
            1
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn native_refusal_preserves_kind_message_and_http_uncertainty() {
    for (status, outcome, kind) in [
        (200, Outcome::Rejected, ErrorKind::Rejected),
        (401, Outcome::Rejected, ErrorKind::AuthInvalid),
        (403, Outcome::Rejected, ErrorKind::AuthPermission),
        (429, Outcome::Rejected, ErrorKind::RateLimited),
        (408, Outcome::Unknown, ErrorKind::Timeout),
        (503, Outcome::Unknown, ErrorKind::VenueUnavailable),
    ] {
        let mut e = engine();
        let r = write(command(&mut e, 7, "", NOW));
        let actions = reply(
            &mut e,
            &r,
            status,
            r#"{"status":"err","response":"native denied"}"#,
            NOW + 1.0,
        );
        failed(&actions, 7, outcome);
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.venue_message.as_deref(), Some("native denied"));
    }
}

#[test]
fn network_and_vault_payloads_use_shared_monotonic_signer_nonce() {
    let mut previous = 0;
    for testnet in [false, true] {
        for vault in [None, Some(format!("0x{}", "33".repeat(20)))] {
            let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
                testnet,
                vault: vault.clone(),
                ..options()
            }));
            let r = write(command(&mut e, 7, "", NOW));
            assert_eq!(
                r.url,
                if testnet {
                    "https://api.hyperliquid-testnet.xyz/exchange"
                } else {
                    "https://api.hyperliquid.xyz/exchange"
                }
            );
            let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["vaultAddress"].as_str(), vault.as_deref());
            let nonce = body["nonce"].as_u64().unwrap();
            assert!(nonce > previous);
            previous = nonce;
        }
    }
}
