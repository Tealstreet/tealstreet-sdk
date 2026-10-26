//! RUST_ERROR_CLASSIFICATION.md P1 #33: an account read's failure kind comes
//! from the shared code table, as a write's does. HTTP status classifies a
//! non-2xx answer, and 10007 is an auth refusal on reads and writes alike.
use safe_cex_engine::{Action, Event, Notice, Response, Value};
mod support;

/// The kind on the read-failure notice for one balance answer.
fn kind(status: u16, body: &str) -> Option<String> {
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-material".into()),
        ..support::Options::default()
    });
    e.start(0.0);
    let mut answered = false;
    for _ in 0..20 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.0);
        if answered {
            return actions.into_iter().find_map(|a| match a {
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::REQUEST_FAILED
                        && event.message.contains("account read") =>
                {
                    event.fields.iter().find_map(|(name, value)| match value {
                        Value::Str(kind) if *name == "kind" => Some(kind.to_string()),
                        _ => None,
                    })
                }
                _ => None,
            });
        }
        for a in actions {
            let Action::Http(r) = a else { continue };
            let path = r.url.split('?').next().unwrap();
            let (status, body) = if path.ends_with("/api/v1/futures/account") {
                answered = true;
                (status, body)
            } else if path.contains("trading_pairs") {
                (200, support::PAIRS)
            } else if path.contains("tickers") {
                (200, support::TICKERS)
            } else {
                continue;
            };
            e.handle(
                0.0,
                Event::HttpResponse {
                    request: r.id,
                    response: Response {
                        status,
                        headers: "",
                        body,
                    },
                },
            );
        }
    }
    panic!("no balance read")
}

#[test]
fn a_non_2xx_account_read_is_classified_by_its_status() {
    assert_eq!(kind(401, "{}").as_deref(), Some("authInvalid"));
    assert_eq!(kind(403, "{}").as_deref(), Some("authPermission"));
    assert_ne!(kind(404, "{}").as_deref(), Some("network"));
}

#[test]
fn a_10007_account_read_is_an_auth_refusal_as_on_writes() {
    assert_eq!(
        kind(200, r#"{"code":10007,"msg":"Sign signature error"}"#).as_deref(),
        Some("authInvalid")
    );
}
