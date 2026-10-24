//! Original real-entry controls for seven local refusals and a fixed Setting ack.
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Error, HttpAction, Preparation};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, Outcome, Response, VenueError,
};
use safe_cex_venue_cryptocom::{Cryptocom, Options};

type TestEngine = Engine<Cryptocom>;
const NOW: f64 = 1_710_000_000_000.;
const OWNER: u32 = 700;
const SYMBOL: &str = "BTCUSD-PERP";

#[derive(Clone, Copy, Debug)]
enum Case {
    MissingCancel,
    MissingAmend,
    InvalidAmend,
    InvalidLeverage,
    Margin,
    MissingKey,
    Plan,
    Mode,
}
impl Case {
    fn command(self, command: u32) -> Command {
        match self {
            Self::MissingCancel => Command::CancelOrders {
                command,
                ids: vec!["123".into(), "missing".into()],
            },
            Self::MissingAmend => Command::AmendOrder {
                command,
                id: "missing".into(),
                price: Some(0.),
                amount: Some(0.),
            },
            Self::InvalidAmend => Command::AmendOrder {
                command,
                id: "123".into(),
                price: Some(0.),
                amount: Some(1.),
            },
            Self::InvalidLeverage => Command::SetLeverage {
                command,
                symbol: SYMBOL.into(),
                leverage: 1.5,
                side: None,
            },
            Self::Margin => Command::SetMarginType {
                command,
                symbol: SYMBOL.into(),
                isolated: true,
            },
            Self::MissingKey => Command::PlaceOrders {
                command,
                orders: vec![placement()],
            },
            Self::Plan => {
                let mut order = placement();
                order.symbol = "UNKNOWN".into();
                Command::PlaceOrders {
                    command,
                    orders: vec![order],
                }
            }
            Self::Mode => Command::SetPositionMode {
                command,
                symbol: None,
                hedged: false,
            },
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::MissingCancel => {
                "Order missing not found; cancellation requires a reconciled order"
            }
            Self::MissingAmend => "Order not found: missing",
            Self::InvalidAmend => "A positive price and quantity are required",
            Self::InvalidLeverage => "Crypto.com account leverage must be an integer from 1 to 100",
            Self::Margin => {
                "Crypto.com does not support switching BTCUSD-PERP to isolated margin through Exchange v1"
            }
            Self::MissingKey => "Crypto.com credentials are required",
            Self::Plan => "Market not found: UNKNOWN",
            Self::Mode => "Position mode is always hedged on Crypto.com",
        }
    }
    fn private(self) -> bool {
        matches!(self, Self::Plan)
    }
}
const REFUSALS: [Case; 7] = [
    Case::MissingCancel,
    Case::MissingAmend,
    Case::InvalidAmend,
    Case::InvalidLeverage,
    Case::Margin,
    Case::MissingKey,
    Case::Plan,
];
fn placement() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.0021,"price":90.04,"clientOrderId":"caller-123"}"#).unwrap()
}
fn stored_order() -> Order {
    Order {
        id: "123".into(),
        parent_id: None,
        symbol: SYMBOL.into(),
        status: OrderStatus::Open,
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 90.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn engine(private: bool) -> TestEngine {
    let mut e = safe_cex_engine::test_support::seeded_engine(Cryptocom::new(Options {
        key: private.then(|| "local-fixture-identity".into()),
        secret: private.then(|| "local-fixture-material".into()),
        ..Default::default()
    }));
    e.store_mut().set_markets(vec![Market {
        id: SYMBOL.into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            price: 0.1,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 1000.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e.store_mut().set_orders(vec![stored_order()]);
    let _ = e.take_frame(NOW);
    e
}
fn execute(e: &mut TestEngine, now: f64, command: Command) -> Vec<Action> {
    e.handle(now, Event::Command(command));
    e.drain_actions().collect()
}
fn terminal(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn no_work(actions: &[Action]) {
    assert!(
        actions.iter().all(|a| matches!(a, Action::Notice(_))),
        "local terminal must have no HTTP, managed dispatch, signing, timer or progress work"
    );
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandProgress { .. } | Notice::AccountAuth { .. })
    )));
}
fn business(actions: &[Action], owner: u32, case: Case) {
    no_work(actions);
    let results = terminal(actions, owner);
    assert_eq!(results.len(), 1);
    if matches!(case, Case::Mode) {
        assert_eq!(results[0].as_ref().unwrap(), "[]");
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message == case.message())).count(), 1);
        assert_eq!(actions.len(), 2);
        return;
    }
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.message, case.message());
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(
        error.kind,
        match case {
            Case::Margin => Some(ErrorKind::Unsupported),
            Case::MissingKey => Some(ErrorKind::AuthInvalid),
            _ => None,
        }
    );
    let errors: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::CommandError {
                command,
                message: error,
            }) if *command == owner => Some((i, error)),
            _ => None,
        })
        .collect();
    if matches!(case, Case::Plan) {
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, case.message());
        let failed = actions
            .iter()
            .position(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
            .unwrap();
        assert!(errors[0].0 < failed);
        assert_eq!(actions.len(), 2);
    } else {
        assert!(errors.is_empty());
        assert_eq!(actions.len(), 1);
    }
}
fn entry_refusal(actions: &[Action], owner: u32, kind: ErrorKind) {
    assert_eq!(
        actions.len(),
        1,
        "shared entry refusal must be the only effect"
    );
    let results = terminal(actions, owner);
    assert_eq!(results.len(), 1);
    let error = results[0]
        .as_ref()
        .expect_err("entry denial cannot acknowledge native success");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_work(actions);
}
fn peer(e: &mut TestEngine, now: f64, owner: u32) -> Preparation {
    let a = execute(
        e,
        now,
        Command::FetchOrderBook {
            command: owner,
            symbol: SYMBOL.into(),
            limit: None,
        },
    );
    a.into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("genuine managed public book occupies an owner slot")
}
fn invalid_clock(case: Case) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(case.private());
        entry_refusal(
            &execute(&mut e, now, case.command(OWNER)),
            OWNER,
            ErrorKind::InvalidInput,
        );
        business(&execute(&mut e, NOW, case.command(OWNER)), OWNER, case);
        assert_eq!(e.store().order("123").unwrap().remaining, 1.);
    }
}
fn full_capacity(case: Case) {
    let mut e = engine(case.private());
    let peers: Vec<_> = (1..=256).map(|id| peer(&mut e, NOW, id)).collect();
    entry_refusal(
        &execute(&mut e, NOW, case.command(OWNER)),
        OWNER,
        ErrorKind::Internal,
    );
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    e.http_preparation_failed(
        NOW + 1.,
        peers[0],
        Error::State,
        VenueError::new("released peer"),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(terminal(&a, 1).len(), 1);
    assert!(terminal(&a, OWNER).is_empty());
    business(&execute(&mut e, NOW + 2., case.command(OWNER)), OWNER, case);
    assert!(peers[1..].iter().all(|p| e.http_preparation_pending(*p)));
    assert_eq!(e.store().order("123").unwrap().remaining, 1.);
}
#[test]
fn missing_cancel_requires_valid_clock() {
    invalid_clock(Case::MissingCancel);
}
#[test]
fn missing_amend_requires_valid_clock() {
    invalid_clock(Case::MissingAmend);
}
#[test]
fn invalid_amend_requires_valid_clock() {
    invalid_clock(Case::InvalidAmend);
}
#[test]
fn invalid_leverage_requires_valid_clock() {
    invalid_clock(Case::InvalidLeverage);
}
#[test]
fn unsupported_margin_requires_valid_clock() {
    invalid_clock(Case::Margin);
}
#[test]
fn missing_key_requires_valid_clock() {
    invalid_clock(Case::MissingKey);
}
#[test]
fn plan_refusal_requires_valid_clock() {
    invalid_clock(Case::Plan);
}
#[test]
fn fixed_mode_requires_valid_clock() {
    invalid_clock(Case::Mode);
}
#[test]
fn missing_cancel_requires_capacity() {
    full_capacity(Case::MissingCancel);
}
#[test]
fn missing_amend_requires_capacity() {
    full_capacity(Case::MissingAmend);
}
#[test]
fn invalid_amend_requires_capacity() {
    full_capacity(Case::InvalidAmend);
}
#[test]
fn invalid_leverage_requires_capacity() {
    full_capacity(Case::InvalidLeverage);
}
#[test]
fn unsupported_margin_requires_capacity() {
    full_capacity(Case::Margin);
}
#[test]
fn missing_key_requires_capacity() {
    full_capacity(Case::MissingKey);
}
#[test]
fn plan_refusal_requires_capacity() {
    full_capacity(Case::Plan);
}
#[test]
fn fixed_mode_requires_capacity() {
    full_capacity(Case::Mode);
}
#[test]
fn native_finite_refusals_preserve_messages_kinds_precedence_and_store() {
    for case in REFUSALS {
        let mut e = engine(case.private());
        business(&execute(&mut e, NOW, case.command(OWNER)), OWNER, case);
        assert_eq!(e.store().orders().len(), 1);
        assert_eq!(e.store().order("123").unwrap().remaining, 1.);
        assert!(!e.store().hedged());
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn native_missing_credentials_mask_pure_placement_plan_error() {
    let mut e = engine(false);
    business(
        &execute(&mut e, NOW, Case::Plan.command(OWNER)),
        OWNER,
        Case::MissingKey,
    );
}
#[test]
fn native_fixed_mode_is_keyless_and_never_rewrites_store() {
    for private in [false, true] {
        for hedged in [false, true] {
            for initial in [false, true] {
                let mut e = engine(private);
                e.store_mut().set_hedged(initial);
                let _ = e.take_frame(NOW);
                business(
                    &execute(
                        &mut e,
                        NOW,
                        Command::SetPositionMode {
                            command: OWNER,
                            symbol: Some("UNKNOWN".into()),
                            hedged,
                        },
                    ),
                    OWNER,
                    Case::Mode,
                );
                assert_eq!(e.store().hedged(), initial);
                assert!(e.take_frame(NOW).is_none());
            }
        }
    }
}

#[test]
fn fixed_mode_warning_is_emitted_only_after_admitted_completion() {
    let mut e = engine(false);
    let a = execute(&mut e, NOW, Case::Mode.command(OWNER));
    business(&a, OWNER, Case::Mode);
    let done = a
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
        .unwrap();
    let warning = a.iter().position(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.message == Case::Mode.message())).unwrap();
    assert!(
        done < warning,
        "shared fixed Setting success must precede its admission-dependent warning"
    );
}
#[test]
fn local_missing_key_is_not_remote_auth_health_evidence() {
    let mut e = engine(false);
    business(
        &execute(&mut e, NOW, Case::MissingKey.command(OWNER)),
        OWNER,
        Case::MissingKey,
    );
    assert_eq!(
        e.health().auth_failures,
        0,
        "no credential was carried or released"
    );
}
#[test]
fn duplicate_live_read_id_cannot_be_replaced_by_local_trading_terminals() {
    for case in REFUSALS.into_iter().chain([Case::Mode]) {
        let mut e = engine(case.private());
        let original = peer(&mut e, NOW, OWNER);
        let a = execute(&mut e, NOW + 1., case.command(OWNER));
        assert!(terminal(&a, OWNER).is_empty());
        assert!(e.http_preparation_pending(original));
        assert!(
            matches!(a.as_slice(), [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
        );
        e.http_preparation_failed(
            NOW + 2.,
            original,
            Error::State,
            VenueError::new("original read"),
        );
        let a: Vec<_> = e.drain_actions().collect();
        let result = terminal(&a, OWNER);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].as_ref().unwrap_err().message, "original read");
    }
}
#[test]
fn repeated_native_refusals_and_fixed_ack_leave_capacity_for_a_real_peer() {
    for case in REFUSALS.into_iter().chain([Case::Mode]) {
        let mut e = engine(case.private());
        for _ in 0..300 {
            business(&execute(&mut e, NOW, case.command(OWNER)), OWNER, case);
        }
        let p = peer(&mut e, NOW + 1., OWNER);
        assert!(e.http_preparation_pending(p));
    }
}
#[test]
fn native_valid_write_keeps_primary_bytes_and_progress_before_http() {
    let mut e = engine(true);
    let a = execute(
        &mut e,
        NOW,
        Command::PlaceOrders {
            command: OWNER,
            orders: vec![placement()],
        },
    );
    let physical = a.iter().position(|a| matches!(a, Action::Http(_))).unwrap();
    let progress = a
        .iter()
        .position(|a| matches!(a, Action::Notice(Notice::CommandProgress { .. })))
        .unwrap();
    assert!(progress < physical);
    assert!(terminal(&a, OWNER).is_empty());
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
    );
    let Action::Http(request) = &a[physical] else {
        unreachable!()
    };
    let body: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["method"], "private/create-order");
    assert_eq!(body["api_key"], "local-fixture-identity");
    assert_eq!(body["params"]["quantity"], "0.0021");
    assert_eq!(body["params"]["price"], "90.04");
    assert_eq!(body["params"]["client_oid"], "caller-123");
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"result":{"order_id":9007199254740993}}"#,
            },
        },
    );
}
#[test]
fn native_empty_batches_keep_the_separate_legacy_job_completion() {
    for command in [
        Command::PlaceOrders {
            command: OWNER,
            orders: vec![],
        },
        Command::CancelOrders {
            command: OWNER,
            ids: vec![],
        },
    ] {
        let mut e = engine(true);
        let a = execute(&mut e, NOW, command);
        no_work(&a);
        assert_eq!(terminal(&a, OWNER), [Ok("[]".into())]);
        assert_eq!(a.len(), 1);
    }
}
#[test]
fn host_boundary_invalid_timeout_is_undispatched_without_fabricated_receipt() {
    for wait in [0, 2_147_483_648] {
        let mut e = engine(false);
        assert!(
            e.handle_command(NOW, Case::Mode.command(OWNER), Some(wait))
                .is_err()
        );
        assert!(e.drain_actions().next().is_none());
        business(
            &execute(&mut e, NOW, Case::Mode.command(OWNER)),
            OWNER,
            Case::Mode,
        );
    }
}
