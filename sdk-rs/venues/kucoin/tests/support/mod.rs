use safe_cex_engine::{Action, Engine, Event, Method};
use safe_cex_venue_kucoin::Kucoin;

/// Scripted responses wait for transport dispatch, including its quota timer.
pub fn wait_for_http(
    engine: &mut Engine<Kucoin>,
    actions: &mut Vec<Action>,
    now: &mut f64,
    path: Option<&str>,
    method: Option<Method>,
) {
    for _ in 0..100 {
        if actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if path.is_none_or(|p| r.url.contains(p)) && method.is_none_or(|m| r.method == m)))
        {
            return;
        }
        let index = actions.iter().enumerate().filter_map(|(i, action)| {
            if let Action::SetTimer { timer, at_ms } = action
                && !actions.iter().any(|a| matches!(a, Action::ClearTimer { timer: cancelled } if cancelled == timer)) {
                Some((i, *at_ms))
            } else { None }
        }).min_by(|a, b| a.1.total_cmp(&b.1)).expect("awaited HTTP has no causal timer").0;
        let Action::SetTimer { timer, at_ms } = actions.remove(index) else {
            unreachable!()
        };
        *now = now.max(at_ms);
        engine.handle(*now, Event::TimerFired { timer });
        actions.extend(engine.drain_actions());
    }
    panic!("awaited HTTP did not dispatch: {actions:?}");
}
