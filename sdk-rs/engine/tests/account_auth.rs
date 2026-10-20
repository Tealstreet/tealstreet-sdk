use safe_cex_engine::book::Book;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, Command, CredentialSlot, Cx, Engine, Event, LogLevel,
    Method, Notice, PayloadSignRequest, RequestId, Response, SignatureTarget, SocketId, TimerId,
    Venue,
};

struct AccountReads {
    streamed: bool,
    tagged: bool,
    delegated: bool,
}

impl Venue for AccountReads {
    type Pending = usize;
    fn start(&mut self, cx: &mut Cx<'_, usize>) {
        let request = if self.streamed {
            cx.request_streamed(
                Method::Get,
                "https://fixture/account".into(),
                vec![],
                ArrayAt::Field("rows"),
                0,
            )
        } else {
            cx.request(
                Method::Get,
                "https://fixture/account".into(),
                vec![],
                None,
                0,
            )
        };
        if self.tagged {
            cx.tag_account_read(request, CredentialSlot::Secondary, AccountReadScope::Orders);
        }
        if self.delegated {
            cx.sign_request(
                request,
                PayloadSignRequest {
                    exchange_name: "fixture",
                    key: "public-key".into(),
                    payload: "payload".into(),
                    scheme: "fixture",
                    encoding: "hex",
                },
                SignatureTarget::Header("signature"),
            )
            .unwrap();
        }
    }
    fn account_auth_outcome(&self, _: &usize, response: &Response<'_>) -> Option<AuthOutcome> {
        if response.status == 401 {
            return Some(AuthOutcome::Rejected);
        }
        #[derive(serde::Deserialize)]
        struct Verdict {
            accepted: bool,
        }
        serde_json::from_str::<Verdict>(response.body)
            .ok()
            .map(|v| {
                if v.accepted {
                    AuthOutcome::Accepted
                } else {
                    AuthOutcome::Rejected
                }
            })
    }
    fn on_item(&mut self, _: &mut Cx<'_, usize>, count: &mut usize, _: &str) {
        *count += 1;
    }
    fn on_response(&mut self, cx: &mut Cx<'_, usize>, count: usize, _: Response<'_>) {
        cx.log(LogLevel::Info, format!("response:{count}"));
    }
    fn on_stream_error(&mut self, cx: &mut Cx<'_, usize>, count: usize, _: Response<'_>) {
        cx.log(LogLevel::Warning, format!("missing:{count}"));
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, usize>, _: usize, _: &str) {
        cx.log(LogLevel::Error, "failed");
    }
    fn on_open(&mut self, _: &mut Cx<'_, usize>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, usize>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, usize>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, usize>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, usize>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn engine(streamed: bool, tagged: bool, delegated: bool) -> Engine<AccountReads> {
    let mut engine = Engine::new(AccountReads {
        streamed,
        tagged,
        delegated,
    });
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine
}

fn complete(engine: &mut Engine<AccountReads>, status: u16, body: &str) -> Vec<Action> {
    engine.handle(
        1.,
        Event::HttpResponse {
            request: RequestId(1),
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    engine.drain_actions().collect()
}

fn evidence(outcome: AuthOutcome) -> Action {
    Action::Notice(Notice::AccountAuth {
        credential: CredentialSlot::Secondary,
        scope: AccountReadScope::Orders,
        outcome,
    })
}

#[test]
fn buffered_evidence_is_attributed_and_precedes_unchanged_response_once() {
    for (status, body, verdict) in [
        (200, r#"{"accepted":true}"#, AuthOutcome::Accepted),
        (200, r#"{"accepted":false}"#, AuthOutcome::Rejected),
        (401, "", AuthOutcome::Rejected),
    ] {
        let mut engine = engine(false, true, false);
        assert_eq!(
            complete(&mut engine, status, body),
            vec![
                evidence(verdict),
                Action::Notice(Notice::Log {
                    level: LogLevel::Info,
                    message: "response:0".into()
                })
            ]
        );
        assert!(complete(&mut engine, status, body).is_empty());
    }
}

#[test]
fn streamed_rows_preserve_context_until_final_envelope() {
    let mut engine = engine(true, true, false);
    engine.handle(
        1.,
        Event::HttpStart {
            request: RequestId(1),
            status: 200,
            headers: "",
        },
    );
    for data in [
        br#"{"rows":[{"id":1},"#.as_slice(),
        br#"{"id":2}],"accepted":true}"#.as_slice(),
    ] {
        engine.handle(
            2.,
            Event::HttpChunk {
                request: RequestId(1),
                data,
            },
        );
        assert!(engine.drain_actions().next().is_none());
    }
    engine.handle(
        3.,
        Event::HttpEnd {
            request: RequestId(1),
        },
    );
    assert_eq!(
        engine.drain_actions().collect::<Vec<_>>(),
        vec![
            evidence(AuthOutcome::Accepted),
            Action::Notice(Notice::Log {
                level: LogLevel::Info,
                message: "response:2".into()
            })
        ]
    );
}

#[test]
fn missing_stream_list_can_reject_but_cannot_report_recovery() {
    for accepted in [false, true] {
        let mut engine = engine(true, true, false);
        let actions = complete(&mut engine, 200, &format!(r#"{{"accepted":{accepted}}}"#));
        let mut expected = Vec::new();
        if !accepted {
            expected.push(evidence(AuthOutcome::Rejected));
        }
        expected.push(Action::Notice(Notice::Log {
            level: LogLevel::Warning,
            message: "missing:0".into(),
        }));
        assert_eq!(actions, expected);
    }
}

#[test]
fn invalid_or_truncated_stream_never_reports_auth_even_with_reject_status() {
    for body in [br#"{"accepted":false,"rows":[{}"#.as_slice(), b"\xff"] {
        let mut engine = engine(true, true, false);
        engine.handle(
            1.,
            Event::HttpStart {
                request: RequestId(1),
                status: 200,
                headers: "",
            },
        );
        engine.handle(
            1.,
            Event::HttpChunk {
                request: RequestId(1),
                data: body,
            },
        );
        engine.handle(
            1.,
            Event::HttpEnd {
                request: RequestId(1),
            },
        );
        assert!(
            engine
                .drain_actions()
                .all(|a| !matches!(a, Action::Notice(Notice::AccountAuth { .. })))
        );
    }
}

#[test]
fn malformed_unstreamed_and_untagged_public_responses_cannot_manufacture_auth() {
    let mut malformed = engine(false, true, false);
    assert!(
        complete(&mut malformed, 200, "not-json")
            .iter()
            .all(|a| !matches!(a, Action::Notice(Notice::AccountAuth { .. })))
    );
    for status in [200, 401] {
        let mut public = engine(false, false, false);
        assert!(
            complete(&mut public, status, r#"{"accepted":true}"#)
                .iter()
                .all(|a| !matches!(a, Action::Notice(Notice::AccountAuth { .. })))
        );
    }
}

#[test]
fn transport_and_sign_failure_drain_context_without_credential_evidence() {
    for delegated in [false, true] {
        let mut engine = engine(false, true, delegated);
        let event = if delegated {
            Event::PayloadSignFailed {
                request: RequestId(1),
                error: "offline",
            }
        } else {
            Event::HttpFailed {
                request: RequestId(1),
                error: "offline",
            }
        };
        engine.handle(1., event);
        assert_eq!(
            engine.drain_actions().collect::<Vec<_>>(),
            vec![Action::Notice(Notice::Log {
                level: LogLevel::Error,
                message: "failed".into()
            })]
        );
        assert!(complete(&mut engine, 401, "").is_empty());
    }
}

#[test]
fn invalid_utf8_buffer_cannot_report_http_status_auth() {
    let mut engine = engine(false, true, false);
    engine.handle(
        1.,
        Event::HttpStart {
            request: RequestId(1),
            status: 401,
            headers: "",
        },
    );
    engine.handle(
        1.,
        Event::HttpChunk {
            request: RequestId(1),
            data: b"\xff",
        },
    );
    engine.handle(
        1.,
        Event::HttpEnd {
            request: RequestId(1),
        },
    );
    assert_eq!(
        engine.drain_actions().collect::<Vec<_>>(),
        vec![Action::Notice(Notice::Log {
            level: LogLevel::Error,
            message: "failed".into()
        })]
    );
}
