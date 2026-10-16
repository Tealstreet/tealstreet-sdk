//! Eight source socket method bodies over caller-owned receiver/timer hosts.
//! Construction, transport execution and whole class composition stay pending.
use std::{cell::RefCell, rc::Rc};

/// The source's newly constructed subscription object before JSON.stringify.
#[derive(Debug, Clone)]
pub struct SubscriptionFrame {
    pub channel: &'static str,
    pub assets_ids: Option<Vec<Vec<u16>>>,
    pub custom_feature_enabled: Option<bool>,
    pub auth: Option<WsCredentials>,
}
/// Credentials strings retain every JavaScript UTF-16 unit.
#[derive(Debug, Clone)]
pub struct WsCredentials {
    pub api_key: Vec<u16>,
    pub secret: Vec<u16>,
    pub passphrase: Vec<u16>,
}
/// Receiver and runtime boundaries. Timers/callbacks are owned by the host
/// independently of returned handles: overwriting pingInterval must not cancel
/// the old timeout. Callback child calls resolve the current receiver methods.
pub trait SocketHost: 'static {
    type Timer: 'static;
    type Error: 'static;
    fn is_disposed(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn log(&self, message: &str, severity: &str) -> Result<(), Self::Error>;
    fn send(&self, message: &str, frame: Option<&SubscriptionFrame>) -> Result<(), Self::Error>;
    fn set_timeout(
        &self,
        delay_ms: f64,
        callback: Rc<dyn Fn() -> Result<(), Self::Error>>,
    ) -> Result<Rc<Self::Timer>, Self::Error>;
    fn record_close_diagnostics(
        &self,
        event: Option<()>,
        synthetic_if_no_socket: bool,
    ) -> Result<(), Self::Error>;
    fn clear_ping_interval(&self) -> Result<(), Self::Error>;
    /// Assign the receiver ws property to undefined.
    fn clear_socket_field(&self) -> Result<(), Self::Error>;
    fn ping(&self) -> Result<(), Self::Error>;
    fn connect_and_subscribe(&self) -> Result<(), Self::Error>;
    fn send_subscription(&self) -> Result<(), Self::Error>;
    fn authenticate(&self) -> Result<(), Self::Error>;
    fn emit_reconnected(&self) -> Result<(), Self::Error>;
}
fn message_type(key: &str) -> &'static str {
    crate::constants::WS_MESSAGE_TYPES
        .iter()
        .find(|(name, _)| *name == key)
        .expect("source message type")
        .1
}
// JSON.stringify's well-formed string quoting, retaining lone surrogate units.
fn quoted(units: &[u16]) -> String {
    let mut out = String::from("\"");
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        index += 1;
        match unit {
            0x22 => out.push_str("\\\""),
            0x5c => out.push_str("\\\\"),
            8 => out.push_str("\\b"),
            9 => out.push_str("\\t"),
            10 => out.push_str("\\n"),
            12 => out.push_str("\\f"),
            13 => out.push_str("\\r"),
            0..=31 => out.push_str(&format!("\\u{unit:04x}")),
            0xd800..=0xdbff
                if units
                    .get(index)
                    .is_some_and(|next| (0xdc00..=0xdfff).contains(next)) =>
            {
                let low = units[index];
                index += 1;
                let scalar =
                    0x10000 + ((u32::from(unit) - 0xd800) << 10) + (u32::from(low) - 0xdc00);
                out.push(char::from_u32(scalar).expect("surrogate pair scalar"));
            }
            0xd800..=0xdfff => out.push_str(&format!("\\u{unit:04x}")),
            _ => out.push(char::from_u32(u32::from(unit)).expect("BMP scalar")),
        }
    }
    out.push('"');
    out
}
fn encoded(frame: &SubscriptionFrame) -> String {
    let mut fields = vec![format!(
        "\"type\":{}",
        quoted(&frame.channel.encode_utf16().collect::<Vec<_>>())
    )];
    if let Some(assets) = &frame.assets_ids {
        fields.push(format!(
            "\"assets_ids\":[{}]",
            assets
                .iter()
                .map(|v| quoted(v))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(feature) = frame.custom_feature_enabled {
        fields.push(format!("\"custom_feature_enabled\":{feature}"));
    }
    if let Some(auth) = &frame.auth {
        fields.push(format!(
            "\"auth\":{{\"apiKey\":{},\"secret\":{},\"passphrase\":{}}}",
            quoted(&auth.api_key),
            quoted(&auth.secret),
            quoted(&auth.passphrase)
        ));
    }
    format!("{{{}}}", fields.join(","))
}

/// Source PolymarketPublicWebSocket#sendSubscription. The getter supplies the
/// insertion-ordered snapshot of the source Set, taken once before sending.
pub fn send_public_subscription<H: SocketHost>(
    host: &H,
    assets: impl FnOnce() -> Result<Vec<Vec<u16>>, H::Error>,
) -> Result<(), H::Error> {
    let asset_ids = assets()?;
    if asset_ids.is_empty() {
        return Ok(());
    }
    let count = asset_ids.len();
    let frame = SubscriptionFrame {
        channel: message_type("MARKET"),
        assets_ids: Some(asset_ids),
        custom_feature_enabled: Some(true),
        auth: None,
    };
    host.send(&encoded(&frame), Some(&frame))?;
    host.log(&format!("Subscribed to {count} assets"), "debug")?;
    Ok(())
}
/// Source PolymarketPrivateWebSocket#authenticate.
pub fn authenticate_private<H: SocketHost>(
    host: &H,
    credentials: impl FnOnce() -> Result<Option<WsCredentials>, H::Error>,
) -> Result<(), H::Error> {
    let Some(credentials) = credentials()? else {
        host.log("Cannot authenticate: no credentials", "error")?;
        return Ok(());
    };
    let frame = SubscriptionFrame {
        channel: message_type("USER"),
        assets_ids: None,
        custom_feature_enabled: None,
        auth: Some(credentials),
    };
    host.send(&encoded(&frame), Some(&frame))?;
    host.log("Sent private WebSocket authentication", "debug")?;
    Ok(())
}
/// Source PolymarketPublicWebSocket.onOpen; guard once at entry.
pub fn public_on_open<H: SocketHost>(host: &H) -> Result<(), H::Error> {
    if !host.is_disposed() {
        host.log("Polymarket public WebSocket connected", "info")?;
        host.send_subscription()?;
        host.ping()?;
        host.emit_reconnected()?;
    }
    Ok(())
}
/// Source PolymarketPrivateWebSocket.onOpen; authenticate may return normally
/// without credentials, after which source still invokes ping.
pub fn private_on_open<H: SocketHost>(host: &H) -> Result<(), H::Error> {
    if !host.is_disposed() {
        host.log("Polymarket private WebSocket connected", "info")?;
        host.authenticate()?;
        host.ping()?;
    }
    Ok(())
}
/// Source PolymarketPublicWebSocket.onClose.
pub fn public_on_close<H: SocketHost>(host: Rc<H>) -> Result<(), H::Error> {
    host.record_close_diagnostics(None, true)?;
    host.log("Polymarket public WebSocket closed", "warning")?;
    host.clear_ping_interval()?;
    host.clear_socket_field()?;
    if !host.is_disposed() {
        let reconnecting = host.clone();
        host.set_timeout(
            crate::constants::WS_CONFIG.reconnect_delay,
            Rc::new(move || reconnecting.connect_and_subscribe()),
        )?;
    }
    Ok(())
}
/// Source PolymarketPrivateWebSocket.onClose.
pub fn private_on_close<H: SocketHost>(host: Rc<H>) -> Result<(), H::Error> {
    host.record_close_diagnostics(None, true)?;
    host.log("Polymarket private WebSocket closed", "warning")?;
    host.clear_ping_interval()?;
    host.clear_socket_field()?;
    if !host.is_disposed() {
        let reconnecting = host.clone();
        host.set_timeout(
            crate::constants::WS_CONFIG.reconnect_delay,
            Rc::new(move || reconnecting.connect_and_subscribe()),
        )?;
    }
    Ok(())
}
/// Source PolymarketPublicWebSocket.ping; send precedes unconditional timeout
/// registration, and replacing the stored handle leaves old host timers alive.
pub fn ping_public<H: SocketHost>(
    host: Rc<H>,
    slot: &RefCell<Option<Rc<H::Timer>>>,
) -> Result<(), H::Error> {
    if host.is_connected() {
        host.send("PING", None)?;
    }
    let ticking = host.clone();
    let callback = Rc::new(move || {
        if !ticking.is_disposed() && ticking.is_connected() {
            ticking.ping()?;
        }
        Ok(())
    });
    let handle = host.set_timeout(crate::constants::WS_CONFIG.ping_interval, callback)?;
    *slot.borrow_mut() = Some(handle);
    Ok(())
}
/// Source PolymarketPrivateWebSocket.ping.
pub fn ping_private<H: SocketHost>(
    host: Rc<H>,
    slot: &RefCell<Option<Rc<H::Timer>>>,
) -> Result<(), H::Error> {
    if host.is_connected() {
        host.send("PING", None)?;
    }
    let ticking = host.clone();
    let callback = Rc::new(move || {
        if !ticking.is_disposed() && ticking.is_connected() {
            ticking.ping()?;
        }
        Ok(())
    });
    let handle = host.set_timeout(crate::constants::WS_CONFIG.ping_interval, callback)?;
    *slot.borrow_mut() = Some(handle);
    Ok(())
}
