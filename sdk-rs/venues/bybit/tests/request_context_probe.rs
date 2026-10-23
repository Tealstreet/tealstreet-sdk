//! Slice 1b: the opposite-environment key probe is a background signed Read on
//! the engine's managed lifecycle. Its native behaviour (2 s window, exact
//! hints, one terminal, late answers ignored) stays pinned by startup_auth.rs.
mod support;
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{Action, Event, Response};
use support::{Harness, Options};

const INVALID_KEY: &str =
    r#"{"retCode":10003,"retMsg":"API key is invalid.","result":{},"retExtInfo":{},"time":1}"#;

#[test]
fn the_environment_probe_is_issued_through_managed_prepare() {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let fees = h.take("/v5/account/fee-rate");
    // Drive the engine directly to observe the probe's own lifecycle actions.
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: fees.id,
            response: Response {
                status: 200,
                headers: "",
                body: INVALID_KEY,
            },
        },
    );
    h.engine.prepare_dispatch(h.now);
    let actions: Vec<Action> = h.engine.drain_actions().collect();
    let prepared = actions.iter().any(|action| {
        matches!(action, Action::ManagedHttp(managed)
            if matches!(managed.as_ref(), HttpAction::Prepare { request, .. }
                if request.url.ends_with("/v5/user/query-api")))
    });
    assert!(prepared, "the probe is admitted through managed Prepare");
    assert!(
        !actions.iter().any(|action| matches!(action, Action::Sign { .. })
            || matches!(action, Action::Http(request) if request.url.ends_with("/v5/user/query-api"))),
        "no legacy signing or unmanaged egress for the probe"
    );
}
