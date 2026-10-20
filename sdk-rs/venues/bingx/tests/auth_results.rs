//! Actual private-read dispatch, credential attribution and typed envelope controls.
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Engine, Event, HttpRequest, Notice,
    Response,
};
use safe_cex_venue_bingx::{Bingx, Options};
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const DAILY: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;
fn drain(e: &mut Engine<Bingx>) -> Vec<Action> {
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
    let mut e = Engine::new(Bingx::new(Options {
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
    let r = http(&a, "/quote/contracts");
    let a = reply(&mut e, &r, 200, MARKETS);
    assert!(auth(&a).is_empty());
    let daily = http(&a, "/quote/ticker");
    let a = reply(&mut e, &daily, 200, DAILY);
    assert!(auth(&a).is_empty());
    let premium = http(&a, "/quote/premiumIndex");
    let a = reply(&mut e, &premium, 200, PREMIUM);
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
        let r = http(&a, "/user/balance");
        let body = format!(r#"{{"code":{code},"msg":"opaque"}}"#);
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
        (200, r#"{"code":0,"data":[false]}"#),
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
