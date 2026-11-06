//! Margin-mode row admission over the real startup read, with crate access.
use super::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice};

fn respond(e: &mut Engine<Orderly>, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        1000.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

fn pending(defaults: &[(&str, MarginType)]) -> (Engine<Orderly>, HttpRequest) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{
        Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
    };
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(format!("ed25519:{}", bs58::encode([0; 32]).into_string())),
                secret: Secret::new(bs58::encode([7; 32]).into_string()),
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base58),
                signing_algorithm: Some(Algorithm::Ed25519),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    let mut venue = Orderly::new(crate::Options {
        key: Some(format!("ed25519:{}", bs58::encode([0; 32]).into_string())),
        secret: Some(bs58::encode([7; 32]).into_string()),
        application_id: Some("test-account".into()),
        identities: identities.clone(),
        ..Default::default()
    });
    venue.account.defaults = defaults.iter().map(|(s, m)| ((*s).into(), *m)).collect();
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.set_signer(Some(std::sync::Arc::new(signer))).unwrap();
    e.set_identities(identities).unwrap();
    e.start(1000.0);
    for _ in 0..8 {
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.0);
        for action in out {
            if let Action::Http(r) = action {
                if r.url.ends_with("/v1/client/margin_modes") {
                    return (e, r);
                }
                let body = if r.url.ends_with("/v1/client/info") {
                    r#"{"success":true,"data":{"max_leverage":20}}"#
                } else {
                    assert!(r.url.ends_with("/v1/public/info"), "{}", r.url);
                    r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01},{"symbol":"PERP_ETH_USDC","base_tick":0.001,"quote_tick":0.01,"base_min":0.001,"base_max":100,"base_imr":0.02}]}}"#
                };
                respond(&mut e, &r, 200, body);
            }
        }
    }
    panic!("margin-mode startup read not issued");
}

fn read(
    defaults: &[(&str, MarginType)],
    rows: &str,
) -> (Engine<Orderly>, Vec<safe_cex_engine::LogEvent>) {
    let (mut e, r) = pending(defaults);
    respond(
        &mut e,
        &r,
        200,
        &format!(r#"{{"success":true,"data":{{"rows":{rows}}}}}"#),
    );
    let skipped = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) if event.code == code::ROW_SKIPPED => Some(event),
            _ => None,
        })
        .collect();
    (e, skipped)
}

#[test]
fn identified_rejection_keeps_only_its_old_mode_and_commits_valid_peers() {
    for bad in [
        r#"{"symbol":"PERP_BTC_USDC","default_margin_mode":"UNKNOWN"}"#,
        r#"{"symbol":"PERP_BTC_USDC","default_margin_mode":null}"#,
        r#"{"symbol":"PERP_BTC_USDC"}"#,
    ] {
        let (e, skipped) = read(
            &[
                ("BTCUSDC", MarginType::Isolated),
                ("ETHUSDC", MarginType::Cross),
                ("gone", MarginType::Isolated),
            ],
            &format!(r#"[{bad},{{"symbol":"PERP_ETH_USDC","default_margin_mode":"ISOLATED"}}]"#),
        );
        assert_eq!(
            e.venue().account.defaults.get("BTCUSDC"),
            Some(&MarginType::Isolated)
        );
        assert_eq!(
            e.venue().account.defaults.get("ETHUSDC"),
            Some(&MarginType::Isolated)
        );
        assert!(!e.venue().account.defaults.contains_key("gone"));
        assert_eq!(
            e.store().setting("ETHUSDC").unwrap().margin_type,
            Some(MarginType::Isolated)
        );
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].level, Level::Warn);
        assert_eq!(
            skipped[0].message,
            "Orderly skipped malformed margin-mode row"
        );
    }
}

#[test]
fn unidentified_rejection_protects_old_scope_while_valid_peer_wins() {
    for bad in [
        "42",
        r#"{"default_margin_mode":"ISOLATED"}"#,
        r#"{"symbol":null}"#,
        r#"{"symbol":"BROKEN","default_margin_mode":"CROSS"}"#,
    ] {
        let (e, skipped) = read(
            &[
                ("BTCUSDC", MarginType::Isolated),
                ("ETHUSDC", MarginType::Cross),
            ],
            &format!(r#"[{bad},{{"symbol":"PERP_ETH_USDC","default_margin_mode":"ISOLATED"}}]"#),
        );
        assert_eq!(
            e.venue().account.defaults.get("BTCUSDC"),
            Some(&MarginType::Isolated)
        );
        assert_eq!(
            e.venue().account.defaults.get("ETHUSDC"),
            Some(&MarginType::Isolated)
        );
        assert_eq!(skipped.len(), 1);
    }
}

#[test]
fn valid_replacements_empty_modes_and_empty_lists_keep_existing_contract() {
    let defaults = [("BTCUSDC", MarginType::Isolated)];
    let (e, _) = read(&defaults, "[]");
    assert!(e.venue().account.defaults.is_empty());
    let (e, _) = read(
        &defaults,
        r#"[{"symbol":"PERP_BTC_USDC","default_margin_mode":"ISOLATED"},{"symbol":"PERP_BTC_USDC","default_margin_mode":""}]"#,
    );
    assert_eq!(
        e.venue().account.defaults.get("BTCUSDC"),
        Some(&MarginType::Cross)
    );
    let (e, _) = read(
        &defaults,
        r#"[{"symbol":"PERP_BTC_USDC","default_margin_mode":"INVALID"},{"symbol":"PERP_BTC_USDC","default_margin_mode":"CROSS"}]"#,
    );
    assert_eq!(
        e.venue().account.defaults.get("BTCUSDC"),
        Some(&MarginType::Cross)
    );
}

#[test]
fn rejected_broker_symbol_retains_its_exact_suffix() {
    let (e, skipped) = read(
        &[("XLMUSDC_mythos", MarginType::Isolated)],
        r#"[{"symbol":"PERP_XLM_USDC_mythos","default_margin_mode":"UNKNOWN"},{"symbol":"PERP_BTC_USDC","default_margin_mode":"ISOLATED"}]"#,
    );
    assert_eq!(
        e.venue().account.defaults.get("XLMUSDC_mythos"),
        Some(&MarginType::Isolated)
    );
    assert_eq!(
        e.venue().account.defaults.get("BTCUSDC"),
        Some(&MarginType::Isolated)
    );
    assert_eq!(skipped.len(), 1);
}

#[test]
fn failed_response_keeps_the_whole_old_map() {
    for (status, body) in [
        (503, "{}"),
        (200, r#"{"success":false,"message":"failed"}"#),
        (200, "{broken"),
        (200, r#"{"success":true,"data":{}}"#),
        (200, r#"{"success":true,"data":{"rows":null}}"#),
        (
            200,
            r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","default_margin_mode":"CROSS"},"#,
        ),
    ] {
        let (mut e, r) = pending(&[("BTCUSDC", MarginType::Isolated)]);
        respond(&mut e, &r, status, body);
        assert_eq!(
            e.venue().account.defaults.get("BTCUSDC"),
            Some(&MarginType::Isolated)
        );
    }
}

#[test]
fn late_modes_read_cannot_overwrite_a_newer_margin_write() {
    let (mut e, modes) = pending(&[]);
    // Cached private state permits a write while the older mode read is pending.
    e.store_mut().set_orders(vec![]);
    e.store_mut().set_positions(vec![]);
    e.store_mut().set_balance(Default::default());
    e.handle(
        1000.0,
        Event::Command(Command::SetMarginType {
            command: 7,
            symbol: "BTCUSDC".into(),
            isolated: true,
        }),
    );
    let write = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/v1/client/margin_mode") => Some(r),
            _ => None,
        })
        .expect("margin write");
    respond(&mut e, &write, 200, r#"{"success":true}"#);
    assert_eq!(
        e.venue().account.defaults.get("BTCUSDC"),
        Some(&MarginType::Isolated)
    );
    respond(
        &mut e,
        &modes,
        200,
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","default_margin_mode":"CROSS"}]}}"#,
    );
    assert_eq!(
        e.venue().account.defaults.get("BTCUSDC"),
        Some(&MarginType::Isolated)
    );
    assert_eq!(
        e.store().setting("BTCUSDC").unwrap().margin_type,
        Some(MarginType::Isolated)
    );
}
