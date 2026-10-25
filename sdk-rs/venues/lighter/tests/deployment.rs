//! Deployment oracles: TS LIGHTER_DEPLOYMENT_PROFILES and builder-fee defaults.
mod support;
use safe_cex_engine::{Action, Command, Event};
use safe_cex_venue_lighter::Lighter;
use serde_json::json;
use support::*;

#[test]
fn every_deployment_uses_its_rest_websocket_and_quote_profile() {
    for (robinhood, testnet, host, ws, quote) in [
        (
            false,
            false,
            "https://mainnet.zklighter.elliot.ai",
            "wss://mainnet.zklighter.elliot.ai/stream",
            "USDC",
        ),
        (
            false,
            true,
            "https://testnet-api.lighter.xyz",
            "wss://testnet.zklighter.elliot.ai/stream",
            "USDC",
        ),
        (
            true,
            false,
            "https://api.rh.lighter.xyz",
            "wss://api.rh.lighter.xyz/stream",
            "USDG",
        ),
        (
            true,
            true,
            "https://api.rh-testnet.lighter.xyz",
            "wss://api.rh-testnet.lighter.xyz/stream",
            "USDG",
        ),
    ] {
        let mut opts = options();
        opts.robinhood = robinhood;
        opts.testnet = testnet;
        let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(opts));
        engine.start(NOW);
        let mut sockets = 0;
        let mut request_paths = Vec::new();
        for _ in 0..30 {
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    Action::Http(request) => {
                        assert!(
                            request.url.starts_with(&format!("{host}/api/v1/")),
                            "{}",
                            request.url
                        );
                        request_paths.push(request.url.clone());
                        let body = if request.url.contains("orderBookDetails") {
                            META
                        } else if request.url.contains("exchangeStats") {
                            r#"{"code":200,"order_book_stats":[]}"#
                        } else if request.url.contains("accountActiveOrders") {
                            OPEN
                        } else if request.url.contains("/account?") {
                            ACCOUNT
                        } else {
                            panic!("unexpected request {}", request.url)
                        };
                        reply(&mut engine, &request, 200, body);
                    }
                    Action::WsConnect { socket, url, .. } => {
                        sockets += 1;
                        assert_eq!(url, ws);
                        engine.handle(NOW, Event::WsOpened { socket });
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(sockets, 2, "public and private use {ws}");
        assert!(
            request_paths
                .iter()
                .any(|url| url.contains("accountActiveOrders"))
        );
        assert_eq!(engine.store().market("ETH").unwrap().quote, quote);
        assert_eq!(engine.store().market("BTC").unwrap().quote, quote);
        assert!(engine.store().loaded().balance);
        assert!(engine.store().loaded().positions);
    }
}

#[test]
fn robinhood_placements_require_their_own_default_builder_approval() {
    for testnet in [false, true] {
        let mut opts = options();
        opts.robinhood = true;
        opts.testnet = testnet;
        let (mut engine, _) = ready_with(opts);
        engine.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![
                    serde_json::from_value(
                        json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}),
                    )
                    .unwrap(),
                ],
            }),
        );
        let requests = requests(&mut engine);
        assert_eq!(requests.len(), 1);
        assert!(requests[0].url.contains("/accountLimits?"));
        assert!(
            !requests
                .iter()
                .any(|request| request.url.contains("nextNonce") || request.url.contains("sendTx"))
        );
    }
}

#[test]
fn explicit_robinhood_builder_still_requires_approval() {
    let mut opts = options();
    opts.robinhood = true;
    opts.integrator_account_index = Some(999);
    let (mut engine, _) = ready_with(opts);
    engine.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![
                serde_json::from_value(
                    json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}),
                )
                .unwrap(),
            ],
        }),
    );
    let requests = requests(&mut engine);
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .url
            .starts_with("https://api.rh.lighter.xyz/api/v1/accountLimits?")
    );
}
