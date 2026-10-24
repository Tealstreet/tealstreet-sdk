use super::*;
use safe_cex_engine::{Action, Command, Event, Notice};

#[path = "managed.rs"]
mod managed;

#[test]
fn preparation_refusal_after_accepted_orders_is_strict_typed_not_sent() {
    let mut engine = crate::setup::engine_with_provider(
        crate::setup::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("passphrase".into()),
            ..Default::default()
        },
        |inner| std::sync::Arc::new(crate::setup::Gated::new(inner, &[true, false])),
    );
    let now = 1_764_505_700_000.0;
    engine.handle(
        now,
        Event::Command(Command::Query {
            command: 7,
            query: Query::OrderHistory {
                symbol: None,
                from_ms: None,
                to_ms: None,
                limit: None,
            },
        }),
    );
    let request = managed::drain(&mut engine, now)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    engine.handle(now,Event::HttpResponse{request,response:Response{status:200,headers:"",body:r#"[{"orderId":702345678901234567,"symbol":"BTCUSDT","side":"BUY","positionSide":"LONG","status":"FILLED","type":"LIMIT","time":1764400000000,"price":"65000","origQty":"0.01","executedQty":"0.01","avgPrice":"64999.5","updateTime":1764400001234}]"#}});
    let actions = managed::drain(&mut engine, now);
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0],
        &safe_cex_engine::signing::SignError::Refused.venue_error()
    );
    assert_eq!(errors[0].outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Notice(
                Notice::CommandDone { .. } | Notice::Deactivate | Notice::AccountAuth { .. }
            )
    )));
    for action in &actions {
        if let Action::SetTimer { timer, .. } = action {
            assert!(
                actions.iter().any(|action| matches!(action,
                    Action::ClearTimer { timer: cleared } if cleared == timer
                )),
                "provider refusal leaves no live managed timer"
            );
        }
    }
    assert!(
        !actions.iter().any(
            |a| matches!(a,Action::Notice(Notice::Event(v)) if v.code==code::HISTORY_INCOMPLETE)
        )
    );
}
