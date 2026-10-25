//! Keyless public reads are managed unsigned reads, never the legacy funnel.
use safe_cex_engine::model::{Interval, Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_bingx::Bingx;
mod support;

const COMMAND: u32 = 42;

fn keyless() -> Engine<Bingx> {
    let mut engine = support::engine(support::Options::default());
    engine.start(0.);
    engine
}
fn prepares(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(a, Action::ManagedHttp(action)
                if matches!(action.as_ref(), HttpAction::Prepare { .. }))
        })
        .count()
}
fn physical(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request.clone()),
            _ => None,
        })
        .collect()
}
fn unsigned(request: &HttpRequest) {
    assert!(
        !request.url.contains("signature=")
            && !request
                .headers
                .iter()
                .any(|(name, _)| *name == "X-BX-APIKEY"),
        "keyless read must not sign: {}",
        request.url
    );
}
fn market() -> Market {
    Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 10.,
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
    }
}
fn issue_candles(engine: &mut Engine<Bingx>) {
    engine.drain_actions().for_each(drop);
    engine.store_mut().set_markets(vec![market()]);
    engine.handle(
        10.,
        Event::Command(Command::FetchCandles {
            command: COMMAND,
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: Some(120_000.),
        }),
    );
}

#[test]
fn keyless_startup_lists_are_managed_unsigned_background_reads() {
    let mut engine = keyless();
    let raw: Vec<_> = engine.drain_actions().collect();
    assert!(prepares(&raw) >= 1, "keyless list reads begin with Prepare");
    assert!(
        physical(&raw).is_empty(),
        "nothing releases before preparation"
    );
}

#[test]
fn keyless_klines_are_one_managed_unsigned_command_read() {
    let mut engine = keyless();
    issue_candles(&mut engine);
    let raw: Vec<_> = engine.drain_actions().collect();
    assert_eq!(prepares(&raw), 1, "one command Read preparation");
    assert!(
        physical(&raw).is_empty(),
        "nothing releases before preparation"
    );
}

#[test]
fn keyless_klines_complete_and_fail_through_their_context() {
    for fail in [false, true] {
        let mut engine = keyless();
        issue_candles(&mut engine);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 10.);
        let request = physical(&actions)
            .into_iter()
            .find(|r| r.url.contains("/quote/klines"))
            .expect("one klines read");
        unsigned(&request);
        if fail {
            engine.handle(
                11.,
                Event::HttpFailed {
                    request: request.id,
                    error: "transport lost",
                },
            );
        } else {
            engine.handle(
                11.,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: r#"{"code":0,"data":[{"time":60000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}]}"#,
                    },
                },
            );
        }
        let done = safe_cex_engine::test_support::prepared_actions(&mut engine, 11.);
        let terminals = done
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    Action::Notice(Notice::CommandDone {
                        command: COMMAND,
                        ..
                    }) | Action::Notice(Notice::CommandFailed {
                        command: COMMAND,
                        ..
                    })
                )
            })
            .count();
        assert_eq!(terminals, 1, "fail={fail}: exactly one terminal");
        assert_eq!(
            done.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed {
                    command: COMMAND,
                    ..
                })
            )),
            fail
        );
    }
}

fn klines_headers(options: support::Options) -> Vec<(&'static str, String)> {
    let mut engine = support::engine(options);
    engine.start(0.);
    issue_candles(&mut engine);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 10.);
    physical(&actions)
        .into_iter()
        .find(|r| r.url.contains("/quote/klines"))
        .expect("one klines read")
        .headers
}

#[test]
fn managed_reads_keep_the_native_public_and_signed_header_vectors() {
    let content = ("Content-Type", "application/json, charset=utf-8".to_owned());
    assert_eq!(
        klines_headers(support::Options::default()),
        [("X-SOURCE-KEY", "Tealstreet".to_owned()), content.clone()]
    );
    let signed = klines_headers(support::Options {
        key: Some("fixture-primary".into()),
        secret: Some("fixture-signing-material".into()),
        ..Default::default()
    });
    assert_eq!(
        signed,
        [
            ("X-SOURCE-KEY", "Tealstreet".to_owned()),
            ("X-BX-APIKEY", "fixture-primary".to_owned()),
            content,
        ]
    );
}
