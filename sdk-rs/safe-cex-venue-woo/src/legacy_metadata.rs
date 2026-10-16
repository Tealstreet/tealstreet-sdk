//! Legacy WOO account mode, metadata cache and leverage/mode method bodies.
use futures_util::FutureExt;
use safe_cex_types::{MarginType, PositionMode, PositionSide};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};
pub type JsString = Vec<u16>;
pub type Task<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
#[derive(Clone, Debug)]
pub struct Metadata {
    pub symbol: JsString,
    pub side: PositionSide,
    pub leverage: f64,
    pub position_mode: PositionMode,
    pub margin_type: MarginType,
}
pub type Rows = Rc<RefCell<Vec<Rc<Metadata>>>>;
#[derive(Clone)]
pub struct Cached {
    pub metadata: Rows,
    pub updated_at: f64,
}
pub struct Shared<T: Clone, E: Clone>(futures_util::future::Shared<Task<T, E>>);
impl<T: Clone, E: Clone> Clone for Shared<T, E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Clone, E: Clone> Shared<T, E> {
    fn new(f: Task<T, E>) -> Self {
        Self(f.shared())
    }
}
impl<T: Clone, E: Clone> Future for Shared<T, E> {
    type Output = Result<T, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}
pub struct Map<V> {
    entries: Vec<(JsString, Option<V>)>,
}
impl<V> Default for Map<V> {
    fn default() -> Self {
        Self { entries: vec![] }
    }
}
impl<V: Clone> Map<V> {
    pub fn get(&self, key: &[u16]) -> Option<V> {
        self.entries
            .iter()
            .rev()
            .find(|(k, v)| k == key && v.is_some())
            .and_then(|(_, v)| v.clone())
    }
    pub fn set(&mut self, key: JsString, value: V) {
        if let Some((_, v)) = self
            .entries
            .iter_mut()
            .rev()
            .find(|(k, v)| k == &key && v.is_some())
        {
            *v = Some(value);
        } else {
            self.entries.push((key, Some(value)));
        }
    }
    pub fn delete(&mut self, key: &[u16]) {
        for (k, v) in &mut self.entries {
            if k == key {
                *v = None;
            }
        }
    }
    pub fn clear(&mut self) {
        for (_, v) in &mut self.entries {
            *v = None;
        }
    }
    pub fn len(&self) -> usize {
        self.entries.iter().filter(|(_, v)| v.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn entries(&self) -> Vec<(JsString, V)> {
        self.entries
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|v| (k.clone(), v.clone())))
            .collect()
    }
}
pub struct State<E: Clone> {
    pub mode: Cell<Option<PositionMode>>,
    pub cache: RefCell<Map<Cached>>,
    pub in_flight: RefCell<Map<Shared<Rows, E>>>,
    pub rate_limited_until: RefCell<Map<f64>>,
}
impl<E: Clone> Default for State<E> {
    fn default() -> Self {
        Self {
            mode: Cell::new(None),
            cache: RefCell::new(Map::default()),
            in_flight: RefCell::new(Map::default()),
            rate_limited_until: RefCell::new(Map::default()),
        }
    }
}
#[derive(Clone, Debug)]
pub enum Field {
    Text(JsString),
    Number(f64),
}
pub type Fields = Vec<(&'static str, Field)>;
/// Host operations preserve JS property/coercion/error identity and eager call effects.
/// Returned tasks model asynchronous continuation; recursive methods use a fresh host frame.
pub trait Host: 'static {
    type Error: Clone + 'static;
    type Value: Clone + 'static;
    type Positions: 'static;
    fn state(&self) -> Rc<State<Self::Error>>;
    fn get(
        &mut self,
        endpoint: &str,
        params: Fields,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn post(&mut self, endpoint: &str, body: Fields) -> Result<Task<(), Self::Error>, Self::Error>;
    fn object(&mut self, v: &Self::Value) -> bool;
    fn truthy(&mut self, v: &Self::Value) -> bool;
    fn field(&mut self, v: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn index(&mut self, v: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn strict_true(&mut self, v: &Self::Value) -> bool;
    fn equals(&mut self, v: &Self::Value, text: &str) -> bool;
    fn text(&mut self, v: &Self::Value) -> Result<JsString, Self::Error>;
    fn response_message(
        &mut self,
        v: &Self::Value,
        fallback: &str,
    ) -> Result<JsString, Self::Error>;
    fn error(&mut self, message: JsString) -> Self::Error;
    fn now(&mut self) -> Result<f64, Self::Error>;
    fn reverse_symbol(&mut self, symbol: &[u16]) -> Result<JsString, Self::Error>;
    fn rate_limit_error(&mut self, error: &Self::Error) -> Result<bool, Self::Error>;
    fn already_set_error(&mut self, error: &Self::Error) -> Result<bool, Self::Error>;
    fn emit_error(
        &mut self,
        error: &Self::Error,
        full: bool,
        response_field: &str,
    ) -> Result<(), Self::Error>;
    fn sleep(&mut self, delay: u64) -> Result<Task<(), Self::Error>, Self::Error>;
    fn fetch_positions(&mut self) -> Result<Task<Self::Positions, Self::Error>, Self::Error>;
    fn update_positions(&mut self, positions: Self::Positions) -> Result<(), Self::Error>;
    fn account_info(&mut self) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn position_mode(&mut self) -> Result<Task<PositionMode, Self::Error>, Self::Error>;
    /// Enqueue independently driven work; polling must begin after this host borrow ends.
    fn spawn_metadata(&mut self, request: Shared<Rows, Self::Error>);
    fn confirm_mode(&mut self, mode: PositionMode) -> Result<Task<(), Self::Error>, Self::Error>;
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn endpoint(k: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == k)
        .expect("legacy endpoint")
        .1
}
fn get<V: Clone>(map: &RefCell<Map<V>>, key: &[u16]) -> Option<V> {
    map.borrow().get(key)
}
fn set<V: Clone>(map: &RefCell<Map<V>>, key: JsString, value: V) {
    map.borrow_mut().set(key, value)
}
fn remove<V: Clone>(map: &RefCell<Map<V>>, key: &[u16]) {
    map.borrow_mut().delete(key)
}
fn mode_text(m: PositionMode) -> &'static str {
    match m {
        PositionMode::OneWay => "oneway",
        PositionMode::Hedged => "hedged",
    }
}
// ts: WOOXExchange#fetchAccountInfo
pub fn fetch_account_info<H: Host>(host: Rc<RefCell<H>>) -> Task<H::Value, H::Error> {
    let response = host.borrow_mut().get(endpoint("ACCOUNT"), vec![]);
    Box::pin(async move {
        let response = response?.await?;
        let mut h = host.borrow_mut();
        if !h.truthy(&response) || !h.object(&response) {
            return Err(h.error(u("WOO X account info response has an invalid shape")));
        }
        let success = h.field(&response, "success")?;
        if !h.strict_true(&success) {
            let m = h.response_message(&response, "WOO X account info request failed")?;
            return Err(h.error(m));
        }
        let data = h.field(&response, "data")?;
        let valid = if !h.truthy(&data) {
            false
        } else {
            let next = h.field(&response, "data")?;
            h.object(&next)
        };
        if !valid {
            let m = h.response_message(&response, "WOO X account info response is missing data")?;
            return Err(h.error(m));
        }
        let data = h.field(&response, "data")?;
        let position = h.field(&data, "positionMode")?;
        if !h.equals(&position, "ONE_WAY") {
            let position = h.field(&data, "positionMode")?;
            if !h.equals(&position, "HEDGE_MODE") {
                return Err(h.error(u(
                    "WOO X account info response has an invalid position mode",
                )));
            }
        }
        Ok(data)
    })
}
// ts: WOOXExchange.getPositionMode
pub fn get_position_mode<H: Host>(host: Rc<RefCell<H>>) -> Task<PositionMode, H::Error> {
    let state = host.borrow().state();
    if let Some(mode) = state.mode.get() {
        return Box::pin(std::future::ready(Ok(mode)));
    }
    let read = host.borrow_mut().account_info();
    Box::pin(async move {
        {
            let data = read?.await?;
            let mut h = host.borrow_mut();
            let m = h.field(&data, "positionMode")?;
            state.mode.set(Some(if h.equals(&m, "ONE_WAY") {
                PositionMode::OneWay
            } else {
                PositionMode::Hedged
            }));
        }
        state
            .mode
            .get()
            .ok_or_else(|| host.borrow_mut().error(u("Position mode became null")))
    })
}
// ts: WOOXExchange#createPositionMetadataCacheKey
pub fn create_position_metadata_cache_key(symbol: &[u16], mode: PositionMode) -> JsString {
    let mut key = symbol.to_vec();
    key.push(58);
    key.extend(u(mode_text(mode)));
    key
}
// ts: WOOXExchange#buildPositionMetadataFromLeverage
pub fn build_position_metadata_from_leverage<H: Host>(
    host: &mut H,
    symbol: JsString,
    mode: PositionMode,
    response: H::Value,
) -> Result<Rows, H::Error> {
    let data = host.field(&response, "data")?;
    let details = host.field(&data, "details")?;
    let first = host.index(&details, 0)?;
    let leverage = host.field(&first, "leverage")?;
    let leverage = host.text(&leverage)?;
    let leverage = safe_cex_num::parse_int(&String::from_utf16_lossy(&leverage), None);
    let mut rows = vec![Rc::new(Metadata {
        symbol: symbol.clone(),
        side: PositionSide::Long,
        leverage,
        position_mode: mode,
        margin_type: MarginType::Cross,
    })];
    if mode != PositionMode::OneWay {
        rows.push(Rc::new(Metadata {
            symbol,
            side: PositionSide::Short,
            leverage,
            position_mode: mode,
            margin_type: MarginType::Cross,
        }));
    }
    Ok(Rc::new(RefCell::new(rows)))
}
fn finish_symbol<H: Host>(
    host: &Rc<RefCell<H>>,
    key: &[u16],
    cached: Option<Cached>,
    result: Result<Rows, H::Error>,
) -> Result<Rows, H::Error> {
    let state = host.borrow().state();
    let committed = result.and_then(|metadata| {
        let now = host.borrow_mut().now()?;
        set(
            &state.cache,
            key.to_vec(),
            Cached {
                metadata: metadata.clone(),
                updated_at: now,
            },
        );
        remove(&state.rate_limited_until, key);
        Ok(metadata)
    });
    let result = (|| match committed {
        Ok(metadata) => Ok(metadata),
        Err(e) => {
            if let Some(cached) = cached {
                if host.borrow_mut().rate_limit_error(&e)? {
                    let until = host.borrow_mut().now()? + 10000.0;
                    set(&state.rate_limited_until, key.to_vec(), until);
                    return Ok(cached.metadata);
                }
            }
            Err(e)
        }
    })();
    remove(&state.in_flight, key);
    result
}
// ts: WOOXExchange#fetchPositionMetadataForSymbol
pub fn fetch_position_metadata_for_symbol<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: JsString,
    mode: PositionMode,
) -> Task<Rows, H::Error> {
    let key = create_position_metadata_cache_key(&symbol, mode);
    let state = host.borrow().state();
    let cached = get(&state.cache, &key);
    let prepared = (|| {
        let now = host.borrow_mut().now()?;
        if let Some(c) = &cached {
            if now - c.updated_at < 30000.0 {
                return Ok(Err(c.metadata.clone()));
            }
        }
        if let Some(in_flight) = get(&state.in_flight, &key) {
            return Ok(Ok(Err(in_flight)));
        }
        if let Some(c) = &cached {
            let until = get(&state.rate_limited_until, &key)
                .filter(|v| *v != 0.0 && !v.is_nan())
                .unwrap_or(0.0);
            if now < until {
                return Ok(Err(c.metadata.clone()));
            }
        }
        let response = (|| {
            let reversed = host.borrow_mut().reverse_symbol(&symbol)?;
            host.borrow_mut().get(
                endpoint("FUTURES_LEVERAGE"),
                vec![
                    ("symbol", Field::Text(reversed)),
                    ("position_mode", Field::Text(u("ONE_WAY"))),
                    ("margin_mode", Field::Text(u("CROSS"))),
                ],
            )
        })();
        Ok(Ok(Ok(response)))
    })();
    let response = match prepared {
        Ok(Err(rows)) => return Box::pin(std::future::ready(Ok(rows))),
        Ok(Ok(Err(shared))) => return Box::pin(shared),
        Err(e) => return Box::pin(std::future::ready(Err(e))),
        Ok(Ok(Ok(response))) => response,
    };
    let task: Task<Rows, H::Error> = match response {
        Err(e) => {
            let r = finish_symbol(&host, &key, cached, Err(e));
            Box::pin(std::future::ready(r))
        }
        Ok(response) => {
            let h = host.clone();
            let key = key.clone();
            Box::pin(async move {
                let r = response.await.and_then(|data| {
                    build_position_metadata_from_leverage(&mut *h.borrow_mut(), symbol, mode, data)
                });
                finish_symbol(&h, &key, cached, r)
            })
        }
    };
    let request = Shared::new(task);
    set(&state.in_flight, key, request.clone());
    host.borrow_mut().spawn_metadata(request.clone());
    Box::pin(request)
}
// ts: WOOXExchange#updateCachedPositionMetadataLeverage
pub fn update_cached_position_metadata_leverage<H: Host>(
    host: &mut H,
    symbol: &[u16],
    leverage: f64,
) -> Result<(), H::Error> {
    let state = host.state();
    let mut prefix = symbol.to_vec();
    prefix.push(58);
    let mut index = 0;
    loop {
        let entry = state.cache.borrow().entries.get(index).cloned();
        let Some((key, cached)) = entry else {
            break;
        };
        index += 1;
        let Some(cached) = cached else {
            continue;
        };
        if !key.starts_with(&prefix) {
            continue;
        }
        let metadata = Rc::new(RefCell::new(
            cached
                .metadata
                .borrow()
                .iter()
                .map(|m| {
                    Rc::new(Metadata {
                        leverage,
                        ..m.as_ref().clone()
                    })
                })
                .collect(),
        ));
        let now = host.now()?;
        set(
            &state.cache,
            key.clone(),
            Cached {
                metadata,
                updated_at: now,
            },
        );
        remove(&state.rate_limited_until, &key);
    }
    Ok(())
}
// ts: WOOXExchange#confirmPositionModeAndRefreshPositions
pub fn confirm_position_mode_and_refresh_positions<H: Host>(
    host: Rc<RefCell<H>>,
    expected: PositionMode,
) -> Task<(), H::Error> {
    let first = {
        let mut h = host.borrow_mut();
        h.state().mode.set(None);
        h.position_mode()
    };
    Box::pin(async move {
        let mut read = first;
        let mut last = None;
        for attempt in 0..3 {
            let observed = read?.await?;
            last = Some(observed);
            if observed == expected {
                let positions = {
                    let mut h = host.borrow_mut();
                    let state = h.state();
                    state.cache.borrow_mut().clear();
                    state.rate_limited_until.borrow_mut().clear();
                    h.fetch_positions()?
                };
                let positions = positions.await?;
                host.borrow_mut().update_positions(positions)?;
                return Ok(());
            }
            if attempt + 1 < 3 {
                let sleep = host.borrow_mut().sleep(250)?;
                sleep.await?;
                let mut h = host.borrow_mut();
                h.state().mode.set(None);
                read = h.position_mode();
            } else {
                break;
            }
        }
        let m = format!(
            "WOO position mode readback stayed {}; expected {}",
            mode_text(last.expect("three reads")),
            mode_text(expected)
        );
        Err(host.borrow_mut().error(u(&m)))
    })
}
// ts: WOOXExchange.setLeverage
pub fn set_leverage<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: JsString,
    leverage: f64,
) -> Task<(), H::Error> {
    let prepared = (|| {
        let mut h = host.borrow_mut();
        let symbol = h.reverse_symbol(&symbol)?;
        h.post(
            endpoint("FUTURES_LEVERAGE"),
            vec![
                ("symbol", Field::Text(symbol)),
                ("margin_mode", Field::Text(u("CROSS"))),
                ("position_side", Field::Text(u("BOTH"))),
                ("leverage", Field::Number(leverage)),
            ],
        )
    })();
    let prepared = match prepared {
        Ok(f) => f,
        Err(e) => {
            let result = host
                .borrow_mut()
                .emit_error(&e, false, "message")
                .and_then(|()| Err(e));
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = async {
            prepared.await?;
            update_cached_position_metadata_leverage(&mut *host.borrow_mut(), &symbol, leverage)
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                host.borrow_mut().emit_error(&e, false, "message")?;
                Err(e)
            }
        }
    })
}
fn change_mode_catch<H: Host>(
    host: Rc<RefCell<H>>,
    expected: PositionMode,
    e: H::Error,
) -> Task<(), H::Error> {
    let already = host.borrow_mut().already_set_error(&e);
    match already {
        Err(e) => Box::pin(std::future::ready(Err(e))),
        Ok(true) => match host.borrow_mut().confirm_mode(expected) {
            Ok(f) => f,
            Err(e) => Box::pin(std::future::ready(Err(e))),
        },
        Ok(false) => {
            let result = host
                .borrow_mut()
                .emit_error(&e, true, "message")
                .and_then(|()| Err(e));
            Box::pin(std::future::ready(result))
        }
    }
}
// ts: WOOXExchange.changePositionMode
pub fn change_position_mode<H: Host>(host: Rc<RefCell<H>>, hedged: bool) -> Task<(), H::Error> {
    let expected = if hedged {
        PositionMode::Hedged
    } else {
        PositionMode::OneWay
    };
    let response = host.borrow_mut().post(
        endpoint("POSITION_MODE"),
        vec![(
            "position_mode",
            Field::Text(u(if hedged { "HEDGE_MODE" } else { "ONE_WAY" })),
        )],
    );
    let response = match response {
        Ok(f) => f,
        Err(e) => return change_mode_catch(host, expected, e),
    };
    Box::pin(async move {
        let result = async {
            response.await?;
            let confirm = host.borrow_mut().confirm_mode(expected)?;
            confirm.await
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(e) => change_mode_catch(host, expected, e).await,
        }
    })
}

// ts: WOOXExchange.afterMethodCalledInWorker
pub fn after_method_called_in_worker<H: Host>(
    host: &mut H,
    method: &str,
    argument: Option<H::Value>,
) {
    if method == "changePositionMode" {
        let hedged = argument.as_ref().is_some_and(|v| host.truthy(v));
        host.state().mode.set(Some(if hedged {
            PositionMode::Hedged
        } else {
            PositionMode::OneWay
        }));
    }
}
