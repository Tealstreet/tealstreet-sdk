//! Production read builders must prepare transport before asking the provider.
use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::signing::{Identity, KeyRef};
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_bingx::{Bingx, Options};

fn identity(slot: &str) -> Identity {
    Identity {
        credential: KeyRef(format!("ref:{slot}-credential")),
        signing: KeyRef(format!("ref:{slot}-signing")),
        passphrase: None,
        stable_id: None,
    }
}
fn engine() -> Engine<Bingx> {
    safe_cex_engine::test_support::seeded_engine(Bingx::new(Options {
        credentials: Some(identity("primary")),
        read_credentials: Some(identity("secondary")),
        public_data_only: false,
    }))
}
fn assert_prepared_read(mut engine: Engine<Bingx>, identity_read: bool) {
    if identity_read {
        engine.handle(
            1_000.,
            Event::Command(Command::FetchAccount { command: 42 }),
        );
    } else {
        engine.start(1_000.);
    }
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let prepares = actions
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        prepares.len(),
        1,
        "the actual native builder must park one unsigned read"
    );
    let (preparation, request) = prepares[0];
    assert_eq!(
        request.headers,
        vec![
            ("X-SOURCE-KEY", "Tealstreet".into()),
            ("Content-Type", "application/json, charset=utf-8".into()),
        ]
    );
    assert!(request.signer_headers.is_empty());
    assert!(!request.url.contains("timestamp="));
    assert!(!request.url.contains("signature="));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
    );
    engine.http_prepared(
        1_001.,
        preparation,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let signs = actions
        .iter()
        .filter_map(|action| match action {
            Action::Sign { work, .. } => Some(work),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(signs.len(), 1);
    assert!(
        matches!(&signs[0].operations[0], safe_cex_engine::signing::Operation::Present { key } if key.0 == "ref:secondary-credential")
    );
    assert_eq!(signs[0].receipt.stamp_ms, Some(1_001.));
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}
#[test]
fn credentialed_market_read_keeps_native_secondary_signing_after_route_preparation() {
    assert_prepared_read(engine(), false);
}
#[test]
fn identity_read_uses_managed_transport_and_the_selected_secondary() {
    assert_prepared_read(engine(), true);
}
