//! Optional credentials for the TS secondary client's GET-only identity.
use zeroize::Zeroize;

/// The native invalid-read-identity error, for venues that validate a provider
/// identity rather than raw `ReadCredentials`.
pub const READ_CREDENTIALS_INVALID: &str =
    "Secondary read credentials require a nonempty key and secret";

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadCredentials {
    pub key: String,
    /// A missing secret parses so `validate` rejects it by name; AsterDEX ignores the field.
    #[serde(default)]
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
        Self::status(!self.key.is_empty() && !self.secret.is_empty())
    }

    /// `validate`'s result from a flag trusted setup computed once, so a host
    /// can drop unused read material and still report the native error.
    pub fn status(valid: bool) -> Result<(), &'static str> {
        if valid {
            Ok(())
        } else {
            Err(READ_CREDENTIALS_INVALID)
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
        let missing: ReadCredentials = serde_json::from_str(r#"{"key":"read"}"#).unwrap();
        assert!(missing.validate().is_err());
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
    fn a_validation_flag_carried_without_the_material_reproduces_validate_exactly() {
        let valid: ReadCredentials =
            serde_json::from_str(r#"{"key":"read","secret":"secret"}"#).unwrap();
        let no_secret: ReadCredentials = serde_json::from_str(r#"{"key":"read"}"#).unwrap();
        let no_key: ReadCredentials =
            serde_json::from_str(r#"{"key":"","secret":"secret"}"#).unwrap();
        let native = Err("Secondary read credentials require a nonempty key and secret");
        assert_eq!(valid.validate(), Ok(()));
        assert_eq!(no_secret.validate(), native);
        assert_eq!(no_key.validate(), native);
        assert_eq!(ReadCredentials::status(true), Ok(()));
        assert_eq!(ReadCredentials::status(false), native);
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
