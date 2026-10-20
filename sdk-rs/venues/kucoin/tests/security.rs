use safe_cex_crypto::secrets::EraseSecrets;
use safe_cex_venue_kucoin::Options;

#[test]
fn credential_options_debug_redacts_and_erasure_scrubs_owned_fields() {
    let mut options = Options {
        key: Some("audit-secret-sentinel".into()),
        secret: Some("audit-secret-sentinel".into()),
        passphrase: Some("audit-secret-sentinel".into()),
        ..Options::default()
    };
    assert!(!format!("{options:?}").contains("audit-secret-sentinel"));
    options.erase_secrets();
    assert!(options.key.as_ref().is_none_or(String::is_empty));
    assert!(options.secret.as_ref().is_none_or(String::is_empty));
    assert!(options.passphrase.as_ref().is_none_or(String::is_empty));
}

#[test]
fn broker_secret_is_redacted_and_scrubbed_with_both_retained_copies() {
    use safe_cex_venue_kucoin::sign::{BrokerOptions, Credentials};
    let broker = BrokerOptions {
        partner: "partner".into(),
        broker_key: "broker-secret-sentinel".into(),
        broker_name: "name".into(),
        verify: None,
    };
    assert!(!format!("{broker:?}").contains("broker-secret-sentinel"));
    let mut options = Options {
        broker: Some(broker.clone()),
        ..Options::default()
    };
    let mut credentials = Credentials {
        key: "private-api-key".into(),
        secret: "private-secret".into(),
        passphrase: "private-passphrase".into(),
        broker: Some(broker),
    };
    options.erase_secrets();
    credentials.erase_secrets();
    assert!(options.broker.as_ref().unwrap().broker_key.is_empty());
    assert!(
        credentials.key.is_empty()
            && credentials.secret.is_empty()
            && credentials.passphrase.is_empty()
    );
    assert!(credentials.broker.as_ref().unwrap().broker_key.is_empty());
}
