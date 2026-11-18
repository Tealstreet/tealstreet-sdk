//! The startup check names a signer that differs from the API wallet field only.
mod support;
use safe_cex_engine::log::code::CONFIG_INVALID;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Notice};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use support::{META, kind, reply};

const OTHER: &str = "0x2222222222222222222222222222222222222222";

fn warned(key: Option<&str>, application_id: Option<&str>) -> bool {
    let (provider, identities) = support::agent(support::TEST_SECRET);
    let mut engine = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(Options {
        key: key.map(Into::into),
        application_id: application_id.map(Into::into),
        credentials: identities.primary.clone(),
        ..Options::default()
    }));
    engine.set_signer(Some(provider)).unwrap();
    engine.set_identities(identities).unwrap();
    engine.initialize_address_nonce(999.0).unwrap();
    engine.start(1000.);
    let mut warned = false;
    for _ in 0..8 {
        for action in prepared_now(&mut engine) {
            match action {
                Action::Notice(Notice::Event(event)) if event.code == CONFIG_INVALID => {
                    warned = true;
                }
                Action::Http(request) => match kind(&request).as_str() {
                    "metaAndAssetCtxs" => reply(&mut engine, &request, 200, META),
                    "perpDexs" => reply(&mut engine, &request, 200, "[null]"),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    warned
}

#[test]
fn a_trading_account_other_than_the_signer_is_not_a_mismatch() {
    assert!(!warned(None, Some(OTHER)));
}

#[test]
fn an_api_wallet_field_other_than_the_signer_is_a_mismatch() {
    assert!(warned(Some(OTHER), None));
}
