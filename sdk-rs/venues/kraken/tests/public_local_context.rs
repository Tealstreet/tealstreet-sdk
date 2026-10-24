mod spot_support;
mod support;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Notice, Query, Response, Venue,
};
use safe_cex_venue_kraken::{Kraken, KrakenSpot, Options};

fn public_options() -> Options {
    Options {
        public_data_only: true,
        ..Default::default()
    }
}
fn actions<V: Venue>(e: &mut Engine<V>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn request<V: Venue>(e: &mut Engine<V>, now: f64, suffix: &str) -> HttpRequest {
    actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with(suffix) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn answer<V: Venue>(e: &mut Engine<V>, now: f64, r: &HttpRequest, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn invalid_clock_refusal<V: Venue>(e: &mut Engine<V>, command: Command) {
    e.handle(f64::NAN, Event::Command(command));
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        matches!(&a[..],[Action::Notice(Notice::CommandFailed{command:7,error})]
        if error.kind==Some(ErrorKind::InvalidInput)),
        "genuine invalid entry: {a:?}"
    );
}
#[test]
fn futures_expired_catalogue_cannot_publish_but_live_catalogue_does() {
    for (age, expected) in [(1., true), (20_000., false)] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Kraken::new(public_options()));
        e.start(support::NOW);
        let r = request(&mut e, support::NOW, "/instruments");
        assert!(r.headers.is_empty());
        assert!(r.body.is_none());
        answer(&mut e, support::NOW + age, &r, support::META);
        assert_eq!(e.store().loaded().markets, expected, "age {age}");
    }
}
#[test]
fn futures_expired_tickers_cannot_publish_but_live_tickers_do() {
    for (age, expected) in [(1., true), (20_000., false)] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Kraken::new(public_options()));
        e.start(support::NOW);
        let r = request(&mut e, support::NOW, "/instruments");
        answer(&mut e, support::NOW, &r, support::META);
        let r = request(&mut e, support::NOW, "/tickers");
        answer(&mut e, support::NOW + age, &r, support::TICKERS);
        assert_eq!(e.store().loaded().tickers, expected, "age {age}");
        if expected {
            assert_eq!(e.store().ticker("PF_XBTUSD").unwrap().last, 100.);
        }
    }
}
#[test]
fn spot_expired_catalogue_cannot_publish_but_live_catalogue_does() {
    for (age, expected) in [(1., true), (20_000., false)] {
        let mut e = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(public_options()));
        e.start(spot_support::NOW);
        let r = request(&mut e, spot_support::NOW, "/AssetPairs");
        assert!(r.headers.is_empty());
        assert!(r.body.is_none());
        answer(&mut e, spot_support::NOW + age, &r, spot_support::META);
        assert_eq!(e.store().loaded().markets, expected, "age {age}");
    }
}
#[test]
fn spot_expired_tickers_cannot_publish_but_live_tickers_do() {
    for (age, expected) in [(1., true), (20_000., false)] {
        let mut e = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(public_options()));
        e.start(spot_support::NOW);
        let r = request(&mut e, spot_support::NOW, "/AssetPairs");
        answer(&mut e, spot_support::NOW, &r, spot_support::META);
        let r = request(&mut e, spot_support::NOW, "/Ticker");
        answer(&mut e, spot_support::NOW + age, &r, spot_support::TICKERS);
        assert_eq!(e.store().loaded().tickers, expected, "age {age}");
        if expected {
            assert_eq!(e.store().ticker("XBT/USD").unwrap().last, 100.);
        }
    }
}
#[test]
fn futures_discovery_cannot_send_before_host_preparation() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Kraken::new(public_options()));
    e.start(support::NOW);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "host has not prepared: {a:?}"
    );
}
#[test]
fn spot_discovery_cannot_send_before_host_preparation() {
    let mut e = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(public_options()));
    e.start(spot_support::NOW);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(
        !a.iter().any(|a| matches!(a, Action::Http(_))),
        "host has not prepared: {a:?}"
    );
}
#[test]
fn futures_local_lookup_refusal_requires_genuine_entry() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Kraken::new(public_options()));
    invalid_clock_refusal(
        &mut e,
        Command::Query {
            command: 7,
            query: Query::OrderByClientId {
                symbol: "PF_XBTUSD".into(),
                client_order_id: "x".into(),
            },
        },
    );
}
#[test]
fn spot_local_leverage_result_requires_genuine_entry() {
    let mut e = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(public_options()));
    invalid_clock_refusal(
        &mut e,
        Command::FetchLeverage {
            command: 7,
            symbol: "XBT/USD".into(),
        },
    );
}
#[test]
fn local_finite_results_and_refusals_keep_native_text_without_network() {
    let mut futures = safe_cex_engine::test_support::seeded_engine(Kraken::new(public_options()));
    futures.handle(
        0.,
        Event::Command(Command::Query {
            command: 7,
            query: Query::OrderByClientId {
                symbol: "PF_XBTUSD".into(),
                client_order_id: "x".into(),
            },
        }),
    );
    let a = actions(&mut futures, 0.);
    assert!(
        matches!(&a[..],[Action::Notice(Notice::CommandFailed{error,..})]
        if error.message=="Kraken does not support caller client order IDs or their lookup")
    );
    futures.handle(
        1.,
        Event::Command(Command::SetPositionMode {
            command: 7,
            hedged: false,
            symbol: None,
        }),
    );
    let a = actions(&mut futures, 1.);
    assert!(matches!(&a[..],[Action::Notice(Notice::CommandDone{result,..})]if result=="null"));
    let mut spot = safe_cex_engine::test_support::seeded_engine(KrakenSpot::new(public_options()));
    spot.handle(
        0.,
        Event::Command(Command::FetchLeverage {
            command: 7,
            symbol: "XBT/USD".into(),
        }),
    );
    let a = actions(&mut spot, 0.);
    assert!(matches!(&a[..],[Action::Notice(Notice::CommandDone{result,..})]if result=="null"));
}
