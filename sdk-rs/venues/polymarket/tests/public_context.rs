use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, Notice, Response};
use safe_cex_venue_polymarket::{Options, Polymarket};

fn engine_with_market() -> Engine<Polymarket> {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
    let mut e = safe_cex_engine::test_support::seeded_engine(Polymarket::new(Options::default()));
    e.store_mut().set_markets(vec![Market {
        id: "107505882767731489358349912513945399560393482969656700824895970500493757150417".into(),
        symbol: "election-Yes".into(),
        pretty_symbol: None,
        base: "Yes".into(),
        quote: "pUSD".into(),
        active: true,
        precision: Precision {
            amount: 2.0,
            price: 2.0,
        },
        limits: Limits {
            amount: MinMax { min: 5.0, max: 1e6 },
            leverage: MinMax { min: 1.0, max: 1.0 },
            price: None,
        },
        contract_size: Some(1.0),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}

fn book() -> Command {
    Command::FetchOrderBook {
        command: 9,
        symbol: "election-Yes".into(),
        limit: None,
    }
}
fn candles() -> Command {
    Command::FetchCandles {
        command: 9,
        symbol: "election-Yes".into(),
        interval: Interval::M1,
        limit: None,
        from_ms: None,
        to_ms: None,
    }
}
fn actions(e: &mut Engine<Polymarket>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn terminal(a: &Action) -> bool {
    matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )
}
fn request(e: &mut Engine<Polymarket>, now: f64) -> safe_cex_engine::HttpRequest {
    actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn answer(
    e: &mut Engine<Polymarket>,
    now: f64,
    id: safe_cex_engine::RequestId,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    actions(e, now)
}
fn assert_timeout(a: &[Action]) {
    let failures: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 9, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "exactly one caller timeout: {a:?}");
    assert_eq!(failures[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(failures[0].outcome, None);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
    );
}
#[test]
fn same_id_cached_read_cannot_steal_a_pending_book_owner() {
    let mut e = engine_with_market();
    e.handle(0., Event::Command(book()));
    let r = request(&mut e, 0.);
    e.handle(1., Event::Command(Command::FetchAccount { command: 9 }));
    let a = actions(&mut e, 1.);
    assert!(
        !a.iter().any(terminal),
        "duplicate entry cannot complete pending book: {a:?}"
    );
    let body = format!(
        r#"{{"asset_id":"{}","bids":[],"asks":[]}}"#,
        e.store().markets()[0].id
    );
    let a = answer(&mut e, 2., r.id, &body);
    assert_eq!(a.iter().filter(|a| terminal(a)).count(), 1);
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:9,result}) if result==r#"{"bids":[],"asks":[]}"#)));
    assert!(!answer(&mut e, 3., r.id, &body).iter().any(terminal));
}
#[test]
fn book_cannot_succeed_after_caller_wait_without_timer_delivery() {
    let mut e = engine_with_market();
    e.handle_command(0., book(), Some(50)).unwrap();
    let r = request(&mut e, 0.);
    let body = format!(
        r#"{{"asset_id":"{}","bids":[],"asks":[]}}"#,
        e.store().markets()[0].id
    );
    assert_timeout(&answer(&mut e, 50., r.id, &body));
    assert!(!answer(&mut e, 51., r.id, &body).iter().any(terminal));
}
#[test]
fn candles_cannot_succeed_after_caller_wait_without_timer_delivery() {
    let mut e = engine_with_market();
    e.handle_command(0., candles(), Some(50)).unwrap();
    let r = request(&mut e, 0.);
    assert_timeout(&answer(
        &mut e,
        50.,
        r.id,
        r#"{"history":[{"t":100,"p":0.3}]}"#,
    ));
}
#[test]
fn a_host_that_never_prepares_still_gets_one_caller_timeout() {
    let mut e = engine_with_market();
    e.handle_command(0., book(), Some(50)).unwrap();
    let held: Vec<_> = e.drain_actions().collect();
    e.prepare_dispatch(50.);
    let a: Vec<_> = e.drain_actions().collect();
    assert_timeout(&a);
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(held.iter().all(|a| !terminal(a)));
}
#[test]
fn expired_background_candles_cannot_publish_but_live_rows_do() {
    for (delay, expected) in [(1., true), (20_000., false)] {
        let mut e = engine_with_market();
        let now = 100_000.;
        e.handle(
            now,
            Event::Command(Command::SubscribeCandles {
                symbol: "election-Yes".into(),
                interval: Interval::M1,
            }),
        );
        let r = request(&mut e, now);
        let a = answer(
            &mut e,
            now + delay,
            r.id,
            r#"{"history":[{"t":99,"p":0.3}]}"#,
        );
        let published = e.take_frame(now + delay).is_some_and(|frame| {
            safe_cex_engine::wire::sections(frame)
                .any(|(kind, _)| kind == safe_cex_engine::wire::section::CANDLES)
        });
        assert_eq!(published, expected, "delay {delay}: {a:?}");
        assert!(!a.iter().any(terminal));
    }
}
#[test]
fn unsupported_mutation_is_typed_not_sent_without_a_credential() {
    let mut e = engine_with_market();
    e.handle(
        0.,
        Event::Command(Command::SetStpMode {
            command: 9,
            mode: safe_cex_engine::model::StpMode::ExpireBoth,
        }),
    );
    let a = actions(&mut e, 0.);
    assert!(
        matches!(&a[..],[Action::Notice(Notice::CommandFailed{error,..})]
        if error.outcome==Some(safe_cex_engine::Outcome::NotSent) && error.message=="Polymarket self-trade prevention settings are unsupported")
    );
}
#[test]
fn local_account_remains_network_free_and_fixed_settings_keep_native_success() {
    let mut e = engine_with_market();
    e.handle(0., Event::Command(Command::FetchAccount { command: 9 }));
    let a = actions(&mut e, 0.);
    assert!(
        matches!(&a[..],[Action::Notice(Notice::CommandDone{result,..})] if result==r#"{"userId":"unknown","affiliateId":null}"#)
    );
    e.handle(
        1.,
        Event::Command(Command::SetLeverage {
            command: 9,
            symbol: "election-Yes".into(),
            leverage: 2.,
            side: None,
        }),
    );
    let a = actions(&mut e, 1.);
    assert!(matches!(&a[..],[Action::Notice(Notice::CommandDone{result,..})] if result=="[]"));
}

#[test]
fn released_attempt_deadline_preserves_typed_read_failure() {
    let mut e = engine_with_market();
    e.handle_command(0., book(), Some(60_000)).unwrap();
    let r = request(&mut e, 0.);
    let a = answer(&mut e, 20_000., r.id, "{}");
    assert_timeout(&a);
}

#[test]
fn a_stalled_prepare_uses_the_shorter_attempt_timer() {
    let mut e = engine_with_market();
    e.handle_command(0., book(), Some(60_000)).unwrap();
    let a: Vec<_> = e.drain_actions().collect();
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_000. => Some(*timer),
            _ => None,
        })
        .expect("managed preparation has the declared attempt timer");
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    e.handle(20_000., Event::TimerFired { timer });
    let a = actions(&mut e, 20_000.);
    let errors: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 9, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn unstamped_local_read_has_a_typed_refusal_without_io() {
    let mut e = engine_with_market();
    e.handle(
        f64::NAN,
        Event::Command(Command::FetchAccount { command: 9 }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(&a[..], [Action::Notice(Notice::CommandFailed { error, .. })]
        if error.kind == Some(ErrorKind::InvalidInput) && error.outcome.is_none())
    );
}
