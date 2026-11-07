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
    /// override native values with case-insensitive last-assignment precedence.
    Http { protected: &'static [&'static str] },
}

pub fn validate(headers: &[(String, String)]) -> Result<(), SignError> {
    validate_pairs(
        headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    )
}

/// Borrowed public declarations are checked before allocating transport values.
pub(crate) fn validate_unsigned(headers: &[(&str, &str)]) -> Result<(), SignError> {
    validate_pairs(headers.iter().copied())
}

/// The whole outbound header list: native/recipe headers plus signer headers.
pub(crate) fn validate_packet(
    headers: &[(&str, String)],
    signer_headers: &[(String, String)],
) -> Result<(), SignError> {
    let pairs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .chain(
            signer_headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str())),
        )
        .collect();
    validate_pairs(pairs.into_iter())
}

fn validate_pairs<'a>(
    headers: impl ExactSizeIterator<Item = (&'a str, &'a str)>,
) -> Result<(), SignError> {
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
    pub fn validate(self, callback: &[(String, String)]) -> Result<(), SignError> {
        validate(callback)?;
        if matches!(self, Self::Deny) && !callback.is_empty() {
            return Err(SignError::InvalidResult);
        }
        Ok(())
    }

    /// Validate the entire callback before changing outbound data. Call only on
    /// the fresh signed HTTP action, never on the retained unsigned recipe.
    pub fn apply(
        self,
        native: &mut Vec<(String, String)>,
        callback: &[(String, String)],
    ) -> Result<(), SignError> {
        self.validate(callback)?;
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
                    use zeroize::Zeroize;
                    current_value.zeroize();
                    return false;
                }
                use zeroize::Zeroize;
                current_value.zeroize();
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

    /// Keep fixed-name native headers in their original representation. Only
    /// names actually overridden by this callback move to delegated custody.
    pub fn apply_http(
        self,
        http: &mut crate::HttpRequest,
        callback: &[(String, String)],
    ) -> Result<(), SignError> {
        self.validate(callback)?;
        if let Self::Http { protected } = self {
            http.signer_headers.retain_mut(|(name, value)| {
                if protected
                    .iter()
                    .any(|fixed| fixed.eq_ignore_ascii_case(name))
                {
                    use zeroize::Zeroize;
                    value.zeroize();
                    false
                } else {
                    true
                }
            });
        }
        self.apply(&mut http.signer_headers, callback)?;
        if let Self::Http { protected } = self {
            http.headers.retain_mut(|(name, value)| {
                if !protected
                    .iter()
                    .any(|fixed| fixed.eq_ignore_ascii_case(name))
                    && callback
                        .iter()
                        .any(|(key, _)| key.eq_ignore_ascii_case(name))
                {
                    use zeroize::Zeroize;
                    value.zeroize();
                    false
                } else {
                    true
                }
            });
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

    #[test]
    fn protected_native_headers_cannot_be_shadowed_by_existing_dynamic_aliases() {
        for callback in [vec![], vec![pair("X-Custom", "callback")]] {
            let mut http = crate::HttpRequest {
                id: crate::RequestId(1),
                method: crate::Method::Get,
                route: None,
                timeout_ms: None,
                url: String::new(),
                body: None,
                headers: vec![("X-BAPI-SIGN", "native".into())],
                signer_headers: vec![
                    pair("x-bapi-sign", "stale"),
                    pair("X-Unrelated", "unchanged"),
                ],
            };
            ResultHeaderPolicy::Http {
                protected: &["X-BAPI-SIGN"],
            }
            .apply_http(&mut http, &callback)
            .unwrap();
            assert_eq!(http.headers, vec![("X-BAPI-SIGN", "native".into())]);
            let mut expected = vec![pair("X-Unrelated", "unchanged")];
            expected.extend(callback);
            assert_eq!(http.signer_headers, expected);
        }
    }
}
