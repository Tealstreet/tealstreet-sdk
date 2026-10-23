//! Positive dispatch proof is separate from standalone WASM decoder admission.
//! Only fixture state is synthetic; capability selection and handlers are native.
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, supported};

struct BookProbe<'a>(&'a str);
impl Visit for BookProbe<'_> {
    type Output = usize;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> usize {
        if !engine.capabilities().order_book_query {
            return 0;
        }
        engine.store_mut().set_markets(vec![Market {
            id: "BTC_USDT".into(),
            symbol: "BTC/USDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 100.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 10.0,
                },
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
        assert!(
            engine.drain_actions().next().is_none(),
            "{}: no startup/earlier actions",
            self.0
        );
        engine.handle(
            1000.0,
            Event::Command(Command::FetchOrderBook {
                command: 71,
                symbol: "BTC/USDT".into(),
                limit: Some(1),
            }),
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1000.0);
        let requests = actions
            .iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            requests.len(),
            1,
            "{}: advertised book handler must issue a request, not a precondition/unsupported/deferred result: {actions:?}",
            self.0
        );
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            )),
            "{}: no terminal result before its request returns",
            self.0
        );
        // This is positive request-dispatch evidence. Endpoint-specific reply
        // decoding and retry/completion policies remain the venue regression
        // suites' responsibility; a transport failure can legitimately retry.
        assert!(
            engine.venue().book("BTC/USDT").is_none(),
            "{}: query does not create a subscription book",
            self.0
        );
        1
    }
}

#[test]
fn every_compiled_advertised_rest_book_handler_dispatches_with_valid_fixture_state() {
    // Read production aliases, not a second list of expected venue capabilities.
    let mut names = Vec::new();
    for line in include_str!("../../hosts/wasm/js/venues.mjs").lines() {
        let Some((name, family)) = line
            .trim()
            .strip_suffix(',')
            .and_then(|line| line.split_once(": "))
        else {
            continue;
        };
        if !family.starts_with('\'') || !family.ends_with('\'') {
            continue;
        }
        let name = name.trim_matches('\'');
        if name.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
            names.push(name);
        }
    }
    assert!(!names.is_empty(), "production alias roster was read");
    let mut dispatched = 0;
    for name in names.into_iter().filter(|name| supported(name)) {
        let mut options = serde_json::json!({ "publicDataOnly": true, "testnet": true });
        if name == "breakoutprop" {
            options["key"] = "user".into();
            options["secret"] = "password".into();
        }
        if name == "hyrotrader" {
            options["endpoints"] = serde_json::json!({
                "rest": "https://hyro.test", "publicWs": "wss://hyro.test/public", "privateWs": "wss://hyro.test/private",
            });
        }
        let options: EngineOptions = serde_json::from_value(options).expect("fixture options");
        dispatched += build(
            name,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            BookProbe(name),
        )
        .unwrap_or_else(|error| {
            panic!("{name}: construction must not skip conformance: {error:?}")
        });
    }
    // Some isolated families intentionally expose no REST-book command. The
    // combined CI run exercises every compiled positive claim; no venue count
    // or expected capability table is duplicated here.
    eprintln!("positive REST-book handler dispatches: {dispatched}");
}
