//! Actual private-read dispatch, credential attribution and typed envelope controls.
mod support;
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Event, HttpRequest, Notice, Response,
};
use support::Options;
const MARKETS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true,"contractSize":0.0001,"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500,"makerFeeRate":0,"takerFeeRate":0.0002}]}"#;
const TICKERS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","lastPrice":60000,"fairPrice":60001,"indexPrice":59999,"bid1":59999.9,"ask1":60000,"riseFallRate":0.01,"holdVol":10000,"fundingRate":0.0001,"volume24":50000,"amount24":300000}]}"#;
fn drain(e: &mut support::TestEngine) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, 1000.)
}
fn http(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn reply(e: &mut support::TestEngine, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        1000.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    drain(e)
}
fn auth(actions: &[Action]) -> Vec<(CredentialSlot, AccountReadScope, AuthOutcome)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::AccountAuth {
                credential,
                scope,
                outcome,
            }) => Some((*credential, *scope, *outcome)),
            _ => None,
        })
        .collect()
}
fn started(read: bool) -> (support::TestEngine, Vec<Action>) {
    let mut e = support::configured(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Options::default()
    });
    e.start(1000.);
    let a = drain(&mut e);
    let r = http(&a, "contract/detail");
    let a = reply(&mut e, &r, 200, MARKETS);
    assert!(auth(&a).is_empty());
    // The position mode read leaves with the ticker seed.
    let ticker = http(&a, "contract/ticker");
    let mode = http(&a, "position_mode");
    let a = reply(&mut e, &ticker, 200, TICKERS);
    assert!(auth(&a).is_empty());
    let a = reply(&mut e, &mode, 200, r#"{"success":true,"code":0,"data":2}"#);
    assert_eq!(
        auth(&a),
        vec![(
            if read {
                CredentialSlot::Secondary
            } else {
                CredentialSlot::Primary
            },
            AccountReadScope::Settings,
            AuthOutcome::Accepted
        )]
    );
    (e, a)
}
#[test]
fn actual_private_read_reports_selected_signer_once_and_public_reads_stay_silent() {
    for read in [false, true] {
        let (mut e, a) = started(read);
        assert!(
            auth(&a).is_empty()
                || matches!(
                    auth(&a).as_slice(),
                    [(_, AccountReadScope::Settings, AuthOutcome::Accepted)]
                )
        );
        let r = http(&a, "account/assets");
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("ApiKey")
                    && v == if read { "read-key" } else { "primary-key" })
        );
        let a = reply(&mut e, &r, 200, r#"{"success":true,"code":0,"data":[]}"#);
        assert_eq!(
            auth(&a),
            vec![(
                if read {
                    CredentialSlot::Secondary
                } else {
                    CredentialSlot::Primary
                },
                AccountReadScope::Balance,
                AuthOutcome::Accepted
            )]
        );
        assert!(
            auth(&reply(
                &mut e,
                &r,
                200,
                r#"{"success":true,"code":0,"data":[]}"#
            ))
            .is_empty()
        );
    }
}
#[test]
fn typed_auth_rejection_on_missing_stream_list_is_observed_but_business_rate_and_malformed_are_not()
{
    for code in [401, 402, 406, 602, 701, 703] {
        let (mut e, a) = started(true);
        let r = http(&a, "account/assets");
        let body = format!(r#"{{"success":false,"code":{code}}}"#);
        assert_eq!(
            auth(&reply(&mut e, &r, 200, &body)),
            vec![(
                CredentialSlot::Secondary,
                AccountReadScope::Balance,
                AuthOutcome::Rejected
            )]
        );
    }
    for (status, body) in [
        (200, r#"{"success":false,"code":513,"message":"602"}"#),
        (429, r#"{"success":false,"code":510}"#),
        (200, r#"{"success":true,"code":0}"#),
        (401, r#"<html>unauthorized</html>"#),
    ] {
        let (mut e, a) = started(true);
        let r = http(&a, "account/assets");
        assert!(
            auth(&reply(&mut e, &r, status, body)).is_empty(),
            "{status} {body}"
        );
    }
}

#[test]
fn a_rejected_order_endpoint_is_not_reset_by_its_sibling_order_reads() {
    let (mut e, a) = started(false);
    let ok = r#"{"success":true,"code":0,"data":[]}"#;
    let refused = r#"{"success":false,"code":701,"message":"Access denied"}"#;
    let orders = |a: &[Action]| -> Vec<AuthOutcome> {
        auth(a)
            .into_iter()
            .filter(|(_, scope, _)| *scope == AccountReadScope::Orders)
            .map(|(_, _, outcome)| outcome)
            .collect()
    };
    let regular = http(&a, "order/list/open_orders");
    let plan = http(&a, "planorder/list/orders");
    let stop = http(&a, "stoporder/list/orders");
    assert_eq!(
        orders(&reply(&mut e, &regular, 200, ok)),
        [AuthOutcome::Accepted]
    );
    assert_eq!(
        orders(&reply(&mut e, &plan, 200, refused)),
        [AuthOutcome::Rejected]
    );
    // Plan orders still refuse the key, so stop orders' success is no evidence.
    assert!(orders(&reply(&mut e, &stop, 200, ok)).is_empty());
}

#[test]
fn row_failures_do_not_withhold_successful_envelope_evidence() {
    for read in [false, true] {
        for (path, scope) in [
            ("account/assets", AccountReadScope::Balance),
            ("position/open_positions", AccountReadScope::Positions),
            ("order/list/open_orders", AccountReadScope::Orders),
            ("planorder/list/orders", AccountReadScope::Orders),
            ("stoporder/list/orders", AccountReadScope::Orders),
        ] {
            let (mut e, all) = started(read);
            let r = http(&all, path);
            let a = reply(
                &mut e,
                &r,
                200,
                r#"{"success":true,"code":0,"data":[false]}"#,
            );
            assert_eq!(
                auth(&a),
                [(
                    if read {
                        CredentialSlot::Secondary
                    } else {
                        CredentialSlot::Primary
                    },
                    scope,
                    AuthOutcome::Accepted
                )],
                "{path}"
            );
        }
    }
}

#[test]
fn accepted_credentials_still_commit_good_asset_peers_and_report_bad_rows() {
    let (mut e, all) = started(true);
    let r = http(&all, "account/assets");
    let a = reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"code":0,"data":[false,{"currency":"USDT","availableBalance":40,"equity":42}]}"#,
    );
    assert_eq!(
        auth(&a),
        [(
            CredentialSlot::Secondary,
            AccountReadScope::Balance,
            AuthOutcome::Accepted
        )]
    );
    assert_eq!(e.store().balance().total, 42.0);
    assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::Event(e)) if e.code == safe_cex_engine::log::code::ROW_SKIPPED)));
}

#[test]
fn independent_depth_queries_never_emit_auth_evidence_with_private_read_credentials() {
    use safe_cex_engine::{Command, ErrorKind};
    for read in [false, true] {
        for (status, body) in [
            (
                200,
                r#"{"success":true,"code":0,"data":{"version":1,"bids":[],"asks":[]}}"#,
            ),
            (
                401,
                r#"{"success":false,"code":602,"message":"invalid api key"}"#,
            ),
            (
                403,
                r#"{"success":false,"code":701,"message":"permission denied"}"#,
            ),
            (
                200,
                r#"{"success":false,"code":602,"message":"invalid api key"}"#,
            ),
        ] {
            let (mut e, _) = started(read);
            let failures = e.health().auth_failures;
            e.handle(
                1000.,
                Event::Command(Command::FetchOrderBook {
                    command: 91,
                    symbol: "BTCUSDT".into(),
                    limit: Some(1),
                }),
            );
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.);
            let query = http(&actions, "contract/depth");
            assert!(auth(&actions).is_empty());
            assert!(query.headers.iter().all(|(name, _)| !matches!(
                name.to_ascii_lowercase().as_str(),
                "apikey" | "signature" | "request-time"
            )));
            let actions = reply(&mut e, &query, status, body);
            assert!(
                auth(&actions).is_empty(),
                "read={read} status={status} body={body}"
            );
            assert_eq!(e.health().auth_failures, failures);
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(
                        a,
                        Action::Notice(
                            Notice::CommandDone { command: 91, .. }
                                | Notice::CommandFailed { command: 91, .. }
                        )
                    ))
                    .count(),
                1
            );
            if status == 200 && body.contains("\"success\":true") {
                assert!(
                    actions.iter().any(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandDone { command: 91, .. })
                    ))
                );
            }
            if body.contains("invalid api key") || status == 403 {
                assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, error }) if error.kind == Some(ErrorKind::VenueUnavailable))));
            }
        }
    }
}
