//! Shared bounded park/resume state for HTTP, WS messages and WS upgrades.
use crate::signing::{Algorithm, Operation, SignError, SignRequest, SignResult};
use crate::{Action, HttpRequest, RequestId, SocketId, TimerId};
use std::collections::HashMap;

/// Canonicalisation and result insertion only; no keys, clocks or I/O.
pub trait SignStep: Send {
    fn prepare(&mut self, outbound: &mut Action, now: f64) -> Result<SignRequest, SignError>;
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError>;
    fn freshness_ms(&self) -> f64;
}

pub(crate) struct Pending {
    /// Always unsigned while stored in Dispatch, including completed metadata.
    pub outbound: Action,
    unsigned: Action,
    step: Box<dyn SignStep>,
    sequence: u32,
    plain: bool,
    pub timer: TimerId,
    pub deadline: f64,
    staged_at: f64,
    prepared_at: Option<f64>,
    prepared_revision: Option<u64>,
    expected: Vec<Option<usize>>,
}
impl Pending {
    pub fn request_id(&self) -> Option<RequestId> {
        match &self.outbound {
            Action::Http(http) => Some(http.id),
            _ => None,
        }
    }
    pub fn socket(&self) -> Option<SocketId> {
        match &self.outbound {
            Action::WsSend { socket, .. } | Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        }
    }
}
#[derive(Default)]
pub(crate) struct Dispatch {
    pending: HashMap<RequestId, Pending>,
    /// Metadata only: materialised values live exclusively in the outbound action.
    outbound: HashMap<RequestId, Pending>,
    next: u32,
}
pub(crate) enum Completion {
    Ready(Pending, Action),
    Stale(Pending),
    Failed(Pending, SignError),
}
impl Dispatch {
    pub fn next_id(&mut self) -> RequestId {
        self.next = self
            .next
            .checked_add(1)
            .expect("signing attempt IDs exhausted");
        RequestId(self.next)
    }
    fn head(&self) -> Option<(RequestId, &Pending)> {
        self.pending
            .iter()
            .chain(self.outbound.iter())
            .min_by_key(|(_, p)| p.sequence)
            .map(|(id, p)| (*id, p))
    }
    pub fn next_work(&self) -> Option<RequestId> {
        self.head().and_then(|(id, p)| {
            (!p.plain && p.prepared_at.is_none() && self.pending.contains_key(&id)).then_some(id)
        })
    }
    pub fn can_prepare(&self, id: RequestId) -> bool {
        self.next_work() == Some(id)
    }
    pub fn take_plain(&mut self) -> Option<Action> {
        let (id, p) = self.head()?;
        if !p.plain {
            return None;
        }
        self.pending.remove(&id).map(|p| p.outbound)
    }
    pub fn ready(&mut self, id: RequestId, pending: Pending) {
        self.outbound.insert(id, pending);
    }
    pub fn release(
        &mut self,
        id: RequestId,
        mut action: Action,
        now: f64,
        clock_revision: u64,
    ) -> Option<Completion> {
        let Some(pending) = self.outbound.remove(&id) else {
            wipe(&mut action);
            return None;
        };
        if !now.is_finite() || now >= pending.deadline {
            wipe(&mut action);
            return Some(Completion::Failed(pending, SignError::TimedOut));
        }
        if pending.prepared_revision != Some(clock_revision)
            || pending
                .prepared_at
                .is_some_and(|at| now - at >= pending.step.freshness_ms())
        {
            wipe(&mut action);
            return Some(Completion::Stale(pending));
        }
        if let Action::Http(http) = &mut action
            && (http.timeout_ms.is_some() || pending.timer.0 != 0 || now != pending.staged_at)
        {
            http.timeout_ms = Some((pending.deadline - now).ceil().max(1.) as u32);
        }
        Some(Completion::Ready(pending, action))
    }
    pub fn has_socket(&self, socket: SocketId) -> bool {
        self.pending
            .values()
            .chain(self.outbound.values())
            .any(|p| p.socket() == Some(socket))
    }
    pub fn hold(&mut self, outbound: Action) -> Result<(), SignError> {
        let id = self.next_id();
        self.stage(id, outbound, Box::new(Plain), TimerId(0), 0.)?;
        if let Some(p) = self.pending.get_mut(&id) {
            p.deadline = f64::MAX;
            p.plain = true;
        }
        Ok(())
    }
    pub fn timer(&self, id: RequestId) -> Option<TimerId> {
        self.pending.get(&id).map(|p| p.timer)
    }
    pub fn arm(&mut self, id: RequestId, timer: TimerId) -> Option<f64> {
        let pending = self.pending.get_mut(&id)?;
        pending.timer = timer;
        Some(pending.deadline)
    }
    pub fn stage(
        &mut self,
        id: RequestId,
        outbound: Action,
        step: Box<dyn SignStep>,
        timer: TimerId,
        now: f64,
    ) -> Result<f64, SignError> {
        if self.pending.len() + self.outbound.len() >= 256 || !now.is_finite() {
            return Err(SignError::Unavailable);
        }
        let timeout = match &outbound {
            Action::Http(http) => http.timeout_ms.unwrap_or(20_000).max(1),
            Action::WsSend { .. } | Action::WsConnect { .. } => 20_000,
            _ => return Err(SignError::InvalidRequest),
        };
        if !step.freshness_ms().is_finite() || step.freshness_ms() <= 0. {
            return Err(SignError::InvalidRequest);
        }
        let deadline = now + f64::from(timeout);
        self.pending.insert(
            id,
            Pending {
                unsigned: outbound.clone(),
                outbound,
                step,
                sequence: id.0,
                plain: false,
                timer,
                deadline,
                staged_at: now,
                prepared_at: None,
                prepared_revision: None,
                expected: vec![],
            },
        );
        Ok(deadline)
    }
    pub fn request_mut(&mut self, request: RequestId) -> Option<&mut HttpRequest> {
        let p = self
            .pending
            .values_mut()
            .find(|p| p.request_id() == Some(request))?;
        match &mut p.outbound {
            Action::Http(http) => Some(http),
            _ => None,
        }
    }
    pub fn timeout_request(&mut self, request: RequestId, timeout: u32) -> Option<(TimerId, f64)> {
        let p = self
            .pending
            .values_mut()
            .find(|p| p.request_id() == Some(request))?;
        if let Action::Http(http) = &mut p.outbound {
            http.timeout_ms = Some(timeout.max(1));
        }
        p.deadline = p.staged_at + f64::from(timeout.max(1));
        Some((p.timer, p.deadline))
    }
    pub fn prepare(
        &mut self,
        id: RequestId,
        now: f64,
        protocol_now: f64,
        clock_revision: u64,
    ) -> Result<SignRequest, SignError> {
        let p = self.pending.get_mut(&id).ok_or(SignError::Unavailable)?;
        if !now.is_finite() || now >= p.deadline {
            return Err(SignError::TimedOut);
        }
        if !protocol_now.is_finite() {
            return Err(SignError::InvalidRequest);
        }
        if let (Action::Http(original), Action::Http(current)) = (&mut p.unsigned, &p.outbound) {
            original.route = current.route;
            original.timeout_ms = current.timeout_ms;
        }
        p.outbound = p.unsigned.clone();
        let work = p.step.prepare(&mut p.outbound, protocol_now)?;
        work.validate()?;
        p.expected = work
            .operations
            .iter()
            .map(|op| match op {
                Operation::Sha256 { .. } => Some(32),
                Operation::Present { .. } => None,
                Operation::Sign { algorithm, .. } => Some(match algorithm {
                    Algorithm::HmacSha256 | Algorithm::Sha256SecretSuffix => 32,
                    Algorithm::HmacSha512
                    | Algorithm::Ed25519
                    | Algorithm::Es256
                    | Algorithm::StarkDigest
                    | Algorithm::SchnorrDigest => 64,
                    Algorithm::Secp256k1Digest => 65,
                }),
            })
            .collect();
        p.prepared_at = Some(now);
        p.prepared_revision = Some(clock_revision);
        Ok(work)
    }
    pub fn timeout_ms(&self, id: RequestId, now: f64) -> u32 {
        self.pending
            .get(&id)
            .map_or(1, |p| (p.deadline - now).ceil().max(1.) as u32)
    }
    pub fn finish(
        &mut self,
        id: RequestId,
        now: f64,
        clock_revision: u64,
        result: Result<SignResult, SignError>,
    ) -> Option<Completion> {
        // Duplicate/unsolicited success cannot consume an unprepared attempt.
        if result.is_ok()
            && self
                .pending
                .get(&id)
                .is_none_or(|p| p.prepared_at.is_none())
        {
            return None;
        }
        let mut p = self.pending.remove(&id)?;
        if !now.is_finite() || now >= p.deadline {
            return Some(Completion::Failed(p, SignError::TimedOut));
        }
        // A stale provider error belongs to the old canonical payload too.
        // Reprepare with a new attempt identity before interpreting any result.
        if p.prepared_revision.is_some_and(|at| at != clock_revision) {
            return Some(Completion::Stale(p));
        }
        let result = match result {
            Ok(result) => result,
            Err(error) => return Some(Completion::Failed(p, error)),
        };
        if p.prepared_at.is_none()
            || result.0.len() != p.expected.len()
            || p.expected
                .iter()
                .zip(&result.0)
                .any(|(n, b)| b.len() > 1024 * 1024 || n.is_some_and(|n| b.len() != n))
        {
            return Some(Completion::Failed(p, SignError::InvalidResult));
        }
        if now - p.prepared_at.unwrap() >= p.step.freshness_ms() {
            return Some(Completion::Stale(p));
        }
        if let Err(error) = p.step.apply(&mut p.outbound, &result) {
            wipe(&mut p.outbound);
            p.outbound = p.unsigned.clone();
            return Some(Completion::Failed(p, error));
        }
        let action = std::mem::replace(&mut p.outbound, p.unsigned.clone());
        Some(Completion::Ready(p, action))
    }
    pub fn retry(&mut self, id: RequestId, mut pending: Pending) {
        pending.prepared_at = None;
        pending.prepared_revision = None;
        self.pending.insert(id, pending);
    }
    pub fn expire(&mut self, timer: TimerId) -> Option<(RequestId, Pending)> {
        let id = *self.pending.iter().find(|(_, p)| p.timer == timer)?.0;
        self.pending.remove(&id).map(|p| (id, p))
    }
    pub fn remove_request(&mut self, request: RequestId) -> Option<Pending> {
        for entries in [&mut self.pending, &mut self.outbound] {
            if let Some(id) = entries
                .iter()
                .find_map(|(id, p)| (p.request_id() == Some(request)).then_some(*id))
            {
                return entries.remove(&id);
            }
        }
        None
    }
    pub fn close_socket(&mut self, socket: SocketId) -> Vec<TimerId> {
        let mut timers = vec![];
        for entries in [&mut self.pending, &mut self.outbound] {
            entries.retain(|_, p| {
                let keep = p.socket() != Some(socket);
                if !keep {
                    timers.push(p.timer);
                }
                keep
            });
        }
        timers
    }
}
fn wipe(action: &mut Action) {
    use zeroize::Zeroize;
    match action {
        Action::Http(http) => {
            http.url.zeroize();
            for (_, value) in &mut http.headers {
                value.zeroize();
            }
            for (_, value) in &mut http.signer_headers {
                value.zeroize();
            }
            if let Some(body) = &mut http.body {
                body.zeroize();
            }
        }
        Action::WsSend { text, .. } => text.zeroize(),
        Action::WsConnect { url, headers, .. } => {
            url.zeroize();
            for (_, value) in headers {
                value.zeroize();
            }
        }
        _ => {}
    }
}
struct Plain;
impl SignStep for Plain {
    fn prepare(&mut self, _: &mut Action, _: f64) -> Result<SignRequest, SignError> {
        Err(SignError::InvalidRequest)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        f64::MAX
    }
}
