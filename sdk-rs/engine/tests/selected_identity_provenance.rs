//! Existing GREEN controls for G2c; new snapshot APIs are not implemented here.
#![cfg(feature = "test-support")]
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::signing::{SignFuture, SignRequest, Signer};
use safe_cex_engine::{Command, Cx, Event, Response, SocketId, TimerId, Venue};
use std::sync::Arc;
struct Provider;
impl Signer for Provider {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
}
#[derive(Default)]
struct Fixture {
    context: Option<Context>,
    foreign: Option<Context>,
    timer: Option<TimerId>,
    checked: bool,
}
impl Venue for Fixture {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.context = Some(
            cx.http_context(
                Scope::Background,
                Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap(),
        );
        self.timer = Some(cx.set_timer(1.));
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: TimerId) {
        assert_eq!(
            cx.validate_http_context(self.context.as_ref().unwrap()),
            Ok(())
        );
        assert_eq!(
            cx.validate_http_context(self.foreign.as_ref().unwrap()),
            Err(Error::Revoked)
        );
        self.checked = true;
    }
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn existing_signer_setter_refuses_reconfiguration_after_startup() {
    let original: safe_cex_engine::signing::Provider = Arc::new(Provider);
    let replacement: safe_cex_engine::signing::Provider = Arc::new(Provider);
    let mut engine = support::seeded_engine(Fixture::default());
    engine.set_signer(Some(original.clone())).unwrap();
    engine.start(0.);
    let error = engine.set_signer(Some(replacement)).unwrap_err();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert_eq!(error.message, "Signer must be configured before startup");
    assert!(Arc::ptr_eq(&engine.signer().unwrap(), &original));
    assert!(engine.set_signer(None).is_err());
    assert!(Arc::ptr_eq(&engine.signer().unwrap(), &original));
}
#[test]
fn existing_context_issuer_refuses_foreign_engine_with_identical_numeric_handles() {
    let mut old = support::seeded_engine(Fixture::default());
    old.start(0.);
    let foreign = old.venue().context.clone().unwrap();
    let mut current = support::seeded_engine(Fixture {
        foreign: Some(foreign.clone()),
        ..Default::default()
    });
    current.start(0.);
    let local = current.venue().context.as_ref().unwrap();
    assert_eq!(local.owner(), foreign.owner());
    assert_eq!(local.leg(), foreign.leg());
    assert_eq!(local.deadline(), foreign.deadline());
    let timer = current.venue().timer.unwrap();
    current.handle(1., Event::TimerFired { timer });
    assert!(current.venue().checked);
}
