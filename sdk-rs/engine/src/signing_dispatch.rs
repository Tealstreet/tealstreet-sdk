//! Shared bounded park/resume state for HTTP, WS messages and WS upgrades.
use crate::signing::{SignError, SignReceipt, SignRequest, SignResult, validate_result_shape};
use crate::signing_headers::ResultHeaderPolicy;
use crate::{Action, HttpRequest, RequestId, SocketId, TimerId};

/// Narrow send-time view: protocol clock and injected signing entropy only.
/// Identity replay seeding never supplies nonce bytes to this context.
pub struct DispatchContext<'a> {
    pub now_ms: f64,
    entropy: Option<&'a crate::entropy::Entropy>,
}
impl<'a> DispatchContext<'a> {
    pub(crate) fn new(now_ms: f64, entropy: Option<&'a crate::entropy::Entropy>) -> Self {
        Self { now_ms, entropy }
    }
    /// Integer protocol milliseconds captured for this signing attempt.
    /// Use this for wire timestamps; `now_ms` retains submillisecond precision
    /// for protocol freshness calculations.
    pub fn stamp_ms(&self) -> f64 {
        crate::time::wire_stamp_ms(self.now_ms)
    }
    /// A wire stamp never negative and, with a floor, strictly after the
    /// released stamp it came from, even on a fixed or backward clock. Without
    /// a floor it is `stamp_ms()` on valid nonnegative clocks.
    pub fn stamp_after(&self, floor: Option<crate::StampFloor>) -> Result<f64, SignError> {
        const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.;
        let current = self.stamp_ms();
        if !current.is_finite() {
            return Err(SignError::InvalidRequest);
        }
        let next = floor.map_or(0., |floor| floor.millis() + 1.);
        let stamp = current.max(0.).max(next);
        if stamp > MAX_SAFE_INTEGER {
            return Err(SignError::InvalidRequest);
        }
        Ok(stamp)
    }
    /// Draw anew on every preparation. Failure clears even partial output.
    pub fn signing_bytes(&self, bytes: &mut [u8]) -> Result<(), SignError> {
        use zeroize::Zeroize;
        let result = self
            .entropy
            .ok_or(SignError::Unavailable)
            .and_then(|entropy| entropy.signing(bytes).map_err(|_| SignError::Unavailable));
        if result.is_err() {
            bytes.zeroize();
        }
        result
    }
}

/// Canonicalisation and result insertion only; no keys or I/O.
pub trait SignStep: Send {
    /// Static public codec data for the unsigned managed transport intent.
    /// Credentials and signatures belong exclusively to provider/apply results.
    /// The managed queue snapshots this once; it grants no callback authority.
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        context: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError>;
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError>;
    fn freshness_ms(&self) -> f64;
    /// Only trusted HTTP recipes may declare callback header precedence.
    fn result_header_policy(&self) -> ResultHeaderPolicy {
        ResultHeaderPolicy::Deny
    }
}

pub(crate) struct Pending {
    /// Always unsigned while stored in Dispatch, including completed metadata.
    pub outbound: Action,
    unsigned: Action,
    step: Box<dyn SignStep>,
    sequence: u64,
    plain: bool,
    pub managed: bool,
    route_ready: bool,
    pub timer: TimerId,
    pub deadline: f64,
    staged_at: f64,
    prepared_at: Option<f64>,
    prepared_revision: Option<u64>,
    expected: Vec<Option<usize>>,
    pub receipt: SignReceipt,
    prepared_protocol_at: Option<f64>,
    header_policy: ResultHeaderPolicy,
    /// Whether the prepared request names primary material (`primary_owns`).
    primary: bool,
}
impl Pending {
    fn receipt_expired(&self, raw_now: f64) -> bool {
        self.receipt.expiry_ms.is_some_and(|expiry| {
            let protocol_now = self.prepared_protocol_at.unwrap_or(f64::NAN)
                + (raw_now - self.prepared_at.unwrap_or(f64::NAN));
            !protocol_now.is_finite() || protocol_now >= expiry
        })
    }
    pub fn managed_freshness(&self) -> Option<(u64, f64)> {
        Some((
            self.prepared_revision?,
            (self.prepared_at? + self.step.freshness_ms()).min(self.deadline),
        ))
    }
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
    pending: crate::IdMap<RequestId, Pending>,
    /// Metadata only: materialised values live exclusively in the outbound action.
    outbound: crate::IdMap<RequestId, Pending>,
    next: crate::resource_ids::ResourceIds,
    /// A preparation that named a missing credential ref, and whether that
    /// was primary material; taken once.
    credential_refused: Option<bool>,
}
pub(crate) enum Completion {
    Ready(Pending, Action),
    Stale(Pending),
    Failed(Pending, SignError),
}
impl Dispatch {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.outbound.is_empty()
    }

    pub fn take_credential_refusal(&mut self) -> Option<bool> {
        self.credential_refused.take()
    }
    /// Whether the attempt's prepared request names primary material.
    pub fn prepared_primary(&self, id: RequestId) -> bool {
        self.pending.get(&id).is_some_and(|p| p.primary)
    }
    pub fn next_id(&mut self) -> RequestId {
        RequestId(self.next.next_or_fatal())
    }
    pub fn available_id(&self) -> bool {
        self.next.available(1)
    }
    #[cfg(feature = "http-plans")]
    pub(crate) fn available_ids(&self, count: u64) -> bool {
        self.next.available(count)
    }
    #[cfg(feature = "http-plans")]
    pub(crate) fn reserve_ids(&mut self, count: u64) -> Option<std::ops::Range<u64>> {
        self.next.reserve(count)
    }
    pub fn failed(&self) -> bool {
        self.next.failed()
    }
    #[cfg(test)]
    pub(crate) fn set_test_ids(&mut self, ids: crate::resource_ids::ResourceIds) {
        self.next = ids;
    }
    fn head(&self) -> Option<(RequestId, &Pending)> {
        let entries = || self.pending.iter().chain(self.outbound.iter());
        let first = entries().min_by_key(|(_, p)| p.sequence)?;
        if first.1.managed {
            // Independent managed HTTP can finish route/provider preparation in
            // either order. Keep the existing non-HTTP/legacy sequence barrier.
            let barrier = entries()
                .filter(|(_, p)| !p.managed)
                .map(|(_, p)| p.sequence)
                .min();
            if let Some(ready) = self
                .pending
                .iter()
                .filter(|(_, p)| {
                    p.managed
                        && p.route_ready
                        && p.prepared_at.is_none()
                        && barrier.is_none_or(|sequence| p.sequence < sequence)
                })
                .min_by_key(|(_, p)| p.sequence)
            {
                return Some((*ready.0, ready.1));
            }
        }
        Some((*first.0, first.1))
    }
    pub fn next_work(&self) -> Option<RequestId> {
        self.head().and_then(|(id, p)| {
            (!p.plain && p.route_ready && p.prepared_at.is_none() && self.pending.contains_key(&id))
                .then_some(id)
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
        if pending.receipt_expired(now)
            || pending.prepared_revision != Some(clock_revision)
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
    /// Read-only target for engine socket epoch/expiry admission. The pending
    /// recipe and all HTTP lifecycle state remain owned by Dispatch.
    #[cfg(feature = "managed-sockets")]
    pub fn outbound_socket(&self, attempt: RequestId) -> Option<(SocketId, bool)> {
        let pending = self
            .pending
            .get(&attempt)
            .or_else(|| self.outbound.get(&attempt))?;
        pending.socket().map(|socket| {
            (
                socket,
                matches!(&pending.outbound, Action::WsConnect { .. }),
            )
        })
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
        let header_policy = step.result_header_policy();
        if !matches!(header_policy, ResultHeaderPolicy::Deny)
            && !matches!(outbound, Action::Http(_))
        {
            return Err(SignError::InvalidRequest);
        }
        self.pending.insert(
            id,
            Pending {
                unsigned: outbound.clone(),
                outbound,
                step,
                sequence: id.0,
                plain: false,
                managed: false,
                route_ready: true,
                timer,
                deadline,
                staged_at: now,
                prepared_at: None,
                prepared_revision: None,
                expected: vec![],
                receipt: Default::default(),
                prepared_protocol_at: None,
                header_policy,
                primary: true,
            },
        );
        Ok(deadline)
    }
    pub fn stage_managed(
        &mut self,
        id: RequestId,
        outbound: Action,
        step: Box<dyn SignStep>,
        now: f64,
        deadline: f64,
    ) -> Result<(), SignError> {
        self.stage(id, outbound, step, TimerId(0), now)?;
        let p = self.pending.get_mut(&id).expect("staged recipe");
        p.managed = true;
        p.route_ready = false;
        p.deadline = deadline;
        Ok(())
    }
    pub(crate) fn validate_managed_activation(
        &self,
        id: RequestId,
        deadline: f64,
    ) -> Result<(), SignError> {
        let p = self.pending.get(&id).ok_or(SignError::Unavailable)?;
        if !p.managed || p.route_ready || !deadline.is_finite() || deadline <= p.staged_at {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
    pub fn activate_managed(&mut self, id: RequestId, deadline: f64) -> Result<(), SignError> {
        self.validate_managed_activation(id, deadline)?;
        let p = self.pending.get_mut(&id).expect("validated managed recipe");
        // The route can only tighten the staged physical cap. Delayed provider
        // preparation and every stale retry retain the same immutable bound.
        p.deadline = p.deadline.min(deadline);
        p.route_ready = true;
        Ok(())
    }
    pub fn signature_pending(&self, id: RequestId) -> bool {
        self.pending
            .get(&id)
            .is_some_and(|p| p.prepared_at.is_some())
    }
    pub fn is_managed(&self, id: RequestId) -> bool {
        self.pending.get(&id).is_some_and(|p| p.managed)
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
        let requested = p.staged_at + f64::from(timeout.max(1));
        p.deadline = if p.managed {
            p.deadline.min(requested)
        } else {
            requested
        };
        if let Action::Http(http) = &mut p.outbound {
            http.timeout_ms = Some(if p.managed {
                (p.deadline - p.staged_at).ceil().max(1.) as u32
            } else {
                timeout.max(1)
            });
        }
        Some((p.timer, p.deadline))
    }
    pub fn prepare(
        &mut self,
        id: RequestId,
        now: f64,
        context: &DispatchContext<'_>,
        clock_revision: u64,
        identities: &crate::signing::Identities,
    ) -> Result<SignRequest, SignError> {
        let p = self.pending.get_mut(&id).ok_or(SignError::Unavailable)?;
        if !now.is_finite() || now >= p.deadline {
            return Err(SignError::TimedOut);
        }
        if !context.now_ms.is_finite() {
            return Err(SignError::InvalidRequest);
        }
        if let (Action::Http(original), Action::Http(current)) = (&mut p.unsigned, &p.outbound) {
            original.route = current.route;
            original.timeout_ms = current.timeout_ms;
        }
        p.outbound = p.unsigned.clone();
        let work = match p.step.prepare(&mut p.outbound, context) {
            Ok(work) => work,
            Err(error) => {
                // A login refused before naming refs uses the primary.
                if error == SignError::MissingKey {
                    let login =
                        matches!(p.outbound, Action::WsSend { .. } | Action::WsConnect { .. });
                    self.credential_refused = Some(login || identities.secondary.is_none());
                }
                return Err(error);
            }
        };
        if !matches!(p.header_policy, ResultHeaderPolicy::Deny)
            && !matches!(p.outbound, Action::Http(_))
        {
            return Err(SignError::InvalidRequest);
        }
        if !matches!(p.header_policy, ResultHeaderPolicy::Deny)
            && work
                .operations
                .iter()
                .filter(|operation| matches!(operation, crate::signing::Operation::Sign { .. }))
                .count()
                != 1
        {
            return Err(SignError::InvalidRequest);
        }
        if let Err(error) = work.validate() {
            if !work.has_invalid_key_ref() {
                return Err(error);
            }
            self.credential_refused = Some(identities.primary_owns(&work));
            return Err(SignError::MissingKey);
        }
        p.primary = identities.primary_owns(&work);
        // Refuse a non-progressing recipe before an immediate provider can enter
        // an endless stale/reprepare loop at one unchanged dispatch clock.
        if work
            .receipt
            .expiry_ms
            .is_some_and(|expiry| expiry <= context.now_ms)
        {
            return Err(SignError::InvalidRequest);
        }
        p.receipt = work.receipt;
        p.prepared_protocol_at = Some(context.now_ms);
        p.expected = work.expected_result_lengths();
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
            || validate_result_shape(&p.expected, p.receipt, &result).is_err()
            || p.header_policy.validate(&result.headers).is_err()
        {
            return Some(Completion::Failed(p, SignError::InvalidResult));
        }
        if p.receipt_expired(now) || now - p.prepared_at.unwrap() >= p.step.freshness_ms() {
            return Some(Completion::Stale(p));
        }
        if let Err(error) = p.step.apply(&mut p.outbound, &result) {
            wipe(&mut p.outbound);
            p.outbound = p.unsigned.clone();
            return Some(Completion::Failed(p, error));
        }
        if !result.headers.is_empty() || matches!(p.header_policy, ResultHeaderPolicy::Http { .. })
        {
            let merged = match &mut p.outbound {
                Action::Http(http) => p.header_policy.apply_http(http, &result.headers),
                _ => Err(SignError::InvalidResult),
            };
            if let Err(error) = merged {
                wipe(&mut p.outbound);
                p.outbound = p.unsigned.clone();
                return Some(Completion::Failed(p, error));
            }
        }
        let action = std::mem::replace(&mut p.outbound, p.unsigned.clone());
        Some(Completion::Ready(p, action))
    }
    pub fn retry(&mut self, id: RequestId, mut pending: Pending) {
        pending.prepared_at = None;
        pending.prepared_revision = None;
        pending.prepared_protocol_at = None;
        pending.receipt = Default::default();
        pending.route_ready = !pending.managed;
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
pub(crate) fn wipe(action: &mut Action) {
    use zeroize::Zeroize;
    match action {
        Action::Http(http) => wipe_http(http),
        // A Bind may carry the Session Bearer, the engine's one token copy.
        #[cfg(feature = "session-bearer")]
        Action::ManagedHttp(managed) => {
            if let crate::request_context::HttpAction::Bind { request, .. } = &mut **managed {
                wipe_http(request);
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
fn wipe_http(http: &mut crate::HttpRequest) {
    use zeroize::Zeroize;
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
struct Plain;
impl SignStep for Plain {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        Err(SignError::InvalidRequest)
    }
    fn apply(&mut self, _: &mut Action, _: &SignResult) -> Result<(), SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        f64::MAX
    }
}

#[cfg(test)]
mod header_tests {
    use super::*;
    use crate::signing::{Algorithm, Context, KeyRef, Operation};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Step {
        policy: ResultHeaderPolicy,
        calls: Arc<AtomicUsize>,
    }
    impl SignStep for Step {
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &DispatchContext<'_>,
        ) -> Result<SignRequest, SignError> {
            Ok(SignRequest {
                receipt: SignReceipt {
                    stamp_ms: Some(1000.),
                    expiry_ms: None,
                },
                operations: vec![Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key: KeyRef("ref:owned".into()),
                    payload: vec![],
                }],
                context: Context::default(),
            })
        }
        fn apply(&mut self, action: &mut Action, _: &SignResult) -> Result<(), SignError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let Action::Http(http) = action else {
                return Err(SignError::InvalidRequest);
            };
            http.headers.push(("X-MBX-APIKEY", "native".into()));
            http.signer_headers
                .push(("x-mbx-apikey".into(), "stale".into()));
            Ok(())
        }
        fn freshness_ms(&self) -> f64 {
            5000.
        }
        fn result_header_policy(&self) -> ResultHeaderPolicy {
            self.policy
        }
    }
    fn action() -> Action {
        Action::Http(HttpRequest {
            id: RequestId(1),
            method: crate::Method::Get,
            route: None,
            timeout_ms: Some(1000),
            url: "https://fixture.invalid/private".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        })
    }
    fn prepared(policy: ResultHeaderPolicy) -> (Dispatch, SignResult, Arc<AtomicUsize>) {
        let mut dispatch = Dispatch::default();
        let calls = Arc::new(AtomicUsize::new(0));
        dispatch
            .stage(
                RequestId(10),
                action(),
                Box::new(Step {
                    policy,
                    calls: calls.clone(),
                }),
                TimerId(5),
                1000.,
            )
            .unwrap();
        let work = dispatch
            .prepare(
                RequestId(10),
                1000.,
                &DispatchContext::new(1000., None),
                3,
                &Default::default(),
            )
            .unwrap();
        let mut result = SignResult::new(vec![vec![7; 32]], work.receipt);
        result.headers = vec![("x-mbx-apikey".into(), "callback".into())];
        (dispatch, result, calls)
    }
    #[test]
    fn fresh_http_headers_follow_recipe_apply_and_never_enter_unsigned_state() {
        let (mut dispatch, result, calls) = prepared(ResultHeaderPolicy::Http { protected: &[] });
        let Some(Completion::Ready(pending, Action::Http(http))) =
            dispatch.finish(RequestId(10), 1010., 3, Ok(result))
        else {
            panic!("expected fresh HTTP")
        };
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(http.headers.is_empty());
        assert_eq!(
            http.signer_headers,
            vec![("x-mbx-apikey".into(), "callback".into())]
        );
        for stored in [&pending.outbound, &pending.unsigned] {
            let Action::Http(stored) = stored else {
                panic!("expected HTTP")
            };
            assert!(stored.headers.is_empty() && stored.signer_headers.is_empty());
        }
    }
    #[test]
    fn forbidden_or_invalid_callback_metadata_fails_before_recipe_mutation() {
        for policy in [
            ResultHeaderPolicy::Deny,
            ResultHeaderPolicy::Http { protected: &[] },
        ] {
            let (mut dispatch, mut result, calls) = prepared(policy);
            if matches!(policy, ResultHeaderPolicy::Http { .. }) {
                result.headers[0].1.push('\n');
            }
            assert!(matches!(
                dispatch.finish(RequestId(10), 1010., 3, Ok(result)),
                Some(Completion::Failed(_, SignError::InvalidResult))
            ));
            assert_eq!(calls.load(Ordering::Relaxed), 0);
        }
    }
    #[test]
    fn protected_policy_is_applied_even_when_callback_headers_are_empty() {
        let (mut dispatch, mut result, calls) = prepared(ResultHeaderPolicy::Http {
            protected: &["X-MBX-APIKEY"],
        });
        result.headers.clear();
        let Some(Completion::Ready(_, Action::Http(http))) =
            dispatch.finish(RequestId(10), 1010., 3, Ok(result))
        else {
            panic!("expected fresh HTTP")
        };
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(http.headers, vec![("X-MBX-APIKEY", "native".into())]);
        assert!(http.signer_headers.is_empty());
    }
    #[test]
    fn stale_timeout_cancel_and_final_release_never_emit_old_headers() {
        for (now, revision, timed_out) in [(1010., 4, false), (2000., 3, true)] {
            let (mut dispatch, result, calls) =
                prepared(ResultHeaderPolicy::Http { protected: &[] });
            let completion = dispatch.finish(RequestId(10), now, revision, Ok(result));
            if timed_out {
                assert!(matches!(
                    completion,
                    Some(Completion::Failed(_, SignError::TimedOut))
                ));
            } else {
                assert!(matches!(completion, Some(Completion::Stale(_))));
            }
            assert_eq!(calls.load(Ordering::Relaxed), 0);
        }
        let (mut dispatch, result, calls) = prepared(ResultHeaderPolicy::Http { protected: &[] });
        assert!(dispatch.remove_request(RequestId(1)).is_some());
        assert!(
            dispatch
                .finish(RequestId(10), 1010., 3, Ok(result))
                .is_none()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let (mut dispatch, result, _) = prepared(ResultHeaderPolicy::Http { protected: &[] });
        let Some(Completion::Ready(pending, action)) =
            dispatch.finish(RequestId(10), 1010., 3, Ok(result))
        else {
            panic!("expected completed signing")
        };
        dispatch.ready(RequestId(10), pending);
        assert!(matches!(
            dispatch.release(RequestId(10), action, 1020., 4),
            Some(Completion::Stale(_))
        ));
    }
    #[test]
    fn optional_empty_headers_keep_the_existing_wire_shape_and_capture_is_default_deny() {
        let (mut dispatch, mut result, _) = prepared(ResultHeaderPolicy::Http { protected: &[] });
        let work = dispatch.pending.remove(&RequestId(10)).unwrap();
        let request = SignRequest {
            receipt: result.receipt,
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("ref:owned".into()),
                payload: vec![],
            }],
            context: Context::default(),
        };
        assert!(request.validate_result(&result).is_err());
        result.headers.clear();
        assert!(request.validate_result(&result).is_ok());
        let encoded = serde_json::to_value(&result).unwrap();
        assert!(encoded.get("headers").is_none());
        let decoded: SignResult = serde_json::from_value(encoded).unwrap();
        assert!(decoded.headers.is_empty());
        drop(work);
    }
}
