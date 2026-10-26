mod support;

use safe_cex_engine::request_context::{HttpAction, Prepared};
use safe_cex_engine::{Action, Event};
use support::{NOW, ready};

#[test]
fn expired_background_preparation_releases_only_its_inflight_generation() {
    expiry_recovers(false);
}
#[test]
fn expired_background_physical_read_releases_only_its_inflight_generation() {
    expiry_recovers(true);
}
fn expiry_recovers(released: bool) {
    let (mut engine, socket) = ready();
    engine.handle(NOW + 1.0, Event::WsOpened { socket });
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let held = actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains("/account?") => Some(*preparation),
                _ => None,
            },
            _ => None,
        })
        .expect("held account snapshot preparation");
    let timers = actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_001.0 => Some(*timer),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!timers.is_empty(), "real managed preparation deadline");
    if released {
        engine.http_prepared(
            NOW + 1.0,
            held,
            Prepared {
                id: held.attempt().request.0,
                expires_at: None,
            },
        );
        let permit = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine released background read");
        assert!(matches!(
            engine.claim_http(NOW + 1.0, permit),
            Ok(safe_cex_engine::request_context::Claim::Released(_))
        ));
    }
    for timer in timers {
        engine.handle(NOW + 20_001.0, Event::TimerFired { timer });
    }
    engine.drain_actions().count();
    engine.http_prepared(
        NOW + 20_002.0,
        held,
        Prepared {
            id: held.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(!engine.drain_actions().any(|action| matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Bind { .. }))));
    engine.handle(NOW + 20_002.0, Event::WsOpened { socket });
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|action| matches!(action,
        Action::ManagedHttp(action) if matches!(action.as_ref(),
            HttpAction::Prepare { preparation, request, .. }
                if request.url.contains("/account?") && preparation.attempt().request != held.attempt().request
        ))).count(), 1, "expired preparation must not leave the account poll coalesced forever");
    assert_eq!(engine.store().balance().total, 1000.0);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(
            safe_cex_engine::Notice::CommandDone { .. }
                | safe_cex_engine::Notice::CommandFailed { .. }
        )
    )));
}
