mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::history::{BackgroundCoverage, Completion};
use safe_cex_engine::log::code;
use safe_cex_engine::{
    Action, Command, Cx, Event, Level, Method, Notice, Query, Response, SocketId, TimerId, Value,
    Venue,
};

struct Fixture {
    coverage: BackgroundCoverage,
}
impl Venue for Fixture {
    type Pending = Completion;
    fn start(&mut self, cx: &mut Cx<'_, Completion>) {
        cx.set_timer(1.0);
        cx.set_timer(2.0);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Completion>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Completion>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Completion>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, Completion>, _: TimerId) {
        self.coverage.report(cx, "native account coverage cause");
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Completion>, command: Command) {
        let Command::Query { command, .. } = command else {
            panic!("history query")
        };
        cx.request(
            Method::Get,
            "https://history.test".into(),
            vec![],
            None,
            Completion::new(command),
        );
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Completion>,
        completion: Completion,
        _: Response<'_>,
    ) {
        completion.finish(cx, Ok("[]".into()));
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Completion>,
        completion: Completion,
        message: &str,
    ) {
        completion.finish(cx, Err(message.into()));
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn background_scope_preserves_levels_and_repeat_delivery_without_completing_a_query() {
    for (coverage, level) in [
        (BackgroundCoverage::Incomplete, Level::Warn),
        (BackgroundCoverage::Exhausted, Level::Error),
    ] {
        let mut engine = support::seeded_engine(Fixture { coverage });
        engine.set_log_level(Level::Error);
        engine.start(0.0);
        let timers = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(timers.len(), 2);
        engine.handle(
            0.0,
            Event::Command(Command::Query {
                command: 41,
                query: Query::OrderHistory {
                    symbol: None,
                    from_ms: None,
                    to_ms: None,
                    limit: None,
                },
            }),
        );
        let requests = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        let health = engine.health().clone();
        for (timer, at) in timers {
            engine.handle(at, Event::TimerFired { timer });
            let actions = engine.drain_actions().collect::<Vec<_>>();
            assert_eq!(
                actions.len(),
                1,
                "one notice, no request or query completion"
            );
            let Action::Notice(Notice::Event(event)) = &actions[0] else {
                panic!("account event")
            };
            assert_eq!(event.code, code::HISTORY_INCOMPLETE);
            assert_eq!(event.level, level);
            assert_eq!(event.message, "native account coverage cause");
            assert_eq!(
                event.fields,
                vec![("scope", Value::Str("account/background".into()))]
            );
            assert_eq!(
                engine.health(),
                &health,
                "preserve existing health-counter policy"
            );
        }
        engine.handle(
            3.0,
            Event::HttpResponse {
                request: requests[0],
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[]",
                },
            },
        );
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(
            actions.len(),
            1,
            "background notice never marks the pending query"
        );
        assert!(
            matches!(&actions[0], Action::Notice(Notice::CommandDone { command: 41, result }) if result == "[]")
        );
    }
}
