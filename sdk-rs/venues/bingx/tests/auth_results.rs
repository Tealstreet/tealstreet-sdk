//! Actual private-read dispatch, credential attribution and typed envelope controls.
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Engine, Event, HttpRequest, Notice,
    Response,
};
use safe_cex_venue_bingx::Bingx;
mod support;
use support::Options;
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const DAILY: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;
fn drain(e: &mut Engine<Bingx>) -> Vec<Action> {
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
fn reply(e: &mut Engine<Bingx>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
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
fn started(read: bool) -> (Engine<Bingx>, Vec<Action>) {
    let mut e = support::engine(Options {
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
    // Private reads may start before the public seed completes, so keep every startup action.
    let mut all = drain(&mut e);
    let r = http(&all, "/quote/contracts");
    let a = reply(&mut e, &r, 200, MARKETS);
    assert!(auth(&a).is_empty());
    all.extend(a);
    let daily = http(&all, "/quote/ticker");
    let a = reply(&mut e, &daily, 200, DAILY);
    assert!(auth(&a).is_empty());
    all.extend(a);
    let premium = http(&all, "/quote/premiumIndex");
    all.extend(reply(&mut e, &premium, 200, PREMIUM));
    (e, all)
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
        let r = http(&a, "/user/balance");
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("X-BX-APIKEY")
                    && v == if read { "read-key" } else { "primary-key" })
        );
        let a = reply(&mut e, &r, 200, r#"{"code":0,"data":[]}"#);
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
        assert!(auth(&reply(&mut e, &r, 200, r#"{"code":0,"data":[]}"#)).is_empty());
    }
}
#[test]
fn typed_auth_rejection_on_missing_stream_list_is_observed_but_business_rate_and_malformed_are_not()
{
    for code in [100001, 100004, 100413, 100419] {
        let (mut e, a) = started(true);
        let mut r = http(&a, "/user/balance");
        let body = format!(r#"{{"code":{code},"msg":"opaque"}}"#);
        if code == 100001 {
            // A signature rejection is re-signed once; only its answer is evidence.
            let resent = reply(&mut e, &r, 200, &body);
            assert!(auth(&resent).is_empty());
            r = http(&resent, "/user/balance");
        }
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
        (200, r#"{"code":109400,"msg":"100001"}"#),
        (429, r#"{"code":100410}"#),
        (200, r#"{"code":0}"#),
        (401, r#"<html>unauthorized</html>"#),
    ] {
        let (mut e, a) = started(true);
        let r = http(&a, "/user/balance");
        assert!(
            auth(&reply(&mut e, &r, status, body)).is_empty(),
            "{status} {body}"
        );
    }
}

fn with_account_lists(read: bool) -> (Engine<Bingx>, Vec<Action>) {
    let (mut e, mut all) = started(read);
    let mode = http(&all, "positionSide/dual");
    all.extend(reply(
        &mut e,
        &mode,
        200,
        r#"{"code":0,"data":{"dualSidePosition":false}}"#,
    ));
    (e, all)
}

#[test]
fn row_failures_do_not_withhold_successful_envelope_evidence() {
    for read in [false, true] {
        for (path, scope, data) in [
            ("/user/balance", AccountReadScope::Balance, "[false]"),
            ("/user/positions", AccountReadScope::Positions, "[false]"),
            ("/trade/openOrders", AccountReadScope::Orders, "[false]"),
            (
                "/trade/openOrders",
                AccountReadScope::Orders,
                r#"{"orders":[false]}"#,
            ),
        ] {
            let (mut e, all) = with_account_lists(read);
            let r = http(&all, path);
            let a = reply(&mut e, &r, 200, &format!(r#"{{"code":0,"data":{data}}}"#));
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
                "{path} {data}"
            );
        }
    }
}

#[test]
fn missing_or_malformed_required_lists_never_accept_credentials() {
    for path in ["/user/balance", "/user/positions", "/trade/openOrders"] {
        for body in [
            r#"{"code":0}"#,
            r#"{"code":0,"data":null}"#,
            r#"{"code":0,"data":{}}"#,
            r#"{"code":0,"data":{"orders":null}}"#,
            r#"{"code":0,"data":[false],"#,
        ] {
            let (mut e, all) = with_account_lists(true);
            let r = http(&all, path);
            assert!(
                auth(&reply(&mut e, &r, 200, body)).is_empty(),
                "{path} {body}"
            );
        }
    }
}

#[test]
fn accepted_credentials_do_not_change_the_existing_bad_balance_snapshot_policy() {
    let (mut e, all) = with_account_lists(true);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 17.0,
        ..Default::default()
    });
    let r = http(&all, "/user/balance");
    let a = reply(
        &mut e,
        &r,
        200,
        r#"{"code":0,"data":[false,{"asset":"USDT","balance":42,"availableMargin":40,"usedMargin":2,"unrealizedProfit":0}]}"#,
    );
    assert_eq!(
        auth(&a),
        [(
            CredentialSlot::Secondary,
            AccountReadScope::Balance,
            AuthOutcome::Accepted
        )]
    );
    assert_eq!(e.store().balance().total, 17.0);
    assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::Event(e)) if e.code == safe_cex_engine::log::code::SNAPSHOT_DISCARDED)));
}
