//! Private native fixture selects the discovered portfolio without a public mutator.
use crate::{Coinbase, Options, ProductKind};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Notice, Query, Response};

#[test]
fn cfm_to_spot_keeps_accepted_rows_and_per_family_limits() {
    for later_failure in [false, true] {
        let mut venue = Coinbase::new(Options {
            key: "organizations/test/apiKeys/test".into(),
            secret: STANDARD.encode([7; 32]),
            ..Default::default()
        })
        .unwrap();
        venue.account.kind = Some(ProductKind::Cfm);
        let mut e = safe_cex_engine::test_support::seeded_engine(venue);
        e.handle(
            1704067200000.,
            Event::Command(Command::Query {
                command: 77,
                query: Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: None,
                    to_ms: None,
                    limit: Some(1),
                },
            }),
        );
        let mut now = 1704067200000.;
        let mut requests: Vec<HttpRequest> = vec![];
        let mut timers = vec![];
        let mut result = None;
        let mut gaps = vec![];
        for _ in 0..8 {
            for a in e.drain_actions() {
                match a {
                    Action::Http(r) => requests.push(r),
                    Action::SetTimer { timer, at_ms } => timers.push((at_ms, timer)),
                    Action::Notice(Notice::CommandDone {
                        command: 77,
                        result: r,
                    }) => result = Some(Ok(r)),
                    Action::Notice(Notice::CommandFailed { command: 77, error }) => {
                        result = Some(Err(error.legacy_text()))
                    }
                    Action::Notice(Notice::Event(event))
                        if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
                    {
                        gaps.push(event)
                    }
                    _ => {}
                }
            }
            if result.is_some() {
                break;
            }
            if let Some(r) = requests.pop() {
                let cfm = r.url.contains("product_types=FUTURE");
                assert!(cfm || r.url.contains("product_types=SPOT"));
                assert!(
                    !r.url.contains("cursor="),
                    "per-family limit must terminate each stage"
                );
                let row = if cfm {
                    r#"{"trade_id":"cfm","order_id":"o-cfm","product_id":"BTC-CDE","trade_type":"FILL","trade_time":"2024-01-01T00:00:00Z","price":"100","size":"1","size_in_quote":false,"commission":"0.1","side":"BUY"}"#
                } else {
                    r#"{"trade_id":"spot","order_id":"o-spot","product_id":"BTC-USD","trade_type":"FILL","trade_time":"2024-01-02T00:00:00Z","price":"100","size":"2","size_in_quote":false,"commission":"0.2","side":"BUY"}"#
                };
                let body = format!(r#"{{"fills":[{row}],"cursor":"unused-next"}}"#);
                e.handle(
                    now,
                    Event::HttpResponse {
                        request: r.id,
                        response: Response {
                            status: if !cfm && later_failure { 503 } else { 200 },
                            headers: "",
                            body: &body,
                        },
                    },
                );
            } else {
                timers.sort_by(|a, b| a.0.total_cmp(&b.0));
                let (at, timer) = timers.remove(0);
                now = at;
                e.handle(now, Event::TimerFired { timer });
            }
        }
        let rows: serde_json::Value = serde_json::from_str(&result.unwrap().unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["id"], if later_failure { "cfm" } else { "spot" });
        assert_eq!(gaps.len(), usize::from(later_failure));
        if later_failure {
            assert_eq!(
                gaps[0].fields,
                vec![("command", safe_cex_engine::Value::Num(77.))]
            );
        }
    }
}

#[test]
fn capability_profile_intx_has_no_income_ledger() {
    assert_no_income_ledger(ProductKind::Intx);
}
#[test]
fn capability_profile_cfm_has_no_income_ledger() {
    assert_no_income_ledger(ProductKind::Cfm);
}
#[test]
fn capability_profile_spot_has_no_income_ledger() {
    assert_no_income_ledger(ProductKind::Spot);
}
fn assert_no_income_ledger(kind: ProductKind) {
    let mut venue = Coinbase::new(Options {
        key: "organizations/org-id/apiKeys/key-id".into(),
        secret: STANDARD.encode([7; 32]),
        spot_only: kind == ProductKind::Spot,
        ..Default::default()
    })
    .unwrap();
    venue.account.kind = Some(kind);
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    assert!(!engine.capabilities().income_history, "{kind:?}");
    assert!(engine.capabilities().fill_history);
    assert!(engine.capabilities().order_history);
    engine.handle(
        1704067200000.,
        Event::Command(Command::Query {
            command: 971,
            query: Query::Income {
                symbol: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: Some(1),
            },
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a|matches!(a, Action::Notice(Notice::CommandFailed { command: 971, error }) if error.message == "Coinbase Advanced Trade does not publish an income ledger API for this account path")));
}
