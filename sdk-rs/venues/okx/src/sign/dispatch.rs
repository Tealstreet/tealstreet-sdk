//! Canonical OKX work. The injected provider owns all credential material.
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, SignStep, num};

fn work(
    identity: &Identity,
    payload: String,
    context: Context,
    stamp_ms: f64,
) -> Result<SignRequest, SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let passphrase = identity
        .passphrase
        .clone()
        .ok_or(SignError::InvalidRequest)?;
    let mut work = hmac_sha256_work(
        identity,
        vec![PayloadPart::Bytes {
            bytes: payload.into_bytes(),
        }],
        context,
        SignReceipt {
            stamp_ms: Some(stamp_ms),
            expiry_ms: None,
        },
    );
    work.operations.push(Operation::Present { key: passphrase });
    Ok(work)
}

fn values(result: &SignResult) -> Result<(&str, String, &str), SignError> {
    fn text(value: &[u8]) -> Result<&str, SignError> {
        std::str::from_utf8(value)
            .ok()
            .filter(|text| !text.is_empty() && !text.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)
    }
    if result.values.len() != 3 {
        return Err(SignError::InvalidResult);
    }
    let signature = result.values[1].as_slice();
    if signature.len() != 32 {
        return Err(SignError::InvalidResult);
    }
    Ok((
        text(&result.values[0])?,
        safe_cex_crypto::encode(signature, safe_cex_crypto::Encoding::Base64),
        text(&result.values[2])?,
    ))
}

fn context(origin: &str, method: &str, path: &str) -> Context {
    Context {
        venue: "okx".into(),
        method: method.into(),
        host: origin
            .split_once("://")
            .map_or(origin, |(_, rest)| rest)
            .split('/')
            .next()
            .unwrap_or_default()
            .into(),
        path: path.into(),
    }
}

pub(crate) struct Http {
    identity: Option<Identity>,
    path: String,
    stamp: String,
}
impl Http {
    pub fn new(identity: Option<Identity>, path: String) -> Self {
        Self {
            identity,
            path,
            stamp: String::new(),
        }
    }
}
impl SignStep for Http {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (_, authority_path) = http
            .url
            .split_once("://")
            .ok_or(SignError::InvalidRequest)?;
        let path_start = authority_path.find('/').ok_or(SignError::InvalidRequest)?;
        if authority_path[path_start..] != self.path {
            return Err(SignError::InvalidRequest);
        }
        let stamp_ms = dispatch.stamp_ms();
        self.stamp = safe_cex_engine::time::iso_millis(stamp_ms as i64);
        work(
            self.identity.as_ref().ok_or(SignError::Unavailable)?,
            format!(
                "{}{}{}{}",
                self.stamp,
                http.method.as_str(),
                self.path,
                http.body.as_deref().unwrap_or_default()
            ),
            context(&http.url, http.method.as_str(), &self.path),
            stamp_ms,
        )
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (key, signature, passphrase) = values(result)?;
        let mut headers = vec![
            ("OK-ACCESS-KEY", key.into()),
            ("OK-ACCESS-SIGN", signature),
            ("OK-ACCESS-TIMESTAMP", self.stamp.clone()),
            ("OK-ACCESS-PASSPHRASE", passphrase.into()),
        ];
        headers.append(&mut http.headers);
        http.headers = headers;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

pub(crate) struct WsLogin {
    identity: Identity,
    origin: String,
    stamp: String,
}
impl WsLogin {
    pub fn new(identity: Identity, origin: String) -> Self {
        Self {
            identity,
            origin,
            stamp: String::new(),
        }
    }
}
impl SignStep for WsLogin {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        if !matches!(outbound, Action::WsSend { .. }) {
            return Err(SignError::InvalidRequest);
        }
        let seconds = (dispatch.stamp_ms() / 1000.).floor();
        self.stamp = num::format(seconds);
        work(
            &self.identity,
            format!("{}GET/users/self/verify", self.stamp),
            context(&self.origin, "WS", "/users/self/verify"),
            seconds * 1000.,
        )
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (key, signature, passphrase) = values(result)?;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Login<'a> {
            api_key: &'a str,
            passphrase: &'a str,
            timestamp: &'a str,
            sign: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            op: &'static str,
            args: [Login<'a>; 1],
        }
        *text = serde_json::to_string(&Payload {
            op: "login",
            args: [Login {
                api_key: key,
                passphrase,
                timestamp: &self.stamp,
                sign: &signature,
            }],
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::{Command, Cx, Engine, Event, Method, Response, SocketId, TimerId, Venue};

    struct Probe {
        identity: Identity,
        origin: String,
        ws: bool,
        mismatch: bool,
    }
    impl Venue for Probe {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            if self.ws {
                let socket = cx.connect(self.origin.clone());
                cx.send_signed(
                    socket,
                    String::new(),
                    Box::new(WsLogin::new(self.identity.clone(), self.origin.clone())),
                );
            } else {
                let path = "/api/v5/trade/batch-orders?tag=a%2Bb";
                let request = cx.request(
                    Method::Post,
                    format!("{}{path}", self.origin),
                    vec![
                        ("x-simulated-trading", "1".into()),
                        ("Content-Type", "application/json".into()),
                    ],
                    Some(r#"[{"instId":"BTC-USDT-SWAP"}]"#.into()),
                    (),
                );
                cx.sign_http(
                    request,
                    Box::new(Http::new(
                        Some(self.identity.clone()),
                        if self.mismatch { "/different" } else { path }.into(),
                    )),
                );
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
        fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn staged(
        ws: bool,
        stamp: f64,
    ) -> (
        Engine<Probe>,
        LocalSigner,
        safe_cex_engine::RequestId,
        SignRequest,
    ) {
        let (provider, identities) = LocalSigner::from_json(r#"{"primary":{"key":"test-key","secret":"test-secret","passphrase":"test-passphrase"}}"#.into()).unwrap();
        let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
            identity: identities.primary.unwrap(),
            origin: if ws {
                "wss://wseeapap.okx.com/ws/v5/private"
            } else {
                "https://eea.okx.com"
            }
            .into(),
            ws,
            mismatch: false,
        });
        engine.start(stamp - 1000.);
        engine.prepare_dispatch(stamp);
        let (attempt, request) = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Sign { attempt, work, .. } => Some((attempt, work)),
                _ => None,
            })
            .unwrap();
        (engine, provider, attempt, request)
    }

    #[test]
    fn http_canonical_bytes_and_passphrase_are_one_atomic_provider_work() {
        let (mut engine, provider, attempt, request) = staged(false, 1_709_209_849_123.9);
        let payload = b"2024-02-29T12:30:49.123ZPOST/api/v5/trade/batch-orders?tag=a%2Bb[{\"instId\":\"BTC-USDT-SWAP\"}]";
        assert_eq!(request.receipt.stamp_ms, Some(1_709_209_849_123.));
        assert_eq!(request.context.host, "eea.okx.com");
        assert!(matches!(&request.operations[0], Operation::Present { .. }));
        assert!(
            matches!(&request.operations[1], Operation::Sign { payload: parts, .. } if parts == &vec![PayloadPart::Bytes { bytes: payload.to_vec() }])
        );
        assert!(matches!(&request.operations[2], Operation::Present { .. }));
        let signed = provider.sign_now(&request).unwrap();
        let expected = safe_cex_crypto::hmac_sha256(
            b"test-secret",
            payload,
            safe_cex_crypto::Encoding::Base64,
        );
        engine.handle(
            1_709_209_849_124.,
            Event::Signed {
                attempt,
                result: Ok(signed),
            },
        );
        let output: Vec<_> = engine.drain_actions().collect();
        let http = output
            .iter()
            .find_map(|action| match action {
                Action::Http(http) => Some(http),
                _ => None,
            })
            .unwrap();
        let header = |name| &http.headers.iter().find(|(key, _)| *key == name).unwrap().1;
        assert_eq!(header("OK-ACCESS-SIGN"), &expected);
        assert_eq!(header("OK-ACCESS-PASSPHRASE"), "test-passphrase");
        assert_eq!(header("OK-ACCESS-TIMESTAMP"), "2024-02-29T12:30:49.123Z");
        assert_eq!(header("x-simulated-trading"), "1");
        assert_eq!(
            http.headers,
            vec![
                ("OK-ACCESS-KEY", "test-key".into()),
                ("OK-ACCESS-SIGN", expected),
                ("OK-ACCESS-TIMESTAMP", "2024-02-29T12:30:49.123Z".into()),
                ("OK-ACCESS-PASSPHRASE", "test-passphrase".into()),
                ("x-simulated-trading", "1".into()),
                ("Content-Type", "application/json".into()),
            ]
        );
        assert_eq!(
            http.body.as_deref(),
            Some(r#"[{"instId":"BTC-USDT-SWAP"}]"#)
        );
    }

    #[test]
    fn mismatched_native_path_never_reaches_provider_or_transport() {
        let (_, identities) = LocalSigner::from_json(r#"{"primary":{"key":"test-key","secret":"test-secret","passphrase":"test-passphrase"}}"#.into()).unwrap();
        let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
            identity: identities.primary.unwrap(),
            origin: "https://eea.okx.com".into(),
            ws: false,
            mismatch: true,
        });
        engine.start(0.);
        engine.prepare_dispatch(1.);
        assert!(
            !engine
                .drain_actions()
                .any(|action| matches!(action, Action::Http(_) | Action::Sign { .. }))
        );
    }

    #[test]
    fn ws_uses_send_time_seconds_and_base64_without_moving_raw_time() {
        let (mut engine, provider, attempt, request) = staged(true, 1_234_567_890_999.9);
        assert_eq!(request.receipt.stamp_ms, Some(1_234_567_890_000.));
        let signed = provider.sign_now(&request).unwrap();
        // Independent fixed vector retained from the pre-migration signer.
        assert_eq!(
            safe_cex_crypto::encode(&signed.values[1], safe_cex_crypto::Encoding::Base64),
            "qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ="
        );
        engine.handle(
            1_234_567_891_000.,
            Event::Signed {
                attempt,
                result: Ok(signed),
            },
        );
        let text = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::WsSend { text, .. } => Some(text),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            text,
            r#"{"op":"login","args":[{"apiKey":"test-key","passphrase":"test-passphrase","timestamp":"1234567890","sign":"qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ="}]}"#
        );
    }
}
