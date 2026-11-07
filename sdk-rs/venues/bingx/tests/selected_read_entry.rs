//! Frozen native controls for the shared selected Read issuer.
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared, RoutingIntent};
use safe_cex_engine::signing::{Identity, KeyRef, Operation, SignResult};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};
use safe_cex_venue_bingx::{Bingx as Venue, Options};
fn engine(secondary: bool) -> Engine<Venue> {
    safe_cex_engine::test_support::seeded_engine(Venue::new(Options {
        credentials: Some(identity("primary")),
        read_credentials: secondary.then(|| identity("secondary")),
        ..Options::default()
    }))
}

const NOW: f64 = 1_000.;
fn identity(name: &str) -> Identity {
    Identity::new(
        KeyRef(format!("opaque:{name}:credential")),
        Some(KeyRef(format!("opaque:{name}:signing"))),
    )
    .with_passphrase(Some(KeyRef(format!("opaque:{name}:passphrase"))))
    .with_stable_id(Some("native-primary-account".into()))
}
fn ask(e: &mut Engine<Venue>, command: u32) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command }));
    e.drain_actions().collect()
}
fn prepare(actions: &[Action]) -> (Preparation, HttpRequest, RoutingIntent) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::Sign { .. }))
    );
    let rows = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => Some((*preparation, request.clone(), *intent)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        1,
        "one actual native selected read must prepare"
    );
    rows.into_iter().next().unwrap()
}

#[test]
fn duplicate_pending_command_cannot_issue_or_retire_the_original_read() {
    let mut e = engine(true);
    let (first, _, _) = prepare(&ask(&mut e, 42));
    let duplicate = ask(&mut e, 42);
    assert!(!duplicate.iter().any(|a| matches!(a, Action::Http(_) | Action::Sign { .. })
        || matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))),
        "reused numeric ID cannot issue a second selected read");
    assert!(
        !duplicate.iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: 42, .. } | Notice::CommandFailed { command: 42, .. }
            )
        )),
        "duplicate entry grants no terminal over its sibling owner"
    );
    e.http_prepared(
        NOW + 1.,
        first,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    assert!(
        e.drain_actions().any(|a| matches!(a, Action::Sign { .. })),
        "original read retains its preparation after duplicate admission refusal"
    );
}

#[test]
fn opaque_remote_read_prepares_then_signs_the_selected_secondary_without_local_keys() {
    let mut e = engine(true);
    let (preparation, unsigned, intent) = prepare(&ask(&mut e, 42));
    assert_eq!(intent, RoutingIntent::Secondary);
    assert_eq!(unsigned.timeout_ms, Some(20_000));
    assert_eq!(
        unsigned.headers,
        vec![
            ("X-SOURCE-KEY", "Tealstreet".into()),
            ("Content-Type", "application/json, charset=utf-8".into())
        ]
    );
    e.http_prepared(
        NOW + 1.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let (attempt, work) = actions
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, work, .. } => Some((*attempt, work)),
            _ => None,
        })
        .expect("remote signing work after real route preparation");
    assert!(
        matches!(&work.operations[0], Operation::Present { key } if key.0=="opaque:secondary:credential")
    );
    assert!(
        matches!(&work.operations[1], Operation::Sign { algorithm: safe_cex_engine::signing::Algorithm::HmacSha256, key, .. } if key.0=="opaque:secondary:signing")
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    // The fixture answers trusted remote work; no local secret or Context exists here.
    let values = vec![b"remote-read-key".to_vec(), vec![7; 32]];

    e.handle(
        NOW + 2.,
        Event::Signed {
            attempt,
            result: Ok(SignResult {
                values,
                receipt: work.receipt,
                headers: Vec::new(),
            }),
        },
    );
    let (permit, signed) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("provider result binds the actual selected read");
    assert!(signed.headers.iter().any(
        |(name, value)| name.eq_ignore_ascii_case("X-BX-APIKEY") && value == "remote-read-key"
    ));
    assert!(matches!(
        e.claim_http(NOW + 2., permit),
        Ok(Claim::Released(_))
    ));
}

#[test]
fn absent_secondary_keeps_primary_identity_and_the_same_native_preparation_deadline() {
    let mut e = engine(false);
    let (_, request, intent) = prepare(&ask(&mut e, 42));
    assert_eq!(intent, RoutingIntent::Primary);
    assert_eq!(request.timeout_ms, Some(20_000));
    assert_eq!(
        request.headers,
        vec![
            ("X-SOURCE-KEY", "Tealstreet".into()),
            ("Content-Type", "application/json, charset=utf-8".into())
        ]
    );
    assert!(request.signer_headers.is_empty());
}
