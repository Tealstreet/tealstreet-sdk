use super::*;
use safe_cex_crypto::{hex, sha256};
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
use safe_cex_engine::{Action, HttpRequest, Notice, ReadCredentials};

const NOW: f64 = 1_700_000_000_123.0;
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    }
}
fn header<'a>(r: &'a HttpRequest, name: &str) -> &'a str {
    r.headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .unwrap()
        .1
        .as_str()
}
fn check_signature(r: &HttpRequest, key: &str, secret: &str, query: &str) {
    assert_eq!(header(r, "api-key"), key);
    assert_eq!(header(r, "timestamp"), "1700000000123");
    let payload = format!(
        "{}{}{key}{query}{}",
        header(r, "nonce"),
        header(r, "timestamp"),
        r.body.as_deref().unwrap_or("")
    );
    let first = hex(&sha256(payload.as_bytes()));
    let expected = hex(&sha256(format!("{first}{secret}").as_bytes()));
    assert_eq!(header(r, "sign"), expected);
}
#[test]
fn secondary_get_signatures_keep_every_mutation_and_websocket_login_primary() {
    for secondary in [false, true] {
        let mut opts = options();
        if !secondary {
            opts.read_credentials = None;
        }
        let venue = ProbeVenue::new(
            BitunixFutures::new(opts),
            |v: &mut BitunixFutures, cx: &mut Cx<'_, Pending>| {
                for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
                    v.signed_request(
                        cx,
                        method,
                        "/secondary-test",
                        sign::Params::default()
                            .str("symbol", "BTCUSDT")
                            .str("marginCoin", "USDT"),
                        (method != Method::Get).then(|| r#"{"amount":"1"}"#.into()),
                        Pending::SymbolSettings {
                            symbol: "BTCUSDT".into(),
                            write_revision: 0,
                            serial: 0,
                        },
                    );
                }
                v.login(cx, SocketId(777));
            },
        );
        let mut e = safe_cex_engine::test_support::seeded_engine(venue);
        e.start(NOW);
        let actions: Vec<_> = e.drain_actions().collect();
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 4);
        for r in requests {
            let (key, secret) = if secondary && r.method == Method::Get {
                ("read-key", "read-secret")
            } else {
                ("primary-key", "primary-secret")
            };
            check_signature(r, key, secret, "marginCoinUSDTsymbolBTCUSDT");
            assert_eq!(
                r.body.as_deref(),
                (r.method != Method::Get).then_some(r#"{"amount":"1"}"#)
            );
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
        let login: Login<'_> = serde_json::from_str(login).unwrap();
        let arg = &login.args[0];
        assert_eq!(arg.api_key, "primary-key");
        assert_eq!(arg.timestamp, "1700000000123");
        let payload = format!("{}{}primary-key", arg.nonce, arg.timestamp);
        let first = hex(&sha256(payload.as_bytes()));
        assert_eq!(
            arg.sign,
            hex(&sha256(format!("{first}primary-secret").as_bytes()))
        );
    }
}
#[test]
fn every_account_axis_settings_and_history_get_uses_read_identity() {
    let venue = ProbeVenue::new(
        BitunixFutures::new(options()),
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
            v.query(
                cx,
                7,
                safe_cex_engine::Query::Income {
                    from_ms: None,
                    to_ms: None,
                    symbol: None,
                    kind: None,
                    limit: None,
                },
            );
        },
    );
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.start(NOW);
    let requests = http_requests(&mut e);
    assert_eq!(requests.len(), 6);
    for r in requests {
        let query = r
            .url
            .split_once('?')
            .map_or(String::new(), |(_, q)| q.replace(['=', '&'], ""));
        check_signature(&r, "read-key", "read-secret", &query);
        assert_eq!(r.method, Method::Get);
    }
}
#[test]
fn public_discovery_remains_unsigned_with_secondary_credentials() {
    let mut e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(options()));
    e.start(NOW);
    let requests = safe_cex_engine::test_support::prepared_actions(&mut e, NOW)
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
fn partial_secondary_credentials_stop_before_any_io_even_public_only() {
    for public_only in [false, true] {
        for (key, secret) in [("", "read-secret"), ("read-key", ""), ("", "")] {
            let mut opts = options();
            opts.public_data_only = public_only;
            let read = opts.read_credentials.as_mut().unwrap();
            read.key = key.into();
            read.secret = secret.into();
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
}
