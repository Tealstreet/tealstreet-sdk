//! Lossless host preparation tokens; claiming never drains engine actions.
use super::*;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Permit, Preparation, Prepared, RoutingIntent,
};
#[cfg(feature = "http-mutation-barriers")]
use safe_cex_engine::request_context::{GroupClaim, GroupPermit};

#[derive(Serialize)]
pub(super) struct HttpPacket<'a> {
    request: u64,
    method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<&'static str>,
    #[serde(rename = "timeoutMs", skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u32>,
    url: &'a str,
    headers: Vec<(&'a str, &'a str)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
}

impl<'a> From<&'a safe_cex_engine::HttpRequest> for HttpPacket<'a> {
    fn from(request: &'a safe_cex_engine::HttpRequest) -> Self {
        Self {
            request: request.id.0,
            method: request.method.as_str(),
            route: request.route.map(safe_cex_engine::RequestRoute::as_str),
            timeout_ms: request.timeout_ms,
            url: &request.url,
            headers: request
                .headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .chain(
                    request
                        .signer_headers
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_str())),
                )
                .collect(),
            body: request.body.as_deref(),
        }
    }
}

#[derive(Serialize)]
pub(super) struct GroupMemberOut<'a> {
    preparation: String,
    packet: Option<HttpPacket<'a>>,
}

#[derive(Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub(super) enum HttpPhaseOut<'a> {
    Prepare {
        preparation: String,
        intent: &'static str,
        packet: HttpPacket<'a>,
    },
    Bind {
        preparation: String,
        permit: String,
        packet: HttpPacket<'a>,
    },
    BindGroup {
        permit: String,
        members: Vec<GroupMemberOut<'a>>,
    },
    Cancel {
        preparation: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signing: Option<u64>,
    },
}

impl<'a> From<&'a HttpAction> for HttpPhaseOut<'a> {
    fn from(action: &'a HttpAction) -> Self {
        match action {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => Self::Prepare {
                preparation: preparation.to_wire(),
                intent: match intent {
                    RoutingIntent::Public => "public",
                    RoutingIntent::Primary => "primary",
                    RoutingIntent::Secondary => "secondary",
                },
                packet: HttpPacket::from(request),
            },
            HttpAction::Bind {
                preparation,
                permit,
                request,
            } => Self::Bind {
                preparation: preparation.to_wire(),
                permit: permit.to_wire(),
                packet: HttpPacket::from(request),
            },
            HttpAction::BindGroup { permit, members } => Self::BindGroup {
                permit: permit.to_wire(),
                members: members
                    .iter()
                    .map(|member| GroupMemberOut {
                        preparation: member.preparation.to_wire(),
                        packet: member.request.as_ref().map(HttpPacket::from),
                    })
                    .collect(),
            },
            HttpAction::Cancel {
                preparation,
                signing,
            } => Self::Cancel {
                preparation: preparation.to_wire(),
                signing: signing.map(|id| id.0),
            },
        }
    }
}

fn resource_id(text: &str) -> Option<u64> {
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum ClaimOut {
    Released,
    Wait { until: f64 },
    Denied,
}

#[wasm_bindgen]
impl WasmEngine {
    #[wasm_bindgen(js_name = signingPending)]
    pub fn signing_pending(&self, attempt: f64) -> Result<bool, js_sys::Error> {
        let attempt = host_resource_id(attempt)?;
        Ok(self.inner.signing_pending(RequestId(attempt)))
    }

    /// Read-only metadata of the current managed signing task, as JSON, or
    /// `undefined` (v1 only). Descriptive; it grants nothing.
    #[cfg(feature = "sign-meta")]
    #[wasm_bindgen(js_name = signMeta)]
    pub fn sign_meta(&self, attempt: f64) -> Result<Option<String>, js_sys::Error> {
        let attempt = host_resource_id(attempt)?;
        Ok(self.inner.sign_meta(RequestId(attempt)).map(|meta| {
            serde_json::to_string(&sign_meta::Out::from(meta)).expect("scalars serialize")
        }))
    }

    #[wasm_bindgen(js_name = httpPreparationPending)]
    pub fn http_preparation_pending(&self, preparation: &str) -> bool {
        Preparation::from_wire(preparation)
            .is_ok_and(|token| self.inner.http_preparation_pending(token))
    }

    #[wasm_bindgen(js_name = httpPrepared)]
    pub fn http_prepared(
        &mut self,
        preparation: &str,
        resource: &str,
        expires_at: Option<f64>,
        now: f64,
    ) -> Result<String, JsError> {
        let token = Preparation::from_wire(preparation)
            .map_err(|_| JsError::new("Invalid HTTP preparation"))?;
        let id = resource_id(resource).ok_or_else(|| JsError::new("Invalid HTTP resource"))?;
        self.inner
            .http_prepared(now, token, Prepared { id, expires_at });
        Ok(self.flush())
    }

    #[wasm_bindgen(js_name = httpPreparationFailed)]
    pub fn http_preparation_failed(
        &mut self,
        preparation: &str,
        code: &str,
        now: f64,
    ) -> Result<String, JsError> {
        let token = Preparation::from_wire(preparation)
            .map_err(|_| JsError::new("Invalid HTTP preparation"))?;
        let (reason, kind) = match code {
            "unsupported" => (Error::Invalid, safe_cex_engine::ErrorKind::Unsupported),
            "timedOut" => (Error::TimedOut, safe_cex_engine::ErrorKind::Timeout),
            "stale" => (Error::Stale, safe_cex_engine::ErrorKind::InvalidInput),
            "invalid" => (Error::Invalid, safe_cex_engine::ErrorKind::InvalidInput),
            _ => (Error::State, safe_cex_engine::ErrorKind::Network),
        };
        self.inner.http_preparation_failed(
            now,
            token,
            reason,
            safe_cex_engine::VenueError::new("HTTP transport preparation failed").kind(kind),
        );
        Ok(self.flush())
    }

    #[cfg(feature = "http-mutation-barriers")]
    #[wasm_bindgen(js_name = httpGroupPending)]
    pub fn http_group_pending(&self, permit: &str) -> bool {
        GroupPermit::from_wire(permit).is_ok_and(|token| self.inner.http_group_pending(&token))
    }

    /// Fixed output storage precedes claim: 0 denied, 1 released, 2 wait at slot 0.
    #[cfg(feature = "http-mutation-barriers")]
    #[wasm_bindgen(js_name = httpClaimGroup)]
    pub fn http_claim_group(&mut self, permit: &str, now: f64, bounds: &mut [f64]) -> u32 {
        let Ok(token) = GroupPermit::from_wire(permit) else {
            return 0;
        };
        if bounds.len() != token.members().len() {
            return 0;
        }
        match self.inner.claim_http_group(now, token) {
            Ok(GroupClaim::Released(releases)) => {
                assert_eq!(
                    releases.len(),
                    bounds.len(),
                    "group claim output cardinality"
                );
                for (bound, release) in bounds.iter_mut().zip(releases) {
                    *bound = release.send_before;
                }
                1
            }
            Ok(GroupClaim::WaitUntil(until)) => {
                bounds[0] = until;
                2
            }
            Err(_) => 0,
        }
    }

    /// No flush, provider invocation or other work before the host API handoff.
    #[wasm_bindgen(js_name = httpClaim)]
    pub fn http_claim(&mut self, permit: &str, now: f64) -> String {
        let result = Permit::from_wire(permit).and_then(|token| self.inner.claim_http(now, token));
        let out = match result {
            Ok(Claim::Released(_)) => ClaimOut::Released,
            Ok(Claim::WaitUntil(until)) => ClaimOut::Wait { until },
            Err(_) => ClaimOut::Denied,
        };
        // Closed finite scalars only; no request, credential or receipt payload.
        serde_json::to_string(&out).expect("HTTP claim scalars serialize")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn managed_wire_keeps_wide_tokens_and_legacy_http_bytes() {
        let request = safe_cex_engine::HttpRequest {
            id: RequestId(7),
            method: safe_cex_engine::Method::Post,
            route: None,
            timeout_ms: Some(900),
            url: "https://venue.test/read".into(),
            headers: vec![],
            signer_headers: vec![],
            body: Some("{}".into()),
        };
        assert_eq!(
            encode(&[Action::Http(request.clone())]),
            r#"[{"op":"http","request":7,"method":"POST","timeoutMs":900,"url":"https://venue.test/read","headers":[],"body":"{}"}]"#
        );
        let text = "p1:7:18446744073709551615:18446744073709551615";
        let preparation = Preparation::from_wire(text).unwrap();
        let permit_text = "r1:7:18446744073709551615:18446744073709551615:18446744073709551615";
        let actions = [
            HttpAction::Prepare {
                preparation,
                intent: RoutingIntent::Secondary,
                request: request.clone(),
            },
            HttpAction::Bind {
                preparation,
                permit: Permit::from_wire(permit_text).unwrap(),
                request,
            },
            HttpAction::Cancel {
                preparation,
                signing: None,
            },
        ]
        .map(|a| Action::ManagedHttp(Box::new(a)));
        let rows: serde_json::Value = serde_json::from_str(&encode(&actions)).unwrap();
        assert_eq!(rows[0]["op"], "managedHttp");
        assert_eq!(rows[0]["intent"], "secondary");
        assert_eq!(rows[0]["preparation"], text);
        assert_eq!(rows[1]["permit"], permit_text);
        assert_eq!(rows[1]["packet"]["request"], 7);
        assert_eq!(rows[2]["phase"], "cancel");
        assert_eq!(resource_id("18446744073709551615"), Some(u64::MAX));
        for invalid in ["0", "01", "+1", " 1", "18446744073709551616"] {
            assert_eq!(resource_id(invalid), None);
        }
    }

    #[test]
    fn managed_packets_and_cancel_signing_keep_safe_wide_numeric_ids() {
        let id = safe_cex_engine::resource_ids::MAX_RESOURCE_ID;
        let request = safe_cex_engine::HttpRequest {
            id: RequestId(id),
            method: safe_cex_engine::Method::Get,
            route: None,
            timeout_ms: Some(900),
            url: "https://venue.test/read".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        let text = format!("p1:{id}:18446744073709551615:0");
        let preparation = Preparation::from_wire(&text).unwrap();
        let actions = [
            Action::Http(request.clone()),
            Action::ManagedHttp(Box::new(HttpAction::Prepare {
                preparation,
                intent: RoutingIntent::Primary,
                request,
            })),
            Action::ManagedHttp(Box::new(HttpAction::Cancel {
                preparation,
                signing: Some(RequestId(id)),
            })),
        ];
        let rows: serde_json::Value = serde_json::from_str(&encode(&actions)).unwrap();
        assert_eq!(rows[0]["request"].as_u64(), Some(id));
        assert_eq!(rows[1]["packet"]["request"].as_u64(), Some(id));
        assert_eq!(rows[1]["preparation"], text);
        assert_eq!(rows[2]["signing"].as_u64(), Some(id));
    }
}

#[cfg(feature = "sign-meta")]
mod sign_meta {
    use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason};
    use safe_cex_engine::sign_meta::{SetupOperation, SignKind, SignMeta};
    use safe_cex_engine::{AccountReadScope, CredentialSlot};
    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    #[serde(untagged)]
    pub(super) enum Out {
        Business(Business),
        Setup {
            task: u64,
            owner: Owner,
            #[serde(rename = "deadlineMs")]
            deadline_ms: f64,
            setup: Setup,
        },
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub(super) struct Business {
        task: u64,
        owner: Owner,
        deadline_ms: f64,
        slot: &'static str,
        role: Role,
    }
    #[derive(Serialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    pub(super) enum Owner {
        Managed { request: u64, generation: u64 },
        Session { generation: u64 },
        Setup { generation: u64 },
    }
    #[derive(Serialize)]
    pub(super) struct Setup {
        operation: &'static str,
        slot: &'static str,
    }
    fn slot_name(slot: CredentialSlot) -> &'static str {
        match slot {
            CredentialSlot::Primary => "primary",
            CredentialSlot::Secondary => "secondary",
        }
    }
    #[derive(Serialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    enum Role {
        Read {
            scope: Option<&'static str>,
        },
        PrimaryRead {
            reason: &'static str,
            scope: Option<&'static str>,
        },
        Mutation,
        SessionAuth,
    }
    fn scope(scope: Option<AccountReadScope>) -> Option<&'static str> {
        scope.map(|scope| match scope {
            AccountReadScope::Account => "account",
            AccountReadScope::Balance => "balance",
            AccountReadScope::Orders => "orders",
            AccountReadScope::Positions => "positions",
            AccountReadScope::Settings => "settings",
        })
    }
    impl From<SignMeta> for Out {
        fn from(meta: SignMeta) -> Self {
            let (owner, slot, role) = match meta.kind {
                SignKind::Setup {
                    generation,
                    operation: SetupOperation::PublicAddress,
                    slot,
                } => {
                    return Self::Setup {
                        task: meta.task.0,
                        owner: Owner::Setup { generation },
                        deadline_ms: meta.deadline_ms,
                        setup: Setup {
                            operation: "publicAddress",
                            slot: slot_name(slot),
                        },
                    };
                }
                SignKind::Managed {
                    request,
                    generation,
                    slot,
                    role,
                } => (
                    Owner::Managed {
                        request: request.0,
                        generation,
                    },
                    slot,
                    role,
                ),
                SignKind::Session {
                    generation,
                    slot,
                    role,
                } => (Owner::Session { generation }, slot, role),
            };
            Self::Business(Business {
                task: meta.task.0,
                owner,
                deadline_ms: meta.deadline_ms,
                slot: slot_name(slot),
                role: match role {
                    CredentialRole::Read { scope: s } => Role::Read { scope: scope(s) },
                    CredentialRole::PrimaryRead { reason, scope: s } => Role::PrimaryRead {
                        reason: match reason {
                            PrimaryReason::ClientOrderLookup => "clientOrderLookup",
                            PrimaryReason::PostUnknownOrderRead => "postUnknownOrderRead",
                            PrimaryReason::DegradedRead => "degradedRead",
                            PrimaryReason::Catalog => "catalog",
                            PrimaryReason::History => "history",
                        },
                        scope: scope(s),
                    },
                    CredentialRole::Mutation => Role::Mutation,
                    CredentialRole::SessionAuth => Role::SessionAuth,
                },
            })
        }
    }
}

#[cfg(all(test, feature = "sign-meta"))]
mod sign_meta_wire_tests {
    use super::sign_meta::Out;
    use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason};
    use safe_cex_engine::sign_meta::{SetupOperation, SignKind, SignMeta};
    use safe_cex_engine::{AccountReadScope, CredentialSlot, RequestId};
    fn wire(kind: SignKind) -> String {
        serde_json::to_string(&Out::from(SignMeta {
            task: RequestId(7),
            deadline_ms: 20001.25,
            kind,
        }))
        .unwrap()
    }
    #[test]
    fn business_metadata_bytes_remain_landed_shape_and_order() {
        for (slot, slot_text) in [
            (CredentialSlot::Primary, "primary"),
            (CredentialSlot::Secondary, "secondary"),
        ] {
            for (scope, text) in [
                (None, "null"),
                (Some(AccountReadScope::Account), "\"account\""),
                (Some(AccountReadScope::Balance), "\"balance\""),
                (Some(AccountReadScope::Orders), "\"orders\""),
                (Some(AccountReadScope::Positions), "\"positions\""),
                (Some(AccountReadScope::Settings), "\"settings\""),
            ] {
                assert_eq!(
                    wire(SignKind::Managed {
                        request: RequestId(19),
                        generation: 3,
                        slot,
                        role: CredentialRole::Read { scope }
                    }),
                    format!(
                        "{{\"task\":7,\"owner\":{{\"kind\":\"managed\",\"request\":19,\"generation\":3}},\"deadlineMs\":20001.25,\"slot\":\"{slot_text}\",\"role\":{{\"kind\":\"read\",\"scope\":{text}}}}}"
                    )
                );
            }
        }
        for (reason, text) in [
            (PrimaryReason::ClientOrderLookup, "clientOrderLookup"),
            (PrimaryReason::PostUnknownOrderRead, "postUnknownOrderRead"),
            (PrimaryReason::DegradedRead, "degradedRead"),
            (PrimaryReason::Catalog, "catalog"),
            (PrimaryReason::History, "history"),
        ] {
            assert_eq!(
                wire(SignKind::Managed {
                    request: RequestId(19),
                    generation: 3,
                    slot: CredentialSlot::Primary,
                    role: CredentialRole::PrimaryRead {
                        reason,
                        scope: None
                    }
                }),
                format!(
                    "{{\"task\":7,\"owner\":{{\"kind\":\"managed\",\"request\":19,\"generation\":3}},\"deadlineMs\":20001.25,\"slot\":\"primary\",\"role\":{{\"kind\":\"primaryRead\",\"reason\":\"{text}\",\"scope\":null}}}}"
                )
            );
        }
        assert_eq!(
            wire(SignKind::Session {
                generation: 3,
                slot: CredentialSlot::Primary,
                role: CredentialRole::SessionAuth
            }),
            r#"{"task":7,"owner":{"kind":"session","generation":3},"deadlineMs":20001.25,"slot":"primary","role":{"kind":"sessionAuth"}}"#
        );
        assert_eq!(
            wire(SignKind::Managed {
                request: RequestId(19),
                generation: 3,
                slot: CredentialSlot::Primary,
                role: CredentialRole::Mutation
            }),
            r#"{"task":7,"owner":{"kind":"managed","request":19,"generation":3},"deadlineMs":20001.25,"slot":"primary","role":{"kind":"mutation"}}"#
        );
    }
    #[test]
    fn setup_metadata_has_only_its_closed_setup_arm() {
        assert_eq!(
            wire(SignKind::Setup {
                generation: 3,
                operation: SetupOperation::PublicAddress,
                slot: CredentialSlot::Secondary
            }),
            r#"{"task":7,"owner":{"kind":"setup","generation":3},"deadlineMs":20001.25,"setup":{"operation":"publicAddress","slot":"secondary"}}"#
        );
    }
}
