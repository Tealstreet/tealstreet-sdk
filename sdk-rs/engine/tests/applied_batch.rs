//! Non-creating native batch acknowledgements must not invent created-order IDs.
#![cfg(feature = "test-support")]
use safe_cex_engine::book::Book;
use safe_cex_engine::classify::{BatchItem, CodeTable, Key, Native, NativeCode, Row, Verdict};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, WriteResult};
use safe_cex_engine::test_support::{prepared_actions, seeded_engine};
use safe_cex_engine::{
    Action, Command, Cx, ErrorKind, Event, Method, Notice, Outcome, Response, SocketId, TimerId,
    Venue, VenueError,
};

const TABLE: CodeTable = CodeTable {
    rows: &[Row {
        key: Key::Int(-1),
        verdict: Verdict::Refused(ErrorKind::Rejected),
        only: None,
    }],
    success: &[],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

struct Probe {
    applied: bool,
    result: Option<WriteResult>,
    guarded: bool,
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let context = cx
            .admit_http_command(Policy {
                operation: if matches!(command, Command::FetchCandles { .. }) {
                    Operation::Read
                } else {
                    Operation::Mutation
                },
                credential: None,
                timeout_ms: 1000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        if matches!(
            command,
            Command::PlaceOrders { .. } | Command::FetchCandles { .. }
        ) {
            assert_eq!(
                cx.http_write_applied_batch(
                    &context,
                    &TABLE,
                    1,
                    &[BatchItem::Accepted("100")],
                    "invalid intent",
                    None
                ),
                Err(safe_cex_engine::request_context::Error::Invalid),
            );
            self.guarded = true;
            return;
        }
        cx.request_managed(
            context.clone(),
            None,
            Method::Delete,
            "https://venue.test/cancel-batch".into(),
            None,
            context,
        )
        .unwrap();
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
        let unknown = BatchItem::Native(Native {
            code: NativeCode::Int(-2),
            message: "unknown row",
        });
        let refused = BatchItem::Native(Native {
            code: NativeCode::Int(-1),
            message: "refused row",
        });
        let items = match response.body {
            "accepted" => vec![BatchItem::Accepted("100"), BatchItem::Accepted("101")],
            "partial" => vec![BatchItem::Accepted("100"), refused],
            "unknown" => vec![BatchItem::Accepted("100"), unknown],
            "rejected" => vec![refused, refused],
            "malformed" => vec![BatchItem::Accepted("100")],
            _ => unreachable!(),
        };
        let result = if self.applied {
            cx.http_write_applied_batch(&context, &TABLE, 2, &items, "cancel batch stopped", None)
        } else {
            cx.http_write_batch(&context, &TABLE, 2, &items, "cancel batch stopped", None)
        }
        .unwrap();
        self.result = Some(result.clone());
        match result {
            WriteResult::Accepted { .. } => cx.done_context(&context, "[]".into()).unwrap(),
            WriteResult::Rejected(error)
            | WriteResult::KnownPartial(error)
            | WriteResult::PartialUnknown(error)
            | WriteResult::Failed(error) => {
                cx.fail_context(&context, error).unwrap();
            }
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: &str) {
        panic!("{error}");
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        safe_cex_engine::request_context::ReconciliationData::new("batch-fixture".into())
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn run(applied: bool, body: &str) -> (WriteResult, Vec<VenueError>) {
    let mut engine = seeded_engine(Probe {
        applied,
        result: None,
        guarded: false,
    });
    engine.start(0.);
    engine.handle(
        1.,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["100".into(), "101".into()],
        }),
    );
    let requests: Vec<_> = prepared_actions(&mut engine, 1.)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    engine.handle(
        2.,
        Event::HttpResponse {
            request: requests[0].id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    let failures = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect();
    (engine.venue().result.clone().unwrap(), failures)
}

fn clear_created_ids(result: &mut WriteResult) {
    match result {
        WriteResult::Accepted { order_ids } => order_ids.clear(),
        WriteResult::Rejected(error)
        | WriteResult::KnownPartial(error)
        | WriteResult::PartialUnknown(error)
        | WriteResult::Failed(error) => {
            if let Some(partial) = &mut error.partial {
                partial.order_ids.clear();
            }
        }
    }
}

#[test]
fn cancel_batch_unknown_keeps_unconfirmed_targets_out_of_created_ids() {
    let (result, failures) = run(true, "unknown");
    let WriteResult::PartialUnknown(error) = result else {
        panic!("{result:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(error.partial.unwrap().order_ids.is_empty());
    assert_eq!(failures.len(), 1);
    assert!(failures[0].partial.as_ref().unwrap().order_ids.is_empty());
}

#[test]
fn applied_batches_preserve_classifier_detail_except_created_ids() {
    for body in ["accepted", "partial", "unknown", "rejected", "malformed"] {
        let (mut ordinary, _) = run(false, body);
        clear_created_ids(&mut ordinary);
        assert_eq!(run(true, body).0, ordinary, "{body}");
    }
}

#[test]
fn applied_batches_refuse_placement_and_read_intents_before_recording() {
    for command in [
        Command::PlaceOrders {
            command: 7,
            orders: vec![],
        },
        Command::FetchCandles {
            command: 7,
            symbol: "BTCUSD".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        },
    ] {
        let mut engine = seeded_engine(Probe {
            applied: true,
            result: None,
            guarded: false,
        });
        engine.start(0.);
        engine.handle(1., Event::Command(command));
        assert!(engine.venue().guarded);
        assert!(engine.venue().result.is_none());
        assert!(
            !prepared_actions(&mut engine, 1.)
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}
