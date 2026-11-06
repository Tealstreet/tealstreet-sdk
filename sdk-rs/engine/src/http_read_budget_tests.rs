//! Class declarations and structural account evidence, with opaque request bytes.
use super::*;
use crate::request_budget::RequestClass;
use crate::request_context::{Policy, RetryPolicy, Scope};

struct Probe {
    live: bool,
    loaded: bool,
    priority: HttpReadClass,
    evidence: bool,
    failures: Vec<Error>,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if self.loaded {
            cx.store.set_loaded(|loaded| {
                loaded.markets = true;
                loaded.tickers = true;
            });
        }
        let policy = Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let context = if self.live {
            cx.http_live_context(Scope::Background, policy)
        } else {
            cx.http_context(Scope::Background, policy)
        }
        .unwrap();
        let request = cx
            .request_managed(
                context,
                None,
                Method::Post,
                "https://opaque.invalid/new-route".into(),
                Some("opaque bytes".into()),
                (),
            )
            .unwrap();
        if self.evidence {
            cx.tag_account_read(request, CredentialSlot::Primary, AccountReadScope::Orders);
        }
    }
    fn http_read_class(&self, _: &()) -> HttpReadClass {
        self.priority
    }
    fn on_http_preparation_failed(
        &mut self,
        _: &mut Cx<'_, ()>,
        _: (),
        reason: Error,
        _: VenueError,
    ) {
        self.failures.push(reason);
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        panic!("{error}");
    }
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn engine(live: bool, priority: HttpReadClass, evidence: bool, loaded: bool) -> Engine<Probe> {
    let mut e = crate::test_support::seeded_engine(Probe {
        live,
        loaded,
        priority,
        evidence,
        failures: vec![],
    });
    e.start(100.);
    e
}
fn preparation(actions: &[Action]) -> (Preparation, &HttpRequest) {
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("one live preparation: {actions:?}"))
}
#[test]
fn live_reads_keep_startup_discovery_and_only_shorten_after_loading() {
    for priority in [HttpReadClass::Normal, HttpReadClass::Bulk] {
        for (loaded, catalogue) in [(false, 90_000), (true, 60_000)] {
            for (live, budget, class) in [
                (false, catalogue, RequestClass::Discovery),
                (
                    true,
                    if loaded { 20_000 } else { 90_000 },
                    if loaded {
                        RequestClass::Read
                    } else {
                        RequestClass::Discovery
                    },
                ),
            ] {
                let mut e = engine(live, priority, false, loaded);
                let actions: Vec<_> = e.drain_actions().collect();
                let (token, request) = preparation(&actions);
                assert_eq!(request.timeout_ms, Some(budget));
                assert_eq!(e.http_request_class(token), Some(class));
                assert_eq!(actions.iter().filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 100. + f64::from(budget))).count(), 1);
            }
        }
    }
}
#[test]
fn account_priority_and_account_scope_override_undeclared_catalogue_metadata() {
    for (priority, evidence) in [
        (HttpReadClass::Critical, false),
        (HttpReadClass::Reconciliation, false),
        (HttpReadClass::Normal, true),
    ] {
        let mut e = engine(false, priority, evidence, false);
        let actions: Vec<_> = e.drain_actions().collect();
        let (token, request) = preparation(&actions);
        assert_eq!(request.timeout_ms, Some(20_000));
        assert_eq!(e.http_request_class(token), Some(RequestClass::Read));
        assert_eq!(
            e.http_read_class(request.id),
            priority,
            "admission priority is unchanged"
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 20_100.))
                .count(),
            1
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == 90_100.))
        );
        assert_eq!(
            e.io.core.managed_http[&request.id]
                .lifecycle
                .context()
                .deadline(),
            90_100.,
            "retained authority is unchanged"
        );
    }
}
#[test]
fn account_repreparation_and_delayed_drain_keep_the_original_issuance_cap() {
    let mut e = engine(false, HttpReadClass::Reconciliation, false, true);
    e.prepare_dispatch(15_100.);
    let actions: Vec<_> = e.drain_actions().collect();
    let (token, request) = preparation(&actions);
    assert_eq!(request.timeout_ms, Some(5_000));
    e.http_prepared(
        15_100.,
        token,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    e.drain_actions().for_each(drop);
    e.prepare_dispatch(16_100.);
    e.reprepare_http(16_100., token);
    let actions: Vec<_> = e.drain_actions().collect();
    let (renewed, request) = preparation(&actions);
    assert_ne!(renewed, token);
    assert_eq!(request.timeout_ms, Some(4_000));
    assert_eq!(
        e.io.core.managed_http[&request.id].lifecycle.deadline(),
        20_100.
    );
}
#[test]
fn an_account_read_expired_before_drain_never_reaches_the_host() {
    let mut e = engine(false, HttpReadClass::Critical, false, false);
    e.prepare_dispatch(20_100.);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
    assert_eq!(e.venue.failures, [Error::TimedOut]);
    assert!(e.io.core.managed_http.is_empty());
}

#[test]
fn a_live_read_issued_during_startup_keeps_its_original_ceiling_after_loading() {
    let mut e = engine(true, HttpReadClass::Normal, false, false);
    e.store_mut().set_loaded(|loaded| {
        loaded.markets = true;
        loaded.tickers = true;
    });
    e.prepare_dispatch(15_100.);
    let actions: Vec<_> = e.drain_actions().collect();
    let (token, request) = preparation(&actions);
    assert_eq!(request.timeout_ms, Some(75_000));
    assert_eq!(e.http_request_class(token), Some(RequestClass::Discovery));
    assert_eq!(
        e.io.core.managed_http[&request.id].lifecycle.deadline(),
        90_100.
    );
    e.http_prepared(
        15_100.,
        token,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    e.drain_actions().for_each(drop);
    e.prepare_dispatch(21_100.);
    e.reprepare_http(21_100., token);
    let actions: Vec<_> = e.drain_actions().collect();
    let (renewed, request) = preparation(&actions);
    assert_ne!(renewed, token);
    assert_eq!(request.timeout_ms, Some(69_000));
    assert_eq!(e.http_request_class(renewed), Some(RequestClass::Discovery));
    assert_eq!(
        e.io.core.managed_http[&request.id].lifecycle.deadline(),
        90_100.
    );
}
