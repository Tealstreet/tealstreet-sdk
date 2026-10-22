//! Lossless host preparation tokens; claiming never drains engine actions.
use super::*;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Permit, Preparation, Prepared, RoutingIntent,
};

#[derive(Serialize)]
pub(super) struct HttpPacket<'a> {
    request: u32,
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
    Cancel {
        preparation: String,
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
            HttpAction::Cancel { preparation } => Self::Cancel {
                preparation: preparation.to_wire(),
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
            HttpAction::Cancel { preparation },
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
}
