//! R3 (#1992): every BingX managed signed read keeps its exact wire and key
//! choice across the cutover to engine Selection, for each key configuration.
use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::signing::{Identity, KeyRef, Operation};
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_bingx::{Bingx, Options};

fn identity(slot: &str) -> Identity {
    Identity::new(
        KeyRef(format!("ref:{slot}-credential")),
        Some(KeyRef(format!("ref:{slot}-signing"))),
    )
}

fn market() -> safe_cex_engine::model::Market {
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
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
                max: 10.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 100.0,
            },
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

/// One line per prepared signed read: method, URL, headers, routing intent and
/// the credential presented when signing.
fn wire(engine: &mut Engine<Bingx>, at: f64, actions: Vec<Action>) -> Vec<String> {
    let mut lines = Vec::new();
    for action in actions {
        let Action::ManagedHttp(action) = action else {
            continue;
        };
        let HttpAction::Prepare {
            preparation,
            request,
            intent,
        } = *action
        else {
            continue;
        };
        engine.http_prepared(
            at,
            preparation,
            Prepared {
                id: 1,
                expires_at: None,
            },
        );
        let presented = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Sign { work, .. } => work.operations.iter().find_map(|op| match op {
                    Operation::Present { key } => Some(key.0.clone()),
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or_else(|| "unsigned".into());
        lines.push(format!(
            "{:?} {} {:?} {:?} -> {presented}",
            request.method, request.url, request.headers, intent
        ));
    }
    lines
}

fn reads(primary: bool, read: bool) -> Vec<String> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Bingx::new(Options {
        credentials: primary.then(|| identity("primary")),
        read_credentials: read.then(|| identity("secondary")),
        public_data_only: false,
    }));
    engine.store_mut().set_markets(vec![market()]);
    engine.start(1_000.);
    let started = engine.drain_actions().collect();
    let mut lines = wire(&mut engine, 1_001., started);
    for command in [
        Command::FetchAccount { command: 41 },
        Command::FetchCandles {
            command: 42,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: Some(120_000.),
        },
    ] {
        engine.handle(1_002., Event::Command(command));
        let actions = engine.drain_actions().collect();
        lines.extend(wire(&mut engine, 1_003., actions));
    }
    lines
}

#[test]
fn managed_signed_reads_keep_their_exact_wire_and_key_choice() {
    let configs = [(true, true), (true, false), (false, true)];
    let all: Vec<_> = configs.map(|(primary, read)| reads(primary, read)).into();
    for ((primary, read), lines) in configs.iter().zip(&all) {
        println!("== primary={primary} read={read}");
        for line in lines {
            println!("{line}");
        }
    }
    for ((primary, read), lines) in configs.into_iter().zip(all) {
        assert_eq!(
            lines,
            expected(primary, read),
            "primary={primary} read={read}"
        );
    }
}

/// Pinned from the base (`ba3d7da1d3a`): GET reads use the read key whenever a
/// primary is also configured; a read key alone signs nothing.
fn expected(primary: bool, read: bool) -> Vec<String> {
    let (intent, key) = match (primary, read) {
        (true, true) => ("Secondary", "ref:secondary-credential"),
        (true, false) => ("Primary", "ref:primary-credential"),
        // Keyless public reads are managed and unsigned; a read key alone signs nothing.
        _ => {
            return [
                "https://open-api.bingx.com/openApi/swap/v2/quote/contracts",
                "https://open-api.bingx.com/openApi/swap/v3/quote/klines?endTime=120000&interval=1m&startTime=0&symbol=BTC-USDT",
            ]
            .map(|url| {
                format!(
                    r#"Get {url} [("X-SOURCE-KEY", "Tealstreet"), ("Content-Type", "application/json, charset=utf-8")] Public -> unsigned"#
                )
            })
            .into();
        }
    };
    let headers =
        r#"[("X-SOURCE-KEY", "Tealstreet"), ("Content-Type", "application/json, charset=utf-8")]"#;
    [
        "https://open-api.bingx.com/openApi/swap/v2/quote/contracts",
        "https://open-api.bingx.com/openApi/swap/v3/user/balance",
        "https://open-api.bingx.com/openApi/swap/v3/quote/klines?endTime=120000&interval=1m&startTime=0&symbol=BTC-USDT",
    ]
    .map(|url| format!("Get {url} {headers} {intent} -> {key}"))
    .into()
}
