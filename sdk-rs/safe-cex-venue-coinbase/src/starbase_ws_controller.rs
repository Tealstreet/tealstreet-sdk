//! Inert WebSocket activation controller; owns no socket, classifier or transport.
use crate::{
    starbase::{
        CoinbaseStarbaseHeartbeatSignal, CoinbaseStarbaseProtocolError,
        CoinbaseStarbaseTransportKind,
    },
    starbase_auth::{
        CoinbaseStarbaseAuthPromise, CoinbaseStarbaseAuthSession, CoinbaseStarbasePromise,
        CoinbaseStarbasePromiseResolver, CoinbaseStarbasePromiseSpawner,
        CoinbaseStarbaseSessionFailure, protocol_failure, queue_job, request_failure,
    },
    starbase_rpc::{
        CoinbaseStarbaseHeartbeatController, CoinbaseStarbaseJsonRpcClient,
        CoinbaseStarbaseJsonRpcParams,
    },
    starbase_session::{
        CoinbaseStarbaseCancelOnDisconnectPolicy, CoinbaseStarbaseWebsocketDisconnectReason,
        CoinbaseStarbaseWebsocketSessionState,
        build_coinbase_starbase_disable_cancel_on_disconnect,
        build_coinbase_starbase_enable_cancel_on_disconnect,
        build_coinbase_starbase_get_cancel_on_disconnect,
        decode_coinbase_starbase_cancel_on_disconnect_mutation_result,
        decode_coinbase_starbase_get_cancel_on_disconnect_result,
    },
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type CoinbaseStarbaseActivationPromise<E> =
    CoinbaseStarbasePromise<Result<(), CoinbaseStarbaseSessionFailure<E>>>;
pub type CoinbaseStarbaseReplaySubscriptions<E> =
    Rc<dyn Fn() -> CoinbaseStarbaseActivationPromise<E>>;
struct ControllerState<E> {
    activation: RefCell<Option<CoinbaseStarbaseActivationPromise<E>>>,
    activation_generation: Cell<f64>,
    auth_session: CoinbaseStarbaseAuthSession<E>,
    cancel_on_disconnect: Option<Rc<RefCell<CoinbaseStarbaseCancelOnDisconnectPolicy>>>,
    client: Rc<CoinbaseStarbaseJsonRpcClient<E>>,
    heartbeat: Rc<CoinbaseStarbaseHeartbeatController<E>>,
    replay_subscriptions: CoinbaseStarbaseReplaySubscriptions<E>,
    state: Cell<CoinbaseStarbaseWebsocketSessionState>,
    spawn: CoinbaseStarbasePromiseSpawner,
}
// ts: CoinbaseStarbaseWebsocketSessionController
pub struct CoinbaseStarbaseWebsocketSessionController<E>(Rc<ControllerState<E>>);
impl<E> Clone for CoinbaseStarbaseWebsocketSessionController<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<E: 'static> CoinbaseStarbaseWebsocketSessionController<E> {
    // ts: CoinbaseStarbaseWebsocketSessionController.constructor
    pub fn new(
        auth_session: CoinbaseStarbaseAuthSession<E>,
        cancel_on_disconnect: Option<Rc<RefCell<CoinbaseStarbaseCancelOnDisconnectPolicy>>>,
        client: Rc<CoinbaseStarbaseJsonRpcClient<E>>,
        heartbeat: Rc<CoinbaseStarbaseHeartbeatController<E>>,
        replay_subscriptions: CoinbaseStarbaseReplaySubscriptions<E>,
        spawn: CoinbaseStarbasePromiseSpawner,
    ) -> Result<Self, CoinbaseStarbaseProtocolError> {
        if client.kind != CoinbaseStarbaseTransportKind::Websocket {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Starbase WebSocket session requires a WebSocket JSON-RPC client",
            ));
        }
        Ok(Self(Rc::new(ControllerState {
            activation: RefCell::new(None),
            activation_generation: Cell::new(0.0),
            auth_session,
            cancel_on_disconnect,
            client,
            heartbeat,
            replay_subscriptions,
            state: Cell::new(CoinbaseStarbaseWebsocketSessionState::Idle),
            spawn,
        })))
    }
    pub fn state(&self) -> CoinbaseStarbaseWebsocketSessionState {
        self.0.state.get()
    }
    pub fn set_state(&self, state: CoinbaseStarbaseWebsocketSessionState) {
        self.0.state.set(state);
    }
    // ts: CoinbaseStarbaseWebsocketSessionController#replaySubscriptions
    fn replay_subscriptions(&self) -> CoinbaseStarbaseActivationPromise<E> {
        (self.0.replay_subscriptions)()
    }
    // ts: CoinbaseStarbaseWebsocketSessionController.activate
    pub fn activate(&self) -> CoinbaseStarbaseActivationPromise<E> {
        if self.state() == CoinbaseStarbaseWebsocketSessionState::Ready {
            return CoinbaseStarbasePromise::resolved(Ok(()));
        }
        if let Some(activation) = self.0.activation.borrow().as_ref() {
            return activation.clone();
        }
        if self.state() == CoinbaseStarbaseWebsocketSessionState::RateLimitBackoff {
            return CoinbaseStarbasePromise::resolved(Err(protocol_failure(
                "Starbase rate-limit backoff must be acknowledged before reconnect",
            )));
        }
        let generation = self.0.activation_generation.get() + 1.0;
        self.0.activation_generation.set(generation);
        self.set_state(CoinbaseStarbaseWebsocketSessionState::Activating);
        let activation = self.activate_generation(generation);
        *self.0.activation.borrow_mut() = Some(activation.clone());
        let identity = activation.clone();
        let state = self.0.clone();
        activation.observe(&self.0.spawn, move |_| {
            let mut current = state.activation.borrow_mut();
            if current
                .as_ref()
                .is_some_and(|value| value.same_promise(&identity))
            {
                *current = None;
            }
        });
        activation
    }
    // ts: CoinbaseStarbaseWebsocketSessionController.maintainAuthentication
    pub fn maintain_authentication(&self) -> CoinbaseStarbaseAuthPromise<E> {
        if let Err(error) = self.assert_ready("refresh authentication") {
            return CoinbaseStarbasePromise::resolved(Err(error));
        }
        let inner = self.0.auth_session.authenticate(false);
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        let spawn = self.0.spawn.clone();
        queue_job(&self.0.spawn, move || {
            inner.observe(&spawn, move |value| complete.resolve(value))
        });
        promise
    }
    // ts: CoinbaseStarbaseWebsocketSessionController.handleClassifiedHeartbeat
    pub fn handle_classified_heartbeat(
        &self,
        signal: CoinbaseStarbaseHeartbeatSignal,
    ) -> CoinbaseStarbasePromise<Result<Option<Value>, CoinbaseStarbaseSessionFailure<E>>> {
        if let Err(error) = self.assert_ready("handle a heartbeat") {
            return CoinbaseStarbasePromise::resolved(Err(error));
        }
        let inner = self
            .0
            .heartbeat
            .handle_signal_promise(signal, &self.0.spawn);
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        let spawn = self.0.spawn.clone();
        queue_job(&self.0.spawn, move || {
            inner.observe(&spawn, move |value| complete.resolve(value))
        });
        promise
    }
    // ts: CoinbaseStarbaseWebsocketSessionController.recordDisconnect
    pub fn record_disconnect(&self, reason: CoinbaseStarbaseWebsocketDisconnectReason) {
        self.0
            .activation_generation
            .set(self.0.activation_generation.get() + 1.0);
        *self.0.activation.borrow_mut() = None;
        self.set_state(
            if reason == CoinbaseStarbaseWebsocketDisconnectReason::RateLimit {
                CoinbaseStarbaseWebsocketSessionState::RateLimitBackoff
            } else {
                CoinbaseStarbaseWebsocketSessionState::Disconnected
            },
        );
    }
    // ts: CoinbaseStarbaseWebsocketSessionController.acknowledgeRateLimitBackoff
    pub fn acknowledge_rate_limit_backoff(&self) -> Result<(), CoinbaseStarbaseSessionFailure<E>> {
        if self.state() != CoinbaseStarbaseWebsocketSessionState::RateLimitBackoff {
            return Err(protocol_failure(
                "Starbase session is not waiting for rate-limit backoff",
            ));
        }
        self.set_state(CoinbaseStarbaseWebsocketSessionState::Disconnected);
        Ok(())
    }
    // ts: CoinbaseStarbaseWebsocketSessionController#activateGeneration
    fn activate_generation(&self, generation: f64) -> CoinbaseStarbaseActivationPromise<E> {
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        let authentication = self.0.auth_session.authenticate(true);
        let this = self.clone();
        authentication.observe(&self.0.spawn, move |result| {
            if !this.activation_checkpoint(generation, &complete, result.map(|_| ())) {
                return;
            }
            let heartbeat = this.0.heartbeat.enable_promise(&this.0.spawn);
            let spawn = this.0.spawn.clone();
            heartbeat.observe(&spawn, move |result| {
                if !this.activation_checkpoint(generation, &complete, result) {
                    return;
                }
                if let Some(policy) = &this.0.cancel_on_disconnect {
                    let mutation = this.apply_cancel_on_disconnect(policy.clone());
                    let spawn = this.0.spawn.clone();
                    mutation.observe(&spawn, move |result| {
                        if this.activation_checkpoint(generation, &complete, result) {
                            this.replay_activation(generation, complete);
                        }
                    });
                } else {
                    this.replay_activation(generation, complete);
                }
            });
        });
        promise
    }
    // Native factoring of the source try/catch and repeated post-await generation checks.
    fn activation_checkpoint(
        &self,
        generation: f64,
        complete: &CoinbaseStarbasePromiseResolver<Result<(), CoinbaseStarbaseSessionFailure<E>>>,
        result: Result<(), CoinbaseStarbaseSessionFailure<E>>,
    ) -> bool {
        let result = result.and_then(|()| self.assert_current_activation(generation));
        match result {
            Ok(()) => true,
            Err(error) => {
                if generation == self.0.activation_generation.get() {
                    self.set_state(CoinbaseStarbaseWebsocketSessionState::Failed);
                }
                complete.resolve(Err(error));
                false
            }
        }
    }
    fn replay_activation(
        &self,
        generation: f64,
        complete: CoinbaseStarbasePromiseResolver<Result<(), CoinbaseStarbaseSessionFailure<E>>>,
    ) {
        let this = self.clone();
        self.replay_subscriptions()
            .observe(&self.0.spawn, move |result| {
                if this.activation_checkpoint(generation, &complete, result) {
                    this.set_state(CoinbaseStarbaseWebsocketSessionState::Ready);
                    complete.resolve(Ok(()));
                }
            });
    }
    // ts: CoinbaseStarbaseWebsocketSessionController#applyCancelOnDisconnect
    fn apply_cancel_on_disconnect(
        &self,
        policy: Rc<RefCell<CoinbaseStarbaseCancelOnDisconnectPolicy>>,
    ) -> CoinbaseStarbaseActivationPromise<E> {
        let scope = policy
            .borrow()
            .scope
            .map(|scope| serde_json::to_value(scope).expect("scope literal"));
        let mutation = if policy.borrow().enabled {
            build_coinbase_starbase_enable_cancel_on_disconnect(scope.as_ref())
        } else {
            build_coinbase_starbase_disable_cancel_on_disconnect(scope.as_ref())
        };
        let mutation = match mutation {
            Ok(value) => value,
            Err(error) => {
                return CoinbaseStarbasePromise::resolved(Err(request_failure(error.into())));
            }
        };
        let response = self.0.client.request_promise(
            Some(&json!(mutation.method)),
            CoinbaseStarbaseJsonRpcParams::new(Value::Object(mutation.params.into())),
            None,
            &self.0.spawn,
        );
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        let this = self.clone();
        response.observe(&self.0.spawn, move |result| {
            let result = result.and_then(|result| decode_coinbase_starbase_cancel_on_disconnect_mutation_result(Some(&result)).map(|_| ()).map_err(|error| request_failure(error.into())));
            if let Err(error) = result { complete.resolve(Err(error)); return; }
            let scope = policy.borrow().scope.map(|scope| serde_json::to_value(scope).expect("scope literal"));
            let readback = match build_coinbase_starbase_get_cancel_on_disconnect(scope.as_ref()) { Ok(value) => value, Err(error) => { complete.resolve(Err(request_failure(error.into()))); return; } };
            let response = this.0.client.request_promise(Some(&json!(readback.method)), CoinbaseStarbaseJsonRpcParams::new(Value::Object(readback.params.into())), None, &this.0.spawn);
            response.observe(&this.0.spawn, move |result| {
                let result = result.and_then(|value| {
                    let current = decode_coinbase_starbase_get_cancel_on_disconnect_result(Some(&value)).map_err(|error| request_failure(error.into()))?;
                    let policy = policy.borrow();
                    let expected_scope = policy.scope.unwrap_or(crate::starbase_session::CoinbaseStarbaseCancelOnDisconnectScope::Connection);
                    if current.enabled != policy.enabled || current.scope != expected_scope { return Err(protocol_failure("Starbase cancel-on-disconnect readback did not match policy")); }
                    Ok(())
                });
                complete.resolve(result);
            });
        });
        promise
    }
    // ts: CoinbaseStarbaseWebsocketSessionController#assertCurrentActivation
    fn assert_current_activation(
        &self,
        generation: f64,
    ) -> Result<(), CoinbaseStarbaseSessionFailure<E>> {
        if generation != self.0.activation_generation.get()
            || self.state() != CoinbaseStarbaseWebsocketSessionState::Activating
        {
            return Err(protocol_failure(
                "Starbase WebSocket activation was superseded by a disconnect",
            ));
        }
        Ok(())
    }
    // ts: CoinbaseStarbaseWebsocketSessionController#assertReady
    fn assert_ready(&self, action: &str) -> Result<(), CoinbaseStarbaseSessionFailure<E>> {
        if self.state() != CoinbaseStarbaseWebsocketSessionState::Ready {
            return Err(protocol_failure(&format!(
                "Starbase WebSocket session must be ready to {action}"
            )));
        }
        Ok(())
    }
}
