//! ApeX signer admission (RUST_APEXOMNI_SIGNER_ADMISSION.md), venue REDs: a
//! refs-only account (no raw key material in `Options`) signs its account
//! reads and private WS login through the provider, and refuses private start
//! typed when the zkLink role is missing.
mod support;
use safe_cex_engine::signing::{CredentialSetupPolicy, Identities};
use safe_cex_engine::{Action, Event, HttpRequest, Notice, SocketChannel};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use serde_json::{Value, json};
use support::*;

/// The pinned zkLink corpus seed: a public test vector, never a real account.
fn seed() -> String {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../crypto/tests/fixtures/zklink/npm-wasm.json"
    ))
    .unwrap();
    corpus["cases"][0]["seed"].as_str().unwrap().to_owned()
}

/// Trusted setup with the plan's policy; `with_seed` installs the zkLink role.
fn provider(with_seed: bool) -> (safe_cex_engine::signing::Provider, Identities) {
    let mut primary = json!({"secretEncoding": "base64Text"});
    let mut setup = json!({"key": "api-key", "secret": "material", "passphrase": "pass"});
    if with_seed {
        primary["extra"] =
            json!({"key": "zklinkSeed", "field": "requiredForPrivate", "encoding": "hex"});
        setup["extra"] = json!({"zklinkSeed": {"value": seed(), "encoding": "hex"}});
    }
    let policy: CredentialSetupPolicy =
        serde_json::from_value(json!({ "primary": primary })).unwrap();
    let (signer, identities) = safe_cex_crypto::signer::LocalSigner::from_json_with_policy(
        json!({ "primary": setup }).to_string(),
        policy,
    )
    .unwrap();
    (std::sync::Arc::new(signer), identities)
}

/// A refs-only private account: no key, signing material, passphrase or
/// seed in `Options`, only the opaque identity.
fn refs_only(with_seed: bool) -> TestEngine {
    let (signer, identities) = provider(with_seed);
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..Default::default()
    }));
    e.set_identities(identities).unwrap();
    e.set_signer(Some(signer)).unwrap();
    e.start(0.0);
    let markets = requests(&mut e).remove(0);
    response(&mut e, &markets, 10.0, MARKETS);
    e
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// R3: the account read is signed by the provider at release, and its headers
/// equal an independent Node HMAC literal over the legacy canonical text
/// (key = the secret's base64 text, base64 output), never via the provider.
#[test]
fn refs_only_account_read_is_signed_by_the_provider_at_release() {
    let mut e = refs_only(true);
    let released = prepared_requests(&mut e, 30.0);
    let account = released
        .iter()
        .find(|request| request.url.ends_with("/v3/account"))
        .expect("account read released from refs alone");
    assert_eq!(
        header(account, "APEX-SIGNATURE"),
        Some("3jgQuVANFNOd7TwencAwQyyK1MWOroBHFFxcsmP4uGE=")
    );
    assert_eq!(header(account, "APEX-API-KEY"), Some("api-key"));
    assert_eq!(header(account, "APEX-PASSPHRASE"), Some("pass"));
    assert_eq!(header(account, "APEX-TIMESTAMP"), Some("30"));
}

/// R4: the private WS login frame is signed through the provider.
#[test]
fn refs_only_private_ws_login_is_signed_by_the_provider() {
    let mut e = refs_only(true);
    let socket = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(socket),
            _ => None,
        })
        .expect("private socket from refs alone");
    e.handle(40.0, Event::WsOpened { socket });
    let frames: Vec<String> = safe_cex_engine::test_support::prepared_actions(&mut e, 40.0)
        .into_iter()
        .filter_map(|action| match action {
            Action::WsSend { socket: s, text } if s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert!(
        frames.iter().any(
            |frame| frame.contains("2gcggc2BWl/6c7Ka4ZifMPtg87q5lpHCq+00MbyEPlI=")
                && frame.contains("api-key")
        ),
        "{frames:?}"
    );
}

/// R6: without the zklinkSeed role, private start refuses typed and sends no
/// private HTTP; public discovery is unaffected.
#[test]
fn refs_without_the_zklink_role_refuse_private_start_typed() {
    let mut e = refs_only(false);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 30.0);
    assert!(
        !actions.iter().any(|action| matches!(
            action,
            Action::Http(request) if request.url.contains("/v3/account")
                || request.url.contains("/v3/open-orders")
        )),
        "no private I/O"
    );
    assert!(
        actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::CONFIG_INVALID
                    && event.message.contains("OmniKey")
        )),
        "typed not-ready refusal"
    );
}
