//! Venue-owned state; base lifecycle and platform objects are injected by the host.
use indexmap::IndexMap;
use safe_cex_num::JsJsonValue;
use std::collections::HashMap;

pub struct PublicSocket<P, R, H, T, S, M> {
    pub parent: P,
    pub route: R,
    pub message_handlers: HashMap<String, Vec<H>>,
    pub topics: IndexMap<String, f64>,
    pub ping_timer: Option<T>,
    pub subscription_timer: Option<T>,
    pub is_secondary: S,
    pub message_count: f64,
    pub last_message_count: f64,
    pub connection_attempts: f64,
    pub max_attempts: f64,
    pub market_symbols: Option<Vec<M>>,
    pub subscription_queue: Vec<Vec<JsJsonValue>>,
}
pub fn new_public_socket<P, R, H, T, S, M: PartialEq, E>(
    parent: P,
    secondary: Option<S>,
    default_secondary: impl FnOnce() -> S,
    market_symbols: Option<Vec<M>>,
    base: impl FnOnce(&P) -> Result<(), E>,
    route: impl FnOnce(&P) -> Result<R, E>,
) -> Result<PublicSocket<P, R, H, T, S, M>, E> {
    let is_secondary = secondary.unwrap_or_else(default_secondary);
    base(&parent)?;
    let route = route(&parent)?;
    let mut unique = None;
    crate::websocket_public::set_market_symbols(
        market_symbols.map(|s| s.into_iter().map(Ok::<_, E>)),
        |value| {
            unique = value;
            Ok(())
        },
    )?;
    Ok(PublicSocket {
        parent,
        route,
        message_handlers: HashMap::new(),
        topics: IndexMap::new(),
        ping_timer: None,
        subscription_timer: None,
        is_secondary,
        message_count: 0.0,
        last_message_count: 0.0,
        connection_attempts: 0.0,
        max_attempts: 2.0,
        market_symbols: unique,
        subscription_queue: Vec::new(),
    })
}
pub struct FuturesSocket<P, R, H, T> {
    pub parent: P,
    pub route: R,
    pub message_handlers: HashMap<String, Vec<H>>,
    pub topics: HashMap<String, f64>,
    pub ping_timer: Option<T>,
    pub message_count: f64,
    pub last_message_count: f64,
    pub connection_attempts: f64,
    pub max_attempts: f64,
}
pub fn new_futures_socket<P, R, H, T, E>(
    parent: P,
    base: impl FnOnce(&P) -> Result<(), E>,
    route: impl FnOnce(&P) -> Result<R, E>,
) -> Result<FuturesSocket<P, R, H, T>, E> {
    base(&parent)?;
    let route = route(&parent)?;
    Ok(FuturesSocket {
        parent,
        route,
        message_handlers: HashMap::new(),
        topics: HashMap::new(),
        ping_timer: None,
        message_count: 0.0,
        last_message_count: 0.0,
        connection_attempts: 0.0,
        max_attempts: 2.0,
    })
}
pub struct PrivateSocket<P, R, T> {
    pub parent: P,
    pub route: R,
    pub ping_timer: Option<T>,
    pub balance_timer: Option<T>,
    pub orders_timer: Option<T>,
    pub positions_timer: Option<T>,
    pub handled_pong: bool,
    pub last_pong: f64,
    pub message_count: f64,
    pub last_message_count: f64,
    pub connection_attempts: f64,
    pub max_attempts: f64,
}
pub fn new_private_socket<P, R, T, E>(
    parent: P,
    base: impl FnOnce(&P) -> Result<(), E>,
    route: impl FnOnce(&P) -> Result<R, E>,
) -> Result<PrivateSocket<P, R, T>, E> {
    base(&parent)?;
    let route = route(&parent)?;
    Ok(PrivateSocket {
        parent,
        route,
        ping_timer: None,
        balance_timer: None,
        orders_timer: None,
        positions_timer: None,
        handled_pong: false,
        last_pong: 0.0,
        message_count: 0.0,
        last_message_count: 0.0,
        connection_attempts: 0.0,
        max_attempts: 2.0,
    })
}
pub struct ClientOrderIdRule {
    pub label: &'static str,
    pub requirement: &'static str,
}
impl ClientOrderIdRule {
    pub fn is_valid(&self, value: &[u16]) -> bool {
        !value.is_empty()
            && value.iter().all(|unit| {
                char::from_u32(u32::from(*unit))
                    .is_none_or(|c| !safe_cex_num::js_trim(&c.to_string()).is_empty())
            })
    }
}
pub struct BitunixExchange<P, A, W> {
    pub client_order_id_rule: ClientOrderIdRule,
    pub parent: P,
    pub xhr: A,
    pub unlimited_xhr: A,
    pub public_socket: W,
    pub secondary_socket: W,
    pub private_socket: W,
    pub futures_socket: W,
    pub public_shards: Vec<W>,
    pub grafana_every: &'static [&'static str],
    pub grafana_periodic: &'static [&'static str],
    pub enable_fake_full_tpsl: bool,
    pub account_position_mode: &'static str,
    pub last_tick_position_mode: &'static str,
    pub symbol_settings: HashMap<String, JsJsonValue>,
    pub position_id_cache: HashMap<String, String>,
    pub order_tif_cache: HashMap<String, String>,
    pub order_reduce_only_cache: HashMap<String, bool>,
}
pub trait ExchangeConstructorHost<N, O, S> {
    type Parent;
    type Client;
    type Socket;
    type Error;
    fn base(&mut self, name: N, options: &O, store: S) -> Result<Self::Parent, Self::Error>;
    fn secondary_client(
        &mut self,
        parent: &Self::Parent,
        options: &O,
        max_rps: f64,
    ) -> Result<Self::Client, Self::Error>;
    fn client(&mut self, parent: &Self::Parent, options: &O) -> Result<Self::Client, Self::Error>;
    fn socket(
        &mut self,
        parent: &Self::Parent,
        kind: &'static str,
        secondary: Option<bool>,
    ) -> Result<Self::Socket, Self::Error>;
}
pub fn new_exchange<N, O, S, H: ExchangeConstructorHost<N, O, S>>(
    host: &mut H,
    name: N,
    options: &O,
    store: S,
) -> Result<BitunixExchange<H::Parent, H::Client, H::Socket>, H::Error> {
    let parent = host.base(name, options, store)?;
    let xhr = host.secondary_client(&parent, options, 5.0)?;
    let unlimited_xhr = host.client(&parent, options)?;
    let public_socket = host.socket(&parent, "public", Some(false))?;
    let secondary_socket = host.socket(&parent, "public", Some(true))?;
    let private_socket = host.socket(&parent, "private", None)?;
    let futures_socket = host.socket(&parent, "futures", None)?;
    Ok(BitunixExchange {
        client_order_id_rule: ClientOrderIdRule {
            label: "Bitunix",
            requirement: r"match /^\S+$/",
        },
        parent,
        xhr,
        unlimited_xhr,
        public_socket,
        secondary_socket,
        private_socket,
        futures_socket,
        public_shards: Vec::new(),
        grafana_every: crate::types::GRAFANA_ENDPOINTS_EVERY,
        grafana_periodic: crate::types::GRAFANA_ENDPOINTS_PERIODIC,
        enable_fake_full_tpsl: false,
        account_position_mode: "hedged",
        last_tick_position_mode: "hedged",
        symbol_settings: HashMap::new(),
        position_id_cache: HashMap::new(),
        order_tif_cache: HashMap::new(),
        order_reduce_only_cache: HashMap::new(),
    })
}
