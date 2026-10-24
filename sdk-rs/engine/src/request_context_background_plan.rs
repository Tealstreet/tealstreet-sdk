//! Background task registration and exact parent/child scheduling custody.
use super::*;
use crate::TimerId;
use crate::read_scheduler::{self, Scheduler};
use crate::request_context::{BackgroundActivation, BackgroundTask, BackgroundTaskPolicy, Session};
use crate::selected_read::SelectedRead;
use std::sync::Arc;

const TASKS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) struct ActivationKey {
    slot: usize,
    generation: u64,
    attempt: read_scheduler::Attempt,
}

impl ActivationKey {
    fn of(activation: &BackgroundActivation) -> Self {
        Self {
            slot: activation.task.slot,
            generation: activation.task.generation,
            attempt: activation.attempt,
        }
    }
}

struct Task {
    handle: BackgroundTask,
    parent: usize,
    selected: Arc<SelectedRead>,
    policy: BackgroundTaskPolicy,
    admissions: u8,
    child: Option<Plan>,
    activation: Option<read_scheduler::Attempt>,
    attempt: Option<read_scheduler::Attempt>,
    removing: bool,
    failure: Option<VenueError>,
    cycle_complete: bool,
    expiry_retry: bool,
}

struct Parent {
    generation: u64,
    timer: TimerId,
    tasks: Vec<usize>,
    scheduler: Scheduler<usize, ()>,
}

#[derive(Default)]
pub(super) struct Tasks {
    tasks: Vec<Option<Task>>,
    parents: Vec<Option<Parent>>,
    count: usize,
    timer_count: usize,
    ready: Vec<usize>,
    cleanup: Vec<usize>,
    notifications: Vec<(BackgroundTask, VenueError)>,
}

impl BackgroundTaskPolicy {
    fn scheduler(self) -> Result<read_scheduler::Policy, Error> {
        if self.calls == 0
            || !self.debounce_ms.is_finite()
            || self.debounce_ms < 0.
            || !self.refresh_debounce_ms.is_finite()
            || self.refresh_debounce_ms < 0.
        {
            return Err(Error::Invalid);
        }
        if usize::from(self.calls) > BACKGROUND_CONTEXT_CAP {
            return Err(Error::Capacity);
        }
        Ok(read_scheduler::Policy {
            debounce_ms: self.debounce_ms,
            refresh_debounce_ms: self.refresh_debounce_ms,
            admission_timeout_ms: 20_000.,
            response_timeout_ms: f64::from(self.calls) * 20_000.,
            response_expiry: read_scheduler::ExpiryPolicy::Retry,
            retry: read_scheduler::RetryPolicy {
                initial_ms: 1000.,
                max_steps: 1,
                cap_ms: 2000.,
            },
        })
    }
}

impl Tasks {
    fn task(&self, task: &BackgroundTask) -> Result<&Task, Error> {
        self.tasks
            .get(task.slot)
            .and_then(Option::as_ref)
            .filter(|entry| entry.handle.same_task(task))
            .ok_or(Error::Revoked)
    }
}

impl ContextRegistry {
    fn background_task(&self, task: &BackgroundTask) -> Result<&Task, Error> {
        if !Arc::ptr_eq(&task.issuer, &self.issuer) {
            return Err(Error::Revoked);
        }
        self.validate_session(&task.parent)?;
        self.background.task(task)
    }
}

impl<P> Cx<'_, P> {
    /// Registration owns no Context credits. Selection is captured once;
    /// installed account/config provenance remains the separate G2c contract.
    pub fn register_background_task(
        &mut self,
        session: &Session,
        selected: SelectedRead,
        policy: BackgroundTaskPolicy,
    ) -> Result<BackgroundTask, Error> {
        self.session_scope()?;
        if !self.now.is_finite() {
            return Err(Error::Invalid);
        }
        let generation = self.io.request_contexts.validate_session(session)?;
        let scheduler_policy = policy.scheduler()?;
        selected.credential().validate()?;
        let selected = Arc::new(selected);
        let existing = self
            .io
            .request_contexts
            .sessions
            .background_parent(generation);
        let tasks = &mut self.io.request_contexts.background;
        if tasks.count >= TASKS {
            return Err(Error::Capacity);
        }
        let slot = tasks
            .tasks
            .iter()
            .position(Option::is_none)
            .unwrap_or(tasks.tasks.len());
        let parent_slot = existing.unwrap_or_else(|| {
            tasks
                .parents
                .iter()
                .position(Option::is_none)
                .unwrap_or(tasks.parents.len())
        });
        let ids = if existing.is_some() { 1 } else { 2 };
        if !self.io.next_id.available(ids) {
            return Err(Error::Capacity);
        }
        if slot == tasks.tasks.len() {
            tasks.tasks.try_reserve(1).map_err(|_| Error::Capacity)?;
        }
        if parent_slot == tasks.parents.len() {
            tasks.parents.try_reserve(1).map_err(|_| Error::Capacity)?;
        }
        tasks
            .cleanup
            .try_reserve_exact((tasks.count + 1).saturating_sub(tasks.cleanup.len()))
            .map_err(|_| Error::Capacity)?;
        if existing.is_none() {
            tasks
                .ready
                .try_reserve_exact((tasks.parents.len() + 1).saturating_sub(tasks.ready.len()))
                .map_err(|_| Error::Capacity)?;
        }
        tasks
            .notifications
            .try_reserve_exact((tasks.count + 1).saturating_sub(tasks.notifications.len()))
            .map_err(|_| Error::Capacity)?;
        let mut fresh = if existing.is_none() {
            self.io.timers.try_reserve(1).map_err(|_| Error::Capacity)?;
            Some(Parent {
                generation,
                timer: TimerId(0),
                tasks: Vec::new(),
                scheduler: Scheduler::new(TASKS, vec![()]).map_err(|_| Error::Capacity)?,
            })
        } else {
            None
        };
        let parent = match existing {
            Some(index) => tasks.parents[index].as_mut().expect("validated parent"),
            None => fresh.as_mut().expect("prepared parent"),
        };
        parent.tasks.try_reserve(1).map_err(|_| Error::Capacity)?;
        parent
            .scheduler
            .register_task(slot, &(), scheduler_policy)
            .map_err(|_| Error::Capacity)?;
        // No fallible work follows registration or either namespace spend.
        let ids = self
            .io
            .reserve_ids(ids)
            .expect("preflighted task identities");
        let handle = BackgroundTask {
            issuer: self.io.request_contexts.issuer.clone(),
            parent: session.clone(),
            slot,
            generation: ids.start,
        };
        self.io
            .request_contexts
            .sessions
            .bind_background_parent(generation, parent_slot);
        let tasks = &mut self.io.request_contexts.background;
        if let Some(mut parent) = fresh {
            parent.timer = TimerId(ids.start + 1);
            if parent_slot == tasks.parents.len() {
                tasks.parents.push(Some(parent));
            } else {
                tasks.parents[parent_slot] = Some(parent);
            }
        }
        tasks.parents[parent_slot]
            .as_mut()
            .expect("committed parent")
            .tasks
            .push(slot);
        let task = Task {
            handle: handle.clone(),
            parent: parent_slot,
            selected,
            policy,
            admissions: 0,
            child: None,
            activation: None,
            attempt: None,
            removing: false,
            failure: None,
            cycle_complete: true,
            expiry_retry: false,
        };
        if slot == tasks.tasks.len() {
            tasks.tasks.push(Some(task));
        } else {
            tasks.tasks[slot] = Some(task);
        }
        tasks.count += 1;
        Ok(handle)
    }
}

impl<P> Cx<'_, P> {
    pub(in crate::engine) fn background_plan_policy(
        &self,
        activation: &BackgroundActivation,
    ) -> Result<(PlanPolicy, u16), Error> {
        if !self.now.is_finite()
            || self.io.http_background_activation != Some(ActivationKey::of(activation))
            || self.io.http_delivery.is_some()
            || self.io.http_preflight_ready.is_some()
            || self.io.http_plan_ready.is_some()
            || self.io.http_plan_expiring.is_some()
        {
            return Err(Error::Revoked);
        }
        let task = self.io.request_contexts.background_task(&activation.task)?;
        if task.activation != Some(activation.attempt)
            || task.child.is_some()
            || task.admissions >= 3
        {
            return Err(Error::Revoked);
        }
        Ok((
            PlanPolicy {
                operation: Operation::Read,
                credential: Some(task.selected.credential()),
                pacing: None,
                retry: crate::request_context::RetryPolicy::Never,
            },
            task.policy.calls,
        ))
    }

    pub(in crate::engine) fn reserve_background_plan(
        &mut self,
        activation: &BackgroundActivation,
        calls: usize,
    ) -> Result<(), Error> {
        let (_, declared) = self.background_plan_policy(activation)?;
        if calls != usize::from(declared) {
            return Err(Error::Invalid);
        }
        let registry = &mut self.io.request_contexts;
        if registry
            .background_context_count(None)?
            .checked_add(calls)
            .is_none_or(|total| total > BACKGROUND_CONTEXT_CAP)
        {
            return Err(Error::Capacity);
        }
        registry
            .physical
            .try_reserve(1)
            .map_err(|_| Error::Capacity)?;
        let retained = registry
            .commands
            .values()
            .filter(|owner| owner.retained())
            .count()
            .checked_add(registry.physical.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Capacity)?;
        registry
            .retirements
            .try_reserve_exact(retained.saturating_sub(registry.retirements.len()))
            .map_err(|_| Error::Capacity)?;
        Ok(())
    }

    pub(in crate::engine) fn commit_background_plan(
        &mut self,
        activation: &BackgroundActivation,
        plan: &Plan,
    ) {
        let tasks = &mut self.io.request_contexts.background;
        let task = tasks.tasks[activation.task.slot]
            .as_mut()
            .expect("validated task");
        assert!(
            task.handle.same_task(&activation.task)
                && task.activation == Some(activation.attempt)
                && task.admissions < 3
                && task.child.is_none()
        );
        assert!(
            tasks.parents[task.parent]
                .as_mut()
                .expect("live task parent")
                .scheduler
                .mark_issued(activation.attempt, self.now)
                .expect("validated activation clock")
        );
        task.activation = None;
        task.admissions += 1;
        task.child = Some(plan.clone());
        task.expiry_retry = false;
    }

    /// Consume the exact scheduled activation; declarations and selection were
    /// fixed at registration. This never admits Mutation or a capped Preflight.
    pub fn admit_background_plan(
        &mut self,
        activation: &BackgroundActivation,
    ) -> Result<Plan, Error> {
        let (policy, calls) = self.background_plan_policy(activation)?;
        let call = [crate::request_context::plan::CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: crate::request_context::plan::RetryWindow::Shared,
            after_refusal: &[],
        }];
        let mut steps = Vec::new();
        steps
            .try_reserve_exact(usize::from(calls))
            .map_err(|_| Error::Capacity)?;
        for _ in 0..calls {
            steps.push(crate::request_context::plan::StepBound {
                wait_ms: 0,
                calls: &call,
            });
        }
        self.admit_background_plan_owned(activation, &steps, policy)
    }
}

impl Tasks {
    fn wake(&mut self, parent: usize) {
        if !self.ready.contains(&parent) {
            assert!(self.ready.len() < self.ready.capacity());
            self.ready.push(parent);
        }
    }
    fn cleanup(&mut self, slot: usize) {
        if !self.cleanup.contains(&slot) {
            assert!(self.cleanup.len() < self.cleanup.capacity());
            self.cleanup.push(slot);
        }
    }
    fn owner_task(&self, entry: &CommandOwner, owner: Owner) -> Result<&Task, Error> {
        self.tasks
            .get(entry.background_task.ok_or(Error::Revoked)?)
            .and_then(Option::as_ref)
            .filter(|task| {
                !task.removing
                    && task
                        .child
                        .as_ref()
                        .is_some_and(|plan| plan.owner() == owner)
            })
            .ok_or(Error::Revoked)
    }
}

impl ContextRegistry {
    pub(in crate::engine) fn validate_background_owner(&self, owner: Owner) -> Result<(), Error> {
        if !matches!(owner, Owner::Background { .. }) || !self.physical_owner(owner) {
            return Ok(());
        }
        let entry = self.owner_entry(owner).ok_or(Error::Revoked)?;
        let task = self.background.owner_task(entry, owner)?;
        self.validate_session(&task.handle.parent)?;
        if self.background.parents[task.parent]
            .as_ref()
            .ok_or(Error::Revoked)?
            .scheduler
            .retiring(&task.handle.slot)
            .is_some()
        {
            return Err(Error::Revoked);
        }
        Ok(())
    }

    pub(in crate::engine) fn background_retirement(
        &self,
        token: &HttpOwnerRetirement,
    ) -> Option<usize> {
        self.owner_entry(token.preflight().owner())?.background_task
    }
}

impl<P> Cx<'_, P> {
    fn rearm_background_parent(&mut self, index: usize) {
        let Some(parent) = self
            .io
            .request_contexts
            .background
            .parents
            .get(index)
            .and_then(Option::as_ref)
        else {
            return;
        };
        let timer = parent.timer;
        let dead =
            !self.io.request_contexts.sessions.live(parent.generation) && parent.tasks.is_empty();
        let deadline = if dead {
            None
        } else {
            parent.scheduler.next_deadline()
        };
        self.io.actions.retain(|action| !matches!(action, crate::Action::SetTimer { timer: held, .. } if *held == timer));
        if self.io.timers.remove(&timer).is_some() {
            self.io.request_contexts.background.timer_count -= 1;
            self.io.actions.push(crate::Action::ClearTimer { timer });
        }
        if let Some(at_ms) = deadline {
            self.io.request_contexts.background.timer_count += 1;
            self.io.timers.insert(timer, ());
            self.io
                .actions
                .push(crate::Action::SetTimer { timer, at_ms });
        }
        if dead {
            self.io.request_contexts.background.parents[index] = None;
            self.io
                .request_contexts
                .background
                .ready
                .retain(|held| *held != index);
        }
    }

    pub fn request_background_task(
        &mut self,
        task: &BackgroundTask,
        demand: read_scheduler::Demand,
    ) -> Result<(), Error> {
        self.session_scope()?;
        let index = self.io.request_contexts.background_task(task)?.parent;
        let deadline = self.now + 20_000.;
        if !deadline.is_finite() || deadline <= self.now {
            return Err(Error::Invalid);
        }
        self.io
            .actions
            .try_reserve(2)
            .map_err(|_| Error::Capacity)?;
        self.io.timers.try_reserve(1).map_err(|_| Error::Capacity)?;
        self.io.request_contexts.background.parents[index]
            .as_mut()
            .expect("live task parent")
            .scheduler
            .request(&task.slot, self.now, demand)
            .map_err(|_| Error::Invalid)?;
        let entry = self.io.request_contexts.background.tasks[task.slot]
            .as_mut()
            .expect("validated task");
        if entry.cycle_complete && entry.child.is_none() && entry.attempt.is_none() {
            entry.admissions = 0;
            entry.cycle_complete = false;
        }
        self.io.request_contexts.background.wake(index);
        self.rearm_background_parent(index);
        Ok(())
    }

    pub fn pause_background_tasks(&mut self, session: &Session, until: f64) -> Result<(), Error> {
        self.session_scope()?;
        let generation = self.io.request_contexts.validate_session(session)?;
        if !until.is_finite() {
            return Err(Error::Invalid);
        }
        let Some(index) = self
            .io
            .request_contexts
            .sessions
            .background_parent(generation)
        else {
            return Ok(());
        };
        self.io
            .actions
            .try_reserve(2)
            .map_err(|_| Error::Capacity)?;
        self.io.timers.try_reserve(1).map_err(|_| Error::Capacity)?;
        let tasks = &mut self.io.request_contexts.background;
        tasks.parents[index]
            .as_mut()
            .expect("live parent")
            .scheduler
            .pause(&(), until)
            .map_err(|_| Error::Invalid)?;
        let parent = tasks.parents[index].as_ref().expect("live parent");
        for i in 0..parent.tasks.len() {
            let slot = tasks.parents[index].as_ref().unwrap().tasks[i];
            if tasks.parents[index]
                .as_ref()
                .unwrap()
                .scheduler
                .retiring(&slot)
                .is_some()
            {
                tasks.cleanup(slot);
            }
        }
        tasks.wake(index);
        self.rearm_background_parent(index);
        Ok(())
    }

    pub fn supersede_background_task(&mut self, task: &BackgroundTask) -> Result<(), Error> {
        self.session_scope()?;
        let entry = self.io.request_contexts.background_task(task)?;
        let index = entry.parent;
        let attempt = entry.attempt.ok_or(Error::Revoked)?;
        self.io
            .actions
            .try_reserve(2)
            .map_err(|_| Error::Capacity)?;
        self.io.timers.try_reserve(1).map_err(|_| Error::Capacity)?;
        let tasks = &mut self.io.request_contexts.background;
        if !tasks.parents[index]
            .as_mut()
            .expect("live parent")
            .scheduler
            .superseded(attempt, self.now)
            .map_err(|_| Error::Invalid)?
        {
            return Err(Error::Revoked);
        }
        tasks.cleanup(task.slot);
        tasks.wake(index);
        self.rearm_background_parent(index);
        Ok(())
    }

    pub fn cancel_background_task(&mut self, task: &BackgroundTask) -> Result<(), Error> {
        self.change_background_task(task, false)
    }
    pub fn retire_background_task(&mut self, task: &BackgroundTask) -> Result<(), Error> {
        self.change_background_task(task, true)
    }
    fn change_background_task(&mut self, task: &BackgroundTask, remove: bool) -> Result<(), Error> {
        self.session_scope()?;
        let index = self.io.request_contexts.background_task(task)?.parent;
        self.io
            .actions
            .try_reserve(2)
            .map_err(|_| Error::Capacity)?;
        self.io.timers.try_reserve(1).map_err(|_| Error::Capacity)?;
        let tasks = &mut self.io.request_contexts.background;
        let parent = tasks.parents[index].as_mut().expect("live task parent");
        if remove {
            parent.scheduler.retire(&task.slot)
        } else {
            parent.scheduler.cancel(&task.slot)
        }
        .map_err(|_| Error::Revoked)?;
        let entry = tasks.tasks[task.slot].as_mut().expect("live task");
        entry.removing |= remove;
        entry.cycle_complete = true;
        tasks.cleanup(task.slot);
        tasks.wake(index);
        self.rearm_background_parent(index);
        Ok(())
    }
}

impl ContextRegistry {
    pub(in crate::engine) fn retire_background_parent(&mut self, generation: u64, index: usize) {
        assert!(
            self.background.parents[index]
                .as_ref()
                .is_some_and(|parent| parent.generation == generation)
        );
        let count = self.background.parents[index].as_ref().unwrap().tasks.len();
        for i in 0..count {
            let slot = self.background.parents[index].as_ref().unwrap().tasks[i];
            self.background.parents[index]
                .as_mut()
                .unwrap()
                .scheduler
                .retire(&slot)
                .expect("registered task retirement");
            self.background.tasks[slot]
                .as_mut()
                .expect("indexed task")
                .removing = true;
            self.background.cleanup(slot);
        }
        self.background.wake(index);
    }

    fn remove_background_task(&mut self, slot: usize) {
        let task = self.background.tasks[slot]
            .take()
            .expect("indexed retiring task");
        assert!(task.child.is_none() && task.attempt.is_none());
        let parent = self.background.parents[task.parent]
            .as_mut()
            .expect("retained parent index");
        parent.tasks.retain(|held| *held != slot);
        self.background.count -= 1;
    }
}

impl<P> Cx<'_, P> {
    pub(in crate::engine) fn background_sealed(
        &mut self,
        plan: &Plan,
        error: Option<VenueError>,
        retry: bool,
    ) {
        let registry = &mut self.io.request_contexts;
        let slot = registry
            .owner_entry(plan.owner())
            .and_then(|entry| entry.background_task)
            .expect("registered Background owner");
        let task = registry.background.tasks[slot]
            .as_mut()
            .expect("live task binding");
        assert!(
            task.child
                .as_ref()
                .is_some_and(|child| child.same_plan(plan))
        );
        let token = task.attempt.expect("admitted scheduler token");
        let success = error.is_none();
        let retry = (retry || task.expiry_retry) && task.admissions < 3 && !task.removing;
        let parent = registry.background.parents[task.parent]
            .as_mut()
            .expect("retained task parent");
        if self.io.http_background_retiring == Some(plan.owner())
            && parent.scheduler.retiring(&slot).is_some()
        {
            if task.admissions >= 3 && !task.removing && !task.cycle_complete {
                parent.scheduler.cancel(&slot).expect("registered task");
                task.cycle_complete = true;
                task.failure = Some(
                    VenueError::new("Background traversal admission budget exhausted")
                        .kind(crate::ErrorKind::InvalidInput),
                );
            } else {
                task.cycle_complete |= task.removing;
                task.failure = error;
            }
            return;
        }
        if success || retry {
            assert!(
                parent
                    .scheduler
                    .finish_task(token, self.now, success)
                    .expect("terminal raw clock")
            );
        } else {
            parent.scheduler.cancel(&slot).expect("registered task");
        }
        task.failure = error;
        task.cycle_complete = success || !retry;
    }

    pub(in crate::engine) fn defer_background_expiry_retry(
        &mut self,
        plan: &Plan,
        retry: bool,
    ) -> Result<(), Error> {
        let registry = &mut self.io.request_contexts;
        let slot = registry
            .owner_entry(plan.owner())
            .and_then(|entry| entry.background_task)
            .ok_or(Error::Revoked)?;
        let task = registry.background.tasks[slot]
            .as_mut()
            .ok_or(Error::Revoked)?;
        if !task
            .child
            .as_ref()
            .is_some_and(|child| child.same_plan(plan))
        {
            return Err(Error::Revoked);
        }
        task.expiry_retry |= retry;
        Ok(())
    }

    pub(in crate::engine) fn acknowledge_background_retirement(
        &mut self,
        slot: usize,
        owner: Owner,
    ) {
        assert!(!self.io.request_contexts.physical_owner(owner));
        let tasks = &mut self.io.request_contexts.background;
        let task = tasks.tasks[slot].as_mut().expect("retiring task");
        assert!(
            task.child
                .as_ref()
                .is_some_and(|plan| plan.owner() == owner)
        );
        task.child = None;
        self.acknowledge_background_task(slot);
    }

    fn acknowledge_background_task(&mut self, slot: usize) {
        let tasks = &mut self.io.request_contexts.background;
        let task = tasks.tasks[slot].as_mut().expect("retiring task");
        let index = task.parent;
        assert!(task.child.is_none());
        if let Some(token) = task.attempt.take() {
            assert!(
                tasks.parents[index]
                    .as_mut()
                    .expect("retained parent")
                    .scheduler
                    .ack_retired(token, self.now, |delay| delay)
                    .expect("cleanup raw clock")
            );
        }
        task.activation = None;
        if let Some(error) = task.failure.take() {
            assert!(tasks.notifications.len() < tasks.notifications.capacity());
            tasks.notifications.push((task.handle.clone(), error));
        }
        if task.removing {
            self.io.request_contexts.remove_background_task(slot);
        }
        self.io.request_contexts.background.wake(index);
        self.rearm_background_parent(index);
    }

    fn flush_background_cleanup(&mut self) {
        while let Some(slot) = self.io.request_contexts.background.cleanup.pop() {
            let Some(task) = self
                .io
                .request_contexts
                .background
                .tasks
                .get(slot)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            if let Some(plan) = task.child.clone() {
                if matches!(
                    self.io
                        .request_contexts
                        .owner_entry(plan.owner())
                        .map(|owner| &owner.phase),
                    Some(CommandPhase::Retiring(_))
                ) {
                    continue;
                }
                self.io.http_background_retiring = Some(plan.owner());
                let result = self.fail_plan(
                    &plan,
                    VenueError::new("Background task retired").kind(crate::ErrorKind::InvalidInput),
                );
                self.io.http_background_retiring = None;
                result.expect("indexed Background failure-only cleanup");
            } else {
                let parent = self.io.request_contexts.background.parents[task.parent]
                    .as_ref()
                    .expect("indexed parent");
                let token = parent.scheduler.retiring(&slot);
                if let Some(token) = token {
                    let task = self.io.request_contexts.background.tasks[slot]
                        .as_mut()
                        .unwrap();
                    task.attempt = Some(token);
                    task.failure
                        .get_or_insert_with(|| Error::Revoked.read_venue_error());
                }
                self.acknowledge_background_task(slot);
            }
        }
    }
}

impl<P> Cx<'_, P> {
    pub fn decline_background_task(
        &mut self,
        activation: &BackgroundActivation,
        error: Error,
    ) -> Result<(), Error> {
        self.background_plan_policy(activation)?;
        let tasks = &mut self.io.request_contexts.background;
        let task = tasks.tasks[activation.task.slot]
            .as_mut()
            .expect("validated activation");
        task.failure = Some(error.read_venue_error());
        task.cycle_complete = true;
        task.activation = None;
        task.attempt = Some(activation.attempt);
        tasks.parents[task.parent]
            .as_mut()
            .expect("live parent")
            .scheduler
            .cancel(&activation.task.slot)
            .expect("registered task");
        tasks.cleanup(activation.task.slot);
        Ok(())
    }
}

impl<V: crate::Venue> super::super::Engine<V> {
    pub(in crate::engine) fn queue_background_timer(&mut self, event: &crate::Event<'_>) -> bool {
        if self.io.request_contexts.background.timer_count == 0 {
            return false;
        }
        let crate::Event::TimerFired { timer } = event else {
            return false;
        };
        let tasks = &mut self.io.request_contexts.background;
        let Some(index) = tasks
            .parents
            .iter()
            .position(|parent| parent.as_ref().is_some_and(|parent| parent.timer == *timer))
        else {
            return false;
        };
        if self.io.timers.remove(timer).is_some() {
            tasks.timer_count -= 1;
        }
        tasks.wake(index);
        true
    }

    pub(in crate::engine) fn flush_background_tasks(&mut self, now: f64) {
        if self.io.request_contexts.background.ready.is_empty()
            && self.io.request_contexts.background.cleanup.is_empty()
            && self.io.request_contexts.background.notifications.is_empty()
        {
            return;
        }
        if !now.is_finite()
            || self.io.http_delivery.is_some()
            || self.io.http_plan_ready.is_some()
            || self.io.http_preflight_ready.is_some()
            || self.io.http_plan_expiring.is_some()
            || self.io.http_background_activation.is_some()
            || !self.io.request_contexts.plan_scope_clear()
        {
            return;
        }
        self.assert_live();
        self.io.http_plan_poison = true;
        self.io
            .actions
            .try_reserve(2 * TASKS)
            .expect("Background cleanup action capacity");
        self.io
            .timers
            .try_reserve(TASKS)
            .expect("Background wake timer capacity");
        Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        }
        .flush_background_cleanup();
        self.io.http_plan_poison = false;
        self.flush_http_owner_retirements();
        while let Some((task, error)) = self.io.request_contexts.background.notifications.pop() {
            self.io.http_owner_poison = true;
            self.venue.on_background_task_failed(&task, error);
            self.io.http_owner_poison = false;
        }
        let mut visits = self.io.request_contexts.background.ready.len();
        while visits > 0 {
            visits -= 1;
            let Some(index) = self.io.request_contexts.background.ready.pop() else {
                break;
            };
            let Some(parent) = self.io.request_contexts.background.parents[index].as_mut() else {
                continue;
            };
            let next = parent
                .scheduler
                .take_ready(now, |delay| delay)
                .map(|next| next.map(|(key, attempt)| (*key, attempt)));
            let invalid = next.is_err();
            let count = self.io.request_contexts.background.parents[index]
                .as_ref()
                .unwrap()
                .tasks
                .len();
            for i in 0..count {
                let tasks = &mut self.io.request_contexts.background;
                let slot = tasks.parents[index].as_ref().unwrap().tasks[i];
                let parent = tasks.parents[index].as_mut().unwrap();
                if invalid {
                    parent.scheduler.cancel(&slot).expect("registered task");
                    let task = tasks.tasks[slot].as_mut().unwrap();
                    task.cycle_complete = true;
                    task.failure = Some(
                        match next {
                            Err(read_scheduler::Error::InvalidTime) => Error::Invalid,
                            _ => Error::Capacity,
                        }
                        .read_venue_error(),
                    );
                }
                if invalid
                    || tasks.parents[index]
                        .as_ref()
                        .unwrap()
                        .scheduler
                        .retiring(&slot)
                        .is_some()
                {
                    tasks.cleanup(slot);
                }
            }
            self.io.http_plan_poison = true;
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .flush_background_cleanup();
            self.io.http_plan_poison = false;
            self.flush_http_owner_retirements();
            let Some((slot, attempt)) = next.ok().flatten() else {
                Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                }
                .rearm_background_parent(index);
                continue;
            };
            let task = self.io.request_contexts.background.tasks[slot]
                .as_mut()
                .expect("scheduler task index");
            assert!(task.child.is_none() && task.attempt.is_none() && !task.removing);
            if task.cycle_complete {
                task.admissions = 0;
                task.cycle_complete = false;
            }
            task.activation = Some(attempt);
            task.attempt = Some(attempt);
            let task = task.handle.clone();
            let activation = BackgroundActivation {
                task: task.clone(),
                attempt,
            };
            self.io.http_background_activation = Some(ActivationKey::of(&activation));
            self.io.http_plan_poison = true;
            #[cfg(feature = "order-locks")]
            let order_response = self.io.order_response_token.take();
            self.venue.on_background_task_ready(
                &mut Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                },
                &task,
                &activation,
            );
            #[cfg(feature = "order-locks")]
            {
                self.io.order_response_token = order_response;
            }
            let entry = self.io.request_contexts.background.tasks[slot]
                .as_ref()
                .expect("activation task retained");
            let child = entry.child.clone();
            if entry.activation.is_some() {
                Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                }
                .decline_background_task(&activation, Error::State)
                .expect("unconsumed task activation");
            } else if let Some(child) = child
                && self
                    .io
                    .request_contexts
                    .owner_entry(child.owner())
                    .is_some_and(|owner| {
                        matches!(owner.phase, CommandPhase::Planned(_)) && owner.pending == 0
                    })
            {
                Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                }
                .fail_plan(&child, Error::State.read_venue_error())
                .expect("unsubmitted Background Plan");
            }
            self.io.request_contexts.end_plan_admission();
            self.io.http_background_activation = None;
            self.io.http_plan_poison = false;
            self.assert_live();
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .flush_background_cleanup();
            self.flush_http_owner_retirements();
            Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            }
            .rearm_background_parent(index);
            if self.io.request_contexts.background.parents[index]
                .as_ref()
                .and_then(|parent| parent.scheduler.next_deadline())
                .is_some_and(|at| at <= now)
            {
                self.io.request_contexts.background.wake(index);
                visits += 1;
            }
        }
        // Expired/pause work is retired before any later activation can reserve.
        Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        }
        .flush_background_cleanup();
        self.flush_http_owner_retirements();
        while let Some((task, error)) = self.io.request_contexts.background.notifications.pop() {
            self.io.http_owner_poison = true;
            self.venue.on_background_task_failed(&task, error);
            self.io.http_owner_poison = false;
        }
    }
}

impl<P> Cx<'_, P> {
    pub(in crate::engine) fn background_activation_owns(&self, owner: Owner) -> bool {
        self.io.http_background_activation.is_some_and(|key| {
            self.io
                .request_contexts
                .background
                .tasks
                .get(key.slot)
                .and_then(Option::as_ref)
                .is_some_and(|task| {
                    task.handle.generation == key.generation
                        && task
                            .child
                            .as_ref()
                            .is_some_and(|plan| plan.owner() == owner)
                })
        })
    }
}

impl<P> Cx<'_, P> {
    /// The task's captured selection supplies signing, routing and evidence.
    /// The caller never reselects an identity for a fresh traversal or leg.
    pub fn queue_background_read(
        &mut self,
        plan: &Plan,
        context: Context,
        request: crate::selected_read::ReadRequest,
        recipe: impl FnOnce(&crate::signing::Identity) -> Box<dyn crate::SignStep>,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        let selected = (|| {
            self.validate_http_context(&context)?;
            let bound = self
                .io
                .request_contexts
                .plan_from_context(&context)
                .ok_or(Error::Revoked)?;
            if !bound.same_plan(plan) {
                return Err(Error::Revoked);
            }
            let entry = self
                .io
                .request_contexts
                .owner_entry(plan.owner())
                .ok_or(Error::Revoked)?;
            let task = self
                .io
                .request_contexts
                .background
                .owner_task(entry, plan.owner())?;
            Ok(task.selected.clone())
        })();
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) => return Err((pending, error)),
        };
        selected.queue_owned_context(self, context, request, recipe, pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential_selection::{SlotPolicy, Slots};
    use crate::selected_read::ReadRole;
    use crate::signing::{Identity, KeyRef};

    fn selected() -> SelectedRead {
        let identity = Identity {
            credential: KeyRef("task-key".into()),
            signing: KeyRef("task-signing".into()),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        };
        SelectedRead::select(
            &Slots::new(Some(identity), None, SlotPolicy::default()),
            ReadRole::Read { scope: None },
        )
        .unwrap()
    }
    fn policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: crate::request_context::RetryPolicy::Never,
        }
    }
    #[test]
    fn activation_scope_denies_session_effects_before_any_resource_changes() {
        let mut io = crate::engine::Io::<Context>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let session_policy = crate::request_context::SessionPolicy {
            credential: None,
            repeat_safe: true,
        };
        let session = cx.open_session(session_policy.clone()).unwrap();
        let parent = cx.session_context(&session, policy()).unwrap();
        let waiter = cx
            .http_context(crate::request_context::Scope::Background, policy())
            .unwrap();
        cx.session_wait(&session, &waiter, waiter.clone()).unwrap();
        let task = cx
            .register_background_task(
                &session,
                selected(),
                BackgroundTaskPolicy {
                    calls: 1,
                    debounce_ms: 0.,
                    refresh_debounce_ms: 0.,
                },
            )
            .unwrap();
        cx.request_background_task(&task, read_scheduler::Demand::Refresh)
            .unwrap();
        let index = cx
            .io
            .request_contexts
            .background
            .task(&task)
            .unwrap()
            .parent;
        let attempt = cx.io.request_contexts.background.parents[index]
            .as_mut()
            .unwrap()
            .scheduler
            .take_ready(0., |delay| delay)
            .unwrap()
            .unwrap()
            .1;
        let activation = BackgroundActivation { task, attempt };
        cx.io.http_background_activation = Some(ActivationKey::of(&activation));
        let before = (
            cx.io.next_id,
            cx.io.timers.len(),
            cx.io.actions.len(),
            cx.io.request_contexts.contexts.len(),
        );
        assert!(cx.open_session(session_policy).err() == Some(Error::State));
        assert!(cx.session_context(&session, policy()).err() == Some(Error::State));
        assert!(cx.http_leg_context(&parent, policy()).err() == Some(Error::State));
        assert!(cx.retire_session(&session).err() == Some(Error::State));
        assert!(cx.session_ready(&session).is_empty());
        assert!(
            cx.session_failed(&session, VenueError::new("foreign"))
                .is_empty()
        );
        assert!(
            cx.session_wait(&session, &waiter, waiter.clone())
                .err()
                .unwrap()
                .1
                == Error::State
        );
        assert!(
            (
                cx.io.next_id,
                cx.io.timers.len(),
                cx.io.actions.len(),
                cx.io.request_contexts.contexts.len()
            ) == before
        );
        assert!(cx.session_live(&session));
        cx.io.http_background_activation = None;
        assert!(cx.session_ready(&session).len() == 1);
    }
    #[test]
    fn idle_task_storage_is_globally_bounded_without_consuming_context_credits() {
        let mut io = crate::engine::Io::<()>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let session = cx
            .open_session(crate::request_context::SessionPolicy {
                credential: None,
                repeat_safe: true,
            })
            .unwrap();
        let policy = BackgroundTaskPolicy {
            calls: 150,
            debounce_ms: 0.,
            refresh_debounce_ms: 0.,
        };
        for _ in 0..TASKS {
            cx.register_background_task(&session, selected(), policy)
                .unwrap();
        }
        let before = cx.io.next_id;
        assert!(
            cx.register_background_task(&session, selected(), policy)
                .err()
                == Some(Error::Capacity)
        );
        assert!(cx.io.next_id == before);
        assert!(cx.io.request_contexts.contexts.is_empty());
        assert!(cx.io.request_contexts.reserved_contexts == 0);
        assert!(cx.io.actions.is_empty() && cx.io.timers.is_empty());
        assert!(cx.io.request_contexts.background.count == TASKS);
    }
    #[test]
    fn share_refusal_preserves_activation_and_all_resource_namespaces() {
        let mut io = crate::engine::Io::<()>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let session = cx
            .open_session(crate::request_context::SessionPolicy {
                credential: None,
                repeat_safe: true,
            })
            .unwrap();
        let policy = BackgroundTaskPolicy {
            calls: 150,
            debounce_ms: 0.,
            refresh_debounce_ms: 0.,
        };
        for number in 0..2 {
            let task = cx
                .register_background_task(&session, selected(), policy)
                .unwrap();
            cx.request_background_task(&task, read_scheduler::Demand::Refresh)
                .unwrap();
            let index = cx
                .io
                .request_contexts
                .background
                .task(&task)
                .unwrap()
                .parent;
            let attempt = cx.io.request_contexts.background.parents[index]
                .as_mut()
                .unwrap()
                .scheduler
                .take_ready(0., |delay| delay)
                .unwrap()
                .unwrap()
                .1;
            let entry = cx.io.request_contexts.background.tasks[task.slot]
                .as_mut()
                .unwrap();
            entry.activation = Some(attempt);
            entry.attempt = Some(attempt);
            let activation = BackgroundActivation {
                task: task.clone(),
                attempt,
            };
            cx.io.http_background_activation = Some(ActivationKey::of(&activation));
            let before = (
                cx.io.next_id,
                cx.io.signing.available_ids(1),
                cx.io
                    .signing
                    .available_ids(crate::resource_ids::MAX_RESOURCE_ID),
                cx.io.request_contexts.reserved_contexts,
                cx.io.request_contexts.contexts.len(),
                cx.io.request_contexts.physical.len(),
                cx.io.actions.len(),
            );
            if number == 0 {
                assert!(cx.admit_background_plan(&activation).is_ok());
                assert!(
                    cx.io
                        .request_contexts
                        .background_context_count(None)
                        .unwrap()
                        == 150
                );
            } else {
                assert!(cx.admit_background_plan(&activation).err() == Some(Error::Capacity));
                assert!(
                    (
                        cx.io.next_id,
                        cx.io.signing.available_ids(1),
                        cx.io
                            .signing
                            .available_ids(crate::resource_ids::MAX_RESOURCE_ID),
                        cx.io.request_contexts.reserved_contexts,
                        cx.io.request_contexts.contexts.len(),
                        cx.io.request_contexts.physical.len(),
                        cx.io.actions.len()
                    ) == before
                );
                assert!(
                    cx.io.request_contexts.background.tasks[task.slot]
                        .as_ref()
                        .unwrap()
                        .admissions
                        == 0
                );
                cx.decline_background_task(&activation, Error::Capacity)
                    .unwrap();
            }
            cx.io.request_contexts.end_plan_admission();
            cx.io.http_background_activation = None;
        }
    }

    #[test]
    fn unrepresentable_lease_refuses_demand_without_consuming_or_poisoning_it() {
        let mut io = crate::engine::Io::<()>::default();
        let mut store = crate::Store::default();
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        let session = cx
            .open_session(crate::request_context::SessionPolicy {
                credential: None,
                repeat_safe: true,
            })
            .unwrap();
        let task = cx
            .register_background_task(
                &session,
                selected(),
                BackgroundTaskPolicy {
                    calls: 1,
                    debounce_ms: 0.,
                    refresh_debounce_ms: 0.,
                },
            )
            .unwrap();
        let before = (cx.io.next_id, cx.io.actions.len(), cx.io.timers.len());
        cx.now = f64::MAX;
        assert!(
            cx.request_background_task(&task, read_scheduler::Demand::Refresh)
                == Err(Error::Invalid)
        );
        assert!((cx.io.next_id, cx.io.actions.len(), cx.io.timers.len()) == before);
        assert!(cx.io.request_contexts.background.ready.is_empty());
        assert!(!cx.io.http_plan_poison);
    }
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    use crate::book::Book;
    use crate::credential_selection::{SlotPolicy, Slots};
    use crate::selected_read::ReadRole;
    use crate::signing::{Identity, KeyRef, SignError, SignRequest, SignResult};
    use crate::{Action, Command, Event, Method, Response, SignStep, SocketId, Venue};

    struct Recipe;
    impl SignStep for Recipe {
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<SignRequest, SignError> {
            Err(SignError::Refused)
        }
        fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
            unreachable!()
        }
        fn freshness_ms(&self) -> f64 {
            1000.
        }
    }
    #[derive(Default)]
    struct Probe {
        session: Option<Session>,
        plan: Option<Plan>,
        timer: Option<TimerId>,
        socket: Option<SocketId>,
    }
    impl Probe {
        fn retire(&self, cx: &mut Cx<'_, Context>) {
            assert!(
                cx.retire_session(self.session.as_ref().unwrap())
                    .unwrap()
                    .is_empty()
            );
        }
    }
    impl Venue for Probe {
        type Pending = Context;
        fn start(&mut self, cx: &mut Cx<'_, Context>) {
            let session = cx
                .open_session(crate::request_context::SessionPolicy {
                    credential: None,
                    repeat_safe: true,
                })
                .unwrap();
            let identity = Identity {
                credential: KeyRef("cleanup-key".into()),
                signing: KeyRef("cleanup-signing".into()),
                passphrase: None,
                stable_id: None,
                extra: Default::default(),
            };
            let selected = SelectedRead::select(
                &Slots::new(Some(identity), None, SlotPolicy::default()),
                ReadRole::Read { scope: None },
            )
            .unwrap();
            let task = cx
                .register_background_task(
                    &session,
                    selected,
                    BackgroundTaskPolicy {
                        calls: 1,
                        debounce_ms: 0.,
                        refresh_debounce_ms: 0.,
                    },
                )
                .unwrap();
            cx.request_background_task(&task, read_scheduler::Demand::Refresh)
                .unwrap();
            self.session = Some(session);
            self.timer = Some(cx.set_timer(10.));
            self.socket = Some(cx.connect("wss://cleanup.test"));
        }
        fn on_background_task_ready(
            &mut self,
            cx: &mut Cx<'_, Context>,
            _: &BackgroundTask,
            activation: &BackgroundActivation,
        ) {
            let plan = cx.admit_background_plan(activation).unwrap();
            let slot = cx.plan_slots(&plan).unwrap().remove(0);
            let context = cx.plan_leg_context(&plan, &slot).unwrap();
            cx.request_signed_managed(
                context.clone(),
                Method::Get,
                "https://cleanup.test/read".into(),
                None,
                Box::new(Recipe),
                context,
            )
            .unwrap();
            self.plan = Some(plan);
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Context>, timer: TimerId) {
            if self.timer == Some(timer) {
                self.retire(cx);
            }
        }
        fn on_message(&mut self, cx: &mut Cx<'_, Context>, _: SocketId, _: &str) {
            self.retire(cx);
        }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_command(&mut self, _: &mut Cx<'_, Context>, _: Command) {}
        fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {
            panic!("unreleased response");
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    #[test]
    fn retiring_a_parent_from_socket_or_timer_cleans_the_child_in_that_same_event() {
        for socket_event in [false, true] {
            let mut engine = crate::test_support::seeded_engine(Probe::default());
            engine.start(0.);
            assert!(engine.io.managed_http.len() == 1);
            let socket = engine.venue().socket.unwrap();
            if socket_event {
                engine.handle(0., Event::WsOpened { socket });
            }
            let plan = engine.venue().plan.as_ref().unwrap().clone();
            let mut timers = vec![
                engine
                    .io
                    .request_contexts
                    .physical_plan_timer(plan.owner())
                    .unwrap(),
            ];
            timers.extend(engine.io.managed_http.values().map(|managed| managed.timer));
            timers.extend(
                engine
                    .io
                    .request_contexts
                    .background
                    .parents
                    .iter()
                    .flatten()
                    .map(|parent| parent.timer),
            );
            assert!(
                timers
                    .iter()
                    .all(|timer| engine.io.timers.contains_key(timer))
            );
            let timer = engine.venue().timer.unwrap();
            engine.handle(
                10.,
                if socket_event {
                    Event::WsMessage {
                        socket,
                        text: "retire",
                    }
                } else {
                    Event::TimerFired { timer }
                },
            );
            // Inspect directly, before prepare_dispatch or another host event can flush.
            assert!(engine.io.managed_http.is_empty());
            assert!(
                timers
                    .iter()
                    .all(|timer| !engine.io.timers.contains_key(timer))
            );
            assert!(engine.io.request_contexts.physical.is_empty());
            assert!(engine.io.request_contexts.background.count == 0);
            assert!(engine.io.request_contexts.background.timer_count == 0);
        }
    }
}
