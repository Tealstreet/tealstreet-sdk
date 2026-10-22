//! Callback headers stay with one signing attempt, under trusted recipe policy.
//! This module never derives permission from advisory signing context strings.
use crate::signing::SignError;

pub const MAX_HEADERS: usize = 32;
pub const MAX_NAME_BYTES: usize = 256;
pub const MAX_VALUE_BYTES: usize = 8192;
pub const MAX_HEADER_BYTES: usize = 32768;

/// Supplied by a trusted HTTP recipe, never by an external signer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultHeaderPolicy {
    #[default]
    Deny,
    /// Native values for these names take precedence; callback values otherwise
    /// override native values, matching Axios's case-insensitive last assignment.
    Http { protected: &'static [&'static str] },
}

pub fn validate(headers: &[(String, String)]) -> Result<(), SignError> {
    if headers.len() > MAX_HEADERS {
        return Err(SignError::InvalidResult);
    }
    let mut total = 0usize;
    for (name, value) in headers {
        if name.is_empty()
            || name.len() > MAX_NAME_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
            || value.len() > MAX_VALUE_BYTES
            || value
                .bytes()
                .any(|byte| (byte < 0x20 && byte != b'\t') || byte == 0x7f)
        {
            return Err(SignError::InvalidResult);
        }
        total = total
            .checked_add(name.len())
            .and_then(|size| size.checked_add(value.len()))
            .filter(|size| *size <= MAX_HEADER_BYTES)
            .ok_or(SignError::InvalidResult)?;
    }
    Ok(())
}

impl ResultHeaderPolicy {
    /// Validate the entire callback before changing outbound data. Call only on
    /// the fresh signed HTTP action, never on the retained unsigned recipe.
    pub fn apply(
        self,
        native: &mut Vec<(String, String)>,
        callback: &[(String, String)],
    ) -> Result<(), SignError> {
        validate(callback)?;
        let protected = match self {
            Self::Deny if callback.is_empty() => return Ok(()),
            Self::Deny => return Err(SignError::InvalidResult),
            Self::Http { protected } => protected,
        };
        for (name, value) in callback {
            if protected
                .iter()
                .any(|fixed| fixed.eq_ignore_ascii_case(name))
            {
                continue;
            }
            let mut replaced = false;
            native.retain_mut(|(current_name, current_value)| {
                if !current_name.eq_ignore_ascii_case(name) {
                    return true;
                }
                if replaced {
                    return false;
                }
                current_value.clone_from(value);
                replaced = true;
                true
            });
            if !replaced {
                native.push((name.clone(), value.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, value: &str) -> (String, String) {
        (name.into(), value.into())
    }

    #[test]
    fn default_denies_nonempty_metadata_without_changing_native_headers() {
        let mut native = vec![pair("X-MBX-APIKEY", "native")];
        let original = native.clone();
        assert!(
            ResultHeaderPolicy::default()
                .apply(&mut native, &[])
                .is_ok()
        );
        assert!(
            ResultHeaderPolicy::default()
                .apply(&mut native, &[pair("X-MBX-APIKEY", "callback")])
                .is_err()
        );
        assert_eq!(native, original);
    }

    #[test]
    fn declarations_preserve_both_header_precedences_and_last_case_alias() {
        let callback = vec![
            pair("x-mbx-apikey", "first"),
            pair("X-MBX-APIKEY", "last"),
            pair("X-Custom", "value"),
        ];
        let mut native = vec![pair("X-MBX-APIKEY", "native")];
        ResultHeaderPolicy::Http { protected: &[] }
            .apply(&mut native, &callback)
            .unwrap();
        assert_eq!(
            native,
            vec![pair("X-MBX-APIKEY", "last"), pair("X-Custom", "value")]
        );

        let mut native = vec![
            pair("X-BAPI-SIGN", "native"),
            pair("X-BAPI-RECV-WINDOW", "5000"),
        ];
        ResultHeaderPolicy::Http {
            protected: &["X-BAPI-SIGN", "X-BAPI-API-KEY", "X-BAPI-TIMESTAMP"],
        }
        .apply(
            &mut native,
            &[
                pair("x-bapi-sign", "ignored"),
                pair("x-bapi-api-key", "ignored-even-if-absent"),
                pair("x-bapi-recv-window", "8765"),
            ],
        )
        .unwrap();
        assert_eq!(
            native,
            vec![
                pair("X-BAPI-SIGN", "native"),
                pair("X-BAPI-RECV-WINDOW", "8765")
            ]
        );
    }

    #[test]
    fn invalid_metadata_is_rejected_atomically_even_if_shadowed() {
        for invalid in [
            pair("Bad Name", "value"),
            pair("X-Fixed", "value\r\ninjected: true"),
            pair("X-Fixed", "value\0"),
            pair("X-Fixed", "value\u{7f}"),
            pair(&"X".repeat(MAX_NAME_BYTES + 1), "value"),
            pair("X-Fixed", &"x".repeat(MAX_VALUE_BYTES + 1)),
        ] {
            let mut native = vec![pair("X-Fixed", "native")];
            let original = native.clone();
            assert!(
                ResultHeaderPolicy::Http {
                    protected: &["X-Fixed"]
                }
                .apply(
                    &mut native,
                    &[pair("X-Custom", "valid-before-invalid"), invalid]
                )
                .is_err()
            );
            assert_eq!(native, original);
        }
    }

    #[test]
    fn callback_override_removes_all_stale_native_case_aliases() {
        let mut native = vec![
            pair("X-Key", "first"),
            pair("X-Unrelated", "untouched"),
            pair("x-key", "stale-last"),
        ];
        let original = native.clone();
        let policy = ResultHeaderPolicy::Http { protected: &[] };
        policy.apply(&mut native, &[]).unwrap();
        assert_eq!(native, original);
        policy
            .apply(
                &mut native,
                &[pair("X-KEY", "first-callback"), pair("x-key", "callback")],
            )
            .unwrap();
        assert_eq!(
            native,
            vec![pair("X-Key", "callback"), pair("X-Unrelated", "untouched")]
        );
    }

    #[test]
    fn count_aggregate_utf8_and_boundary_values_are_bounded() {
        assert!(validate(&vec![pair("X", ""); MAX_HEADERS + 1]).is_err());
        assert!(validate(&vec![pair("X", &"x".repeat(MAX_VALUE_BYTES)); 4]).is_err());
        assert!(validate(&vec![pair("X", &"é".repeat(MAX_VALUE_BYTES / 2)); 4]).is_err());
        assert!(validate(&vec![pair("X", ""); MAX_HEADERS]).is_ok());
        assert!(
            validate(&[pair(
                &"X".repeat(MAX_NAME_BYTES),
                &"x".repeat(MAX_VALUE_BYTES)
            )])
            .is_ok()
        );
        assert!(validate(&[pair("X", "\tUTF-8 é")]).is_ok());
        let mut exact = vec![pair("X", &"x".repeat(MAX_VALUE_BYTES - 1)); 4];
        assert!(validate(&exact).is_ok()); // 4 * (1 + 8191) = 32768.
        exact[0].1.push('x');
        assert!(validate(&exact).is_err()); // 32769, with each value still <=8192.
    }
}
