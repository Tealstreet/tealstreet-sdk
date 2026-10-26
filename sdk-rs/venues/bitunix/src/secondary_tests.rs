use super::*;
use safe_cex_crypto::{hex, sha256};
use safe_cex_engine::signing::{KeyRef, Provider};
use safe_cex_engine::test_support::{ProbeVenue, prepared_actions};
use safe_cex_engine::{Action, HttpRequest, Notice};

const NOW: f64 = 1_700_000_000_123.0;
fn options() -> (Options, Provider) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let slot = |key: &str, material: &str| LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(material.into()),
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(slot("primary-key", "primary-material")),
        secondary: Some(slot("read-key", "read-material")),
    })
    .unwrap();
    (
        Options {
            credentials: identities.primary,
            read_credentials: identities.secondary,
            ..Options::default()
        },
        std::sync::Arc::new(provider),
    )
}
fn header<'a>(r: &'a HttpRequest, name: &str) -> &'a str {
    r.headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .unwrap()
        .1
        .as_str()
}
fn check_signature(r: &HttpRequest, key: &str, material: &str, query: &str) {
    assert_eq!(header(r, "api-key"), key);
    assert_eq!(header(r, "timestamp"), "1700000000123");
    let payload = format!(
        "{}{}{key}{query}{}",
        header(r, "nonce"),
        header(r, "timestamp"),
        r.body.as_deref().unwrap_or("")
    );
    let first = hex(&sha256(payload.as_bytes()));
    let expected = hex(&sha256(format!("{first}{material}").as_bytes()));
    assert_eq!(header(r, "sign"), expected);
}
/// Selection is by role, not method: reads sign with the read key, and a
/// trading-job request and the socket login sign Primary whatever the method.
#[test]
fn reads_use_the_read_key_and_trading_and_login_stay_primary() {
    for secondary in [false, true] {
        let (mut opts, provider) = options();
        if !secondary {
            opts.read_credentials = None;
        }
        let venue = ProbeVenue::new(
            BitunixFutures::new(opts),
            |v: &mut BitunixFutures, cx: &mut Cx<'_, Pending>| {
                let params = || {
                    sign::Params::default()
                        .str("symbol", "BTCUSDT")
                        .str("marginCoin", "USDT")
                };
                v.signed_request(
                    cx,
                    Method::Get,
                    "/secondary-test",
                    params(),
                    None,
                    Pending::SymbolSettings {
                        symbol: "BTCUSDT".into(),
                        write_revision: 0,
                        serial: 0,
                        custody: None,
                    },
                );
                for method in [Method::Get, Method::Post] {
                    v.signed_request(
                        cx,
                        method,
                        "/secondary-test",
                        params(),
                        (method != Method::Get).then(|| r#"{"amount":"1"}"#.into()),
                        Pending::Trade(trading::Request::leverage_probe(9)),
                    );
                }
                v.login(cx, SocketId(777));
            },
        );
        let mut e = safe_cex_engine::test_support::seeded_engine(venue);
        e.set_signer(Some(provider)).unwrap();
        e.start(NOW);
        let actions = prepared_actions(&mut e, NOW);
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 3);
        for (index, r) in requests.into_iter().enumerate() {
            let (key, material) = if secondary && index == 0 {
                ("read-key", "read-material")
            } else {
                ("primary-key", "primary-material")
            };
            check_signature(r, key, material, "marginCoinUSDTsymbolBTCUSDT");
        }
        #[derive(serde::Deserialize)]
        struct Login<'a> {
            #[serde(borrow)]
            args: Vec<LoginArg<'a>>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LoginArg<'a> {
            api_key: &'a str,
            timestamp: &'a str,
            nonce: &'a str,
            sign: &'a str,
        }
        let login = actions
            .iter()
            .find_map(|a| match a {
                Action::WsSend {
                    socket: SocketId(777),
                    text,
                } => Some(text),
                _ => None,
            })
            .unwrap();
        let login: Login<'_> = serde_json::from_str(login).unwrap();
        let arg = &login.args[0];
        assert_eq!(arg.api_key, "primary-key");
        assert_eq!(arg.timestamp, "1700000000123");
        let payload = format!("{}{}primary-key", arg.nonce, arg.timestamp);
        let first = hex(&sha256(payload.as_bytes()));
        assert_eq!(
            arg.sign,
            hex(&sha256(format!("{first}primary-material").as_bytes()))
        );
    }
}
#[test]
/// History signs with the read key too; tests/history.rs pins it on its own
/// caller root (a probe has no genuine command entry).
fn every_account_axis_and_settings_get_uses_read_identity() {
    let (opts, provider) = options();
    let venue = ProbeVenue::new(
        BitunixFutures::new(opts),
        |v: &mut BitunixFutures, cx: &mut Cx<'_, Pending>| {
            cx.store.set_markets(vec![Market {
                id: "BTC/USDT".into(),
                symbol: "BTCUSDT".into(),
                base: "BTC".into(),
                quote: "USDT".into(),
                active: true,
                precision: Precision {
                    amount: 0.001,
                    price: 0.1,
                },
                limits: Limits {
                    amount: MinMax { min: 0., max: 100. },
                    leverage: MinMax { min: 1., max: 100. },
                    price: None,
                },
                pretty_symbol: None,
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            }]);
            assert!(cx.store.market("BTCUSDT").is_some());
            for axis in [
                account::Axis::Balance,
                account::Axis::Positions,
                account::Axis::Orders,
                account::Axis::Tpsl,
            ] {
                v.read_axis(cx, axis);
            }
            v.read_symbol_settings(cx, "BTCUSDT");
        },
    );
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.set_signer(Some(provider)).unwrap();
    e.start(NOW);
    let requests: Vec<_> = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 5);
    for r in requests {
        let query = r
            .url
            .split_once('?')
            .map_or(String::new(), |(_, q)| q.replace(['=', '&'], ""));
        check_signature(&r, "read-key", "read-material", &query);
        assert_eq!(r.method, Method::Get);
    }
}
#[test]
fn public_discovery_remains_unsigned_with_secondary_credentials() {
    let (opts, provider) = options();
    let mut e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(opts));
    e.set_signer(Some(provider)).unwrap();
    e.start(NOW);
    let requests = prepared_actions(&mut e, NOW)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].url.ends_with("/trading_pairs"));
    assert!(
        !requests[0]
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("api-key") || n.eq_ignore_ascii_case("sign"))
    );
}
#[test]
fn an_invalid_read_identity_stops_before_any_io_even_public_only() {
    for public_only in [false, true] {
        let (mut opts, _) = options();
        opts.public_data_only = public_only;
        opts.read_credentials = Some(Identity::new(
            KeyRef(String::new()),
            Some(KeyRef(String::new())),
        ));
        let mut e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(opts));
        e.start(NOW);
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
        );
        assert!(actions.iter().any(|n| matches!(n, Action::Notice(Notice::Event(event)) if event.code == code::CONFIG_INVALID)));
    }
}
