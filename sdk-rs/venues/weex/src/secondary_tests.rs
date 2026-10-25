use super::*;
use crate::setup::{Options as SetupOptions, with_venue};
use safe_cex_engine::test_support::ProbeVenue;
use safe_cex_engine::{Action, ReadCredentials};
fn options() -> SetupOptions {
    SetupOptions {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),
        passphrase: Some("primary-pass".into()),
        ..SetupOptions::default()
    }
}

fn probe(
    venue: WeexFutures,
) -> ProbeVenue<WeexFutures, impl FnMut(&mut WeexFutures, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, _cx| {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            let role = if method == Method::Get {
                safe_cex_engine::credential_selection::CredentialRole::Read { scope: None }
            } else {
                safe_cex_engine::credential_selection::CredentialRole::Mutation
            };
            let (selection, identity) = venue.slots.select(role).unwrap();
            let slot = selection.slot();
            let (expected, expected_slot) = if method == Method::Get {
                (
                    venue.credentials.read.as_ref().unwrap(),
                    safe_cex_engine::CredentialSlot::Secondary,
                )
            } else {
                (
                    venue.credentials.primary.as_ref().unwrap(),
                    safe_cex_engine::CredentialSlot::Primary,
                )
            };
            assert_eq!(identity, expected);
            assert_eq!(slot, expected_slot);
        }
    })
}

#[test]
fn secondary_get_keeps_primary_mutations() {
    let mut engine = with_venue(options(), probe);
    engine.start(1700000000000.0);
}

#[test]
fn empty_secondary_material_stops_startup_before_io() {
    // Trusted setup marks empty material with an invalid ref; the venue refuses it.
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = crate::setup::engine(opts);
    engine.start(1700000000000.0);
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(event)) if event.code == code::CONFIG_INVALID
    )));
}

#[test]
fn public_only_account_with_secondary_inheriting_primary_passphrase_starts() {
    let opts = SetupOptions {
        public_data_only: true,
        read_credentials: Some(ReadCredentials {
            passphrase: None,
            ..options().read_credentials.unwrap()
        }),
        ..options()
    };
    let mut engine = crate::setup::engine(opts);
    engine.start(1700000000000.0);
    assert!(
        safe_cex_engine::test_support::prepared_actions(&mut engine, 1700000000000.0)
            .into_iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
}
