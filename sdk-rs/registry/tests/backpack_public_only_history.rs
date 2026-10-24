//! Native public-only mode still permits explicitly configured private history reads.
#![cfg(feature = "backpack")]

use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Query, Response, Venue};
use safe_cex_venues::{EngineOptions, Visit, build};
use serde_json::json;

const READ_KEY: &str = "history-read-key";
const READ_SEED: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";

struct HistoryProbe {
    read_key: bool,
}
impl Visit for HistoryProbe {
    type Output = ();
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) {
        engine.start(1000.);
        engine.drain_actions().for_each(drop);
        let cases = [
            (
                Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
                "/wapi/v1/history/fills",
                "fillHistoryQueryAll",
            ),
            (
                Query::OrderHistory {
                    symbol: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
                "/wapi/v1/history/orders",
                "orderHistoryQueryAll",
            ),
            (
                Query::Income {
                    symbol: None,
                    kind: Some("FUNDING".into()),
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
                "/wapi/v1/history/funding",
                "fundingHistoryQueryAll",
            ),
        ];
        for (index, (query, path, instruction)) in cases.into_iter().enumerate() {
            let command = index as u32 + 1;
            let now = 2000. + index as f64;
            let before = engine.account_snapshot_frame(now).to_vec();
            engine.handle(now, Event::Command(Command::Query { command, query }));
            let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, now);
            if !self.read_key {
                assert!(
                    !actions
                        .iter()
                        .any(|action| matches!(action, Action::Http(_)))
                );
                let failures: Vec<_> = actions
                    .iter()
                    .filter_map(|action| match action {
                        Action::Notice(Notice::CommandFailed { command: id, error })
                            if *id == command =>
                        {
                            Some(error)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(failures.len(), 1);
                assert_eq!(failures[0].message, "Backpack credentials unavailable");
                assert_eq!(failures[0].kind, None);
                assert_eq!(failures[0].outcome, None);
                assert_eq!(engine.account_snapshot_frame(now), before);
                continue;
            }
            assert!(!actions.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { .. } | Notice::CommandError { .. })
            )));
            let requests: Vec<_> = actions
                .into_iter()
                .filter_map(|action| match action {
                    Action::Http(request) => Some(request),
                    _ => None,
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let request = &requests[0];
            assert_eq!(request.method, Method::Get);
            assert_eq!(
                request.url,
                format!("https://api.backpack.exchange{path}?limit=1&sortDirection=Desc")
            );
            assert_eq!(request.body, None);
            assert_eq!(request.route, None);
            let params = [
                ("limit".into(), "1".into()),
                ("sortDirection".into(), "Desc".into()),
            ]
            .into();
            let native =
                safe_cex_venue_backpack::sign::Credentials::new(READ_KEY.into(), READ_SEED)
                    .unwrap();
            assert_eq!(request.headers, native.headers(instruction, &[params], now));
            engine.handle(
                now,
                Event::HttpResponse {
                    request: request.id,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "[]",
                    },
                },
            );
            let terminal: Vec<_> = engine
                .drain_actions()
                .filter_map(|action| match action {
                    Action::Notice(Notice::CommandDone {
                        command: id,
                        result,
                    }) if id == command => Some(result),
                    Action::Notice(Notice::CommandFailed { .. } | Notice::CommandError { .. }) => {
                        panic!("native private history must succeed with saved read credentials")
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(terminal, ["[]"]);
            assert_eq!(engine.account_snapshot_frame(now), before);
        }
    }
}

fn exercise(read_key: bool) {
    let mut options = json!({ "publicDataOnly": true });
    if read_key {
        options["readCredentials"] = json!({ "key": READ_KEY, "secret": READ_SEED });
    }
    let options: EngineOptions = serde_json::from_value(options).unwrap();
    build(
        "backpack",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
        HistoryProbe { read_key },
    )
    .unwrap();
}

#[test]
fn public_only_with_saved_read_key_preserves_all_three_private_history_routes() {
    exercise(true);
}

#[test]
fn public_only_without_read_key_keeps_native_history_refusal() {
    exercise(false);
}
