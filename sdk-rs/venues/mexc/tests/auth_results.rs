//! Actual private-read dispatch, credential attribution and typed envelope controls.
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Engine, Event, HttpRequest, Notice,
    Response,
};
use safe_cex_venue_mexc::{MexcFutures, Options};
const MARKETS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true,"contractSize":0.0001,"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500,"makerFeeRate":0,"takerFeeRate":0.0002}]}"#;
const TICKERS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","lastPrice":60000,"fairPrice":60001,"indexPrice":59999,"bid1":59999.9,"ask1":60000,"riseFallRate":0.01,"holdVol":10000,"fundingRate":0.0001,"volume24":50000,"amount24":300000}]}"#;
fn drain(e: &mut Engine<MexcFutures>) -> Vec<Action> {
    e.drain_actions().collect()
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
fn reply(e: &mut Engine<MexcFutures>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
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
fn started(read: bool) -> (Engine<MexcFutures>, Vec<Action>) {
    let mut e = Engine::new(MexcFutures::new(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Options::default()
    }));
    e.start(1000.);
    let a = drain(&mut e);
    let r = http(&a, "contract/detail");
    let a = reply(&mut e, &r, 200, MARKETS);
    assert!(auth(&a).is_empty());
    let r = http(&a, "contract/ticker");
    let a = reply(&mut e, &r, 200, TICKERS);
    assert!(auth(&a).is_empty());
    let r = http(&a, "position_mode");
    let a = reply(&mut e, &r, 200, r#"{"success":true,"code":0,"data":2}"#);
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
        (200, r#"{"success":true,"code":0,"data":[false]}"#),
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
