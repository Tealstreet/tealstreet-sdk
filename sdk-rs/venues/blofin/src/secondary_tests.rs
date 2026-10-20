use super::*;
use safe_cex_engine::ReadCredentials;
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),
        passphrase: Some("primary-pass".into()),
        ..Options::default()
    }
}

#[test]
fn secondary_get_keeps_primary_mutations() {
    let venue = Blofin::new(options());
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let headers = venue
            .headers("/secondary-test", method, 1700000000000.0, "")
            .unwrap();
        let key = if method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let pass = if method == Method::Get {
            "read-pass"
        } else {
            "primary-pass"
        };
        let opts = options();
        let secret = if method == Method::Get {
            opts.read_credentials.as_ref().unwrap().secret.as_str()
        } else {
            opts.secret.as_deref().unwrap()
        };
        let nonce = headers
            .iter()
            .find(|(name, _)| *name == "ACCESS-NONCE")
            .unwrap()
            .1
            .as_str();
        let signature = sign::signature(
            secret,
            "/secondary-test",
            method.as_str(),
            "1700000000000",
            nonce,
            "",
        );
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| *name == "ACCESS-SIGN")
                .unwrap()
                .1,
            signature
        );
        assert!(
            headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("ACCESS-KEY") && value == key)
        );
        assert!(
            headers.iter().any(
                |(name, value)| name.eq_ignore_ascii_case("ACCESS-PASSPHRASE") && value == pass
            )
        );
    }
}
