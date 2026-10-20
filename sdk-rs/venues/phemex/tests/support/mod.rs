//! Scripted fixtures keep their original response timestamps. Advance only an
//! emitted timer when a required request is queued; these fixtures do not model
//! the production startup clock transition (covered by the budget unit tests).
use safe_cex_engine::{Action, Event, TimerId, Venue};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

pub struct Engine<V: Venue> {
    inner: safe_cex_engine::Engine<V>,
    timers: HashMap<TimerId, f64>,
    now: f64,
}
impl<V: Venue> Engine<V> {
    pub fn new(venue: V) -> Self {
        Self {
            inner: safe_cex_engine::Engine::new(venue),
            timers: HashMap::new(),
            now: 0.,
        }
    }
    pub fn handle(&mut self, now: f64, event: Event<'_>) {
        if let Event::TimerFired { timer } = &event {
            self.timers.remove(timer);
        }
        self.now = now;
        self.inner.handle(now, event);
    }
    pub fn drain_actions(&mut self) -> std::vec::IntoIter<Action> {
        let actions: Vec<_> = self.inner.drain_actions().collect();
        for action in &actions {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(timer);
                }
                _ => {}
            }
        }
        actions.into_iter()
    }
    pub fn wait_for(
        &mut self,
        mut actions: Vec<Action>,
        ready: impl Fn(&[Action]) -> bool,
    ) -> Vec<Action> {
        for _ in 0..100 {
            if ready(&actions) {
                return actions;
            }
            let (&timer, &at) = self
                .timers
                .iter()
                .min_by(|a, b| a.1.total_cmp(b.1))
                .expect("request queued without timer");
            self.handle(at.max(self.now), Event::TimerFired { timer });
            actions.extend(self.drain_actions());
        }
        panic!("required request did not arrive within 100 causal timers")
    }
}
impl<V: Venue> Deref for Engine<V> {
    type Target = safe_cex_engine::Engine<V>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<V: Venue> DerefMut for Engine<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
