//! The DXtrade password login runs as an engine session write on the primary
//! private egress. No credentials or live requests are used.
mod support;
use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
use safe_cex_engine::{Action, Method};
use safe_cex_venue_breakoutprop::{BreakoutProp, Options};

#[test]
fn login_is_prepared_on_the_primary_private_egress_with_its_bound() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(BreakoutProp::new(Options {
        key: "example-user".into(),
        secret: "example-password".into(),
        public_data_only: true,
        timeout_ms: 7_000,
        ..Options::default()
    }));
    engine.start(0.);
    engine.prepare_dispatch(0.);
    let login = engine.drain_actions().find_map(|action| match action {
        Action::ManagedHttp(action) => match *action {
            HttpAction::Prepare {
                request, intent, ..
            } if request.url.ends_with("/login") => Some((request, intent)),
            _ => None,
        },
        _ => None,
    });
    let Some((request, intent)) = login else {
        panic!("the login must be prepared as managed session work");
    };
    assert_eq!(
        intent,
        RoutingIntent::Primary,
        "the body carries the password; public/CORS egress would leak it"
    );
    assert_eq!(request.route, None);
    assert_eq!(request.timeout_ms, Some(7_000), "the configured bound");
    assert_eq!(request.method, Method::Post);
    assert_eq!(
        request.url,
        "https://breakout.prosp.devexperts.com/dxsca-web/login"
    );
    assert_eq!(
        request.body.as_deref(),
        Some(r#"{"domain":"default","username":"example-user","password":"example-password"}"#)
    );
    assert_eq!(
        request.headers,
        [("Content-Type", "application/json".to_owned())]
    );
}

#[test]
fn only_a_credential_refusal_types_the_waiters_it_never_sent() {
    let mut rig = support::Rig::defaults();
    rig.command(safe_cex_engine::Command::FetchCandles {
        command: 9,
        symbol: "ETHUSD".into(),
        interval: safe_cex_engine::model::Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let login = rig.next_request();
    assert!(login.url.ends_with("/login"));
    rig.reply(&login, 429, "{}");
    let [error] = rig.failures.as_slice() else {
        panic!("one failed waiter: {:?}", rig.failures);
    };
    assert_eq!(
        error.kind, None,
        "a throttled login is not the waiter's kind"
    );
}
