//! An invalid read identity stops startup with the native config error, before
//! any market read; the venue reports it without the legacy read type.
mod support;
use safe_cex_engine::{Action, Level, Notice, log::code};

#[test]
fn an_invalid_read_identity_stops_startup_with_the_native_message() {
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-material".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: String::new(),
            secret: "read-material".into(),
            passphrase: None,
        }),
        ..support::Options::default()
    });
    e.start(0.0);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
    assert!(
        actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(event))
                if event.code == code::CONFIG_INVALID
                    && event.level == Level::Error
                    && event.message == safe_cex_engine::READ_CREDENTIALS_INVALID)),
        "{actions:?}"
    );
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "no market read after a refused read identity"
    );
    assert_eq!(
        safe_cex_engine::ReadCredentials::status(false),
        Err(safe_cex_engine::READ_CREDENTIALS_INVALID),
        "the constant is the native validate message"
    );
}
