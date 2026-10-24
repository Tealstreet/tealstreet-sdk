use super::*;
use safe_cex_engine::{Action, Command, Event, Notice, SocketId, Venue};

// Revoke trusted setup between an admitted page and preparing its next
// endpoint. This exercises the real native refusal seam, without a public
// credential mutator or changing production construction.
struct Revoke(WeexFutures);
impl Venue for Revoke {
    type Pending = Pending;
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.0.on_command(cx, command);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.0.credentials = Default::default();
        self.0.slots = Default::default();
        self.0.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.0.on_request_failed(cx, pending, error);
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn preparation_refusal_after_accepted_orders_is_strict_typed_not_sent() {
    let mut engine = crate::setup::with_venue(
        crate::setup::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            ..Default::default()
        },
        Revoke,
    );
    let now = 1_764_505_700_000.0;
    engine.handle(
        now,
        Event::Command(Command::Query {
            command: 7,
            query: Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    engine.handle(now,Event::HttpResponse{request,response:Response{status:200,headers:"",body:r#"[{"orderId":702345678901234567,"symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","status":"FILLED","type":"LIMIT","time":1764400000000,"price":"65000","origQty":"0.01","executedQty":"0.01","avgPrice":"64999.5","updateTime":1764400001234}]"#}});
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::SetTimer { .. } | Action::Notice(Notice::CommandDone { .. })
    )));
    assert!(
        !actions.iter().any(
            |a| matches!(a,Action::Notice(Notice::Event(v)) if v.code==code::HISTORY_INCOMPLETE)
        )
    );
}
