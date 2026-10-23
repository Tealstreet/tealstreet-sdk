//! Construction-owned delegated signing choices, separate from account scope.
use crate::signing_headers::ResultHeaderPolicy;

/// The trusted family declaration selects how raw MAC bytes are presented.
/// Provider requests/results continue to carry raw bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SignatureEncoding {
    #[default]
    Hex,
    Base64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrivateFeedPolicy {
    #[default]
    Streaming,
    /// Private REST remains enabled; no private subscription/session upkeep.
    RestOnly,
}
impl PrivateFeedPolicy {
    pub fn streaming(self) -> bool {
        matches!(self, Self::Streaming)
    }
}

/// Generated from the same per-alias declarations consumed by SDK construction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemoteSigningPolicy {
    /// Runtime adoption is separate from the presence of future codec metadata.
    pub enabled: bool,
    /// A private PayloadSigner must have a valid REST-only account lifecycle.
    pub payload_supported: bool,
    pub payload_rest_only: bool,
    pub protected_headers: Option<&'static [&'static str]>,
    pub signature_encoding: SignatureEncoding,
    /// Presentation comes from an owned passphrase ref, never SDK options.
    pub passphrase_header: Option<&'static str>,
}
impl RemoteSigningPolicy {
    pub fn private_feed(self, payload_signer: bool) -> PrivateFeedPolicy {
        if payload_signer && self.payload_rest_only {
            PrivateFeedPolicy::RestOnly
        } else {
            PrivateFeedPolicy::Streaming
        }
    }
    pub fn header_policy(self) -> ResultHeaderPolicy {
        self.protected_headers
            .map_or(ResultHeaderPolicy::Deny, |protected| {
                ResultHeaderPolicy::Http { protected }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undeclared_policies_preserve_streaming_and_deny_callback_headers() {
        for payload in [false, true] {
            assert_eq!(
                RemoteSigningPolicy::default().private_feed(payload),
                PrivateFeedPolicy::Streaming
            );
        }
        assert_eq!(
            RemoteSigningPolicy::default().header_policy(),
            ResultHeaderPolicy::Deny
        );
        let declared = RemoteSigningPolicy {
            enabled: true,
            payload_supported: true,
            payload_rest_only: true,
            protected_headers: Some(&[]),
            ..Default::default()
        };
        assert_eq!(declared.private_feed(false), PrivateFeedPolicy::Streaming);
        assert_eq!(declared.private_feed(true), PrivateFeedPolicy::RestOnly);
        assert_eq!(
            declared.header_policy(),
            ResultHeaderPolicy::Http { protected: &[] }
        );
    }
}
