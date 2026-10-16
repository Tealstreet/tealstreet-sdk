//! Core private websocket derived construction and source field initialization.
//! The class frame and complete callable dispatcher remain separate work. This
//! constructor owns field order; native operations and mapped arrow allocation
//! are explicit embedding bindings.
use crate::ws_ping_lifecycle::WsPingHost;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorePrivateArrow {
    ConnectAndSubscribe,
    OnOpen,
    StartPingLoop,
    SchedulePing,
    StopPingLoop,
    Subscribe,
    Ping,
    SubscribeSystemTopics,
    SubscribeAllActiveAssetData,
    OnMessage,
    SendWithDiagnostics,
    EmitReceiveDiagnostics,
    HandleUserEvents,
    Dispose,
    HandleHip3ClearinghouseState,
    HandleFills,
    HandleFundings,
    HandleUserNonFundingLedgerUpdates,
    HandleOrderUpdates,
}
#[derive(Clone, Debug)]
pub struct CorePrivateCapture<V> {
    pub class: V,
    pub receiver: V,
    pub body: CorePrivateArrow,
}
pub trait PrivateConstructorHost: WsPingHost {
    fn number(&self, value: f64) -> Self::Value;
    fn array(&mut self) -> Result<Self::Value, Self::Error>;
    fn construct(
        &mut self,
        constructor: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    /// Native super(parent), using the current superclass of class and the
    /// original newTarget, including prototype resolution and base replacement.
    fn private_super(
        &mut self,
        class: &Self::Value,
        parent: &Self::Value,
        new_target: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Allocate a fresh lexical arrow bound to its mapped Rust body. Preserve
    /// name/length, class binding and original constructed receiver even when
    /// detached or called with a different receiver. Full class dispatch is a
    /// separate composition root, not an implementation supplied by this body.
    fn private_arrow(
        &mut self,
        capture: CorePrivateCapture<Self::Value>,
        name: &str,
        length: u8,
    ) -> Result<Self::Value, Self::Error>;
}
fn collection<H: PrivateConstructorHost>(h: &mut H, name: &str) -> Result<H::Value, H::Error> {
    let constructor = h.global(name)?;
    h.construct(&constructor, &[])
}
/// Source HyperliquidPrivateWebSocket.constructor, including derived fields.
/// class is the defining lexical constructor, not this.constructor/newTarget.
pub fn new_core_private_socket<H: PrivateConstructorHost>(
    h: &mut H,
    class: &H::Value,
    parent: &H::Value,
    new_target: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let native = h.private_super(class, parent, new_target.unwrap_or(class))?;
    let value = h.undefined();
    h.define_data(&native, "hlPingTimeoutId", &value)?;
    let value = {
        let state = h.object()?;
        let last_time = collection(h, "Map")?;
        h.define_data(&state, "lastTime", &last_time)?;
        let last_hash = collection(h, "Map")?;
        h.define_data(&state, "lastDataHash", &last_hash)?;
        state
    };
    h.define_data(&native, "timestampState", &value)?;
    let value = h.number(0.0);
    h.define_data(&native, "lastCoreClearinghouseStateAt", &value)?;
    let value = h.number(0.0);
    h.define_data(&native, "lastCoreClearinghouseStateServerTime", &value)?;
    let value = collection(h, "Map")?;
    h.define_data(&native, "realizedPnlByPositionKey", &value)?;
    let value = collection(h, "Set")?;
    h.define_data(&native, "processedRealizedPnlFillIds", &value)?;
    let value = h.array()?;
    h.define_data(&native, "processedRealizedPnlFillQueue", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::ConnectAndSubscribe,
            class: class.clone(),
            receiver: native.clone(),
        },
        "connectAndSubscribe",
        0,
    )?;
    h.define_data(&native, "connectAndSubscribe", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::OnOpen,
            class: class.clone(),
            receiver: native.clone(),
        },
        "onOpen",
        0,
    )?;
    h.define_data(&native, "onOpen", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::StartPingLoop,
            class: class.clone(),
            receiver: native.clone(),
        },
        "startPingLoop",
        0,
    )?;
    h.define_data(&native, "startPingLoop", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::SchedulePing,
            class: class.clone(),
            receiver: native.clone(),
        },
        "schedulePing",
        0,
    )?;
    h.define_data(&native, "schedulePing", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::StopPingLoop,
            class: class.clone(),
            receiver: native.clone(),
        },
        "stopPingLoop",
        0,
    )?;
    h.define_data(&native, "stopPingLoop", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::Subscribe,
            class: class.clone(),
            receiver: native.clone(),
        },
        "subscribe",
        0,
    )?;
    h.define_data(&native, "subscribe", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::Ping,
            class: class.clone(),
            receiver: native.clone(),
        },
        "ping",
        0,
    )?;
    h.define_data(&native, "ping", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::SubscribeSystemTopics,
            class: class.clone(),
            receiver: native.clone(),
        },
        "subscribeSystemTopics",
        0,
    )?;
    h.define_data(&native, "subscribeSystemTopics", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::SubscribeAllActiveAssetData,
            class: class.clone(),
            receiver: native.clone(),
        },
        "subscribeAllActiveAssetData",
        0,
    )?;
    h.define_data(&native, "subscribeAllActiveAssetData", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::OnMessage,
            class: class.clone(),
            receiver: native.clone(),
        },
        "onMessage",
        1,
    )?;
    h.define_data(&native, "onMessage", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::SendWithDiagnostics,
            class: class.clone(),
            receiver: native.clone(),
        },
        "sendWithDiagnostics",
        1,
    )?;
    h.define_data(&native, "sendWithDiagnostics", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::EmitReceiveDiagnostics,
            class: class.clone(),
            receiver: native.clone(),
        },
        "emitReceiveDiagnostics",
        2,
    )?;
    h.define_data(&native, "emitReceiveDiagnostics", &value)?;
    let value = {
        let buffer = h.object()?;
        h.define_data(&buffer, "last", &h.number(0.0))?;
        h.define_data(&buffer, "midPrice", &h.number(0.0))?;
        buffer
    };
    h.define_data(&native, "_hip3MidsUpdateBuf", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::HandleUserEvents,
            class: class.clone(),
            receiver: native.clone(),
        },
        "handleUserEvents",
        1,
    )?;
    h.define_data(&native, "handleUserEvents", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::Dispose,
            class: class.clone(),
            receiver: native.clone(),
        },
        "dispose",
        0,
    )?;
    h.define_data(&native, "dispose", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::HandleHip3ClearinghouseState,
            class: class.clone(),
            receiver: native.clone(),
        },
        "handleHip3ClearinghouseState",
        2,
    )?;
    h.define_data(&native, "handleHip3ClearinghouseState", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::HandleFills,
            class: class.clone(),
            receiver: native.clone(),
        },
        "handleFills",
        1,
    )?;
    h.define_data(&native, "handleFills", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::HandleFundings,
            class: class.clone(),
            receiver: native.clone(),
        },
        "handleFundings",
        1,
    )?;
    h.define_data(&native, "handleFundings", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::HandleUserNonFundingLedgerUpdates,
            class: class.clone(),
            receiver: native.clone(),
        },
        "handleUserNonFundingLedgerUpdates",
        1,
    )?;
    h.define_data(&native, "handleUserNonFundingLedgerUpdates", &value)?;
    let value = h.private_arrow(
        CorePrivateCapture {
            body: CorePrivateArrow::HandleOrderUpdates,
            class: class.clone(),
            receiver: native.clone(),
        },
        "handleOrderUpdates",
        1,
    )?;
    h.define_data(&native, "handleOrderUpdates", &value)?;
    Ok(native)
}
