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

fn recover_catalogue<V: Venue>(venue: V, suffix: &str, body: &str) {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.start(0.);
    let old = request(&mut e, 0., suffix);
    answer(&mut e, 20_000., &old, body);
    assert!(!e.store().loaded().markets);
    let a = actions(&mut e, 20_000.);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (20_750. ..=21_000.).contains(at_ms) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("native first discovery backoff");
    e.handle(21_000., Event::TimerFired { timer });
    let current = request(&mut e, 21_000., suffix);
    assert_ne!(old.id, current.id);
    answer(&mut e, 21_001., &old, body);
    assert!(!e.store().loaded().markets);
    assert!(
        !actions(&mut e, 21_001.)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    answer(&mut e, 21_002., &current, body);
    assert!(e.store().loaded().markets);
    actions(&mut e, 21_002.);
    answer(&mut e, 21_003., &old, "{}");
    assert!(e.store().loaded().markets);
    assert!(
        !actions(&mut e, 21_003.)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
}

#[test]
fn futures_timeout_retires_old_catalogue_and_native_retry_recovers() {
    recover_catalogue(Kraken::new(public_options()), "/instruments", support::META);
}

#[test]
fn spot_timeout_retires_old_catalogue_and_native_retry_recovers() {
    recover_catalogue(
        KrakenSpot::new(public_options()),
        "/AssetPairs",
        spot_support::META,
    );
}

fn held_prepare_recovers<V: Venue>(venue: V, suffix: &str, body: &str) {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.start(0.);
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_000. => Some(*timer),
            _ => None,
        })
        .expect("preparation is deadline bounded");
    e.handle(20_000., Event::TimerFired { timer });
    let a = actions(&mut e, 20_000.);
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (20_750. ..=21_000.).contains(at_ms) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("stalled preparation releases native discovery busy state");
    e.handle(21_000., Event::TimerFired { timer });
    let current = request(&mut e, 21_000., suffix);
    answer(&mut e, 21_001., &current, body);
    assert!(e.store().loaded().markets);
}

#[test]
fn futures_held_preparation_expires_and_releases_busy_state() {
    held_prepare_recovers(Kraken::new(public_options()), "/instruments", support::META);
}

#[test]
fn spot_held_preparation_expires_and_releases_busy_state() {
    held_prepare_recovers(
        KrakenSpot::new(public_options()),
        "/AssetPairs",
        spot_support::META,
    );
}
