mod support;
use safe_cex_engine::signing_policy::PrivateFeedPolicy;
use safe_cex_engine::{Action, Event};
use support::{INSTRUMENTS, TICKERS, fixture};

#[test]
fn payload_signer_keeps_signed_rest_reconciliation_without_opening_private_ws() {
    let mut engine = fixture::engine(fixture::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    });
    engine
        .set_private_feed_policy(PrivateFeedPolicy::RestOnly)
        .unwrap();
    let now = 1_610_000_000_000.;
    engine.start(now);
    let mut observed = vec![];
    for (path, body) in [
        ("public/get-instruments", INSTRUMENTS),
        ("public/get-tickers", TICKERS),
    ] {
        let actions = fixture::actions(&mut engine, now);
        let request = actions
            .iter()
            .find_map(|action| match action {
                Action::Http(r) if r.url.contains(path) => Some(r),
                _ => None,
            })
            .unwrap();
        engine.handle(
            now,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        observed.extend(actions);
    }
    observed.extend(fixture::actions(&mut engine, now));
    assert!(observed.iter().any(|a| matches!(a, Action::Http(r) if r.url.contains("private/") && r.body.as_ref().is_some_and(|b| b.contains("\"sig\"")))));
    assert!(
        !observed
            .iter()
            .any(|a| matches!(a, Action::WsConnect { url, .. } if url.ends_with("/user")))
    );
}
