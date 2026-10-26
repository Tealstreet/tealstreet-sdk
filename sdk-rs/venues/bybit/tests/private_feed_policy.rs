//! Declared REST-only provider feeds retain the existing account poll lifecycle.
mod support;
use safe_cex_engine::SocketChannel;
use safe_cex_engine::signing_policy::PrivateFeedPolicy;
use safe_cex_venue_bybit::Variant;
use support::{Harness, Options};

const EMPTY: &str = r#"{"retCode":0,"result":{"list":[],"nextPageCursor":""}}"#;
const WALLET: &str = r#"{"retCode":0,"result":{"list":[{"accountType":"UNIFIED","totalEquity":"1100","totalWalletBalance":"1000","totalInitialMargin":"10","totalMaintenanceMargin":"1","totalAvailableBalance":"900","coin":[]}]}}"#;
fn no_private(h: &Harness) {
    assert!(
        h.socket_channels
            .iter()
            .all(|(_, channel)| *channel != Some(SocketChannel::Private))
    );
    assert!(
        h.sent
            .iter()
            .all(|(_, text)| !text.contains("\"op\":\"auth\""))
    );
}
fn reply(h: &mut Harness, variant: Variant) -> usize {
    let mut reads = 0;
    while !h.requests.is_empty() {
        no_private(h);
        let request = h.requests.remove(0);
        let body = if request.url.contains("status=PreLaunch") {
            EMPTY.to_owned()
        } else if request.url.contains("/v5/market/instruments-info") {
            if variant.is_inverse() {
                support::INSTRUMENTS
                    .replace("USDT", "USD")
                    .replace("LinearPerpetual", "InversePerpetual")
            } else {
                support::INSTRUMENTS.to_owned()
            }
        } else if request.url.contains("/v5/market/tickers") {
            if variant.is_inverse() {
                support::TICKERS.replace("USDT", "USD")
            } else {
                support::TICKERS.to_owned()
            }
        } else if request.url.contains("/v5/account/info") {
            support::ACCOUNT_INFO.to_owned()
        } else if request.url.contains("/v5/account/fee-rate") {
            support::FEE_RATES.to_owned()
        } else if request.url.contains("/v5/account/wallet-balance") {
            reads += 1;
            WALLET.to_owned()
        } else if request.url.contains("/v5/order/realtime")
            || request.url.contains("/v5/position/list")
        {
            reads += 1;
            EMPTY.to_owned()
        } else {
            panic!("unexpected REST account request {}", request.url);
        };
        h.reply(request.id, 200, &body);
    }
    reads
}
#[test]
fn four_rest_account_aliases_load_and_poll_without_private_auth_or_subscriptions() {
    for variant in [
        Variant::Linear,
        Variant::Inverse,
        Variant::Spot,
        Variant::Demo,
    ] {
        let mut engine = support::engine(Options {
            variant,
            key: Some("fixture-public".into()),
            secret: Some("fixture-provider".into()),
            ..Default::default()
        });
        engine
            .set_private_feed_policy(PrivateFeedPolicy::RestOnly)
            .unwrap();
        let mut h = Harness::started_engine(engine);
        assert!(reply(&mut h, variant) >= 2);
        let loaded = h.engine.store().loaded();
        assert!(
            loaded.balance && loaded.orders && loaded.positions,
            "{variant:?}"
        );
        no_private(&h);
        let mut reads = 0;
        for _ in 0..24 {
            h.advance(1000.0);
            reads += reply(&mut h, variant);
            no_private(&h);
        }
        assert!(reads >= 2, "{variant:?} retains REST reconciliation");
    }
}

struct CallbackHeaders(safe_cex_engine::signing::Provider);
impl safe_cex_engine::signing::Signer for CallbackHeaders {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(
        &self,
        work: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        self.0.sign_ready(work).map(|value| {
            value.map(|mut result| {
                result.headers = vec![
                    ("x-bapi-api-key".into(), "untrusted-key".into()),
                    ("x-bapi-sign".into(), "untrusted-signature".into()),
                    ("x-bapi-timestamp".into(), "0".into()),
                    ("x-bapi-recv-window".into(), "1000".into()),
                    ("X-Proof".into(), "same-attempt".into()),
                ];
                result
            })
        })
    }
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.0.sign(work)
    }
}
#[test]
fn declared_http_policy_protects_auth_but_preserves_callback_window_precedence() {
    use safe_cex_engine::signing_policy::RemoteSigningPolicy;
    let (venue, provider) = Options {
        key: Some("fixture-public".into()),
        secret: Some("fixture-provider".into()),
        signing_policy: RemoteSigningPolicy {
            enabled: true,
            payload_supported: true,
            payload_rest_only: true,
            protected_headers: Some(&["X-BAPI-SIGN", "X-BAPI-API-KEY", "X-BAPI-TIMESTAMP"]),
            ..Default::default()
        },
        ..Default::default()
    }
    .build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine
        .set_signer(Some(std::sync::Arc::new(CallbackHeaders(provider))))
        .unwrap();
    engine
        .set_private_feed_policy(PrivateFeedPolicy::RestOnly)
        .unwrap();
    let h = Harness::started_engine(engine);
    let requests: Vec<_> = h
        .requests
        .iter()
        .filter(|request| request.url.contains("/v5/account/"))
        .collect();
    assert!(!requests.is_empty());
    for request in requests {
        let headers: Vec<_> = request
            .headers
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .chain(
                request
                    .signer_headers
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            )
            .collect();
        let find = |name: &str| {
            let values: Vec<_> = headers
                .iter()
                .filter(|(key, _)| key.eq_ignore_ascii_case(name))
                .collect();
            assert_eq!(values.len(), 1);
            values[0].1
        };
        assert_eq!(find("X-BAPI-API-KEY"), "fixture-public");
        assert_eq!(find("X-BAPI-TIMESTAMP"), "1000");
        let payload = request
            .url
            .split_once('?')
            .map_or("", |(_, payload)| payload);
        assert_eq!(
            find("X-BAPI-SIGN"),
            safe_cex_crypto::hmac_sha256(
                b"fixture-provider",
                format!("1000fixture-public20000{payload}").as_bytes(),
                safe_cex_crypto::Encoding::Hex
            )
        );
        assert_eq!(find("X-BAPI-RECV-WINDOW"), "1000");
        assert_eq!(find("X-Proof"), "same-attempt");
    }
}
