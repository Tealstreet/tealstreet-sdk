//! Drain exact expired Plan calls through native typed cleanup before sealing.
use super::*;
use crate::request_context::Plan;

struct ExpiryScope<'a, V: Venue> {
    engine: &'a mut Engine<V>,
}
impl<V: Venue> Drop for ExpiryScope<'_, V> {
    fn drop(&mut self) {
        if self.engine.io.core.http_plan_expiring.is_some() {
            self.engine.io.core.http_plan_poison = true;
        }
    }
}

impl<V: Venue> Engine<V> {
    pub(super) fn expire_plan_calls(&mut self, now: f64, plan: &Plan) {
        assert!(self.io.core.http_plan_expiring.is_none());
        self.io.core.http_plan_expiring = Some(plan.clone());
        let scope = ExpiryScope { engine: self };
        loop {
            let next = scope
                .engine
                .io
                .core
                .managed_http
                .iter()
                .filter_map(|(request, managed)| {
                    let delivery = managed.delivery();
                    scope
                        .engine
                        .io
                        .core
                        .request_contexts
                        .plan_from_context(&delivery.context)
                        .is_some_and(|current| current.same_plan(plan))
                        .then_some((*request, managed.timer))
                })
                .min_by_key(|(request, _)| request.0);
            let Some((request, timer)) = next else {
                break;
            };
            assert!(
                scope
                    .engine
                    .expire_http_timer(now, &Event::TimerFired { timer })
            );
            assert!(!scope.engine.io.core.managed_http.contains_key(&request));
        }
        assert!(scope.engine.io.core.http_delivery.is_none());
        assert!(scope.engine.io.core.request_contexts.plan_scope_clear());
        scope.engine.io.core.http_plan_expiring = None;
    }

    pub(super) fn expire_plan_before_http_failure(&mut self, now: f64, request: RequestId) -> bool {
        if !now.is_finite() || self.io.core.http_plan_expiring.is_some() {
            return false;
        }
        let expired = self
            .io
            .core
            .managed_http
            .get(&request)
            .and_then(|managed| {
                self.io
                    .core
                    .request_contexts
                    .plan_from_context(&managed.delivery().context)
            })
            .is_some_and(|plan| now >= plan.deadline());
        if !expired {
            return false;
        }
        self.flush_http_plans(now);
        !self.io.core.managed_http.contains_key(&request)
    }
}
