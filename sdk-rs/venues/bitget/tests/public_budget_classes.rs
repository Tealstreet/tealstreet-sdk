mod support;
use safe_cex_engine::{
    Action, Event, Response,
    request_budget::RequestClass,
    request_context::{Claim, HttpAction, Prepared},
};
#[test]
fn catalogue_and_ticker_classes_are_declared_at_issuance() {
    let mut engine = support::engine(support::FixtureOptions {
        public_data_only: true,
        ..Default::default()
    });
    engine.start(1_000.);
    for (path, class, budget, body) in [
        (
            "/contracts",
            RequestClass::Discovery,
            90_000,
            support::CONTRACTS,
        ),
        ("/tickers", RequestClass::Read, 20_000, support::TICKERS),
    ] {
        let (token, request) = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.contains(path) => Some((preparation, request)),
                    _ => None,
                },
                _ => None,
            })
            .expect("native read preparation");
        assert_eq!(engine.http_request_class(token), Some(class));
        assert_eq!(request.timeout_ms, Some(budget));
        engine.http_prepared(
            1_000.,
            token,
            Prepared {
                id: request.id.0,
                expires_at: None,
            },
        );
        let permit = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("native read binding");
        assert!(matches!(
            engine.claim_http(1_000., permit),
            Ok(Claim::Released(_))
        ));
        engine.handle(
            1_000.,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
}
