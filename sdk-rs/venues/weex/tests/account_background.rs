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
    other: Vec<Action>,
}
fn drain(e: &mut Engine<WeexFutures>, releases: &mut Releases) {
    drain_at(e, releases, NOW);
}
fn drain_at(e: &mut Engine<WeexFutures>, releases: &mut Releases, now: f64) {
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
                        now,
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
                    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
                    releases.claimed += usize::from(account(&request.url));
                    releases.requests.push(request);
                }
                HttpAction::BindGroup { .. } => {
                    panic!("WEEX background reads declare no Mutation barriers")
                }
                HttpAction::Cancel { .. } => {}
            },
            action => releases.other.push(action),
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
    startup_with_refusals(secondary, false)
}
fn startup_with_refusals(secondary: bool, refuse: bool) -> (Engine<WeexFutures>, Releases) {
    let options = Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        read_credentials: secondary.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    };
    let mut e = if refuse {
        support::setup::engine_with_provider(options, |inner| {
            std::sync::Arc::new(support::setup::Gated::new(inner, &[false; 6]))
        })
    } else {
        support::setup::engine(options)
    };
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

fn no_auth_or_deactivation(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(
            safe_cex_engine::Notice::AccountAuth { .. } | safe_cex_engine::Notice::Deactivate
        )
    )));
}
fn recover_poll(e: &mut Engine<WeexFutures>, releases: &Releases, origin: f64, expected: usize) {
    let (timer, at) = releases
        .other
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if (origin + 3000. ..=origin + 4000.).contains(at_ms) =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("bounded failed-cycle recovery poll");
    e.handle(at, Event::TimerFired { timer });
    let mut next = Releases::default();
    drain_at(e, &mut next, at);
    assert_eq!(
        (next.legacy, next.prepared, next.claimed),
        (0, expected, expected)
    );
    assert_eq!(
        next.requests.iter().filter(|r| account(&r.url)).count(),
        expected
    );
    for request in next.requests.iter().filter(|r| account(&r.url)) {
        assert!(
            request
                .headers
                .iter()
                .any(|(key, value)| *key == "ACCESS-SIGN" && !value.is_empty())
        );
        if expected == 4 {
            assert!(
                !request.url.contains("Config"),
                "native settings backoff stays independent"
            );
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(key, value)| *key == "ACCESS-KEY" && value == "read-key")
            );
        }
    }
}
#[test]
fn provider_refusal_is_pre_send_and_next_poll_uses_the_normal_recipe() {
    let (mut e, releases) = startup_with_refusals(true, true);
    assert_eq!(
        (releases.legacy, releases.prepared, releases.claimed),
        (0, 6, 0)
    );
    assert!(releases.requests.iter().all(|r| !account(&r.url)));
    no_auth_or_deactivation(&releases.other);
    assert_eq!(e.health().auth_failures, 0);
    recover_poll(&mut e, &releases, NOW, 4);
}
#[test]
fn transport_failures_never_retry_the_attempt_and_allow_next_poll() {
    let (mut e, releases) = startup(true);
    let mut failed = Releases::default();
    for r in releases.requests.iter().filter(|r| account(&r.url)) {
        e.handle(
            NOW + 1.,
            Event::HttpFailed {
                request: r.id,
                error: "offline",
            },
        );
        drain_at(&mut e, &mut failed, NOW + 1.);
    }
    assert_eq!((failed.prepared, failed.claimed, failed.legacy), (0, 0, 0));
    no_auth_or_deactivation(&failed.other);
    recover_poll(&mut e, &failed, NOW + 1., 4);
}
#[test]
fn physical_timeouts_and_late_rows_do_not_publish_or_latch_the_cycle() {
    let (mut e, releases) = startup(false);
    let timers: Vec<_> = releases
        .other
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .collect();
    assert!(
        timers.len() >= 7,
        "native cycle plus six managed physical timers"
    );
    let mut expired = Releases::default();
    for timer in timers {
        e.handle(NOW + 20_000., Event::TimerFired { timer });
        drain_at(&mut e, &mut expired, NOW + 20_000.);
    }
    for r in releases.requests.iter().filter(|r| account(&r.url)) {
        e.handle(
            NOW + 20_001.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"[{"asset":"USDT","balance":"999","availableBalance":"999"}]"#,
                },
            },
        );
        drain_at(&mut e, &mut expired, NOW + 20_001.);
    }
    assert_eq!(e.store().balance().free, 0.);
    assert!(!e.store().loaded().balance);
    assert_eq!(
        (expired.prepared, expired.claimed, expired.legacy),
        (0, 0, 0)
    );
    no_auth_or_deactivation(&expired.other);
    recover_poll(&mut e, &expired, NOW + 20_000., 6);
}
