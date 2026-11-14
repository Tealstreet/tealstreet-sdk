//! Mutation-only socket intents share managed HTTP's authority and write journal.
use super::*;

/// The shared queue's outbound envelope uses zero for a mutation handoff.
/// Real signing attempts start at one. Keeping this encoding internal preserves
/// existing exhaustive Action/HttpAction consumers and venue behavior.
pub(super) fn outbound(packet: Box<HttpAction>) -> Action {
    Action::SignedOutbound {
        attempt: RequestId(0),
        outbound: Box::new(Action::ManagedHttp(packet)),
    }
}

pub use crate::request_context::WebSocketCommit;

fn fresh_issuer() -> Option<u64> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    })
    .ok()
}

/// Physical handoff after admission. Revalidate the connection, then call
/// `commit` with a callback that attempts the frame exactly once. No venue
/// callback runs inside `commit`; false handoff leaves the attempt NotSent.
pub trait WebSocketHandoff {
    fn send(&mut self, socket: SocketId, text: String, commit: &mut WebSocketCommit<'_>) -> bool;
}

impl<F> WebSocketHandoff for F
where
    F: FnMut(SocketId, String, &mut WebSocketCommit<'_>) -> bool,
{
    fn send(&mut self, socket: SocketId, text: String, commit: &mut WebSocketCommit<'_>) -> bool {
        self(socket, text, commit)
    }
}

/// Opaque bound mutation. A host checks readiness, then consumes this with
/// `Engine::claim_websocket_write` immediately before its socket send.
#[derive(Debug)]
pub struct WebSocketWriteBinding {
    packet: Box<HttpAction>,
    issuer: u64,
}

impl WebSocketWriteBinding {
    /// Decode the engine's reserved outbound envelope. Decoding grants no send
    /// authority: the engine must still claim the live single-use permit.
    pub fn from_action(action: Action) -> Option<Self> {
        let Action::SignedOutbound {
            attempt: RequestId(0),
            outbound,
        } = action
        else {
            return None;
        };
        let Action::ManagedHttp(mut packet) = *outbound else {
            return None;
        };
        let HttpAction::Bind { permit, .. } = packet.as_mut() else {
            return None;
        };
        let issuer = permit.take_websocket_issuer();
        Some(Self { packet, issuer })
    }
    pub fn preparation(&self) -> Preparation {
        let HttpAction::Bind { preparation, .. } = self.packet.as_ref() else {
            unreachable!()
        };
        *preparation
    }
}

/// An engine-validated socket mutation intent. This is deliberately neither a
/// String nor an Action: control-send APIs cannot accept it. Only the managed
/// release boundary may turn it into a physical frame.
///
/// ```compile_fail
/// use safe_cex_engine::{Cx, SocketId, WebSocketWrite};
/// fn control_send<P>(cx: &mut Cx<'_, P>, write: WebSocketWrite) {
///     cx.send(SocketId(1), write);
/// }
/// ```
pub struct WebSocketWrite {
    context: Context,
    socket: SocketId,
    text: String,
}

impl std::fmt::Debug for WebSocketWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSocketWrite").finish_non_exhaustive()
    }
}

impl<P> Cx<'_, P> {
    /// Construct a typed mutation intent using the same engine-issued Context
    /// as HTTP writes. Reads and Session authentication cannot mint this type.
    pub fn websocket_write(
        &self,
        context: Context,
        socket: SocketId,
        text: String,
    ) -> Result<WebSocketWrite, Error> {
        if context.operation() != Operation::Mutation
            || !matches!(context.owner(), Owner::Command { .. })
            || context.egress().is_some()
            || self.io.managed_socket_active()
        {
            return Err(Error::Invalid);
        }
        self.io
            .core
            .request_contexts
            .validate_send(&context, self.now)?;
        // Mutation groups have an HTTP-only multi-packet host contract.
        #[cfg(feature = "http-mutation-barriers")]
        if self
            .io
            .core
            .request_contexts
            .validate_mutation_group_submission(&context)?
            .is_some()
        {
            return Err(Error::Invalid);
        }
        if !matches!(self.io.core.sockets.role_of(socket), Some(1 | 2)) {
            return Err(Error::Revoked);
        }
        Ok(WebSocketWrite {
            context,
            socket,
            text,
        })
    }

    /// Queue a mutation without granting transport permission. Signing requires
    /// its live mutation context; physical sending requires the final claim.
    /// Credential-bearing contexts require a recipe, just as HTTP writes do.
    /// Recipes see WsSend; the shared lifecycle's HTTP packet is internal only.
    /// Correlate the returned RequestId with a reply and deliver it through
    /// Event::HttpResponse/HttpFailed, retaining the existing write-fact hooks.
    pub fn send_websocket_write(
        &mut self,
        write: WebSocketWrite,
        step: Option<Box<dyn crate::SignStep>>,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        if !self.io.core.websocket_write_host {
            return Err((pending, Error::Invalid));
        }
        let WebSocketWrite {
            context,
            socket,
            text,
        } = write;
        let write = match self.websocket_write(context, socket, text) {
            Ok(write) => write,
            Err(error) => return Err((pending, error)),
        };
        let recipe = match step {
            Some(step) => ManagedRecipe::Signed(Box::new(SocketRecipe { socket, step })),
            None => ManagedRecipe::Unsigned(&[]),
        };
        // Reuse preparation, signature freshness, single-use permits, pacing,
        // scheduler admission, reconciliation and uncertain-outcome handling.
        // This envelope never reaches an HTTP executor.
        let request = self.queue_managed(
            write.context,
            None,
            Method::Post,
            String::new(),
            Some(write.text),
            recipe,
            pending,
        )?;
        self.io
            .core
            .managed_http
            .get_mut(&request)
            .expect("queued managed write")
            .websocket = Some(socket);
        for action in self.io.core.actions.iter_mut() {
            if matches!(action, Action::ManagedHttp(packet)
                if matches!(packet.as_ref(), HttpAction::Prepare { preparation, .. }
                    if preparation.attempt().request == request))
            {
                let Action::ManagedHttp(packet) = action else {
                    unreachable!()
                };
                *action = outbound(packet.clone());
            }
        }
        Ok(request)
    }
}

struct SocketRecipe {
    socket: SocketId,
    step: Box<dyn crate::SignStep>,
}

impl SocketRecipe {
    fn map<T>(
        &mut self,
        outbound: &mut Action,
        f: impl FnOnce(&mut dyn crate::SignStep, &mut Action) -> Result<T, crate::signing::SignError>,
    ) -> Result<T, crate::signing::SignError> {
        let Action::Http(packet) = outbound else {
            return Err(crate::signing::SignError::InvalidRequest);
        };
        let mut frame = Action::WsSend {
            socket: self.socket,
            text: packet.body.take().unwrap_or_default(),
        };
        let result = f(self.step.as_mut(), &mut frame);
        match frame {
            Action::WsSend { socket, text } if socket == self.socket => {
                packet.body = Some(text);
                result
            }
            mut frame => {
                crate::signing_dispatch::wipe(&mut frame);
                Err(crate::signing::SignError::InvalidRequest)
            }
        }
    }
}

impl crate::SignStep for SocketRecipe {
    fn nonce_claim(&self) -> Option<&crate::nonce::NonceClaim> {
        self.step.nonce_claim()
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        context: &crate::DispatchContext<'_>,
    ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
        self.map(outbound, |step, frame| step.prepare(frame, context))
    }
    fn apply(
        &mut self,
        outbound: &mut Action,
        result: &crate::signing::SignResult,
    ) -> Result<(), crate::signing::SignError> {
        self.map(outbound, |step, frame| step.apply(frame, result))
    }
    fn freshness_ms(&self) -> f64 {
        self.step.freshness_ms()
    }
}

impl<V: Venue> Engine<V> {
    /// Consume internal preparation and retain the typed binding for the host's
    /// final claim. Existing control frames and HTTP actions keep their path.
    pub(in crate::engine) fn dispatch_websocket_write_action(
        &mut self,
        now: f64,
        mut action: Action,
    ) -> Option<Action> {
        let Action::SignedOutbound {
            attempt: RequestId(0),
            ref outbound,
        } = action
        else {
            return Some(action);
        };
        let Action::ManagedHttp(managed_action) = outbound.as_ref() else {
            crate::signing_dispatch::wipe(&mut action);
            return None;
        };
        let token = match managed_action.as_ref() {
            HttpAction::Prepare { preparation, .. } | HttpAction::Bind { preparation, .. } => {
                *preparation
            }
            _ => return None,
        };
        let Some(socket) = self
            .io
            .core
            .managed_http
            .get(&token.attempt().request)
            .and_then(|managed| managed.websocket)
        else {
            // A retired internal envelope can never become HTTP work.
            crate::signing_dispatch::wipe(&mut action);
            return None;
        };
        if !matches!(self.io.core.sockets.role_of(socket), Some(1 | 2)) {
            self.http_preparation_failed(
                now,
                token,
                Error::Revoked,
                VenueError::new("WebSocket write connection is no longer live"),
            );
            crate::signing_dispatch::wipe(&mut action);
            return None;
        }
        match managed_action.as_ref() {
            HttpAction::Prepare { .. } => {
                self.http_prepared(
                    now,
                    token,
                    Prepared {
                        id: socket.0,
                        expires_at: None,
                    },
                );
            }
            HttpAction::Bind { .. } => return Some(action),
            _ => unreachable!("only preparation and binding enter this boundary"),
        }
        None
    }

    /// Register a host which claims each typed Bind immediately before its
    /// synchronous socket send. This capability grants no mutation authority.
    pub fn enable_websocket_write_host(&mut self) {
        if self.io.core.websocket_write_issuer == 0 {
            let Some(issuer) = fresh_issuer() else {
                return;
            };
            self.io.core.websocket_write_issuer = issuer;
        }
        self.io.core.websocket_write_host = true;
    }

    /// The bound socket, for checking host readiness before consuming admission.
    pub fn websocket_write_socket(&self, binding: &WebSocketWriteBinding) -> Option<SocketId> {
        if binding.issuer == 0 || binding.issuer != self.io.core.websocket_write_issuer {
            return None;
        }
        let HttpAction::Bind {
            preparation,
            permit,
            request,
        } = binding.packet.as_ref()
        else {
            return None;
        };
        let managed = self
            .io
            .core
            .managed_http
            .get(&preparation.attempt().request)?;
        (managed.lifecycle.preparation() == *preparation
            && managed.lifecycle.is_current_permit(*permit)
            && request.id == preparation.attempt().request)
            .then_some(managed.websocket)
            .flatten()
    }

    /// Reserve admission, then commit only through the synchronous physical handoff.
    pub fn claim_websocket_write(
        &mut self,
        now: f64,
        binding: WebSocketWriteBinding,
        handoff: &mut dyn WebSocketHandoff,
    ) -> Result<(), Error> {
        self.assert_live();
        let socket = match self.websocket_write_socket(&binding) {
            Some(socket) => socket,
            None => {
                crate::signing_dispatch::wipe(&mut Action::ManagedHttp(binding.packet));
                return Err(Error::Revoked);
            }
        };
        let HttpAction::Bind {
            preparation,
            permit,
            mut request,
        } = *binding.packet
        else {
            unreachable!()
        };
        if !matches!(self.io.core.sockets.role_of(socket), Some(1 | 2))
            || !self.io.core.sockets.is_open(socket)
        {
            self.http_preparation_failed(
                now,
                preparation,
                Error::Revoked,
                VenueError::new("WebSocket write connection is not open; no frame sent"),
            );
            crate::signing_dispatch::wipe(&mut Action::Http(request));
            return Err(Error::Revoked);
        }
        let result = match self.claim_http_with_handoff(now, permit, &mut |commit| {
            handoff.send(socket, request.body.take().unwrap_or_default(), commit)
        }) {
            Ok(Claim::Released(_)) => return Ok(()),
            Ok(Claim::WaitUntil(_)) => {
                self.http_preparation_failed(
                    now,
                    preparation,
                    Error::Revoked,
                    VenueError::new("WebSocket write pacing is not ready"),
                );
                Err(Error::Revoked)
            }
            Err(error) => Err(error),
        };
        crate::signing_dispatch::wipe(&mut Action::Http(request));
        result
    }
}
