//! AsterDEX V3 nonces come from the engine's address domain, keyed by the API
//! wallet address the installed signing key proves at trusted setup.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, Response};
mod support;
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"}]}]}"#;
/// The fixture API wallet: `Options::v3_wallet` signs with this address's key.
const WALLET: &str = "0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0";
/// A second public test wallet and its key (the web3 documentation example).
const OTHER_KEY: &str = "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318";
const OTHER: &str = "0x2c7536E3605D9C16a7a3D7b1898e529396a65c23";
/// A valid address no fixture key signs for.
const STRANGER: &str = "0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e";

struct H {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    signed: Vec<HttpRequest>,
    logs: Vec<String>,
}

impl H {
    fn with(engine: Engine<AsterDex>) -> Self {
        let mut h = Self {
            engine,
            now: 1_700_000_000_000.0,
            requests: vec![],
            signed: vec![],
            logs: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        if h.requests.iter().any(|r| r.url.ends_with("/time")) {
            h.reply("/time", r#"{"serverTime":1700000000000}"#);
        }
        h.reply("/exchangeInfo", MARKETS);
        h.reply("/leverageBracket", "[]");
        h.reply(
            "/ticker/24hr",
            r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#,
        );
        h.reply("/openInterest", "[]");
        h
    }
    fn start(options: Options) -> Self {
        Self::with(support::engine(options))
    }
    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => {
                    if request.url.contains("signer=") || request.url.contains("signature=") {
                        self.signed.push(request.clone());
                    }
                    self.requests.push(request)
                }
                Action::Notice(Notice::Event(event)) => self.logs.push(event.message),
                _ => {}
            }
        }
    }
    fn reply(&mut self, suffix: &str, body: &str) {
        let Some(position) = self
            .requests
            .iter()
            .position(|r| r.url.split('?').next().unwrap().ends_with(suffix))
        else {
            return;
        };
        let request = self.requests.remove(position);
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    fn param<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
        request
            .url
            .split(['?', '&'])
            .find_map(|pair| pair.strip_prefix(name)?.strip_prefix('='))
    }
    fn signers(&self) -> Vec<&str> {
        self.signed.iter().filter_map(|r| Self::param(r, "signer")).collect()
    }
    fn nonces(&self) -> Vec<u64> {
        self.signed
            .iter()
            .filter_map(|r| Self::param(r, "nonce")?.parse().ok())
            .collect()
    }
    fn keys(&self) -> Vec<&str> {
        self.signed
            .iter()
            .flat_map(|r| r.headers.iter())
            .filter(|(name, _)| name.eq_ignore_ascii_case("X-MBX-APIKEY"))
            .map(|(_, value)| value.as_str())
            .collect()
    }
}

fn wallet() -> Options {
    Options {
        application_id: Some(STRANGER.into()),
        ..Options::v3_wallet()
    }
}

#[test]
fn n1_mismatched_configured_signer_is_refused_at_startup_naming_both_addresses() {
    let h = H::start(Options {
        auth_signer_address: Some(OTHER.into()),
        ..wallet()
    });
    assert!(h.signed.is_empty(), "nothing signs: {:?}", h.signers());
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains(OTHER) && log.contains(WALLET) && log.contains("does not match")),
        "{:?}",
        h.logs
    );
}

#[test]
fn n1_mismatched_read_agent_refuses_only_its_reads() {
    let h = H::start(Options {
        read_key: Some(WALLET.into()),
        read_secret: Some(OTHER_KEY.into()),
        ..Options {
            key: Some(OTHER.into()),
            secret: Some(OTHER_KEY.into()),
            ..wallet()
        }
    });
    // The read slot claims the fixture wallet but its key signs for OTHER.
    assert!(!h.keys().contains(&WALLET), "no read signs: {:?}", h.keys());
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains(WALLET) && log.contains(OTHER) && log.contains("does not match")),
        "{:?}",
        h.logs
    );
}

#[test]
fn v3_private_work_without_address_setup_is_refused_not_sent() {
    let (venue, signer) = wallet().build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(signer)).unwrap();
    let h = H::with(engine);
    assert!(h.signed.is_empty(), "nothing signs: {:?}", h.signers());
}

// Controls: green before and after adoption.

#[test]
fn wire_signer_is_the_checksummed_wallet_and_nonces_strictly_increase() {
    let h = H::start(Options {
        auth_signer_address: Some(WALLET.to_ascii_lowercase()),
        ..wallet()
    });
    assert!(!h.signed.is_empty());
    assert!(h.signers().iter().all(|signer| *signer == WALLET), "{:?}", h.signers());
    let nonces = h.nonces();
    assert!(nonces.windows(2).all(|pair| pair[0] < pair[1]), "{nonces:?}");
    assert!(nonces.iter().all(|nonce| *nonce >= 1_700_000_000_000_000));
}

#[test]
fn an_aliased_read_slot_shares_the_wallet_sequence() {
    let h = H::start(Options {
        auth_private_key: Some("4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1".into()),
        auth_signer_address: Some(WALLET.into()),
        read_key: Some("0x0000000000000000000000000000000000000001".into()),
        read_secret: Some("unused".into()),
        ..wallet()
    });
    assert!(h.signed.len() >= 2);
    assert!(h.signers().iter().all(|signer| *signer == WALLET));
    let nonces = h.nonces();
    assert!(nonces.windows(2).all(|pair| pair[0] < pair[1]), "{nonces:?}");
}

#[test]
fn v1_hmac_accounts_carry_no_nonce() {
    let h = H::start(Options::v1_with_read_agent());
    assert!(!h.signed.is_empty());
    assert!(h.nonces().is_empty());
    assert!(h.signers().is_empty());
}
