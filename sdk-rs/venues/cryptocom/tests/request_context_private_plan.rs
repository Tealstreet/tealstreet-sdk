//! Whole-plan limits and native final-timer custody through actual dispatch.
mod support;
use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice};
use support::Harness;

const OWNER: u32 = 700;
const PEER: u32 = 701;
fn cancel(command: u32) -> Command {
    Command::CancelSymbolOrders {
        command,
        symbol: "BTCUSD-PERP".into(),
    }
}
fn terminals(actions: &[Action], command: u32) -> Vec<&Notice> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(
                notice @ (Notice::CommandDone { command: id, .. }
                | Notice::CommandFailed { command: id, .. }),
            ) if *id == command => Some(notice),
            _ => None,
        })
        .collect()
}
fn timer_at(actions: &[Action], at: f64) -> safe_cex_engine::TimerId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("actual declared timer")
}
fn loaded() -> Harness {
    let mut h = Harness::new();
    h.load();
    h.actions.clear();
    h
}

#[test]
fn whole_plan_admits_128_writes_and_refuses_129_before_progress_or_io() {
    for count in [128, 129] {
        let mut h = loaded();
        let orders = (0..count)
            .map(|index| {
                serde_json::from_value(serde_json::json!({
                    "symbol":"BTCUSD-PERP", "type":"limit", "side":"buy",
                    "amount":0.0021, "price":90.04, "clientOrderId":format!("batch-{index}")
                }))
                .unwrap()
            })
            .collect();
        h.event(Event::Command(Command::PlaceOrders {
            command: OWNER,
            orders,
        }));
        if count == 128 {
            assert_eq!(
                h.actions
                    .iter()
                    .filter(|a| matches!(a, Action::Http(_)))
                    .count(),
                1
            );
            assert!(terminals(&h.actions, OWNER).is_empty());
        } else {
            assert!(!h.actions.iter().any(|a| matches!(
                a,
                Action::Http(_)
                    | Action::Sign { .. }
                    | Action::Notice(Notice::CommandProgress { .. })
            )));
            assert!(matches!(
                terminals(&h.actions, OWNER).as_slice(),
                [Notice::CommandFailed { .. }]
            ));
            h.actions.clear();
            h.event(Event::Command(cancel(PEER)));
            let _ = h.http("private/cancel-all-orders");
        }
    }
}

#[test]
fn live_final_timer_finishes_once_and_releases_one_parked_fifo_owner() {
    let mut h = loaded();
    h.event(Event::Command(cancel(OWNER)));
    let first = h.http("private/cancel-all-orders");
    h.event(Event::Command(cancel(PEER)));
    h.actions.clear();
    h.respond(&first, r#"{"code":0}"#);
    assert!(terminals(&h.actions, OWNER).is_empty());
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = timer_at(&h.actions, h.now + 20.);
    h.now += 20.;
    h.event(Event::TimerFired { timer });
    assert!(matches!(
        terminals(&h.actions, OWNER).as_slice(),
        [Notice::CommandDone { .. }]
    ));
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1
    );
    h.event(Event::TimerFired { timer });
    assert_eq!(terminals(&h.actions, OWNER).len(), 1);
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1
    );
}

#[test]
fn final_timer_at_root_equality_loses_to_expiry_in_both_event_orders() {
    for root_first in [false, true] {
        let mut h = loaded();
        let start = h.now;
        h.event(Event::Command(cancel(OWNER)));
        let first = h.http("private/cancel-all-orders");
        h.event(Event::Command(cancel(PEER)));
        h.actions.clear();
        h.now = start + 19_990.;
        h.respond(&first, r#"{"code":40101,"message":"bad signature"}"#);
        let retry = h.http("private/cancel-all-orders");
        assert_ne!(first.id, retry.id);
        h.actions.clear();
        h.now = start + 39_980.;
        h.respond(&retry, r#"{"code":0}"#);
        assert!(terminals(&h.actions, OWNER).is_empty());
        let timers: Vec<_> = h
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == start + 40_000. => Some(*timer),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 2, "engine root and native tail timers");
        let [root_timer, final_timer]: [_; 2] = timers.try_into().unwrap();
        assert_ne!(root_timer, final_timer);
        h.actions.clear();
        h.now = start + 40_000.;
        let ordered = if root_first {
            [root_timer, final_timer]
        } else {
            [final_timer, root_timer]
        };
        for timer in ordered {
            h.event(Event::TimerFired { timer });
        }
        assert!(
            matches!(terminals(&h.actions, OWNER).as_slice(), [Notice::CommandFailed { error, .. }] if error.kind == Some(ErrorKind::Timeout))
        );
        assert_eq!(
            h.actions
                .iter()
                .filter(|a| matches!(a, Action::Http(_)))
                .count(),
            1,
            "one native FIFO wake"
        );
        assert!(terminals(&h.actions, PEER).is_empty());
    }
}
