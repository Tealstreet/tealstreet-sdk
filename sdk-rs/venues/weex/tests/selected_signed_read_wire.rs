//! R4 (#1992): every WEEX managed signed read keeps its exact wire and key
//! choice across the cutover to engine Selection, for each key configuration.
use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::signing::{Identity, KeyRef, Operation};
use safe_cex_engine::{Action, Command, Engine, Event};
use safe_cex_venue_weex::{Options, WeexFutures as Venue};

fn identity(slot: &str) -> Identity {
    Identity::new(
        KeyRef(format!("ref:{slot}-credential")),
        Some(KeyRef(format!("ref:{slot}-signing"))),
    )
    .with_passphrase(Some(KeyRef(format!("ref:{slot}-passphrase"))))
    .with_stable_id(Some("native-primary-account".into()))
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
fn wire(engine: &mut Engine<Venue>, at: f64, actions: Vec<Action>) -> Vec<String> {
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
    let mut engine = safe_cex_engine::test_support::seeded_engine(Venue::new(Options {
        credentials: primary.then(|| identity("primary")),
        read_credentials: read.then(|| identity("secondary")),
        ..Options::default()
    }));
    engine.store_mut().set_markets(vec![market()]);
    engine.start(1_000.);
    engine.drain_actions().for_each(drop);
    let mut lines = Vec::new();
    for command in [
        Command::FetchAccount { command: 41 },
        Command::FetchLeverage {
            command: 42,
            symbol: "BTCUSDT".into(),
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

/// Pinned from the base (`ba3d7da1d3a`): GET reads use the read key when one is
/// configured, even without a primary. Identity (accountConfig) is issued only
/// with a primary; leverage (symbolConfig) is served by either key.
fn expected(primary: bool, read: bool) -> Vec<String> {
    let (intent, key) = if read {
        ("Secondary", "ref:secondary-credential")
    } else {
        ("Primary", "ref:primary-credential")
    };
    let headers = r#"[("Content-Type", "application/json"), ("User-Agent", "Tealstreet")]"#;
    let rest = "https://api-contract.weex.com/capi/v3/account";
    let identity = primary.then(|| format!("{rest}/accountConfig"));
    identity
        .into_iter()
        .chain([format!("{rest}/symbolConfig?symbol=BTC-USDT")])
        .map(|url| format!("Get {url} {headers} {intent} -> {key}"))
        .collect()
}
