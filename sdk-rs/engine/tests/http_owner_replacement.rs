//! Host callbacks from a retired engine must not acquire its replacement's HTTP work.
#![cfg(feature = "test-support")]

use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Permit, Policy, Preparation, Prepared,
    ReconciliationData, RetryPolicy,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, Response, SocketId, TimerId, Venue, VenueError,
};

struct Account<const OWNER: u8>;
impl<const OWNER: u8> Venue for Account<OWNER> {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        cx.store.set_markets(vec![Market {
            id: "BTCUSD".into(),
            symbol: "BTCUSD".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USD".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 100.,
                },
                leverage: MinMax { min: 1., max: 100. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("managed failure requires the typed callback")
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        context: Context,
        _: Error,
        error: VenueError,
    ) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("account".into())
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let context = cx
            .admit_http_command(Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 1000,
                pacing: None,
                retry: RetryPolicy::Never,
            })
            .unwrap();
        cx.request_managed(
            context.clone(),
            None,
            Method::Post,
            "https://venue.test/order".into(),
            Some(format!("owner-{}", char::from(OWNER))),
            context,
        )
        .unwrap();
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn pending<const OWNER: u8>(frame_fed: bool) -> (Engine<Account<OWNER>>, Preparation) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Account::<OWNER>);
    if frame_fed {
        engine.set_frame_fed().unwrap();
    }
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        1.,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![],
        }),
    );
    let tokens: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(tokens.len(), 1, "same-owner preparation control");
    (engine, tokens[0])
}

fn bind<const OWNER: u8>(engine: &mut Engine<Account<OWNER>>, token: Preparation) -> Permit {
    engine.http_prepared(
        2.,
        token,
        Prepared {
            id: 77,
            expires_at: None,
        },
    );
    let permits: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert_eq!(request.method, Method::Post);
                    assert_eq!(request.url, "https://venue.test/order");
                    assert_eq!(request.body, Some(format!("owner-{}", char::from(OWNER))));
                    Some(permit)
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(permits.len(), 1, "same-owner binding control");
    permits[0]
}

fn retired_tokens(frame_fed: bool, released: bool) -> (String, String) {
    let (mut a, preparation) = pending::<b'A'>(frame_fed);
    let permit = bind(&mut a, preparation);
    if released {
        assert!(matches!(a.claim_http(3., permit), Ok(Claim::Released(_))));
    }
    // Retain only serialized host work, not an Arc that could keep A's issuer alive.
    (preparation.to_wire(), permit.to_wire())
}

fn retired_preparation(frame_fed: bool) -> String {
    let (a, preparation) = pending::<b'A'>(frame_fed);
    let wire = preparation.to_wire();
    drop(a);
    wire
}

#[test]
fn retired_preparation_cannot_bind_replacement_owner() {
    for frame_fed in [false, true] {
        let old_preparation = retired_preparation(frame_fed);
        let old = Preparation::from_wire(&old_preparation).unwrap();
        let (mut b, current) = pending::<b'B'>(frame_fed);
        assert_eq!(
            old.attempt(),
            current.attempt(),
            "local counters deliberately collide"
        );
        assert!(
            !b.http_preparation_pending(old),
            "retired A preparation must not identify B"
        );
        b.http_prepared(
            2000.,
            old,
            Prepared {
                id: 77,
                expires_at: None,
            },
        );
        assert!(
            b.drain_actions().next().is_none(),
            "drop stale completion without binding B"
        );
        let permit = bind(&mut b, current);
        assert!(matches!(b.claim_http(3., permit), Ok(Claim::Released(_))));
    }
}

#[test]
fn retired_permit_cannot_release_replacement_owner() {
    for (frame_fed, released) in [(false, false), (true, false), (false, true), (true, true)] {
        let (_, old_permit) = retired_tokens(frame_fed, released);
        let old = Permit::from_wire(&old_permit).unwrap();
        let (mut b, preparation) = pending::<b'B'>(frame_fed);
        let current = bind(&mut b, preparation);
        assert_eq!(
            old.attempt(),
            current.attempt(),
            "local counters deliberately collide"
        );
        assert_eq!(old.prepared_id(), current.prepared_id());
        assert_eq!(b.claim_http(2000., old), Err(Error::Revoked));
        assert!(
            b.drain_actions().next().is_none(),
            "foreign claim must not settle B"
        );
        assert!(matches!(b.claim_http(3., current), Ok(Claim::Released(_))));
    }
}

#[test]
fn retired_preparation_failure_cannot_fail_replacement_owner() {
    for frame_fed in [false, true] {
        let old_preparation = retired_preparation(frame_fed);
        let old = Preparation::from_wire(&old_preparation).unwrap();
        let (mut b, current) = pending::<b'B'>(frame_fed);
        b.http_preparation_failed(
            2000.,
            old,
            Error::Invalid,
            VenueError::new("retired A route"),
        );
        assert!(
            b.drain_actions().next().is_none(),
            "foreign failure must not settle B"
        );
        assert!(b.http_preparation_pending(current));
        let permit = bind(&mut b, current);
        assert!(matches!(b.claim_http(3., permit), Ok(Claim::Released(_))));
    }
}
