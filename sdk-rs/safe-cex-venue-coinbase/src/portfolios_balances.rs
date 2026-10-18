//! Portfolio selection and balance field bodies over observed portfolio fields and JSON replies.
//! Full exchange construction and HTTP binding remain separate. Hosts drive hot promises.
use crate::starbase_auth::{CoinbaseStarbasePromise, CoinbaseStarbasePromiseSpawner};
use indexmap::IndexMap;
use safe_cex_num::{js_number, js_truthy, parse_float};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// A fulfilled source Promise retains its one mutable returned array.
pub type SupportedTypes = Rc<RefCell<Vec<String>>>;
/// Mutable selection state, projected at the HTTP boundary to the two fields the
/// source reads. Unobserved metadata is retained without applying the wire DTO's
/// declaration-time requirements to a runtime response.
#[derive(Debug, Clone)]
pub struct PortfolioState {
    pub uuid: String,
    pub r#type: String,
    pub metadata: IndexMap<Vec<u16>, Input>,
}
impl<'de> serde::Deserialize<'de> for PortfolioState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let input = Input::deserialize(deserializer)?;
        let Input::Object(fields) = input else {
            return Err(D::Error::custom("Coinbase portfolio must be an object"));
        };
        let mut metadata: IndexMap<_, _> = fields.into_iter().collect();
        let mut text = |key: &str| {
            let value = metadata.shift_remove(&key.encode_utf16().collect::<Vec<_>>());
            match value {
                Some(Input::Text(units)) => String::from_utf16(&units).map_err(D::Error::custom),
                _ => Err(D::Error::custom(format!(
                    "Coinbase portfolio {key} must be a string"
                ))),
            }
        };
        let uuid = text("uuid")?;
        let r#type = text("type")?;
        Ok(Self {
            uuid,
            r#type,
            metadata,
        })
    }
}
impl serde::Serialize for PortfolioState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut fields = vec![
            (
                "uuid".encode_utf16().collect(),
                Input::Text(self.uuid.encode_utf16().collect()),
            ),
            (
                "type".encode_utf16().collect(),
                Input::Text(self.r#type.encode_utf16().collect()),
            ),
        ];
        fields.extend(
            self.metadata
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        Input::Object(fields).serialize(serializer)
    }
}
pub type Portfolio = Rc<RefCell<PortfolioState>>;
pub type Portfolios = Rc<RefCell<Vec<Portfolio>>>;
pub type PortfolioFailure = Rc<PortfolioThrown>;
/// A thrown JSON value or an original native Store failure, shared by all callers.
#[derive(Debug)]
pub struct PortfolioThrown {
    /// JSON property view for existing HTTP/message-policy bodies; not the native origin.
    pub value: Option<Value>,
    store_mutation: Option<safe_cex_store::StoreMutationError>,
    native: Option<Rc<safe_cex_exchange::runtime::ExchangeError>>,
}
impl PortfolioThrown {
    /// None is the source undefined value. JSON throws have no native Store origin.
    pub fn from_value(value: Option<Value>) -> Self {
        Self {
            value,
            store_mutation: None,
            native: None,
        }
    }
    pub(crate) fn from_native(error: safe_cex_exchange::runtime::ExchangeError) -> Self {
        let mut value = serde_json::Map::from_iter([
            ("name".into(), Value::String(error.name.clone())),
            ("message".into(), Value::String(error.message.clone())),
        ]);
        for (key, field) in [
            ("response", &error.response_json_view()),
            ("config", &error.config),
        ] {
            if let Some(field) = field {
                value.insert(key.into(), field.clone());
            }
        }
        if let Some(code) = &error.code {
            match safe_cex_exchange::js_runtime::value::to_json(code) {
                Ok(code) => {
                    value.insert("code".into(), code);
                }
                Err(cause) => {
                    return Self {
                        value: None,
                        store_mutation: None,
                        native: Some(Rc::new(cause.to_exchange_error())),
                    };
                }
            }
        }
        Self {
            value: Some(Value::Object(value)),
            store_mutation: None,
            native: Some(Rc::new(error)),
        }
    }
    pub(crate) fn native(&self) -> Option<&safe_cex_exchange::runtime::ExchangeError> {
        self.native.as_deref()
    }
    /// Access the retained failure, including subscriber identity and original cause.
    pub fn store_mutation(&self) -> Option<&safe_cex_store::StoreMutationError> {
        self.store_mutation.as_ref()
    }
}
impl From<safe_cex_store::StoreMutationError> for PortfolioThrown {
    fn from(error: safe_cex_store::StoreMutationError) -> Self {
        // Snapshot publication propagates the original error without inspecting properties.
        // Other catch/message-policy bodies must bind their native property access separately.
        Self {
            value: None,
            store_mutation: Some(error),
            native: None,
        }
    }
}
impl std::fmt::Display for PortfolioThrown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(error) = &self.store_mutation {
            error.fmt(f)
        } else if let Some(value) = &self.value {
            value.fmt(f)
        } else {
            f.write_str("undefined")
        }
    }
}
impl std::error::Error for PortfolioThrown {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.native
            .as_deref()
            .map(|error| error as &dyn std::error::Error)
            .or_else(|| {
                self.store_mutation
                    .as_ref()
                    .map(|error| error as &dyn std::error::Error)
            })
    }
}
/// HTTP data is a plain, inert JSON record. Portfolio arrays additionally retain identity.
#[derive(Clone)]
pub struct PortfolioReply {
    pub data: Input,
    pub portfolios: Option<Portfolios>,
}
pub type Balance = Rc<RefCell<PortfolioBalance>>;
pub type Currencies = Rc<RefCell<PortfolioCurrencies>>;
/// Inline balance object, including shallow retained extra fields from cached balances.
#[derive(Debug, Default)]
pub struct PortfolioBalance {
    pub total: f64,
    pub free: f64,
    pub used: f64,
    pub upnl: f64,
    pub currencies: Option<Currencies>,
    /// Distinguishes an absent property from an own property containing undefined.
    pub has_currencies: bool,
    pub extra: IndexMap<String, Rc<RefCell<Input>>>,
    /// Imported typed Store snapshot, before JSON projection of optional model
    /// fields. Source scalar/currency edits still live in the fields above.
    pub native_snapshot: Option<Rc<safe_cex_types::CurrencyBalance>>,
}
/// Ordinary-object reducer: own keys follow JS order; __proto__ sets the prototype.
#[derive(Debug, Default)]
pub struct PortfolioCurrencies {
    pub own: JsObjectMap<Balance>,
    pub prototype: Option<Balance>,
}
/// External call sites only; all mapped selection/balance bodies live in this module.
pub trait PortfolioHost {
    fn get(
        &self,
        endpoint: &'static str,
        portfolio_uuid: Option<String>,
    ) -> CoinbaseStarbasePromise<Result<PortfolioReply, PortfolioFailure>>;
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure>;
    fn cached_balance(&self) -> Result<Balance, PortfolioFailure>;
    fn position_upnls(&self) -> Result<Vec<f64>, PortfolioFailure>;
    /// Optional virtual method override, e.g. an application-specific supported-type provider.
    fn supported_types_override(
        &self,
    ) -> Option<CoinbaseStarbasePromise<Result<SupportedTypes, PortfolioFailure>>> {
        None
    }
}
/// Live class calls retain one source receiver and read its current method slots.
pub trait PortfolioClassBinding {
    fn public_only(&self) -> bool;
    fn support_spot(&self) -> bool;
    fn find(&self) -> CoinbaseStarbasePromise<Result<Portfolio, PortfolioFailure>>;
    fn supported(&self) -> CoinbaseStarbasePromise<Result<SupportedTypes, PortfolioFailure>>;
}
/// Native receiver fields; Rc portfolio roots preserve mutation and selection identity.
pub struct PortfolioReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
    pub selected: Rc<RefCell<Option<Portfolio>>>,
    pub public_only: Cell<bool>,
    pub support_spot: Cell<bool>,
    pub spot: bool,
    pub class: Option<Rc<dyn PortfolioClassBinding>>,
}
const LIST: &str = "/api/v3/brokerage/portfolios";
const BREAKDOWN: &str = "/api/v3/brokerage/portfolios/{portfolio_uuid}";
const CFM: &str = "/api/v3/brokerage/cfm/balance_summary";
const INTX: &str = "/api/v3/brokerage/intx/portfolio/{portfolio_uuid}";
fn error(name: &str, message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":name,"message":message}),
    )))
}
fn detail(err: &PortfolioFailure) -> Option<Value> {
    let remote = err
        .value
        .as_ref()
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("message"));
    if js_truthy(remote) {
        remote.cloned()
    } else {
        err.value.as_ref().and_then(|v| v.get("message")).cloned()
    }
}
fn emit<H: PortfolioHost>(r: &PortfolioReceiver<H>, message: &str) -> Result<(), PortfolioFailure> {
    r.host
        .emit_and_log_error(&[Some(Value::String(message.into()))])
}
fn prop<'a>(v: Option<&'a Input>, key: &str) -> Result<Option<&'a Input>, PortfolioFailure> {
    match v {
        None | Some(Input::Null | Input::Undefined) => Err(error(
            "TypeError",
            &format!(
                "Cannot read properties of {} (reading '{key}')",
                if v.is_none() { "undefined" } else { "null" }
            ),
        )),
        Some(v) => Ok(v.get(key)),
    }
}
fn amount(data: &Input, group: &str, field: &str) -> Result<f64, PortfolioFailure> {
    let v = prop(prop(prop(Some(data), group)?, field)?, "value")?;
    safe_cex_num::js_retained_string(v)
        .map(|v| parse_float(&v))
        .map_err(|e| error(&e.name, &e.message))
}
fn scalar_balance(total: f64, free: f64, upnl: f64) -> Balance {
    Rc::new(RefCell::new(PortfolioBalance {
        total,
        free,
        used: total - free,
        upnl,
        ..Default::default()
    }))
}
fn all<T: Clone + 'static>(
    promises: Vec<CoinbaseStarbasePromise<T>>,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> CoinbaseStarbasePromise<Vec<T>> {
    let (promise, resolve) = CoinbaseStarbasePromise::pending();
    if promises.is_empty() {
        resolve.resolve(vec![]);
        return promise;
    }
    let remaining = Rc::new(Cell::new(promises.len()));
    let values = Rc::new(RefCell::new(vec![None; promises.len()]));
    for (i, child) in promises.into_iter().enumerate() {
        let (values, remaining, resolve) = (values.clone(), remaining.clone(), resolve.clone());
        child.observe(spawn, move |value| {
            values.borrow_mut()[i] = Some(value);
            remaining.set(remaining.get() - 1);
            if remaining.get() == 0 {
                resolve.resolve(
                    values
                        .borrow()
                        .iter()
                        .map(|v| v.clone().expect("settled child"))
                        .collect(),
                );
            }
        });
    }
    promise
}
fn find<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Portfolio, PortfolioFailure>> {
    if let Some(class) = &r.class {
        return class.find();
    }
    if r.spot {
        find_selected_spot_portfolio(r)
    } else {
        find_selected_portfolio(r)
    }
}
fn supported<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<SupportedTypes, PortfolioFailure>> {
    if let Some(class) = &r.class {
        return class.supported();
    }
    if let Some(promise) = r.host.supported_types_override() {
        return promise;
    }
    if r.spot {
        get_spot_supported_types(r)
    } else {
        get_supported_types(r)
    }
}
/// Source spot probe predicate, including numeric coercion and undefined status.
pub fn should_rethrow_portfolio_probe_error(
    err: &PortfolioFailure,
) -> Result<bool, PortfolioFailure> {
    let status = err
        .value
        .as_ref()
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("status"));
    if status.and_then(Value::as_f64) == Some(429.0) {
        return Ok(true);
    }
    let number = js_number(status).map_err(|e| error(&e.name, &e.message))?;
    Ok(number >= 500.0 || status.is_none())
}
pub fn fetch_portfolios<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Portfolios, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            match r.host.get(LIST, None).await {
                Ok(reply) => Ok(reply.portfolios.expect("typed LIST_PORTFOLIOS response")),
                Err(err) => {
                    r.host
                        .emit_and_log_error(&[err.value.clone(), detail(&err)])?;
                    Ok(Rc::new(RefCell::new(vec![])))
                }
            }
        }),
        &spawn,
    )
}
pub fn find_selected_portfolio<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Portfolio, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let portfolios = fetch_portfolios(r.clone()).await?;
            // Array.map captures the initial length but reads each live index.
            // Never retain a RefCell guard across an external eager HTTP invocation.
            let length = portfolios.borrow().len();
            let mut probes = Vec::with_capacity(length);
            for index in 0..length {
                let portfolio = portfolios.borrow().get(index).cloned();
                if let Some(portfolio) = portfolio {
                    let uuid = portfolio.borrow().uuid.clone();
                    let request = r.host.get(BREAKDOWN, Some(uuid));
                    probes.push(CoinbaseStarbasePromise::from_future(
                        Box::pin(async move { request.await.is_ok() }),
                        &r.spawn,
                    ));
                } else {
                    // A removed array index becomes a hole: Promise.all resolves
                    // it as undefined, which the source tests as !== null.
                    probes.push(CoinbaseStarbasePromise::resolved(true));
                }
            }
            let replies = all(probes, &r.spawn).await;
            let Some(index) = replies.iter().position(|success| *success) else {
                emit(&r, "No portfolio with permissions found")?;
                return Err(error("Error", "No portfolio with permissions found"));
            };
            let portfolio = portfolios.borrow().get(index).cloned().ok_or_else(|| {
                error(
                    "TypeError",
                    "Cannot read properties of undefined (reading 'type')",
                )
            })?;
            let is_intx = portfolio.borrow().r#type == "INTX";
            if !is_intx {
                if let Ok(reply) = r.host.get(CFM, None).await {
                    if reply
                        .data
                        .get("balance_summary")
                        .is_some_and(Input::is_truthy)
                    {
                        portfolio.borrow_mut().r#type = "CFM".into();
                    }
                }
            }
            Ok(portfolio)
        }),
        &spawn,
    )
}
pub fn find_selected_spot_portfolio<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Portfolio, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let portfolios = match r.host.get(LIST, None).await {
                Ok(reply) => reply.portfolios.expect("typed LIST_PORTFOLIOS response"),
                Err(err) => {
                    r.host
                        .emit_and_log_error(&[err.value.clone(), detail(&err)])?;
                    return Err(err);
                }
            };
            let mut index = 0;
            loop {
                let portfolio = portfolios.borrow().get(index).cloned();
                let Some(portfolio) = portfolio else {
                    break;
                };
                index += 1;
                if portfolio.borrow().r#type == "INTX" {
                    continue;
                }
                let uuid = portfolio.borrow().uuid.clone();
                match r.host.get(BREAKDOWN, Some(uuid)).await {
                    Ok(_) => return Ok(portfolio),
                    Err(err) => {
                        if should_rethrow_portfolio_probe_error(&err)? {
                            r.host
                                .emit_and_log_error(&[err.value.clone(), detail(&err)])?;
                            return Err(err);
                        }
                    }
                }
            }
            emit(&r, "No Coinbase spot portfolio with permissions found")?;
            Err(error(
                "Error",
                "No Coinbase spot portfolio with permissions found",
            ))
        }),
        &spawn,
    )
}
pub fn get_supported_types<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<SupportedTypes, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            if r.class
                .as_ref()
                .map_or_else(|| r.public_only.get(), |class| class.public_only())
            {
                return Ok(Rc::new(RefCell::new(vec!["intx".into()])));
            }
            let missing = r.selected.borrow().is_none();
            if missing {
                let p = find(r.clone()).await?;
                *r.selected.borrow_mut() = Some(p);
            }
            let portfolio = r.selected.borrow().as_ref().unwrap().clone();
            let mut supported = vec![];
            if portfolio.borrow().r#type == "INTX" {
                supported.push("intx".into());
            } else if portfolio.borrow().r#type == "CFM" {
                supported.push("cfm".into());
            }
            if r.class
                .as_ref()
                .map_or_else(|| r.support_spot.get(), |class| class.support_spot())
                && portfolio.borrow().r#type != "INTX"
            {
                supported.push("spot".into());
            }
            if supported.is_empty() {
                emit(&r, "No supported portfolios found")?;
            }
            Ok(Rc::new(RefCell::new(supported)))
        }),
        &spawn,
    )
}
pub fn get_spot_supported_types<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<SupportedTypes, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let missing = r.selected.borrow().is_none();
            if missing {
                let p = find(r.clone()).await?;
                *r.selected.borrow_mut() = Some(p);
            }
            Ok(Rc::new(RefCell::new(vec!["spot".into()])))
        }),
        &spawn,
    )
}
pub fn fetch_balance_cfm<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Balance, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let result = async {
                let reply = r.host.get(CFM, None).await?;
                Ok(scalar_balance(
                    amount(&reply.data, "balance_summary", "total_usd_balance")?,
                    amount(&reply.data, "balance_summary", "available_margin")?,
                    0.0,
                ))
            }
            .await;
            match result {
                Ok(balance) => Ok(balance),
                Err(err) => {
                    r.host.emit_and_log_error(&[detail(&err)])?;
                    Ok(r.host.cached_balance()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_balance_intx<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Balance, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let portfolio = r.selected.borrow().clone();
            let portfolio = match portfolio {
                Some(p) if p.borrow().r#type == "INTX" => p,
                _ => {
                    emit(&r, "No INTX portfolio found")?;
                    return r.host.cached_balance();
                }
            };
            let result = async {
                let uuid = portfolio.borrow().uuid.clone();
                let reply = r.host.get(INTX, Some(uuid)).await?;
                let total = amount(&reply.data, "summary", "total_balance")?;
                let free = amount(&reply.data, "summary", "buying_power")?;
                let upnl = r.host.position_upnls()?.iter().fold(0.0, |acc, p| acc + p);
                Ok(scalar_balance(total, free, upnl))
            }
            .await;
            match result {
                Ok(balance) => Ok(balance),
                Err(err) => {
                    r.host.emit_and_log_error(&[detail(&err)])?;
                    Ok(r.host.cached_balance()?)
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_spot_currencies<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Option<Currencies>, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let portfolio = r.selected.borrow().clone();
            let portfolio = match portfolio {
                Some(p) if p.borrow().r#type != "INTX" => p,
                _ => {
                    emit(&r, "No spot portfolio found")?;
                    return Ok(r.host.cached_balance()?.borrow().currencies.clone());
                }
            };
            let result = async {
                let uuid = portfolio.borrow().uuid.clone();
                let reply = r.host.get(BREAKDOWN, Some(uuid)).await?;
                let breakdown = prop(Some(&reply.data), "breakdown")?;
                let positions_value = prop(breakdown, "spot_positions")?;
                // Evaluate the .reduce property access before checking callability.
                prop(positions_value, "reduce")?;
                let positions = positions_value.and_then(Input::as_array).ok_or_else(|| {
                    error(
                        "TypeError",
                        "data.breakdown.spot_positions.reduce is not a function",
                    )
                })?;
                let mut currencies = PortfolioCurrencies::default();
                for p in positions {
                    // TS subtracts and keys by these wire fields with JS coercion, never throwing.
                    let coerce = |e: safe_cex_num::JsCoercionError| error(&e.name, &e.message);
                    let free = safe_cex_num::js_retained_number(p.get("available_to_trade_crypto"))
                        .map_err(coerce)?;
                    let total = safe_cex_num::js_retained_number(p.get("total_balance_crypto"))
                        .map_err(coerce)?;
                    let balance = scalar_balance(free, free, 0.0);
                    balance.borrow_mut().used = total - free;
                    let asset = safe_cex_num::js_retained_string(p.get("asset")).map_err(coerce)?;
                    let asset = asset.as_str();
                    if asset == "__proto__" {
                        currencies.prototype = Some(balance);
                    } else {
                        currencies.own.insert(asset.into(), balance);
                    }
                }
                Ok(Some(Rc::new(RefCell::new(currencies))))
            }
            .await;
            match result {
                Ok(currencies) => Ok(currencies),
                Err(err) => {
                    r.host.emit_and_log_error(&[detail(&err)])?;
                    Ok(r.host.cached_balance()?.borrow().currencies.clone())
                }
            }
        }),
        &spawn,
    )
}
pub fn fetch_balance<H: PortfolioHost + 'static>(
    r: Rc<PortfolioReceiver<H>>,
) -> CoinbaseStarbasePromise<Result<Balance, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    CoinbaseStarbasePromise::from_future(
        Box::pin(async move {
            let missing = r.selected.borrow().is_none();
            if missing {
                let p = find(r.clone()).await?;
                *r.selected.borrow_mut() = Some(p);
            }
            let supported = supported(r.clone()).await?;
            let has = |kind: &str| supported.borrow().iter().any(|v| v == kind);
            if has("intx") && has("cfm") {
                emit(&r, "Cannot have both INTX and CFM portfolios")?;
                return r.host.cached_balance();
            }
            let mut balance = scalar_balance(0.0, 0.0, 0.0);
            if has("intx") {
                balance = fetch_balance_intx(r.clone()).await?;
            }
            if has("cfm") {
                balance = fetch_balance_cfm(r.clone()).await?;
            }
            if has("spot") {
                let currencies = fetch_spot_currencies(r.clone()).await?;
                let replacement = {
                    let b = balance.borrow();
                    Rc::new(RefCell::new(PortfolioBalance {
                        total: b.total,
                        free: b.free,
                        used: b.used,
                        upnl: b.upnl,
                        currencies,
                        has_currencies: true,
                        extra: b
                            .extra
                            .iter()
                            .map(|(k, v)| {
                                let copy = if v.borrow().is_object() || v.borrow().is_array() {
                                    v.clone()
                                } else {
                                    Rc::new(RefCell::new(v.borrow().clone()))
                                };
                                (k.clone(), copy)
                            })
                            .collect(),
                        native_snapshot: b.native_snapshot.clone(),
                    }))
                };
                balance = replacement;
            }
            Ok(balance)
        }),
        &spawn,
    )
}

// A retained class host is the same imported HTTP/Store capability, not a copy.
impl<H: PortfolioHost> PortfolioHost for Rc<H> {
    fn get(
        &self,
        endpoint: &'static str,
        uuid: Option<String>,
    ) -> CoinbaseStarbasePromise<Result<PortfolioReply, PortfolioFailure>> {
        (**self).get(endpoint, uuid)
    }
    fn emit_and_log_error(&self, args: &[Option<Value>]) -> Result<(), PortfolioFailure> {
        (**self).emit_and_log_error(args)
    }
    fn cached_balance(&self) -> Result<Balance, PortfolioFailure> {
        (**self).cached_balance()
    }
    fn position_upnls(&self) -> Result<Vec<f64>, PortfolioFailure> {
        (**self).position_upnls()
    }
    fn supported_types_override(
        &self,
    ) -> Option<CoinbaseStarbasePromise<Result<SupportedTypes, PortfolioFailure>>> {
        (**self).supported_types_override()
    }
}
