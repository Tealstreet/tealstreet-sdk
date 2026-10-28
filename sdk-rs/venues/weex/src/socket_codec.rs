//! Native wire codecs attached to the engine's two independent socket epochs.
use crate::{Pending, WeexFutures};
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, Epoch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, SocketError, TrafficPolicy,
};
use safe_cex_engine::{Cx, SocketChannel, TimerId, Venue};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) public: Option<FeedId>,
    pub(crate) private: Option<FeedId>,
    failed: bool,
    clock: Cell<f64>,
    staged: RefCell<Option<Encoding>>,
    quota_not_before: Cell<Option<f64>>,
    desired: Vec<String>,
    committed: Vec<String>,
    pending: Option<(u64, Vec<String>)>,
    retry: Option<Observation>,
    observations: u8,
}
struct Observation {
    timer: Option<TimerId>,
    not_before: f64,
    epoch: Epoch,
}
struct Encoding {
    next_id: u32,
    operations: VecDeque<f64>,
}
fn policy(role: SocketChannel) -> FeedPolicy {
    FeedPolicy {
        role,
        supervision: Config {
            connect_timeout_ms: 30_000.,
            idle_timeout_ms: 30_000.,
            ping_interval_ms: Some(5_000.),
            backoff_base_ms: 1_000.,
            backoff_cap_ms: 16_000.,
            ..Config::default()
        },
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::Native,
        invalidation: Invalidation::Required,
        subscription_interval_ms: 0.,
        close_floors: &[],
    }
}
impl WeexFutures {
    pub(crate) fn register_feeds(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        if self.sockets.failed || !cx.now.is_finite() {
            return false;
        }
        if self.sockets.public.is_some() && self.sockets.private.is_some() {
            return true;
        }
        let registered = (|| {
            let public = cx.register_socket_feed(policy(SocketChannel::Public))?;
            self.sockets.public = Some(public);
            let private = cx.register_socket_feed(policy(SocketChannel::Private))?;
            self.sockets.private = Some(private);
            Ok::<(), SocketError>(())
        })();
        if let Err(error) = registered {
            self.sockets.failed = true;
            if let Some(feed) = self.sockets.public {
                cx.stop_socket_feed(feed);
            }
            cx.notice(
                safe_cex_engine::Level::Warn,
                safe_cex_engine::log::code::STREAM_UNAVAILABLE,
                format!("WEEX socket registration failed: {error:?}"),
            );
            return false;
        }
        true
    }
    fn commit_encoding(&mut self) {
        if let Some(staged) = self.sockets.staged.get_mut().take() {
            self.public.next_id = staged.next_id;
            self.public.operations = staged.operations;
        }
    }
    pub(crate) fn encode_interests(
        &self,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if Some(feed) != self.sockets.public {
            return Err(SocketError::NotPermitted);
        }
        let now = self.sockets.clock.get();
        if !now.is_finite() {
            return Err(SocketError::InvalidTime);
        }
        let order = if subscribe {
            self.sockets
                .pending
                .as_ref()
                .map_or(&self.sockets.committed, |p| &p.1)
        } else {
            &self.sockets.committed
        };
        let mut ordered: Vec<_> = items.iter().collect();
        ordered.sort_by_key(|item| {
            order
                .iter()
                .position(|key| key == item.key.as_ref())
                .unwrap_or(usize::MAX)
        });
        let mut staged = self.sockets.staged.borrow_mut();
        let state = staged.get_or_insert_with(|| Encoding {
            next_id: self.public.next_id,
            operations: self.public.operations.clone(),
        });
        while state
            .operations
            .front()
            .is_some_and(|at| now - at >= 3_600_000.)
        {
            state.operations.pop_front();
        }
        let required = state.operations.len() + items.len().div_ceil(50);
        if required > 240 {
            let floor = state
                .operations
                .get(required - 241)
                .map(|at| at + 3_600_000.);
            self.sockets
                .quota_not_before
                .set(floor.filter(|at| at.is_finite() && *at > now));
            return Err(SocketError::Capacity);
        }
        let method = if subscribe {
            "SUBSCRIBE"
        } else {
            "UNSUBSCRIBE"
        };
        for lot in ordered.chunks(50) {
            let id = state.next_id;
            let next = id.checked_add(1).ok_or(SocketError::Capacity)?;
            let params: Vec<_> = lot.iter().map(|item| item.wire_args.as_ref()).collect();
            let params =
                serde_json::to_string(&params).map_err(|_| SocketError::InvalidInterest)?;
            let text = format!(r#"{{"method":"{method}","params":{params},"id":{id}}}"#);
            if text.len() > safe_cex_engine::socket_lifecycle::MAX_FRAME_BYTES {
                return Err(SocketError::Capacity);
            }
            emit(EncodedBatch {
                text,
                covered_keys: lot.iter().map(|i| i.key.clone()).collect(),
            })?;
            state.next_id = next;
            state.operations.push_back(now);
        }
        Ok(())
    }
    pub(crate) fn update_public_interests(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        desired: Vec<String>,
    ) {
        self.sockets.clock.set(cx.now);
        self.sockets.desired = desired;
        if self.sockets.pending.is_none() && self.sockets.desired == self.sockets.committed {
            if let Some(retry) = self.sockets.retry.take()
                && let Some(timer) = retry.timer
            {
                cx.clear_timer(timer);
            }
            self.sockets.observations = 0;
            return;
        }
        self.propose_public_interests(cx, false);
    }
    fn propose_public_interests(&mut self, cx: &mut Cx<'_, Pending>, observation: bool) {
        if !cx.now.is_finite() {
            return;
        }
        if let Some(retry) = self.sockets.retry.as_mut() {
            if retry.timer.is_none() && cx.socket_epoch(retry.epoch.feed()) == Some(retry.epoch) {
                retry.timer = Some(cx.set_timer((retry.not_before - cx.now).max(0.)));
            }
            return;
        }
        if !self.public.open
            || self.sockets.pending.is_some()
            || self.sockets.observations >= 3 && !observation
        {
            return;
        }
        if self.sockets.desired == self.sockets.committed {
            return;
        }
        let Some(feed) = self.sockets.public else {
            return;
        };
        let desired = self.sockets.desired.clone();
        self.sockets.staged.get_mut().take();
        self.sockets.quota_not_before.set(None);
        match cx.set_socket_interests(
            feed,
            desired
                .iter()
                .cloned()
                .map(Interest::from_wire_args)
                .collect(),
        ) {
            Ok(proposal) => self.sockets.pending = Some((proposal, desired)),
            Err(_) => self.defer_interests(cx),
        }
    }
    fn defer_interests(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.now.is_finite() || self.sockets.retry.is_some() || self.sockets.observations >= 3 {
            return;
        }
        let Some(epoch) = self.sockets.public.and_then(|f| cx.socket_epoch(f)) else {
            return;
        };
        let not_before = (cx.now + 1_000.).max(
            self.sockets
                .quota_not_before
                .get()
                .unwrap_or(f64::NEG_INFINITY),
        );
        if !not_before.is_finite() || not_before <= cx.now {
            return;
        }
        self.sockets.retry = Some(Observation {
            timer: Some(cx.set_timer(not_before - cx.now)),
            not_before,
            epoch,
        });
    }
    pub(crate) fn interest_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if !self
            .sockets
            .retry
            .as_ref()
            .is_some_and(|r| r.timer == Some(timer))
        {
            return false;
        }
        let mut retry = self.sockets.retry.take().expect("matched observation");
        if cx.socket_epoch(retry.epoch.feed()) != Some(retry.epoch) {
            return true;
        }
        if !cx.now.is_finite() {
            retry.timer = None;
            self.sockets.retry = Some(retry);
            return true;
        }
        if cx.now < retry.not_before {
            retry.timer = Some(cx.set_timer(retry.not_before - cx.now));
            self.sockets.retry = Some(retry);
            return true;
        }
        if self.sockets.observations >= 3 {
            return true;
        }
        self.sockets.observations += 1;
        self.sockets.clock.set(cx.now);
        // This wake observes demand only; the engine still owns all socket sends.
        self.propose_public_interests(cx, true);
        true
    }
    pub(crate) fn socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        self.sockets.clock.set(cx.now);
        match event {
            LifecycleEvent::Opened(epoch) => {
                if Some(epoch.feed()) == self.sockets.public {
                    self.sockets.observations = 0;
                    self.on_open(cx, epoch.socket());
                } else if Some(epoch.feed()) == self.sockets.private {
                    self.account_open(cx, epoch.socket());
                }
            }
            LifecycleEvent::Ready { epoch, .. } if Some(epoch.feed()) == self.sockets.public => {
                self.commit_encoding()
            }
            LifecycleEvent::Ping(epoch) => {
                if Some(epoch.feed()) == self.sockets.public {
                    let id = self.public.next_id;
                    self.public.next_id += 1;
                    cx.send(epoch.socket(), format!(r#"{{"method":"PING","id":{id}}}"#));
                } else if Some(epoch.feed()) == self.sockets.private {
                    self.account_ping(cx, epoch.socket());
                }
            }
            LifecycleEvent::Reconnect(feed) => {
                if Some(feed) == self.sockets.public {
                    self.connect_public(cx);
                    let symbols: Vec<_> = self.books.keys().cloned().collect();
                    for symbol in symbols {
                        self.snapshot(cx, &symbol);
                    }
                } else if Some(feed) == self.sockets.private {
                    self.connect_account(cx);
                }
            }
            LifecycleEvent::Retired { epoch, .. } => {
                self.on_close(cx, epoch.socket(), 1006);
                if Some(epoch.feed()) == self.sockets.public {
                    if let Some(retry) = self.sockets.retry.take()
                        && let Some(timer) = retry.timer
                    {
                        cx.clear_timer(timer);
                    }
                    self.sockets.pending = None;
                    self.sockets.staged.get_mut().take();
                    self.sockets.committed.clear();
                    self.sockets.desired.clear();
                    // A reconnect must not replay depth made ineligible by native close.
                    cx.set_socket_interests(epoch.feed(), Vec::new())?;
                }
            }
            LifecycleEvent::InterestsCommitted { feed, proposal }
                if Some(feed) == self.sockets.public =>
            {
                if self
                    .sockets
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.0 == proposal)
                {
                    let (_, desired) = self.sockets.pending.take().expect("matching proposal");
                    self.commit_encoding();
                    self.sockets.committed = desired;
                    if self.sockets.committed == self.sockets.desired {
                        if let Some(retry) = self.sockets.retry.take()
                            && let Some(timer) = retry.timer
                        {
                            cx.clear_timer(timer);
                        }
                        self.sockets.observations = 0;
                    }
                    self.propose_public_interests(cx, false);
                } else {
                    self.sockets.staged.get_mut().take();
                }
            }
            LifecycleEvent::InterestsRefused { feed, proposal, .. }
                if Some(feed) == self.sockets.public =>
            {
                self.sockets.staged.get_mut().take();
                if self
                    .sockets
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.0 == proposal)
                {
                    self.sockets.pending = None;
                    if self.sockets.quota_not_before.get().is_some() {
                        cx.notice(
                            safe_cex_engine::Level::Warn,
                            safe_cex_engine::log::code::SUBSCRIPTION_REJECTED,
                            "WEEX subscription operation limit reached",
                        );
                    }
                    self.defer_interests(cx);
                }
            }
            _ => {}
        }
        Ok(())
    }
}
