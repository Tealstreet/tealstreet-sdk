//! A keyless account: the Stark key a host-answered provider bound at setup is
//! the only key its signatures verify under, so `/auth` sends only with it.
mod support;
use safe_cex_engine::Action;
use safe_cex_engine::Event;
use safe_cex_venue_paradex::Options;
use support::*;

const CONFIG: &str = r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#;

fn options() -> Options {
    Options {
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    }
}

/// Answers the pending `/auth` sign task with the host's provider.
fn sign_auth(h: &mut Harness, signer: &safe_cex_engine::signing::Provider) {
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply("/system/config", CONFIG);
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::Sign { .. }))
        .expect("host-answered /auth sign task");
    let Action::Sign { attempt, work, .. } = h.actions.remove(i) else {
        unreachable!()
    };
    let result = signer.sign_ready(&work).unwrap().unwrap();
    h.e.handle(
        h.now,
        Event::Signed {
            attempt,
            result: Ok(result),
        },
    );
    h.collect();
}

#[test]
fn the_setup_bound_key_verifies_auth_and_a_different_one_never_sends() {
    let (mut h, signer) = Harness::keyless(options(), STARK_KEY, |_| {});
    sign_auth(&mut h, &signer);
    let auth = h.take("/auth/");
    assert!(
        auth.headers
            .iter()
            .any(|(name, _)| *name == "PARADEX-STARKNET-SIGNATURE")
    );

    // The host bound a different key: every signature fails verification.
    let (mut h, signer) = Harness::keyless(options(), STARK_KEY, |key| key[31] ^= 1);
    sign_auth(&mut h, &signer);
    assert!(!h.pending.iter().any(|r| r.url.contains("/auth/")));
}
