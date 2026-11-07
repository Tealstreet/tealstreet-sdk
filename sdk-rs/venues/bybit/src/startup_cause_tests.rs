use super::*;
use safe_cex_engine::{Action, Event, Notice, log::Value};

#[test]
fn native_public_refusal_is_in_the_watchdog_without_raw_failure_text() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(BybitLinear::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    engine.start(0.);
    let requests: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    let markets = requests
        .iter()
        .find(|r| r.url.contains("instruments-info"))
        .unwrap();
    engine.handle(
        1.,
        Event::HttpResponse {
            request: markets.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"retCode":10006,"retMsg":"raw private text","result":null}"#,
            },
        },
    );
    engine.handle(60_000., Event::PollScaleChanged { scale: 1.0 });
    let event = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(e)) if e.code == code::STARTUP_INCOMPLETE => Some(e),
            _ => None,
        })
        .unwrap();
    for field in [
        ("startupMarketsState", "failed".into()),
        ("startupMarketsKind", "rateLimited".into()),
        ("startupMarketsVenueCode", "10006".into()),
        ("startupMarketsHttpStatus", 200_u16.into()),
        ("startupTickersState", "started".into()),
    ] {
        assert!(
            event.fields.contains(&field),
            "missing {field:?}: {:?}",
            event.fields
        );
    }
    assert!(
        !event
            .fields
            .iter()
            .any(|(_, v)| matches!(v, Value::Str(v) if v.contains("raw private text")))
    );
    assert_eq!(
        event.message,
        "Startup incomplete: still waiting for markets, tickers"
    );
}
