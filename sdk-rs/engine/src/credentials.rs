//! Optional credentials for the TS secondary client's GET-only identity.
use zeroize::Zeroize;

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadCredentials {
    pub key: String,
    pub secret: String,
    #[serde(default)]
    pub passphrase: Option<String>,
}

impl ReadCredentials {
    /// Erase an engine-owned copy before its containing options are released.
    pub fn erase_secrets(&mut self) {
        self.key.zeroize();
        self.secret.zeroize();
        self.passphrase.zeroize();
    }

    pub fn validate_passphrase(&self, primary: Option<&str>) -> Result<(), &'static str> {
        if self
            .passphrase
            .as_deref()
            .or(primary)
            .is_none_or(str::is_empty)
        {
            Err("Secondary read credentials require a passphrase")
        } else {
            Ok(())
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.key.is_empty() || self.secret.is_empty() {
            Err("Secondary read credentials require a nonempty key and secret")
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Debug for ReadCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadCredentials { .. }")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secondary_credentials_require_both_keys_and_inherit_passphrase_only_when_omitted() {
        assert!(serde_json::from_str::<ReadCredentials>(r#"{"key":"read"}"#).is_err());
        let mut read: ReadCredentials =
            serde_json::from_str(r#"{"key":"read","secret":"secret"}"#).unwrap();
        assert!(read.validate().is_ok());
        assert!(read.validate_passphrase(Some("primary-pass")).is_ok());
        assert!(read.validate_passphrase(None).is_err());
        read.passphrase = Some(String::new());
        assert!(read.validate_passphrase(Some("primary-pass")).is_err());
        read.secret.clear();
        assert!(read.validate().is_err());
    }
    #[test]
    fn secondary_credentials_debug_never_exposes_signing_material() {
        let read = ReadCredentials {
            key: "test-key".into(),
            secret: "test-secret".into(),
            passphrase: Some("test-pass".into()),
        };
        let debug = format!("{read:?}");
        for part in ["test-key", "test-secret", "test-pass"] {
            assert!(!debug.contains(part));
        }
    }

    #[test]
    fn secondary_credential_cleanup_erases_all_owned_fields() {
        let mut read = ReadCredentials {
            key: "secondary-key".into(),
            secret: "secondary-secret".into(),
            passphrase: Some("secondary-passphrase".into()),
        };
        read.erase_secrets();
        assert!(read.key.is_empty());
        assert!(read.secret.is_empty());
        assert!(read.passphrase.as_deref().is_none_or(str::is_empty));
    }
}
