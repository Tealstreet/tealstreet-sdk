//! Inert JSON-RPC transport, client and heartbeat operations from coinbase.starbase.ts.
use crate::starbase::{
    CoinbaseStarbaseHeartbeatSignal, CoinbaseStarbaseJsonRpcErrorObject,
    CoinbaseStarbaseJsonRpcErrorResponse, CoinbaseStarbaseJsonRpcVersion,
    CoinbaseStarbaseProtocolError, CoinbaseStarbaseTransportContext, CoinbaseStarbaseTransportKind,
    is_record, is_safe_request_id, parse_rpc_error, require_coinbase_starbase_method,
    resolve_coinbase_starbase_endpoint,
};
use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, Ref, RefCell, RefMut},
    fmt,
    future::Future,
    pin::Pin,
    rc::Rc,
};

// ts: CoinbaseStarbaseJsonRpcParams
// Retain the caller's object handle in requests rather than copying its properties.
#[derive(Debug, Clone, PartialEq)]
pub struct CoinbaseStarbaseJsonRpcParams(Rc<RefCell<Value>>);
impl CoinbaseStarbaseJsonRpcParams {
    pub fn new(value: Value) -> Self {
        Self(Rc::new(RefCell::new(value)))
    }
    pub fn borrow(&self) -> Ref<'_, Value> {
        self.0.borrow()
    }
    pub fn borrow_object_mut(&self) -> Option<RefMut<'_, Map<String, Value>>> {
        RefMut::filter_map(self.0.borrow_mut(), Value::as_object_mut).ok()
    }
    pub fn shares_object(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Serialize for CoinbaseStarbaseJsonRpcParams {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.borrow().serialize(serializer)
    }
}

fn serialize_id<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    // Requests built by the client contain safe integers; preserve JS's integer JSON spelling.
    if value.is_finite() && value.fract() == 0.0 && value.abs() <= 9_007_199_254_740_991.0 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseHttpMethod {
    #[serde(rename = "POST")]
    Post,
}

// ts: CoinbaseStarbaseJsonRpcRequest
#[derive(Debug, Clone, PartialEq, Serialize)]
// Erase the source record generic to the shared JSON handle; the client validates its object shape.
pub struct CoinbaseStarbaseJsonRpcRequest {
    pub jsonrpc: CoinbaseStarbaseJsonRpcVersion,
    #[serde(serialize_with = "serialize_id")]
    pub id: f64,
    pub method: String,
    pub params: CoinbaseStarbaseJsonRpcParams,
}
// Native shared object used by the source callbacks and response parser.
#[derive(Debug, Clone)]
pub struct CoinbaseStarbaseRequestHandle(Rc<RefCell<CoinbaseStarbaseJsonRpcRequest>>);
impl CoinbaseStarbaseRequestHandle {
    pub fn new(request: CoinbaseStarbaseJsonRpcRequest) -> Self {
        Self(Rc::new(RefCell::new(request)))
    }
    pub fn borrow(&self) -> Ref<'_, CoinbaseStarbaseJsonRpcRequest> {
        self.0.borrow()
    }
    pub fn borrow_mut(&self) -> RefMut<'_, CoinbaseStarbaseJsonRpcRequest> {
        self.0.borrow_mut()
    }
    pub fn shares_request(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Serialize for CoinbaseStarbaseRequestHandle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.borrow().serialize(serializer)
    }
}

// ts: CoinbaseStarbaseJsonRpcSuccess
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseJsonRpcSuccess<Result> {
    pub jsonrpc: CoinbaseStarbaseJsonRpcVersion,
    #[serde(serialize_with = "serialize_id")]
    pub id: f64,
    pub result: Result,
}
// ts: CoinbaseStarbaseJsonRpcResponse
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CoinbaseStarbaseJsonRpcResponse<Result> {
    Success(CoinbaseStarbaseJsonRpcSuccess<Result>),
    Error(CoinbaseStarbaseJsonRpcErrorResponse),
}

// ts: CoinbaseStarbaseRpcError
#[derive(Debug, Clone, PartialEq)]
pub struct CoinbaseStarbaseRpcError {
    pub name: String,
    pub message: String,
    pub code: f64,
    pub data: Option<Value>,
    pub method: String,
    pub request_id: Option<f64>,
}
impl CoinbaseStarbaseRpcError {
    // ts: CoinbaseStarbaseRpcError.constructor
    pub fn new(
        error: CoinbaseStarbaseJsonRpcErrorObject,
        method: String,
        request_id: Option<f64>,
    ) -> Self {
        Self {
            name: "CoinbaseStarbaseRpcError".into(),
            message: error.message,
            code: error.code,
            data: error.data,
            method,
            request_id,
        }
    }
}
impl fmt::Display for CoinbaseStarbaseRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name.is_empty() {
            f.write_str(&self.message)
        } else if self.message.is_empty() {
            f.write_str(&self.name)
        } else {
            write!(f, "{}: {}", self.name, self.message)
        }
    }
}
impl std::error::Error for CoinbaseStarbaseRpcError {}

#[derive(Debug, Clone, PartialEq)]
pub enum CoinbaseStarbaseRequestError<E> {
    Protocol(CoinbaseStarbaseProtocolError),
    Rpc(CoinbaseStarbaseRpcError),
    Transport(E),
}
impl<E> From<CoinbaseStarbaseProtocolError> for CoinbaseStarbaseRequestError<E> {
    fn from(error: CoinbaseStarbaseProtocolError) -> Self {
        Self::Protocol(error)
    }
}
pub type CoinbaseStarbaseRpcFuture<E> =
    Pin<Box<dyn Future<Output = Result<Value, CoinbaseStarbaseRequestError<E>>>>>;
// ts: CoinbaseStarbaseTransport
pub type CoinbaseStarbaseTransport<E> = Rc<
    dyn Fn(
        CoinbaseStarbaseRequestHandle,
        CoinbaseStarbaseTransportContext,
    ) -> CoinbaseStarbaseRpcFuture<E>,
>;
// ts: CoinbaseStarbaseHttpRequest
#[derive(Debug, Clone, Serialize)]
pub struct CoinbaseStarbaseHttpRequest {
    pub body: CoinbaseStarbaseRequestHandle,
    pub headers: indexmap::IndexMap<String, String>,
    pub method: CoinbaseStarbaseHttpMethod,
    pub url: String,
}
// ts: CoinbaseStarbaseHttpSender
pub type CoinbaseStarbaseHttpSender<E> =
    Rc<dyn Fn(CoinbaseStarbaseHttpRequest) -> Pin<Box<dyn Future<Output = Result<Value, E>>>>>;

// ts: createCoinbaseStarbaseHttpTransport
pub fn create_coinbase_starbase_http_transport<E: 'static>(
    send: CoinbaseStarbaseHttpSender<E>,
) -> CoinbaseStarbaseTransport<E> {
    Rc::new(move |request, context| {
        let prepared = (|| {
            if context.kind != CoinbaseStarbaseTransportKind::Http {
                return Err(CoinbaseStarbaseProtocolError::new(
                    "HTTP transport cannot send a WebSocket request",
                ));
            }
            let method_value = Value::String(request.borrow().method.clone());
            let method = require_coinbase_starbase_method(Some(&method_value))?;
            let mut headers =
                indexmap::IndexMap::from_iter([("Content-Type".into(), "application/json".into())]);
            if let Some(authorization) = context.authorization.filter(|value| !value.is_empty()) {
                headers.insert("Authorization".into(), authorization);
            }
            let url = format!("{}/{method}", context.endpoint.trim_end_matches('/'));
            Ok(CoinbaseStarbaseHttpRequest {
                body: request,
                headers,
                method: CoinbaseStarbaseHttpMethod::Post,
                url,
            })
        })();
        // Invoke the callback eagerly, like the TS async closure before its first await.
        match prepared {
            Ok(request) => {
                let response = send(request);
                Box::pin(async move {
                    response
                        .await
                        .map_err(CoinbaseStarbaseRequestError::Transport)
                })
            }
            Err(error) => Box::pin(async move { Err(error.into()) }),
        }
    })
}

// ts: CoinbaseStarbaseJsonRpcClient
pub struct CoinbaseStarbaseJsonRpcClient<E> {
    pub kind: CoinbaseStarbaseTransportKind,
    next_request_id: Cell<f64>,
    transport: CoinbaseStarbaseTransport<E>,
}
impl<E: 'static> CoinbaseStarbaseJsonRpcClient<E> {
    // ts: CoinbaseStarbaseJsonRpcClient.constructor
    pub fn new(
        transport: CoinbaseStarbaseTransport<E>,
        kind: CoinbaseStarbaseTransportKind,
        initial_request_id: Option<&Value>,
    ) -> Result<Self, CoinbaseStarbaseProtocolError> {
        let default = json!(1);
        let initial_request_id = initial_request_id
            .filter(|value| !value.is_null())
            .unwrap_or(&default);
        if !is_safe_request_id(Some(initial_request_id)) {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Initial JSON-RPC request ID must be a positive safe integer",
            ));
        }
        Ok(Self {
            kind,
            next_request_id: Cell::new(initial_request_id.as_f64().expect("validated number")),
            transport,
        })
    }
    // ts: CoinbaseStarbaseJsonRpcClient#transport
    fn transport(
        &self,
        request: CoinbaseStarbaseRequestHandle,
        context: CoinbaseStarbaseTransportContext,
    ) -> CoinbaseStarbaseRpcFuture<E> {
        (self.transport)(request, context)
    }
    // ts: CoinbaseStarbaseJsonRpcClient.buildRequest
    pub fn build_request(
        &self,
        method: Option<&Value>,
        params: CoinbaseStarbaseJsonRpcParams,
    ) -> Result<CoinbaseStarbaseRequestHandle, CoinbaseStarbaseProtocolError> {
        let method = require_coinbase_starbase_method(method)?;
        if !is_record(Some(&params.borrow())) {
            return Err(CoinbaseStarbaseProtocolError::new(
                "JSON-RPC params must be an object",
            ));
        }
        let id = self.next_request_id.get();
        if !id.is_finite() || id.fract() != 0.0 || id.abs() > 9_007_199_254_740_991.0 {
            return Err(CoinbaseStarbaseProtocolError::new(
                "JSON-RPC request ID space exhausted",
            ));
        }
        let request = CoinbaseStarbaseJsonRpcRequest {
            jsonrpc: CoinbaseStarbaseJsonRpcVersion::V2,
            id,
            method: method.into(),
            params,
        };
        self.next_request_id.set(id + 1.0);
        Ok(CoinbaseStarbaseRequestHandle::new(request))
    }
    // ts: CoinbaseStarbaseJsonRpcClient.request
    pub fn request(
        &self,
        method: Option<&Value>,
        params: CoinbaseStarbaseJsonRpcParams,
        authorization: Option<String>,
    ) -> CoinbaseStarbaseRpcFuture<E> {
        match self.build_request(method, params) {
            Err(error) => Box::pin(async move { Err(error.into()) }),
            Ok(request) => {
                let context = CoinbaseStarbaseTransportContext {
                    authorization,
                    endpoint: resolve_coinbase_starbase_endpoint(self.kind).into(),
                    kind: self.kind,
                };
                let response = self.transport(request.clone(), context);
                Box::pin(async move { Self::parse_response(response.await?, &request) })
            }
        }
    }
    // Host Promise continuation view of the same request operation, used by session controllers.
    pub(crate) fn request_promise(
        &self,
        method: Option<&Value>,
        params: CoinbaseStarbaseJsonRpcParams,
        authorization: Option<String>,
        spawn: &crate::starbase_auth::CoinbaseStarbasePromiseSpawner,
    ) -> crate::starbase_auth::CoinbaseStarbasePromise<
        Result<Value, crate::starbase_auth::CoinbaseStarbaseSessionFailure<E>>,
    > {
        use crate::starbase_auth::{CoinbaseStarbasePromise, request_failure};
        let request = match self.build_request(method, params) {
            Err(error) => {
                return CoinbaseStarbasePromise::resolved(Err(request_failure(error.into())));
            }
            Ok(request) => request,
        };
        let response = self.transport(
            request.clone(),
            CoinbaseStarbaseTransportContext {
                authorization,
                endpoint: resolve_coinbase_starbase_endpoint(self.kind).into(),
                kind: self.kind,
            },
        );
        // The host Future adapter models the transport's async Promise producer.
        let response = CoinbaseStarbasePromise::from_future(
            Box::pin(async move { response.await.map_err(request_failure) }),
            spawn,
        );
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        response.observe(spawn, move |response| {
            complete.resolve(response.and_then(|response| {
                Self::parse_response(response, &request).map_err(request_failure)
            }));
        });
        promise
    }
    // ts: CoinbaseStarbaseJsonRpcClient#parseResponse
    fn parse_response(
        mut response: Value,
        request: &CoinbaseStarbaseRequestHandle,
    ) -> Result<Value, CoinbaseStarbaseRequestError<E>> {
        let request = request.borrow();
        let fail = |message: String| {
            CoinbaseStarbaseRequestError::Protocol(CoinbaseStarbaseProtocolError::new(message))
        };
        if !is_record(Some(&response))
            || response.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        {
            return Err(fail(format!(
                "Invalid JSON-RPC response for {}",
                request.method
            )));
        }
        let object = response.as_object_mut().expect("validated object");
        let has_result = object.contains_key("result");
        let has_error = object.contains_key("error");
        if has_result == has_error {
            return Err(fail(format!(
                "JSON-RPC response for {} must contain exactly one of result or error",
                request.method
            )));
        }
        if has_error {
            let Some(error) = parse_rpc_error(object.get("error")) else {
                return Err(fail(format!(
                    "Invalid JSON-RPC error for {}",
                    request.method
                )));
            };
            if object
                .get("id")
                .is_some_and(|id| id.as_f64() != Some(request.id))
            {
                return Err(fail(format!(
                    "Mismatched JSON-RPC response ID for {}",
                    request.method
                )));
            }
            let request_id = object
                .get("id")
                .filter(|id| is_safe_request_id(Some(id)))
                .and_then(Value::as_f64);
            return Err(CoinbaseStarbaseRequestError::Rpc(
                CoinbaseStarbaseRpcError::new(error, request.method.clone(), request_id),
            ));
        }
        if object.get("id").and_then(Value::as_f64) != Some(request.id) {
            return Err(fail(format!(
                "Mismatched JSON-RPC response ID for {}",
                request.method
            )));
        }
        Ok(object.remove("result").expect("own result"))
    }
}

// ts: CoinbaseStarbaseHeartbeatController
pub struct CoinbaseStarbaseHeartbeatController<E> {
    client: Rc<CoinbaseStarbaseJsonRpcClient<E>>,
    interval_seconds: f64,
}
impl<E: 'static> CoinbaseStarbaseHeartbeatController<E> {
    // ts: CoinbaseStarbaseHeartbeatController.constructor
    pub fn new(
        client: Rc<CoinbaseStarbaseJsonRpcClient<E>>,
        interval_seconds: f64,
    ) -> Result<Self, CoinbaseStarbaseProtocolError> {
        if client.kind != CoinbaseStarbaseTransportKind::Websocket {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Starbase heartbeat is WebSocket-only",
            ));
        }
        if !interval_seconds.is_finite() || interval_seconds < 10.0 {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Starbase heartbeat interval must be at least 10 seconds",
            ));
        }
        Ok(Self {
            client,
            interval_seconds,
        })
    }
    // ts: CoinbaseStarbaseHeartbeatController.enable
    pub fn enable(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<(), CoinbaseStarbaseRequestError<E>>>>> {
        let response = self.client.request(
            Some(&json!("public/set_heartbeat")),
            CoinbaseStarbaseJsonRpcParams::new(json!({"interval": self.interval_seconds})),
            None,
        );
        Box::pin(async move {
            if response.await? != json!("ok") {
                return Err(CoinbaseStarbaseProtocolError::new(
                    "Unexpected public/set_heartbeat result",
                )
                .into());
            }
            Ok(())
        })
    }
    pub(crate) fn enable_promise(
        &self,
        spawn: &crate::starbase_auth::CoinbaseStarbasePromiseSpawner,
    ) -> crate::starbase_auth::CoinbaseStarbasePromise<
        Result<(), crate::starbase_auth::CoinbaseStarbaseSessionFailure<E>>,
    > {
        use crate::starbase_auth::{CoinbaseStarbasePromise, protocol_failure};
        let response = self.client.request_promise(
            Some(&json!("public/set_heartbeat")),
            CoinbaseStarbaseJsonRpcParams::new(json!({"interval":self.interval_seconds})),
            None,
            spawn,
        );
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        response.observe(spawn, move |response| {
            complete.resolve(response.and_then(|response| {
                if response == json!("ok") {
                    Ok(())
                } else {
                    Err(protocol_failure("Unexpected public/set_heartbeat result"))
                }
            }));
        });
        promise
    }
    pub(crate) fn handle_signal_promise(
        &self,
        signal: CoinbaseStarbaseHeartbeatSignal,
        spawn: &crate::starbase_auth::CoinbaseStarbasePromiseSpawner,
    ) -> crate::starbase_auth::CoinbaseStarbasePromise<
        Result<Option<Value>, crate::starbase_auth::CoinbaseStarbaseSessionFailure<E>>,
    > {
        use crate::starbase_auth::CoinbaseStarbasePromise;
        if signal == CoinbaseStarbaseHeartbeatSignal::Heartbeat {
            return CoinbaseStarbasePromise::resolved(Ok(None));
        }
        let response = self.client.request_promise(
            Some(&json!("public/test")),
            CoinbaseStarbaseJsonRpcParams::new(json!({})),
            None,
            spawn,
        );
        let (promise, complete) = CoinbaseStarbasePromise::pending();
        // Async return adopts the inner Promise in a separate job.
        let spawn2 = spawn.clone();
        crate::starbase_auth::queue_job(spawn, move || {
            response.observe(&spawn2, move |response| {
                complete.resolve(response.map(Some))
            })
        });
        promise
    }
    // ts: CoinbaseStarbaseHeartbeatController.disable
    pub fn disable(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<(), CoinbaseStarbaseRequestError<E>>>>> {
        let response = self.client.request(
            Some(&json!("public/disable_heartbeat")),
            CoinbaseStarbaseJsonRpcParams::new(json!({})),
            None,
        );
        Box::pin(async move {
            if response.await? != json!("ok") {
                return Err(CoinbaseStarbaseProtocolError::new(
                    "Unexpected public/disable_heartbeat result",
                )
                .into());
            }
            Ok(())
        })
    }
    // ts: CoinbaseStarbaseHeartbeatController.handleSignal
    pub fn handle_signal(
        &self,
        signal: CoinbaseStarbaseHeartbeatSignal,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Value>, CoinbaseStarbaseRequestError<E>>>>> {
        if signal == CoinbaseStarbaseHeartbeatSignal::Heartbeat {
            return Box::pin(async { Ok(None) });
        }
        let response = self.client.request(
            Some(&json!("public/test")),
            CoinbaseStarbaseJsonRpcParams::new(json!({})),
            None,
        );
        Box::pin(async move { response.await.map(Some) })
    }
}
