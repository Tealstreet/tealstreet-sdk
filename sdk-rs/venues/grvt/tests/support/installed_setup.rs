use super::*;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding, SigningPolicy,
};
fn setup_policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            signing: SigningPolicy::Optional,
            trim_credential: Some(false),
            secret_encoding: Some(KeyEncoding::Hex),
            signing_algorithm: Some(Algorithm::Secp256k1Digest),
            ..Default::default()
        }),
        secondary: None,
    }
}

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::KeyRef;

#[test]
fn trusted_presence_and_public_mode_control_login_and_identity_eligibility() {
    for (key, expected_login, eligible) in [
        (None, None, false),
        (Some(""), Some(PrimaryLogin::Empty), false),
        (Some(" \t "), Some(PrimaryLogin::Installed), false),
        (Some("api-key"), Some(PrimaryLogin::Installed), true),
    ] {
        for scalar in [false, true] {
            for public in [false, true] {
                #[derive(serde::Serialize)]
                struct Primary<'a> {
                    #[serde(skip_serializing_if = "Option::is_none")]
                    key: Option<&'a str>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    secret: Option<String>,
                }
                #[derive(serde::Serialize)]
                struct Setup<'a> {
                    primary: Primary<'a>,
                }
                let setup = serde_json::to_string(&Setup {
                    primary: Primary {
                        key,
                        secret: scalar.then(|| format!("{:064x}", 1)),
                    },
                })
                .unwrap();
                let (_, identities, facts) =
                    LocalSigner::from_json_with_facts(setup, setup_policy(), public).unwrap();
                let installed = Installed::new(identities, Some(facts), public);
                assert_eq!(
                    installed
                        .login
                        .select(CredentialRole::SessionAuth)
                        .ok()
                        .map(|(_, login)| *login),
                    expected_login.filter(|_| !public)
                );
                assert_eq!(installed.identity_eligible(), eligible && !public);
                assert_eq!(
                    installed.primary().is_some(),
                    eligible_or_blank(key) && !public
                );
                assert_eq!(
                    installed.write_credential().is_some(),
                    installed.primary().is_some()
                );
                assert!(installed.signer.address().is_none());
            }
        }
    }
}

#[test]
fn injected_full_identity_never_creates_an_empty_login() {
    let primary = Identity::new(KeyRef("api".into()), Some(KeyRef("scalar".into())));
    let installed = Installed::new(
        Identities {
            primary: Some(primary),
            secondary: None,
        },
        None,
        false,
    );
    assert_eq!(installed.facts, None);
    assert_eq!(
        *installed
            .login
            .select(CredentialRole::SessionAuth)
            .unwrap()
            .1,
        PrimaryLogin::Installed
    );
    assert!(installed.identity_eligible());
    let (_, identities, facts) = LocalSigner::from_json_with_facts(
        format!(r#"{{"primary":{{"secret":"{:064x}"}}}}"#, 1),
        setup_policy(),
        false,
    )
    .unwrap();
    assert_eq!(facts.primary_key(), KeyPresence::Absent);
    let secret_only = Installed::new(identities, Some(facts), false);
    assert!(
        secret_only
            .login
            .select(CredentialRole::SessionAuth)
            .is_err()
    );
    let absent = Installed::new(Identities::default(), None, false);
    assert!(absent.login.select(CredentialRole::SessionAuth).is_err());
    let credential_only = Installed::new(
        Identities {
            primary: Some(Identity::new(KeyRef("api".into()), None)),
            secondary: None,
        },
        None,
        false,
    );
    assert!(
        credential_only
            .login
            .select(CredentialRole::SessionAuth)
            .is_err()
    );
    assert!(!credential_only.identity_eligible());
    assert!(credential_only.primary().is_none());
    for identity in [
        Identity::new(KeyRef("api".into()), Some(KeyRef(String::new()))),
        Identity::new(KeyRef(String::new()), Some(KeyRef("scalar".into()))),
    ] {
        let installed = Installed::new(
            Identities {
                primary: Some(identity),
                secondary: None,
            },
            None,
            false,
        );
        assert!(installed.login.select(CredentialRole::SessionAuth).is_err());
        assert!(installed.primary().is_none());
        assert!(!installed.identity_eligible());
    }
}

fn eligible_or_blank(key: Option<&str>) -> bool {
    key.is_some_and(|key| !key.is_empty())
}

#[test]
fn unsigned_commands_do_not_borrow_a_signing_credential_and_signed_commands_never_fall_back() {
    use safe_cex_engine::signing::SignError;
    for scalar in [false, true] {
        let setup = if scalar {
            format!(r#"{{"primary":{{"key":"api","secret":"{:064x}"}}}}"#, 1)
        } else {
            r#"{"primary":{"key":"api"}}"#.to_owned()
        };
        let (_, identities, facts) =
            LocalSigner::from_json_with_facts(setup, setup_policy(), false).unwrap();
        let installed = Installed::new(identities, Some(facts), false);
        for command in [
            Command::CancelOrders {
                command: 1,
                ids: vec![],
            },
            Command::CancelSymbolOrders {
                command: 1,
                symbol: "BTC/USDT:USDT".into(),
            },
            Command::SetLeverage {
                command: 1,
                symbol: "BTC/USDT:USDT".into(),
                leverage: 2.,
                side: None,
            },
        ] {
            assert_eq!(installed.command_credential(&command), Ok(None));
        }
        for command in [
            Command::PlaceOrders {
                command: 1,
                orders: vec![],
            },
            Command::AmendOrder {
                command: 1,
                id: "order".into(),
                price: Some(2.),
                amount: None,
            },
        ] {
            let credential = installed.command_credential(&command);
            if scalar {
                assert_eq!(credential, Ok(installed.write_credential()));
                assert!(credential.unwrap().is_some());
            } else {
                assert_eq!(credential, Err(SignError::SigningNotConfigured));
            }
        }
    }
}
