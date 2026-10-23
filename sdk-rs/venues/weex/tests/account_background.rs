//! Account-cycle release contract through the native startup and signer.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, ReadCredentials, Response};
use safe_cex_venue_weex::WeexFutures;
use std::collections::VecDeque;
use support::setup::Options;
const NOW: f64 = 1_773_295_700_000.;
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
fn account(url: &str) -> bool {
    url.contains("/capi/v3/account/") || url.contains("/capi/v3/open")
}
#[derive(Default)]
struct Releases {
    requests: Vec<HttpRequest>,
    prepared: usize,
    claimed: usize,
    legacy: usize,
}
fn drain(e: &mut Engine<WeexFutures>, releases: &mut Releases) {
    let mut queue: VecDeque<_> = e.drain_actions().collect();
    while let Some(action) = queue.pop_front() {
        match action {
            Action::Http(r) => {
                releases.legacy += usize::from(account(&r.url));
                releases.requests.push(r);
            }
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(e.http_preparation_pending(preparation));
                    releases.prepared += usize::from(account(&request.url));
                    e.http_prepared(
                        NOW,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation.attempt().request, request.id);
                    assert!(matches!(e.claim_http(NOW, permit), Ok(Claim::Released(_))));
                    releases.claimed += usize::from(account(&request.url));
                    releases.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
            },
            _ => {}
        }
        queue.extend(e.drain_actions());
    }
}
fn respond(e: &mut Engine<WeexFutures>, r: &HttpRequest, body: &str, releases: &mut Releases) {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    drain(e, releases);
}
fn startup(secondary: bool) -> (Engine<WeexFutures>, Releases) {
    let mut e = support::setup::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: secondary.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    e.start(NOW);
    let mut releases = Releases::default();
    drain(&mut e, &mut releases);
    let info = releases
        .requests
        .iter()
        .find(|r| r.url.contains("exchangeInfo"))
        .unwrap()
        .clone();
    respond(&mut e, &info, INFO, &mut releases);
    let public: Vec<_> = releases
        .requests
        .iter()
        .filter(|r| r.url.contains("24hr") || r.url.contains("bookTicker"))
        .cloned()
        .collect();
    assert_eq!(public.len(), 2);
    for r in public {
        respond(
            &mut e,
            &r,
            r#"[{"symbol":"BTCUSDT","lastPrice":"90000","bidPrice":"89999","askPrice":"90001"}]"#,
            &mut releases,
        );
    }
    (e, releases)
}
fn native_positive(secondary: bool) -> Releases {
    let (mut e, mut releases) = startup(secondary);
    let reads: Vec<_> = releases
        .requests
        .iter()
        .filter(|r| account(&r.url))
        .cloned()
        .collect();
    assert_eq!(reads.len(), 6);
    for r in reads {
        assert_eq!(r.method, safe_cex_engine::Method::Get);
        assert!(r.body.is_none());
        let header = |name| {
            r.headers
                .iter()
                .find(|(k, _)| *k == name)
                .unwrap()
                .1
                .as_str()
        };
        assert_eq!(
            header("ACCESS-KEY"),
            if secondary { "read-key" } else { "primary-key" }
        );
        assert_eq!(header("ACCESS-PASSPHRASE"), "primary-pass");
        assert!(!header("ACCESS-SIGN").is_empty());
        if r.url.contains("/openOrders") {
            assert!(r.url.ends_with("?limit=100&page=0"));
        }
        if r.url.contains("/openAlgoOrders") {
            assert!(r.url.ends_with("?limit=100&page=1"));
        }
        let body = if r.url.contains("accountConfig") {
            r#"{"dualSidePosition":false,"canTrade":true}"#
        } else if r.url.contains("/balance") {
            r#"[{"asset":"USDT","balance":"100","availableBalance":"90","frozen":"10","unrealizePnl":"0"}]"#
        } else {
            "[]"
        };
        respond(&mut e, &r, body, &mut releases);
    }
    assert_eq!(e.store().balance().free, 90.);
    assert!(
        e.store().loaded().orders && e.store().loaded().balance && e.store().loaded().positions
    );
    releases
}
#[test]
fn native_account_recipe_and_snapshot_control() {
    native_positive(false);
    native_positive(true);
}
#[test]
fn primary_account_cycle_requires_managed_release() {
    let r = native_positive(false);
    assert_eq!((r.legacy, r.prepared, r.claimed), (0, 6, 6));
}
#[test]
fn secondary_account_cycle_requires_managed_release() {
    let r = native_positive(true);
    assert_eq!((r.legacy, r.prepared, r.claimed), (0, 6, 6));
}
