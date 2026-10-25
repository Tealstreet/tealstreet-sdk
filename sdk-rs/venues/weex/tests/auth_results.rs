//! Actual private-read dispatch, credential attribution and typed envelope controls.
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Engine, Event, HttpRequest, Notice,
    Response,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::PreparedActions;
use support::setup::Options;
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
fn drain(e: &mut Engine<WeexFutures>) -> Vec<Action> {
    e.prepared_actions(1000.).collect()
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
fn reply(e: &mut Engine<WeexFutures>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
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
fn started(read: bool) -> (Engine<WeexFutures>, Vec<Action>) {
    let mut e = support::setup::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        passphrase: Some("primary-pass".into()),
        ..Options::default()
    });
    e.start(1000.);
    let a = drain(&mut e);
    let r = http(&a, "exchangeInfo");
    let a = reply(&mut e, &r, 200, INFO);
    assert!(auth(&a).is_empty());
    let daily = http(&a, "24hr");
    let book = http(&a, "bookTicker");
    let a = reply(
        &mut e,
        &daily,
        200,
        r#"[{"symbol":"BTCUSDT","lastPrice":"90000"}]"#,
    );
    assert!(auth(&a).is_empty());
    let a = reply(
        &mut e,
        &book,
        200,
        r#"[{"symbol":"BTCUSDT","bidPrice":"89999","askPrice":"90001"}]"#,
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
        let r = http(&a, "/balance");
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("ACCESS-KEY")
                    && v == if read { "read-key" } else { "primary-key" })
        );
        let a = reply(&mut e, &r, 200, r#"[]"#);
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
        assert!(auth(&reply(&mut e, &r, 200, r#"[]"#)).is_empty());
    }
}
#[test]
fn typed_auth_rejection_on_missing_stream_list_is_observed_but_business_rate_and_malformed_are_not()
{
    for code in [40006, 40009, 40012, 40014, 40018] {
        let (mut e, a) = started(true);
        let r = http(&a, "/balance");
        let body = format!(r#"{{"code":"{code}","msg":"opaque"}}"#);
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
        (400, r#"{"code":40008,"msg":"40009"}"#),
        (429, r#"[]"#),
        (200, r#"{}"#),
        (401, r#"<html>unauthorized</html>"#),
    ] {
        let (mut e, a) = started(true);
        let r = http(&a, "/balance");
        assert!(
            auth(&reply(&mut e, &r, status, body)).is_empty(),
            "{status} {body}"
        );
    }
    // Undecodable rows are skipped individually; the signed array still proves
    // the read key was accepted.
    let (mut e, a) = started(true);
    let r = http(&a, "/balance");
    assert_eq!(
        auth(&reply(&mut e, &r, 200, r#"[false]"#)),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Balance,
            AuthOutcome::Accepted
        )]
    );
}

#[test]
fn a_rejected_order_endpoint_is_not_reset_by_its_sibling_order_read() {
    let (mut e, a) = started(false);
    let orders = |a: &[Action]| -> Vec<AuthOutcome> {
        auth(a)
            .into_iter()
            .filter(|(_, scope, _)| *scope == AccountReadScope::Orders)
            .map(|(_, _, outcome)| outcome)
            .collect()
    };
    let algo = http(&a, "openAlgoOrders");
    let normal = http(&a, "/openOrders");
    assert_eq!(
        orders(&reply(
            &mut e,
            &algo,
            200,
            r#"{"code":40014,"msg":"Permission denied"}"#
        )),
        [AuthOutcome::Rejected]
    );
    // Algo orders still refuse the key, so normal orders' success is no evidence.
    assert!(orders(&reply(&mut e, &normal, 200, "[]")).is_empty());
}

#[test]
fn row_failures_do_not_withhold_successful_envelope_evidence() {
    for read in [false, true] {
        for (path, scope) in [
            ("/balance", AccountReadScope::Balance),
            ("/position", AccountReadScope::Positions),
            ("/openOrders", AccountReadScope::Orders),
            ("openAlgoOrders", AccountReadScope::Orders),
        ] {
            let (mut e, all) = started(read);
            let r = http(&all, path);
            let a = reply(&mut e, &r, 200, "[false]");
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
fn accepted_credentials_still_commit_good_balance_peers_and_report_bad_rows() {
    let (mut e, all) = started(true);
    let r = http(&all, "/balance");
    let a = reply(
        &mut e,
        &r,
        200,
        r#"[false,{"asset":"USDT","balance":42,"availableBalance":40}]"#,
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
