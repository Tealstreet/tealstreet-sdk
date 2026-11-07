//! Native POST selection stays Primary even when a read key is configured.
mod support;
use safe_cex_engine::{Action, Command, Event, Method, Query, ReadCredentials};
use support::fixture::{self, Options};
#[test]
fn posted_read_keeps_primary_identity_with_secondary_configured() {
    let mut engine = fixture::engine(Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    });
    engine.handle(
        1700000000000.,
        Event::Command(Command::Query {
            command: 42,
            query: Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(100),
            },
        }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 1700000000000.);
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::Post);
    let expected = r#"{"id":1700000000000,"method":"private/get-trades","api_key":"primary-key","params":{"end_time":1700000000000000000,"limit":100,"start_time":1699913600000000000},"sig":"a10b8a4f0116285d81f5a67607ee9e35620cdd2f6b469023b36a810987aeb69c","nonce":1700000000000}"#;
    assert_eq!(requests[0].body.as_deref(), Some(expected));
    assert_eq!(requests[0].route, None);
}
