use super::*;
use safe_cex_engine::{Action, Command, Event, Notice};

#[path = "managed.rs"]
mod managed;

#[test]
fn preparation_refusal_after_accepted_orders_is_strict_typed_not_sent() {
    let mut engine = crate::setup::engine_with_provider(
        crate::setup::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            ..Default::default()
        },
        |inner| std::sync::Arc::new(crate::setup::Gated::new(inner, &[true, false])),
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
    let request = managed::drain(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    engine.handle(now,Event::HttpResponse{request,response:Response{status:200,headers:"",body:r#"[{"orderId":702345678901234567,"symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","status":"FILLED","type":"LIMIT","time":1764400000000,"price":"65000","origQty":"0.01","executedQty":"0.01","avgPrice":"64999.5","updateTime":1764400001234}]"#}});
    let actions = managed::drain(&mut engine, now);
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0],
        &safe_cex_engine::signing::SignError::Refused.venue_error()
    );
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Notice(
                Notice::CommandDone { .. } | Notice::Deactivate | Notice::AccountAuth { .. }
            )
    )));
    for action in &actions {
        if let Action::SetTimer { timer, .. } = action {
            assert!(
                actions.iter().any(|action| matches!(action,
                    Action::ClearTimer { timer: cleared } if cleared == timer
                )),
                "provider refusal leaves no live managed timer"
            );
        }
    }
    assert!(
        !actions.iter().any(
            |a| matches!(a,Action::Notice(Notice::Event(v)) if v.code==code::HISTORY_INCOMPLETE)
        )
    );
}

struct RevokedReadProbe {
    native: WeexFutures,
    delay: Option<TimerId>,
    retained_before_refusal: bool,
}
impl safe_cex_engine::Venue for RevokedReadProbe {
    type Pending = Pending;
    fn start(&mut self, _: &mut Cx<'_, Pending>) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {
        panic!("fixture stays parked");
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        panic!("fixture sends nothing");
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.native.history_timer(cx, timer);
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::Query { command, query } => {
                self.native.paused_until = cx.now + 1000.;
                self.native.history_query(cx, command, query);
            }
            Command::SetTickerSymbols { .. } => {
                let read = self.native.history.reads.get(&7).unwrap();
                let root = read.root().context().clone();
                self.delay = match read {
                    Read::List { delay, .. } | Read::Income { delay, .. } => *delay,
                };
                cx.fail_context(&root, "fixture non-expiry revocation")
                    .unwrap();
                self.retained_before_refusal = self.native.history.reads.contains_key(&7);
                self.native.history_admission_failed(cx, 7, Error::Revoked);
            }
            _ => panic!("fixture command"),
        }
    }
}

#[test]
fn non_expiry_revocation_drops_parked_native_read_and_clears_its_delay() {
    let mut engine = crate::setup::with_venue(
        crate::setup::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..Default::default()
        },
        |native| RevokedReadProbe {
            native,
            delay: None,
            retained_before_refusal: false,
        },
    );
    engine.handle(
        0.,
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
    let parked: Vec<_> = engine.drain_actions().collect();
    assert!(
        parked
            .iter()
            .all(|action| matches!(action, Action::SetTimer { .. }))
    );
    assert_eq!(engine.venue().native.history.reads.len(), 1);
    engine.handle(
        1.,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    let retired: Vec<_> = engine.drain_actions().collect();
    assert!(engine.venue().retained_before_refusal);
    assert_eq!(retired.iter().filter(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: 7, error }) if error.message == "fixture non-expiry revocation"
    )).count(), 1);
    assert!(engine.venue().native.history.reads.is_empty());
    let delay = engine.venue().delay.unwrap();
    assert!(
        retired
            .iter()
            .any(|action| matches!(action, Action::ClearTimer { timer } if *timer == delay))
    );
    engine.handle(1000., Event::TimerFired { timer: delay });
    assert!(engine.drain_actions().next().is_none());
}
