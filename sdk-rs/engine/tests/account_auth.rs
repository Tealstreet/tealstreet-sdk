mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, Command, CredentialSlot, Cx, Engine, Event, Level,
    LogEvent, Method, Notice, RequestId, Response, SocketId, TimerId, Venue,
};

/// Diagnostic the fixture venue emits so tests can see which callback ran.
const MARKER: &str = "test.marker";
fn marker(level: Level, message: &str) -> Action {
    Action::Notice(Notice::Event(LogEvent {
        level,
        code: MARKER,
        message: message.into(),
        fields: Vec::new(),
    }))
}

struct AccountReads {
    streamed: bool,
    tagged: bool,
    valid_before: Option<f64>,
    token: Option<safe_cex_engine::OrderSnapshotToken>,
}

impl Venue for AccountReads {
    type Pending = usize;
    fn start(&mut self, cx: &mut Cx<'_, usize>) {
        self.token = cx.store.begin_orders_snapshot();
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
    }
    fn account_auth_outcome(
        &self,
        _: &usize,
        response: &Response<'_>,
        now: f64,
        store: &safe_cex_engine::Store,
    ) -> Option<AuthOutcome> {
        if !self
            .token
            .as_ref()
            .is_some_and(|token| store.orders_snapshot_current(token))
            || self.valid_before.is_some_and(|deadline| now >= deadline)
        {
            return None;
        }
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
        cx.store.invalidate_order_snapshots();
        cx.notice(Level::Info, MARKER, format!("response:{count}"));
    }
    fn on_stream_error(&mut self, cx: &mut Cx<'_, usize>, count: usize, _: Response<'_>) {
        cx.store.invalidate_order_snapshots();
        cx.notice(Level::Warn, MARKER, format!("missing:{count}"));
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, usize>, _: usize, _: &str) {
        cx.notice(Level::Error, MARKER, "failed");
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

fn engine(streamed: bool, tagged: bool) -> Engine<AccountReads> {
    let mut engine = support::seeded_engine(AccountReads {
        streamed,
        tagged,
        valid_before: None,
        token: None,
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
        let mut engine = engine(false, true);
        assert_eq!(
            complete(&mut engine, status, body),
            vec![evidence(verdict), marker(Level::Info, "response:0")]
        );
        assert!(complete(&mut engine, status, body).is_empty());
    }
}

#[test]
fn streamed_rows_preserve_context_until_final_envelope() {
    let mut engine = engine(true, true);
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
            marker(Level::Info, "response:2")
        ]
    );
}

#[test]
fn missing_stream_list_can_reject_but_cannot_report_recovery() {
    for accepted in [false, true] {
        let mut engine = engine(true, true);
        let actions = complete(&mut engine, 200, &format!(r#"{{"accepted":{accepted}}}"#));
        let mut expected = Vec::new();
        if !accepted {
            expected.push(evidence(AuthOutcome::Rejected));
        }
        expected.push(marker(Level::Warn, "missing:0"));
        assert_eq!(actions, expected);
    }
}

#[test]
fn invalid_or_truncated_stream_never_reports_auth_even_with_reject_status() {
    for body in [br#"{"accepted":false,"rows":[{}"#.as_slice(), b"\xff"] {
        let mut engine = engine(true, true);
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
    let mut malformed = engine(false, true);
    assert!(
        complete(&mut malformed, 200, "not-json")
            .iter()
            .all(|a| !matches!(a, Action::Notice(Notice::AccountAuth { .. })))
    );
    for status in [200, 401] {
        let mut public = engine(false, false);
        assert!(
            complete(&mut public, status, r#"{"accepted":true}"#)
                .iter()
                .all(|a| !matches!(a, Action::Notice(Notice::AccountAuth { .. })))
        );
    }
}

#[test]
fn transport_and_sign_failure_drain_context_without_credential_evidence() {
    let mut engine = engine(false, true);
    engine.handle(
        1.,
        Event::HttpFailed {
            request: RequestId(1),
            error: "offline",
        },
    );
    assert_eq!(
        engine.drain_actions().collect::<Vec<_>>(),
        vec![marker(Level::Error, "failed")]
    );
    assert!(complete(&mut engine, 401, "").is_empty());
}

#[test]
fn invalid_utf8_buffer_cannot_report_http_status_auth() {
    let mut engine = engine(false, true);
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
        vec![marker(Level::Error, "failed")]
    );
}

#[test]
fn response_event_time_preserves_time_sensitive_classifier_admission() {
    for (now, accepted) in [(1.999, true), (2., false), (3., false)] {
        let mut engine = support::seeded_engine(AccountReads {
            streamed: false,
            tagged: true,
            valid_before: Some(2.),
            token: None,
        });
        engine.start(0.);
        engine.drain_actions().for_each(drop);
        engine.handle(
            now,
            Event::HttpResponse {
                request: RequestId(1),
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"accepted":true}"#,
                },
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(
                    action,
                    Action::Notice(Notice::AccountAuth {
                        outcome: AuthOutcome::Accepted,
                        ..
                    })
                ))
                .count(),
            usize::from(accepted)
        );
        assert!(
            matches!(actions.last(), Some(Action::Notice(Notice::Event(event))) if event.level == Level::Info && event.message == "response:0")
        );
    }
}

#[test]
fn token_stale_auth_is_suppressed_using_the_same_store_before_account_callbacks() {
    for streamed in [false, true] {
        let mut subject = engine(streamed, true);
        subject.store_mut().invalidate_order_snapshots();
        let actions = complete(&mut subject, 200, r#"{"accepted":true,"rows":[]}"#);
        assert_eq!(actions, vec![marker(Level::Info, "response:0")]);
        let mut subject = engine(streamed, true);
        subject.store_mut().invalidate_order_snapshots();
        let actions = complete(&mut subject, 200, r#"{"accepted":false}"#);
        assert_eq!(
            actions,
            vec![marker(
                if streamed { Level::Warn } else { Level::Info },
                if streamed { "missing:0" } else { "response:0" }
            )]
        );
    }
}
