//! Concrete Bitunix exchange: composition state bound to BaseExchange, HTTP lanes and sockets.
use crate::{
    composition::{BitunixExchange, ExchangeConstructorHost, new_exchange},
    native_http::{BuildHttp, Client, InstallTransport, Lane},
    sockets::{OwnerSlot, Socket},
};
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    http::HttpClient,
    runtime::{ExchangeError, ExchangeFuture, ExchangeTask},
    websocket::WsRuntime,
};
use safe_cex_num::{JsCoercionError, JsJsonValue, js_json_stringify};
use safe_cex_types::{ExchangeOptions, PositionMode};
use serde_json::Value;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(crate) type ExternalFactory =
    safe_cex_exchange::js_runtime::base_exchange::ExternalPublicDataFactory;
pub(crate) type State = BitunixExchange<Rc<BaseExchange>, Rc<Client>, Rc<Socket>>;

/// ts: BitunixExchange (bound to the shared exchange runtime)
pub struct BitunixVenue {
    pub(crate) base: Rc<BaseExchange>,
    pub(crate) state: RefCell<State>,
    pub(crate) runtime: Rc<dyn WsRuntime>,
    pub(crate) capabilities: ExchangeCapabilities,
    pub(crate) self_ref: Weak<Self>,
    pub(crate) tasks: RefCell<Vec<Box<dyn ExchangeTask>>>,
    pub(crate) external: ExternalFactory,
    pub(crate) remote_account_name: Option<String>,
}

/// Error carrier for ported bodies whose host error must absorb JS coercion failures.
pub(crate) struct Fault(pub(crate) ExchangeError);
impl From<JsCoercionError> for Fault {
    fn from(e: JsCoercionError) -> Self {
        Self(coercion(e))
    }
}
impl From<ExchangeError> for Fault {
    fn from(e: ExchangeError) -> Self {
        Self(e)
    }
}
impl From<String> for Fault {
    fn from(e: String) -> Self {
        Self(ExchangeError::new(e))
    }
}
pub(crate) fn coercion(e: JsCoercionError) -> ExchangeError {
    let mut out = ExchangeError::new(e.message);
    out.name = e.name.into();
    out
}
pub(crate) fn json(value: &JsJsonValue) -> Result<Value, ExchangeError> {
    let text = js_json_stringify(value).map_err(coercion)?;
    text.map_or(Ok(Value::Null), |s| {
        serde_json::from_str(&s).map_err(|e| ExchangeError::new(e.to_string()))
    })
}
pub(crate) fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, ExchangeError> {
    serde_json::to_value(value).map_err(|e| ExchangeError::new(e.to_string()))
}
pub(crate) fn from_value<T: serde::de::DeserializeOwned>(value: Value) -> Result<T, ExchangeError> {
    serde_json::from_value(value).map_err(|e| ExchangeError::new(e.to_string()))
}
pub(crate) fn to_input<T: serde::Serialize>(
    value: &T,
) -> Result<safe_cex_types::retained_json::RetainedJsonInput, ExchangeError> {
    use safe_cex_exchange::js_runtime::value::{from_typed, to_retained_input};
    Ok(to_retained_input(&from_typed(value)?)?)
}
pub(crate) fn from_input<T: serde::de::DeserializeOwned>(
    value: safe_cex_types::retained_json::RetainedJsonInput,
) -> Result<T, ExchangeError> {
    use safe_cex_exchange::js_runtime::value::{from_retained_input, to_typed};
    Ok(to_typed(&from_retained_input(value))?)
}
pub(crate) fn request_payload(
    value: safe_cex_types::retained_json::RetainedJsonInput,
) -> Result<JsJsonValue, ExchangeError> {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    Ok(match value {
        Input::Undefined => JsJsonValue::Undefined,
        Input::Null => JsJsonValue::Null,
        Input::Bool(value) => JsJsonValue::Bool(value),
        Input::Float(value) => JsJsonValue::Number(value),
        Input::Lexeme(value) => {
            JsJsonValue::Number(value.parse().map_err(|error: std::num::ParseFloatError| {
                ExchangeError::new(error.to_string())
            })?)
        }
        Input::Text(value) => JsJsonValue::String(utf8(&value)?),
        Input::Array(values) => JsJsonValue::Array(
            values
                .into_iter()
                .map(request_payload)
                .collect::<Result<_, _>>()?,
        ),
        Input::Object(values) => JsJsonValue::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((utf8(&key)?, request_payload(value)?)))
                .collect::<Result<_, ExchangeError>>()?,
        ),
    })
}
pub(crate) fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
pub(crate) fn utf8(s: &[u16]) -> Result<String, ExchangeError> {
    String::from_utf16(s).map_err(|e| ExchangeError::new(format!("Native UTF-8 boundary: {e}")))
}
/// err?.response?.data?.msg || err?.message
pub(crate) fn reason(e: &ExchangeError) -> String {
    e.response_json_view()
        .as_ref()
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("msg"))
        .filter(|m| safe_cex_num::js_truthy(Some(m)))
        .map(|m| m.as_str().map_or_else(|| m.to_string(), str::to_owned))
        .unwrap_or_else(|| e.message.clone())
}

struct Constructor {
    base: Option<BaseExchange>,
    build: BuildHttp,
    install: InstallTransport,
    runtime: Rc<dyn WsRuntime>,
    owner: OwnerSlot,
}
fn secondary_options(options: &ExchangeOptions) -> Option<ExchangeOptions> {
    let key = options.secondary_key.as_deref().filter(|k| !k.is_empty())?;
    let mut secondary = options.clone();
    secondary.key = key.to_owned();
    secondary.secret = options.secondary_secret.clone().unwrap_or_default();
    secondary.passphrase = options.secondary_passphrase.clone();
    Some(secondary)
}
impl ExchangeConstructorHost<(), ExchangeOptions, ()> for Constructor {
    type Parent = Rc<BaseExchange>;
    type Client = Rc<Client>;
    type Socket = Rc<Socket>;
    type Error = ExchangeError;
    fn base(
        &mut self,
        (): (),
        _: &ExchangeOptions,
        (): (),
    ) -> Result<Rc<BaseExchange>, ExchangeError> {
        let mut base = self
            .base
            .take()
            .ok_or_else(|| ExchangeError::new("Bitunix base already constructed"))?;
        // patternClientOrderIdRule('Bitunix', /^\S+$/); new() checks it against the venue state.
        let rule = crate::composition::ClientOrderIdRule {
            label: "Bitunix",
            requirement: r"match /^\S+$/",
        };
        base.settings.client_order_id_rule = Some(
            safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
                rule.label.into(),
                r"/^\S+$/".into(),
                Rc::new(move |id: &str| rule.is_valid(&units(id))),
                Default::default(),
            ),
        );
        Ok(Rc::new(base))
    }
    fn secondary_client(
        &mut self,
        parent: &Rc<BaseExchange>,
        options: &ExchangeOptions,
        max_rps: f64,
    ) -> Result<Rc<Client>, ExchangeError> {
        let primary = Lane::create(options.clone(), &self.build, &self.install)?;
        primary.limit(parent, max_rps as usize);
        let secondary = match secondary_options(options) {
            Some(secondary) => Lane::create(secondary, &self.build, &self.install)?,
            None => primary.clone(),
        };
        Ok(Rc::new(Client {
            primary,
            secondary,
            base: parent.clone(),
        }))
    }
    fn client(
        &mut self,
        parent: &Rc<BaseExchange>,
        options: &ExchangeOptions,
    ) -> Result<Rc<Client>, ExchangeError> {
        let lane = Lane::create(options.clone(), &self.build, &self.install)?;
        Ok(Rc::new(Client {
            secondary: lane.clone(),
            primary: lane,
            base: parent.clone(),
        }))
    }
    fn socket(
        &mut self,
        parent: &Rc<BaseExchange>,
        kind: &'static str,
        secondary: Option<bool>,
    ) -> Result<Rc<Socket>, ExchangeError> {
        Socket::new(
            self.owner.clone(),
            parent,
            self.runtime.clone(),
            kind,
            secondary,
            None,
        )
    }
}

impl BitunixVenue {
    pub(crate) fn validate_placement_input(
        &self,
        options: &safe_cex_types::PlaceOrderOpts,
    ) -> Result<(), ExchangeError> {
        if options.amount.is_finite() {
            return Ok(());
        }
        let message = format!(
            "Bitunix placement amount must be finite (received {})",
            safe_cex_num::js_number_string(options.amount)
        );
        self.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message.clone()),
            None,
            None,
        )?;
        let mut error = ExchangeError::new(message);
        error.name = "TypeError".into();
        Err(error)
    }
    /// The caller builds `base` with `BaseExchange::new(name, options, store, runtime, settings)`.
    pub fn new(
        base: BaseExchange,
        build: Rc<dyn Fn(&ExchangeOptions) -> Result<HttpClient, ExchangeError>>,
        install: Rc<dyn Fn(&HttpClient) -> Result<(), ExchangeError>>,
        runtime: Rc<dyn WsRuntime>,
        external: ExternalFactory,
        remote_account_name: Option<String>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let options = base.options();
        let owner: OwnerSlot = Rc::new(RefCell::new(Weak::new()));
        let mut host = Constructor {
            base: Some(base),
            build,
            install,
            runtime: runtime.clone(),
            owner: owner.clone(),
        };
        let state = new_exchange(&mut host, (), &options, ())?;
        let shared = state.parent.settings.client_order_id_rule.as_ref();
        if shared.is_none_or(|r| {
            r.label != state.client_order_id_rule.label
                || r.requirement != state.client_order_id_rule.requirement
        }) {
            return Err(ExchangeError::new(
                "Bitunix clientOrderIdRule drifted from the venue state",
            ));
        }
        let base = state.parent.clone();
        let venue = Rc::new_cyclic(|weak| Self {
            base,
            state: RefCell::new(state),
            runtime,
            capabilities: Self::capabilities_table(),
            self_ref: weak.clone(),
            tasks: RefCell::new(vec![]),
            external,
            remote_account_name,
        });
        *owner.borrow_mut() = Rc::downgrade(&venue);
        let state = venue.state.borrow();
        use safe_cex_exchange::socket_lifecycle::SocketSlot;
        for (slot, socket) in [
            (SocketSlot::Public, &state.public_socket),
            (SocketSlot::PublicSecondary, &state.secondary_socket),
            (SocketSlot::Private, &state.private_socket),
            (SocketSlot::Futures, &state.futures_socket),
        ] {
            venue.base.set_socket(slot, Some(socket.base.clone()));
        }
        drop(state);
        Ok(venue)
    }
    pub(crate) fn owner(&self) -> Rc<Self> {
        self.self_ref.upgrade().expect("Owned Bitunix instance")
    }
    pub(crate) fn xhr(&self) -> Rc<Client> {
        self.state.borrow().xhr.clone()
    }
    pub(crate) fn unlimited(&self) -> Rc<Client> {
        self.state.borrow().unlimited_xhr.clone()
    }
    pub(crate) fn socket(&self, kind: &str) -> Rc<Socket> {
        let state = self.state.borrow();
        match kind {
            "public" => state.public_socket.clone(),
            "secondary" => state.secondary_socket.clone(),
            "private" => state.private_socket.clone(),
            _ => state.futures_socket.clone(),
        }
    }
    pub(crate) fn account_mode(&self) -> PositionMode {
        if self.state.borrow().account_position_mode == "one-way" {
            PositionMode::OneWay
        } else {
            PositionMode::Hedged
        }
    }
    pub(crate) fn set_account_mode(&self, mode: PositionMode) {
        self.state.borrow_mut().account_position_mode = if mode == PositionMode::OneWay {
            "one-way"
        } else {
            "hedged"
        };
    }
    pub(crate) fn public_only(&self) -> bool {
        Exchange::is_public_data_only(self)
    }
    pub(crate) fn log(&self, message: &str, severity: &str) -> Result<(), ExchangeError> {
        self.base.log(message, severity)
    }
    pub(crate) fn emit_error(&self, message: &str) -> Result<(), ExchangeError> {
        self.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message.into()),
            None,
            None,
        )
    }
    /// Own a detached task until dispose.
    pub(crate) fn spawn(&self, label: &'static str, task: ExchangeFuture<'static, ()>) {
        let report = self.runtime.clone();
        let handle = self.runtime.spawn_local(Box::pin(async move {
            if let Err(e) = task.await {
                report.report_exception(&e, &serde_json::json!({ "operation": label }).into());
            }
        }));
        self.tasks.borrow_mut().push(handle);
    }
    pub(crate) fn dispose_venue(&self) -> Result<(), ExchangeError> {
        for task in self.tasks.borrow_mut().drain(..) {
            task.cancel();
        }
        let shards = self.state.borrow().public_shards.clone();
        let (public, secondary, futures, private) = (
            self.socket("public"),
            self.socket("secondary"),
            self.socket("futures"),
            self.socket("private"),
        );
        crate::exchange_lifecycle::dispose(
            || self.base.dispose(),
            || public.dispose(),
            || secondary.dispose(),
            || Ok((shards.len(), |i: usize| Ok(shards.get(i).cloned()))),
            |shard: Rc<Socket>| shard.dispose(),
            || futures.dispose(),
            || private.dispose(),
        )
    }
}

impl From<safe_cex_types::StoreReadError> for Fault {
    fn from(error: safe_cex_types::StoreReadError) -> Self {
        Self(ExchangeError::from(error))
    }
}
