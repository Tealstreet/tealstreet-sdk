//! Actual history dispatch keeps read preference and required-primary eligibility.
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, Method, Notice, Query, ReadCredentials, Response,
};
use safe_cex_venue_weex::WeexFutures;
use support::setup::Options;

const NOW: f64 = 1_764_505_700_000.;

fn options(read: bool, public_only: bool) -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-material".into()),
        passphrase: Some("primary-pass".into()),
        public_data_only: public_only,
        read_credentials: read.then(|| ReadCredentials {
            key: "read-key".into(),
            secret: "read-material".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Default::default()
    }
}

fn query(income: bool) -> Query {
    if income {
        Query::Income {
            symbol: None,
            kind: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        }
    } else {
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        }
    }
}

fn ask(engine: &mut Engine<WeexFutures>, income: bool) -> Vec<Action> {
    engine.handle(
        NOW,
        Event::Command(Command::Query {
            command: 7,
            query: query(income),
        }),
    );
    support::managed::drain(engine, NOW)
}

#[test]
fn native_history_roles_keep_exact_provider_headers_and_finish_once() {
    for read in [false, true] {
        for income in [false, true] {
            let mut engine = support::setup::engine(options(read, false));
            let actions = ask(&mut engine, income);
            let requests: Vec<_> = actions
                .iter()
                .filter_map(|action| match action {
                    Action::Http(request) => Some(request),
                    _ => None,
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let request = requests[0];
            assert_eq!(
                request.method,
                if income { Method::Post } else { Method::Get }
            );
            assert_eq!(request.body.as_deref(), income.then_some(r#"{"asset":"","symbol":"","incomeType":"","startTime":null,"endTime":null,"limit":1}"#));
            let secondary = read && !income;
            let path = request
                .url
                .strip_prefix("https://api-contract.weex.com")
                .unwrap();
            assert!(path.starts_with(if income {
                "/capi/v3/account/income"
            } else {
                "/capi/v3/userTrades"
            }));
            assert_eq!(
                request.headers,
                support::setup::oracle(
                    if secondary { "read-key" } else { "primary-key" },
                    if secondary {
                        "read-material"
                    } else {
                        "primary-material"
                    },
                    if secondary {
                        "read-pass"
                    } else {
                        "primary-pass"
                    },
                    request.method,
                    path,
                    request.body.as_deref().unwrap_or(""),
                    &format!("{NOW:.0}"),
                )
            );
            engine.handle(
                NOW,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: if income { r#"{"items":[]}"# } else { "[]" },
                    },
                },
            );
            let completed = support::managed::drain(&mut engine, NOW);
            assert_eq!(completed.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")).count(), 1);
            assert!(!completed.iter().any(|action| matches!(
                action,
                Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
            )));
            engine.handle(
                NOW,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: if income { r#"{"items":[]}"# } else { "[]" },
                    },
                },
            );
            assert!(support::managed::drain(&mut engine, NOW).is_empty());
        }
    }
}

#[test]
fn public_only_does_not_promote_secondary_into_private_history_eligibility() {
    for income in [false, true] {
        let mut engine = support::setup::engine(options(true, true));
        let actions = ask(&mut engine, income);
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Http(_)
                | Action::ManagedHttp(_)
                | Action::Sign { .. }
                | Action::SignPayload { .. }
        )));
        assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "[]")).count(), 1);
    }
}
