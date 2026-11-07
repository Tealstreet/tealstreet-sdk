use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Admission, Context, Error, Operation, Policy, ReconciliationData, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, HttpRequest, Response, SocketChannel, SocketSpec};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
struct Probe {
    socket: Option<SocketId>,
    deny: bool,
    releases: usize,
    failures: Vec<VenueError>,
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        self.socket = Some(
            cx.connect(SocketSpec::new("wss://venue.test/orders").channel(SocketChannel::Business)),
        );
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let Command::SetLeverage { command, .. } = command else {
            return;
        };
        let context = cx
            .http_context(
                Scope::Command(command),
                Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let write = cx
            .websocket_write(context.clone(), self.socket.unwrap(), "setting".into())
            .unwrap();
        cx.send_websocket_write(write, None, context).unwrap();
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("setting".into())
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::Mutable(|this, _, _| {
            this.releases += 1;
            Ok(if this.deny {
                Admission::Denied
            } else {
                Admission::Unscheduled
            })
        })
    }
    fn on_http_preparation_failed(
        &mut self,
        _: &mut Cx<'_, Context>,
        _: Context,
        _: Error,
        error: VenueError,
    ) {
        self.failures.push(error);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct Captured(Rc<RefCell<Option<mpsc::UnboundedReceiver<String>>>>);
impl Transport for Captured {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        let (sender, receiver) = mpsc::unbounded_channel();
        *self.0.borrow_mut() = Some(receiver);
        sender
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("socket write reached HTTP transport");
    }
}

#[test]
fn native_websocket_write_claims_at_send_and_refuses_denied_expired_or_missing_transport() {
    for refusal in 0..4 {
        let captured = Rc::new(RefCell::new(None));
        let engine = safe_cex_engine::test_support::seeded_engine(Probe {
            deny: refusal == 1,
            ..Probe::default()
        });
        let mut host = Host::new(engine, Captured(captured.clone()));
        host.clock = || 0.;
        host.engine.start(0.);
        host.execute(&mut |_| {});
        host.engine.handle(
            0.,
            Event::Command(Command::SetLeverage {
                command: 71,
                symbol: "BTC".into(),
                leverage: 2.,
                side: None,
            }),
        );
        let actions = host.engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(host.engine.venue().releases, 0);
        assert!(captured.borrow_mut().as_mut().unwrap().try_recv().is_err());
        if refusal == 2 {
            host.clock = || 2_000.;
        }
        if refusal == 3 {
            host.sockets.clear();
        }
        let binding = actions
            .into_iter()
            .find_map(safe_cex_engine::WebSocketWriteBinding::from_action)
            .expect("typed write binding");
        host.execute_websocket_write(binding);
        let sent = captured.borrow_mut().as_mut().unwrap().try_recv().ok();
        assert_eq!(sent.as_deref(), (refusal == 0).then_some("setting"));
        assert_eq!(host.engine.venue().releases, usize::from(refusal <= 1));
        if refusal != 0 {
            assert_eq!(host.engine.venue().failures.len(), 1);
            assert_eq!(
                host.engine.venue().failures[0].outcome,
                Some(safe_cex_engine::Outcome::NotSent)
            );
        }
    }
}
