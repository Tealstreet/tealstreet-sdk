//! Inert Starbase cutover contracts; activation remains disabled.
use serde::{Deserialize, Serialize};
fn deserialize_present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseImplementationStatus {
    #[serde(rename = "not-implemented")]
    NotImplemented,
    #[serde(rename = "implemented")]
    Implemented,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseImplementationStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "not-implemented")]
            NotImplemented,
            #[serde(rename = "implemented")]
            Implemented,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::NotImplemented => Ok(Self::NotImplemented),
            Wire::Implemented => Ok(Self::Implemented),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseVerificationStatus {
    #[serde(rename = "unverified")]
    Unverified,
    #[serde(rename = "verified")]
    Verified,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseVerificationStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "unverified")]
            Unverified,
            #[serde(rename = "verified")]
            Verified,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Unverified => Ok(Self::Unverified),
            Wire::Verified => Ok(Self::Verified),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverEvidence {
    #[serde(rename = "credentialedHttpAuthentication")]
    pub credentialed_http_authentication: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "credentialedHttpReadParity")]
    pub credentialed_http_read_parity: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "credentialedWebsocketSessionLifecycle")]
    pub credentialed_websocket_session_lifecycle: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "credentialedWriteCanary")]
    pub credentialed_write_canary: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "cutoverRunbook")]
    pub cutover_runbook: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "httpAuthenticationContract")]
    pub http_authentication_contract: CoinbaseStarbaseImplementationStatus,
    #[serde(rename = "privateHttpResponseContracts")]
    pub private_http_response_contracts: CoinbaseStarbaseImplementationStatus,
    #[serde(rename = "privateNotificationEnvelope")]
    pub private_notification_envelope: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "publicHttpResponseContracts")]
    pub public_http_response_contracts: CoinbaseStarbaseImplementationStatus,
    #[serde(rename = "unitParity")]
    pub unit_parity: CoinbaseStarbaseVerificationStatus,
    #[serde(rename = "websocketSessionContract")]
    pub websocket_session_contract: CoinbaseStarbaseImplementationStatus,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for CoinbaseStarbaseCutoverEvidence {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "credentialedHttpAuthentication")]
            credentialed_http_authentication: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "credentialedHttpReadParity")]
            credentialed_http_read_parity: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "credentialedWebsocketSessionLifecycle")]
            credentialed_websocket_session_lifecycle: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "credentialedWriteCanary")]
            credentialed_write_canary: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "cutoverRunbook")]
            cutover_runbook: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "httpAuthenticationContract")]
            http_authentication_contract: CoinbaseStarbaseImplementationStatus,
            #[serde(rename = "privateHttpResponseContracts")]
            private_http_response_contracts: CoinbaseStarbaseImplementationStatus,
            #[serde(rename = "privateNotificationEnvelope")]
            private_notification_envelope: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "publicHttpResponseContracts")]
            public_http_response_contracts: CoinbaseStarbaseImplementationStatus,
            #[serde(rename = "unitParity")]
            unit_parity: CoinbaseStarbaseVerificationStatus,
            #[serde(rename = "websocketSessionContract")]
            websocket_session_contract: CoinbaseStarbaseImplementationStatus,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            credentialed_http_authentication: v.credentialed_http_authentication,
            credentialed_http_read_parity: v.credentialed_http_read_parity,
            credentialed_websocket_session_lifecycle: v.credentialed_websocket_session_lifecycle,
            credentialed_write_canary: v.credentialed_write_canary,
            cutover_runbook: v.cutover_runbook,
            http_authentication_contract: v.http_authentication_contract,
            private_http_response_contracts: v.private_http_response_contracts,
            private_notification_envelope: v.private_notification_envelope,
            public_http_response_contracts: v.public_http_response_contracts,
            unit_parity: v.unit_parity,
            websocket_session_contract: v.websocket_session_contract,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpShadowBlocker {
    #[serde(rename = "credentialed-http-authentication-unverified")]
    CredentialedHttpAuthenticationUnverified,
    #[serde(rename = "http-authentication-contract-not-implemented")]
    HttpAuthenticationContractNotImplemented,
    #[serde(rename = "private-http-response-contracts-not-implemented")]
    PrivateHttpResponseContractsNotImplemented,
    #[serde(rename = "product-outside-intx-migration")]
    ProductOutsideIntxMigration,
    #[serde(rename = "public-http-response-contracts-not-implemented")]
    PublicHttpResponseContractsNotImplemented,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseHttpShadowBlocker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "credentialed-http-authentication-unverified")]
            CredentialedHttpAuthenticationUnverified,
            #[serde(rename = "http-authentication-contract-not-implemented")]
            HttpAuthenticationContractNotImplemented,
            #[serde(rename = "private-http-response-contracts-not-implemented")]
            PrivateHttpResponseContractsNotImplemented,
            #[serde(rename = "product-outside-intx-migration")]
            ProductOutsideIntxMigration,
            #[serde(rename = "public-http-response-contracts-not-implemented")]
            PublicHttpResponseContractsNotImplemented,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::CredentialedHttpAuthenticationUnverified => {
                Ok(Self::CredentialedHttpAuthenticationUnverified)
            }
            Wire::HttpAuthenticationContractNotImplemented => {
                Ok(Self::HttpAuthenticationContractNotImplemented)
            }
            Wire::PrivateHttpResponseContractsNotImplemented => {
                Ok(Self::PrivateHttpResponseContractsNotImplemented)
            }
            Wire::ProductOutsideIntxMigration => Ok(Self::ProductOutsideIntxMigration),
            Wire::PublicHttpResponseContractsNotImplemented => {
                Ok(Self::PublicHttpResponseContractsNotImplemented)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseWebsocketBlocker {
    #[serde(rename = "credentialed-websocket-session-lifecycle-unverified")]
    CredentialedWebsocketSessionLifecycleUnverified,
    #[serde(rename = "private-notification-envelope-unverified")]
    PrivateNotificationEnvelopeUnverified,
    #[serde(rename = "websocket-activation-disabled")]
    WebsocketActivationDisabled,
    #[serde(rename = "websocket-session-contract-not-implemented")]
    WebsocketSessionContractNotImplemented,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseWebsocketBlocker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "credentialed-websocket-session-lifecycle-unverified")]
            CredentialedWebsocketSessionLifecycleUnverified,
            #[serde(rename = "private-notification-envelope-unverified")]
            PrivateNotificationEnvelopeUnverified,
            #[serde(rename = "websocket-activation-disabled")]
            WebsocketActivationDisabled,
            #[serde(rename = "websocket-session-contract-not-implemented")]
            WebsocketSessionContractNotImplemented,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::CredentialedWebsocketSessionLifecycleUnverified => {
                Ok(Self::CredentialedWebsocketSessionLifecycleUnverified)
            }
            Wire::PrivateNotificationEnvelopeUnverified => {
                Ok(Self::PrivateNotificationEnvelopeUnverified)
            }
            Wire::WebsocketActivationDisabled => Ok(Self::WebsocketActivationDisabled),
            Wire::WebsocketSessionContractNotImplemented => {
                Ok(Self::WebsocketSessionContractNotImplemented)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverBlocker {
    #[serde(rename = "credentialed-http-authentication-unverified")]
    CredentialedHttpAuthenticationUnverified,
    #[serde(rename = "http-authentication-contract-not-implemented")]
    HttpAuthenticationContractNotImplemented,
    #[serde(rename = "private-http-response-contracts-not-implemented")]
    PrivateHttpResponseContractsNotImplemented,
    #[serde(rename = "product-outside-intx-migration")]
    ProductOutsideIntxMigration,
    #[serde(rename = "public-http-response-contracts-not-implemented")]
    PublicHttpResponseContractsNotImplemented,
    #[serde(rename = "credentialed-websocket-session-lifecycle-unverified")]
    CredentialedWebsocketSessionLifecycleUnverified,
    #[serde(rename = "private-notification-envelope-unverified")]
    PrivateNotificationEnvelopeUnverified,
    #[serde(rename = "websocket-activation-disabled")]
    WebsocketActivationDisabled,
    #[serde(rename = "websocket-session-contract-not-implemented")]
    WebsocketSessionContractNotImplemented,
    #[serde(rename = "credentialed-http-read-parity-unverified")]
    CredentialedHttpReadParityUnverified,
    #[serde(rename = "credentialed-write-canary-unverified")]
    CredentialedWriteCanaryUnverified,
    #[serde(rename = "cutover-runbook-unverified")]
    CutoverRunbookUnverified,
    #[serde(rename = "runtime-activation-disabled")]
    RuntimeActivationDisabled,
    #[serde(rename = "runtime-product-binding-unavailable")]
    RuntimeProductBindingUnavailable,
    #[serde(rename = "unit-parity-unverified")]
    UnitParityUnverified,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverBlocker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "credentialed-http-authentication-unverified")]
            CredentialedHttpAuthenticationUnverified,
            #[serde(rename = "http-authentication-contract-not-implemented")]
            HttpAuthenticationContractNotImplemented,
            #[serde(rename = "private-http-response-contracts-not-implemented")]
            PrivateHttpResponseContractsNotImplemented,
            #[serde(rename = "product-outside-intx-migration")]
            ProductOutsideIntxMigration,
            #[serde(rename = "public-http-response-contracts-not-implemented")]
            PublicHttpResponseContractsNotImplemented,
            #[serde(rename = "credentialed-websocket-session-lifecycle-unverified")]
            CredentialedWebsocketSessionLifecycleUnverified,
            #[serde(rename = "private-notification-envelope-unverified")]
            PrivateNotificationEnvelopeUnverified,
            #[serde(rename = "websocket-activation-disabled")]
            WebsocketActivationDisabled,
            #[serde(rename = "websocket-session-contract-not-implemented")]
            WebsocketSessionContractNotImplemented,
            #[serde(rename = "credentialed-http-read-parity-unverified")]
            CredentialedHttpReadParityUnverified,
            #[serde(rename = "credentialed-write-canary-unverified")]
            CredentialedWriteCanaryUnverified,
            #[serde(rename = "cutover-runbook-unverified")]
            CutoverRunbookUnverified,
            #[serde(rename = "runtime-activation-disabled")]
            RuntimeActivationDisabled,
            #[serde(rename = "runtime-product-binding-unavailable")]
            RuntimeProductBindingUnavailable,
            #[serde(rename = "unit-parity-unverified")]
            UnitParityUnverified,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::CredentialedHttpAuthenticationUnverified => {
                Ok(Self::CredentialedHttpAuthenticationUnverified)
            }
            Wire::HttpAuthenticationContractNotImplemented => {
                Ok(Self::HttpAuthenticationContractNotImplemented)
            }
            Wire::PrivateHttpResponseContractsNotImplemented => {
                Ok(Self::PrivateHttpResponseContractsNotImplemented)
            }
            Wire::ProductOutsideIntxMigration => Ok(Self::ProductOutsideIntxMigration),
            Wire::PublicHttpResponseContractsNotImplemented => {
                Ok(Self::PublicHttpResponseContractsNotImplemented)
            }
            Wire::CredentialedWebsocketSessionLifecycleUnverified => {
                Ok(Self::CredentialedWebsocketSessionLifecycleUnverified)
            }
            Wire::PrivateNotificationEnvelopeUnverified => {
                Ok(Self::PrivateNotificationEnvelopeUnverified)
            }
            Wire::WebsocketActivationDisabled => Ok(Self::WebsocketActivationDisabled),
            Wire::WebsocketSessionContractNotImplemented => {
                Ok(Self::WebsocketSessionContractNotImplemented)
            }
            Wire::CredentialedHttpReadParityUnverified => {
                Ok(Self::CredentialedHttpReadParityUnverified)
            }
            Wire::CredentialedWriteCanaryUnverified => Ok(Self::CredentialedWriteCanaryUnverified),
            Wire::CutoverRunbookUnverified => Ok(Self::CutoverRunbookUnverified),
            Wire::RuntimeActivationDisabled => Ok(Self::RuntimeActivationDisabled),
            Wire::RuntimeProductBindingUnavailable => Ok(Self::RuntimeProductBindingUnavailable),
            Wire::UnitParityUnverified => Ok(Self::UnitParityUnverified),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseTestProductClassification {
    #[serde(rename = "spot")]
    Spot,
    #[serde(rename = "cfm")]
    Cfm,
    #[serde(rename = "intx")]
    Intx,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseTestProductClassification {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "spot")]
            Spot,
            #[serde(rename = "cfm")]
            Cfm,
            #[serde(rename = "intx")]
            Intx,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Spot => Ok(Self::Spot),
            Wire::Cfm => Ok(Self::Cfm),
            Wire::Intx => Ok(Self::Intx),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpShadowReadinessPlanAuthentication {
    #[serde(rename = "starbase-http")]
    StarbaseHttp,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseHttpShadowReadinessPlanAuthentication {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "starbase-http")]
            StarbaseHttp,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::StarbaseHttp => Ok(Self::StarbaseHttp),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads {
    #[serde(rename = "starbase-http-json-rpc-results")]
    StarbaseHttpJsonRpcResults,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "starbase-http-json-rpc-results")]
            StarbaseHttpJsonRpcResults,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::StarbaseHttpJsonRpcResults => Ok(Self::StarbaseHttpJsonRpcResults),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpShadowReadinessPlanPublicReads {
    #[serde(rename = "starbase-http-json-rpc-results")]
    StarbaseHttpJsonRpcResults,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseHttpShadowReadinessPlanPublicReads {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "starbase-http-json-rpc-results")]
            StarbaseHttpJsonRpcResults,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::StarbaseHttpJsonRpcResults => Ok(Self::StarbaseHttpJsonRpcResults),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpShadowReadinessPlanTransport {
    #[serde(rename = "http")]
    Http,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseHttpShadowReadinessPlanTransport {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "http")]
            Http,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Http => Ok(Self::Http),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpShadowReadinessPlanWrites {
    #[serde(rename = "disabled")]
    Disabled,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseHttpShadowReadinessPlanWrites {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "disabled")]
            Disabled,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Disabled => Ok(Self::Disabled),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseHttpShadowReadinessPlan {
    #[serde(rename = "authentication")]
    pub authentication: CoinbaseStarbaseHttpShadowReadinessPlanAuthentication,
    #[serde(rename = "privateReads")]
    pub private_reads: CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads,
    #[serde(rename = "publicReads")]
    pub public_reads: CoinbaseStarbaseHttpShadowReadinessPlanPublicReads,
    #[serde(rename = "transport")]
    pub transport: CoinbaseStarbaseHttpShadowReadinessPlanTransport,
    #[serde(rename = "writes")]
    pub writes: CoinbaseStarbaseHttpShadowReadinessPlanWrites,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for CoinbaseStarbaseHttpShadowReadinessPlan
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "authentication")]
            authentication: CoinbaseStarbaseHttpShadowReadinessPlanAuthentication,
            #[serde(rename = "privateReads")]
            private_reads: CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads,
            #[serde(rename = "publicReads")]
            public_reads: CoinbaseStarbaseHttpShadowReadinessPlanPublicReads,
            #[serde(rename = "transport")]
            transport: CoinbaseStarbaseHttpShadowReadinessPlanTransport,
            #[serde(rename = "writes")]
            writes: CoinbaseStarbaseHttpShadowReadinessPlanWrites,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            authentication: v.authentication,
            private_reads: v.private_reads,
            public_reads: v.public_reads,
            transport: v.transport,
            writes: v.writes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseHttpShadowReadiness {
    #[serde(rename = "eligible")]
    pub eligible: bool,
    #[serde(rename = "productClassification")]
    pub product_classification: CoinbaseStarbaseTestProductClassification,
    #[serde(rename = "blockers")]
    pub blockers: Vec<CoinbaseStarbaseHttpShadowBlocker>,
    #[serde(rename = "plan")]
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present"
    )]
    pub plan: Option<CoinbaseStarbaseHttpShadowReadinessPlan>,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for CoinbaseStarbaseHttpShadowReadiness {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "eligible")]
            eligible: bool,
            #[serde(rename = "productClassification")]
            product_classification: CoinbaseStarbaseTestProductClassification,
            #[serde(rename = "blockers")]
            blockers: Vec<CoinbaseStarbaseHttpShadowBlocker>,
            #[serde(rename = "plan")]
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                deserialize_with = "deserialize_present"
            )]
            plan: Option<CoinbaseStarbaseHttpShadowReadinessPlan>,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            eligible: v.eligible,
            product_classification: v.product_classification,
            blockers: v.blockers,
            plan: v.plan,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanProductBindingRequired {
    #[serde(rename = "trusted-runtime-intx-portfolio-binding")]
    TrustedRuntimeIntxPortfolioBinding,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanProductBindingRequired {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "trusted-runtime-intx-portfolio-binding")]
            TrustedRuntimeIntxPortfolioBinding,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::TrustedRuntimeIntxPortfolioBinding => {
                Ok(Self::TrustedRuntimeIntxPortfolioBinding)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanProductBindingStatus {
    #[serde(rename = "unbound")]
    Unbound,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanProductBindingStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "unbound")]
            Unbound,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Unbound => Ok(Self::Unbound),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverPlanProductBinding {
    #[serde(rename = "required")]
    pub required: CoinbaseStarbaseCutoverPlanProductBindingRequired,
    #[serde(rename = "status")]
    pub status: CoinbaseStarbaseCutoverPlanProductBindingStatus,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for CoinbaseStarbaseCutoverPlanProductBinding
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "required")]
            required: CoinbaseStarbaseCutoverPlanProductBindingRequired,
            #[serde(rename = "status")]
            status: CoinbaseStarbaseCutoverPlanProductBindingStatus,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            required: v.required,
            status: v.status,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads {
    #[serde(rename = "legacy-coinbase")]
    LegacyCoinbase,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "legacy-coinbase")]
            LegacyCoinbase,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::LegacyCoinbase => Ok(Self::LegacyCoinbase),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanAuthoritativePublicReads {
    #[serde(rename = "legacy-coinbase")]
    LegacyCoinbase,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanAuthoritativePublicReads {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "legacy-coinbase")]
            LegacyCoinbase,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::LegacyCoinbase => Ok(Self::LegacyCoinbase),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanAuthoritativeWrites {
    #[serde(rename = "legacy-coinbase")]
    LegacyCoinbase,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanAuthoritativeWrites {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "legacy-coinbase")]
            LegacyCoinbase,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::LegacyCoinbase => Ok(Self::LegacyCoinbase),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverPlanAuthoritative {
    #[serde(rename = "privateReads")]
    pub private_reads: CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads,
    #[serde(rename = "publicReads")]
    pub public_reads: CoinbaseStarbaseCutoverPlanAuthoritativePublicReads,
    #[serde(rename = "writes")]
    pub writes: CoinbaseStarbaseCutoverPlanAuthoritativeWrites,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>>
    for CoinbaseStarbaseCutoverPlanAuthoritative
{
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "privateReads")]
            private_reads: CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads,
            #[serde(rename = "publicReads")]
            public_reads: CoinbaseStarbaseCutoverPlanAuthoritativePublicReads,
            #[serde(rename = "writes")]
            writes: CoinbaseStarbaseCutoverPlanAuthoritativeWrites,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            private_reads: v.private_reads,
            public_reads: v.public_reads,
            writes: v.writes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "bool", into = "bool")]
pub enum CoinbaseStarbaseFalse {
    False,
}
impl From<CoinbaseStarbaseFalse> for bool {
    fn from(_: CoinbaseStarbaseFalse) -> Self {
        false
    }
}
impl TryFrom<bool> for CoinbaseStarbaseFalse {
    type Error = &'static str;
    fn try_from(value: bool) -> Result<Self, Self::Error> {
        if value {
            Err("expected false")
        } else {
            Ok(Self::False)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem {
    #[serde(rename = "credentialed-http-authentication-unverified")]
    CredentialedHttpAuthenticationUnverified,
    #[serde(rename = "http-authentication-contract-not-implemented")]
    HttpAuthenticationContractNotImplemented,
    #[serde(rename = "private-http-response-contracts-not-implemented")]
    PrivateHttpResponseContractsNotImplemented,
    #[serde(rename = "product-outside-intx-migration")]
    ProductOutsideIntxMigration,
    #[serde(rename = "public-http-response-contracts-not-implemented")]
    PublicHttpResponseContractsNotImplemented,
    #[serde(rename = "runtime-product-binding-unavailable")]
    RuntimeProductBindingUnavailable,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "credentialed-http-authentication-unverified")]
            CredentialedHttpAuthenticationUnverified,
            #[serde(rename = "http-authentication-contract-not-implemented")]
            HttpAuthenticationContractNotImplemented,
            #[serde(rename = "private-http-response-contracts-not-implemented")]
            PrivateHttpResponseContractsNotImplemented,
            #[serde(rename = "product-outside-intx-migration")]
            ProductOutsideIntxMigration,
            #[serde(rename = "public-http-response-contracts-not-implemented")]
            PublicHttpResponseContractsNotImplemented,
            #[serde(rename = "runtime-product-binding-unavailable")]
            RuntimeProductBindingUnavailable,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::CredentialedHttpAuthenticationUnverified => {
                Ok(Self::CredentialedHttpAuthenticationUnverified)
            }
            Wire::HttpAuthenticationContractNotImplemented => {
                Ok(Self::HttpAuthenticationContractNotImplemented)
            }
            Wire::PrivateHttpResponseContractsNotImplemented => {
                Ok(Self::PrivateHttpResponseContractsNotImplemented)
            }
            Wire::ProductOutsideIntxMigration => Ok(Self::ProductOutsideIntxMigration),
            Wire::PublicHttpResponseContractsNotImplemented => {
                Ok(Self::PublicHttpResponseContractsNotImplemented)
            }
            Wire::RuntimeProductBindingUnavailable => Ok(Self::RuntimeProductBindingUnavailable),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverPlanHttpShadow {
    #[serde(rename = "active")]
    pub active: CoinbaseStarbaseFalse,
    #[serde(rename = "eligible")]
    pub eligible: CoinbaseStarbaseFalse,
    #[serde(rename = "runtimeActivationEnabled")]
    pub runtime_activation_enabled: CoinbaseStarbaseFalse,
    #[serde(rename = "blockers")]
    pub blockers: Vec<CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem>,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for CoinbaseStarbaseCutoverPlanHttpShadow {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "active")]
            active: CoinbaseStarbaseFalse,
            #[serde(rename = "eligible")]
            eligible: CoinbaseStarbaseFalse,
            #[serde(rename = "runtimeActivationEnabled")]
            runtime_activation_enabled: CoinbaseStarbaseFalse,
            #[serde(rename = "blockers")]
            blockers: Vec<CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem>,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            active: v.active,
            eligible: v.eligible,
            runtime_activation_enabled: v.runtime_activation_enabled,
            blockers: v.blockers,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications {
    #[serde(rename = "disabled")]
    Disabled,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "disabled")]
            Disabled,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Disabled => Ok(Self::Disabled),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions {
    #[serde(rename = "disabled")]
    Disabled,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "disabled")]
            Disabled,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Disabled => Ok(Self::Disabled),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseCutoverPlanWebsocketSession {
    #[serde(rename = "disabled")]
    Disabled,
}
impl<'de> Deserialize<'de> for CoinbaseStarbaseCutoverPlanWebsocketSession {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Wire {
            #[serde(rename = "disabled")]
            Disabled,
        }
        // TS literal unions accept strings only; derived enums also accept tagged objects.
        let value = String::deserialize(deserializer)?;
        match Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))? {
            Wire::Disabled => Ok(Self::Disabled),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverPlanWebsocket {
    #[serde(rename = "active")]
    pub active: CoinbaseStarbaseFalse,
    #[serde(rename = "privateNotifications")]
    pub private_notifications: CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications,
    #[serde(rename = "publicSubscriptions")]
    pub public_subscriptions: CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions,
    #[serde(rename = "runtimeActivationEnabled")]
    pub runtime_activation_enabled: CoinbaseStarbaseFalse,
    #[serde(rename = "session")]
    pub session: CoinbaseStarbaseCutoverPlanWebsocketSession,
    #[serde(rename = "blockers")]
    pub blockers: Vec<CoinbaseStarbaseWebsocketBlocker>,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for CoinbaseStarbaseCutoverPlanWebsocket {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "active")]
            active: CoinbaseStarbaseFalse,
            #[serde(rename = "privateNotifications")]
            private_notifications: CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications,
            #[serde(rename = "publicSubscriptions")]
            public_subscriptions: CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions,
            #[serde(rename = "runtimeActivationEnabled")]
            runtime_activation_enabled: CoinbaseStarbaseFalse,
            #[serde(rename = "session")]
            session: CoinbaseStarbaseCutoverPlanWebsocketSession,
            #[serde(rename = "blockers")]
            blockers: Vec<CoinbaseStarbaseWebsocketBlocker>,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            active: v.active,
            private_notifications: v.private_notifications,
            public_subscriptions: v.public_subscriptions,
            runtime_activation_enabled: v.runtime_activation_enabled,
            session: v.session,
            blockers: v.blockers,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverPlanCutover {
    #[serde(rename = "eligible")]
    pub eligible: CoinbaseStarbaseFalse,
    #[serde(rename = "runtimeActivationEnabled")]
    pub runtime_activation_enabled: CoinbaseStarbaseFalse,
    #[serde(rename = "blockers")]
    pub blockers: Vec<CoinbaseStarbaseCutoverBlocker>,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for CoinbaseStarbaseCutoverPlanCutover {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "eligible")]
            eligible: CoinbaseStarbaseFalse,
            #[serde(rename = "runtimeActivationEnabled")]
            runtime_activation_enabled: CoinbaseStarbaseFalse,
            #[serde(rename = "blockers")]
            blockers: Vec<CoinbaseStarbaseCutoverBlocker>,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            eligible: v.eligible,
            runtime_activation_enabled: v.runtime_activation_enabled,
            blockers: v.blockers,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "serde_json::Map<String, serde_json::Value>")]
pub struct CoinbaseStarbaseCutoverPlan {
    #[serde(rename = "productBinding")]
    pub product_binding: CoinbaseStarbaseCutoverPlanProductBinding,
    #[serde(rename = "authoritative")]
    pub authoritative: CoinbaseStarbaseCutoverPlanAuthoritative,
    #[serde(rename = "httpShadow")]
    pub http_shadow: CoinbaseStarbaseCutoverPlanHttpShadow,
    #[serde(rename = "websocket")]
    pub websocket: CoinbaseStarbaseCutoverPlanWebsocket,
    #[serde(rename = "cutover")]
    pub cutover: CoinbaseStarbaseCutoverPlanCutover,
}
impl TryFrom<serde_json::Map<String, serde_json::Value>> for CoinbaseStarbaseCutoverPlan {
    type Error = serde_json::Error;
    fn try_from(map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "productBinding")]
            product_binding: CoinbaseStarbaseCutoverPlanProductBinding,
            #[serde(rename = "authoritative")]
            authoritative: CoinbaseStarbaseCutoverPlanAuthoritative,
            #[serde(rename = "httpShadow")]
            http_shadow: CoinbaseStarbaseCutoverPlanHttpShadow,
            #[serde(rename = "websocket")]
            websocket: CoinbaseStarbaseCutoverPlanWebsocket,
            #[serde(rename = "cutover")]
            cutover: CoinbaseStarbaseCutoverPlanCutover,
        }
        let v: Fields = serde_json::from_value(serde_json::Value::Object(map))?;
        Ok(Self {
            product_binding: v.product_binding,
            authoritative: v.authoritative,
            http_shadow: v.http_shadow,
            websocket: v.websocket,
            cutover: v.cutover,
        })
    }
}

// ts: collectHttpShadowBlockers
pub fn collect_http_shadow_blockers(
    product: CoinbaseStarbaseTestProductClassification,
    evidence: &CoinbaseStarbaseCutoverEvidence,
) -> Vec<CoinbaseStarbaseHttpShadowBlocker> {
    use CoinbaseStarbaseHttpShadowBlocker::*;
    use CoinbaseStarbaseImplementationStatus::Implemented;
    use CoinbaseStarbaseVerificationStatus::Verified;
    if product != CoinbaseStarbaseTestProductClassification::Intx {
        return vec![ProductOutsideIntxMigration];
    }
    let mut blockers = Vec::new();
    if evidence.public_http_response_contracts != Implemented {
        blockers.push(PublicHttpResponseContractsNotImplemented);
    }
    if evidence.private_http_response_contracts != Implemented {
        blockers.push(PrivateHttpResponseContractsNotImplemented);
    }
    if evidence.http_authentication_contract != Implemented {
        blockers.push(HttpAuthenticationContractNotImplemented);
    }
    if evidence.credentialed_http_authentication != Verified {
        blockers.push(CredentialedHttpAuthenticationUnverified);
    }
    blockers
}

// ts: collectWebsocketBlockers
pub fn collect_websocket_blockers(
    evidence: &CoinbaseStarbaseCutoverEvidence,
) -> Vec<CoinbaseStarbaseWebsocketBlocker> {
    use CoinbaseStarbaseImplementationStatus::Implemented;
    use CoinbaseStarbaseVerificationStatus::Verified;
    use CoinbaseStarbaseWebsocketBlocker::*;
    let mut blockers = Vec::new();
    if evidence.websocket_session_contract != Implemented {
        blockers.push(WebsocketSessionContractNotImplemented);
    }
    if evidence.credentialed_websocket_session_lifecycle != Verified {
        blockers.push(CredentialedWebsocketSessionLifecycleUnverified);
    }
    if evidence.private_notification_envelope != Verified {
        blockers.push(PrivateNotificationEnvelopeUnverified);
    }
    blockers.push(WebsocketActivationDisabled);
    blockers
}

// ts: validateEvidence
pub fn validate_evidence(
    value: Option<&serde_json::Value>,
) -> Result<(), crate::starbase::CoinbaseStarbaseProtocolError> {
    use crate::starbase::CoinbaseStarbaseProtocolError;
    const KEYS: [&str; 11] = [
        "credentialedHttpAuthentication",
        "credentialedHttpReadParity",
        "credentialedWebsocketSessionLifecycle",
        "credentialedWriteCanary",
        "cutoverRunbook",
        "httpAuthenticationContract",
        "privateHttpResponseContracts",
        "privateNotificationEnvelope",
        "publicHttpResponseContracts",
        "unitParity",
        "websocketSessionContract",
    ];
    let error = |suffix: String| {
        CoinbaseStarbaseProtocolError::new(format!("Invalid Starbase cutover evidence{suffix}"))
    };
    let object = value
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| error(String::new()))?;
    let keys = safe_cex_num::js_own_keys(object.keys().map(String::as_str));
    // ts-bug: validateEvidence tests the first unknown key's truthiness. An empty
    // first unknown key suppresses rejection of every later unknown string key.
    if let Some(key) = keys
        .into_iter()
        .find(|key| !KEYS.contains(key))
        .filter(|key| !key.is_empty())
    {
        return Err(error(format!(": unknown key {key}")));
    }
    if let Some(key) = KEYS.iter().find(|key| !object.contains_key(**key)) {
        return Err(error(format!(": missing key {key}")));
    }
    for key in KEYS {
        let status = object.get(key).and_then(serde_json::Value::as_str);
        let valid = if key.ends_with("Contract") || key.ends_with("Contracts") {
            matches!(status, Some("not-implemented" | "implemented"))
        } else {
            matches!(status, Some("unverified" | "verified"))
        };
        if !valid {
            return Err(error(format!(": {key}")));
        }
    }
    Ok(())
}

// ts: evaluateCoinbaseStarbaseHttpShadowReadinessForTestsOnly
pub fn evaluate_coinbase_starbase_http_shadow_readiness_for_tests_only(
    options: &serde_json::Map<String, serde_json::Value>,
) -> Result<CoinbaseStarbaseHttpShadowReadiness, crate::starbase::CoinbaseStarbaseProtocolError> {
    use crate::starbase::CoinbaseStarbaseProtocolError;
    let evidence = options.get("evidence");
    validate_evidence(evidence)?;
    let product_classification = match options
        .get("productClassification")
        .and_then(serde_json::Value::as_str)
    {
        Some("spot") => CoinbaseStarbaseTestProductClassification::Spot,
        Some("cfm") => CoinbaseStarbaseTestProductClassification::Cfm,
        Some("intx") => CoinbaseStarbaseTestProductClassification::Intx,
        _ => {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Invalid test-only Starbase product classification",
            ));
        }
    };
    // Validation has already checked each required status. Decode the checked
    // evidence without adding a panic path if its declared DTO later changes.
    let evidence = serde_json::from_value(evidence.cloned().unwrap_or(serde_json::Value::Null))
        .map_err(|_| CoinbaseStarbaseProtocolError::new("Invalid Starbase cutover evidence"))?;
    let blockers = collect_http_shadow_blockers(product_classification, &evidence);
    let eligible = blockers.is_empty();
    Ok(CoinbaseStarbaseHttpShadowReadiness {
        eligible,
        product_classification,
        blockers,
        plan: eligible.then_some(CoinbaseStarbaseHttpShadowReadinessPlan {
            authentication: CoinbaseStarbaseHttpShadowReadinessPlanAuthentication::StarbaseHttp,
            private_reads:
                CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads::StarbaseHttpJsonRpcResults,
            public_reads:
                CoinbaseStarbaseHttpShadowReadinessPlanPublicReads::StarbaseHttpJsonRpcResults,
            transport: CoinbaseStarbaseHttpShadowReadinessPlanTransport::Http,
            writes: CoinbaseStarbaseHttpShadowReadinessPlanWrites::Disabled,
        }),
    })
}

// ts: selectCoinbaseStarbaseCutoverPlan
pub fn select_coinbase_starbase_cutover_plan()
-> Result<CoinbaseStarbaseCutoverPlan, crate::starbase::CoinbaseStarbaseProtocolError> {
    use crate::protocol::{
        COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE,
        COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED,
        COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED,
        COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED,
    };
    use CoinbaseStarbaseVerificationStatus::Verified;
    let codec_error = |error: serde_json::Error| {
        crate::starbase::CoinbaseStarbaseProtocolError::new(error.to_string())
    };
    let evidence = serde_json::Value::Object(
        COINBASE_STARBASE_CURRENT_CUTOVER_EVIDENCE
            .iter()
            .map(|(key, value)| {
                (
                    key.to_string(),
                    serde_json::Value::String(value.to_string()),
                )
            })
            .collect(),
    );
    validate_evidence(Some(&evidence))?;
    let evidence: CoinbaseStarbaseCutoverEvidence =
        serde_json::from_value(evidence).map_err(codec_error)?;
    let http_blockers =
        collect_http_shadow_blockers(CoinbaseStarbaseTestProductClassification::Intx, &evidence);
    let websocket_blockers = collect_websocket_blockers(&evidence);
    let mut http_shadow_blockers = vec![serde_json::json!("runtime-product-binding-unavailable")];
    http_shadow_blockers.extend(
        http_blockers
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(codec_error)?,
    );
    let mut cutover_blockers = http_shadow_blockers.clone();
    if evidence.unit_parity != Verified {
        cutover_blockers.push(serde_json::json!("unit-parity-unverified"));
    }
    if evidence.credentialed_http_read_parity != Verified {
        cutover_blockers.push(serde_json::json!(
            "credentialed-http-read-parity-unverified"
        ));
    }
    cutover_blockers.extend(
        websocket_blockers
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()
            .map_err(codec_error)?,
    );
    if evidence.credentialed_write_canary != Verified {
        cutover_blockers.push(serde_json::json!("credentialed-write-canary-unverified"));
    }
    if evidence.cutover_runbook != Verified {
        cutover_blockers.push(serde_json::json!("cutover-runbook-unverified"));
    }
    cutover_blockers.push(serde_json::json!("runtime-activation-disabled"));
    // Convert the closed source object into its existing strict DTO. The current
    // constants satisfy that shape; propagate any future codec mismatch as an
    // error rather than creating a panic path in a browser consumer.
    serde_json::from_value(serde_json::json!({
        "productBinding": {"required":"trusted-runtime-intx-portfolio-binding","status":"unbound"},
        "authoritative": {"privateReads":"legacy-coinbase","publicReads":"legacy-coinbase","writes":"legacy-coinbase"},
        "httpShadow": {"active":false,"eligible":false,"runtimeActivationEnabled":COINBASE_STARBASE_HTTP_SHADOW_ACTIVATION_ENABLED,"blockers":http_shadow_blockers},
        "websocket": {"active":false,"privateNotifications":"disabled","publicSubscriptions":"disabled","runtimeActivationEnabled":COINBASE_STARBASE_WEBSOCKET_ACTIVATION_ENABLED,"session":"disabled","blockers":websocket_blockers},
        "cutover": {"eligible":false,"runtimeActivationEnabled":COINBASE_STARBASE_RUNTIME_ACTIVATION_ENABLED,"blockers":cutover_blockers},
    })).map_err(codec_error)
}
