//! Public socket derived constructors and source field initialization.
//! Full class frames and callable dispatch composition remain separate roots.
use crate::ws_ping_lifecycle::WsPingHost;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicSocketKind {
    Core,
    Spot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicSocketArrow {
    ConnectAndSubscribe,
    OnOpen,
    StartPingLoop,
    SchedulePing,
    StopPingLoop,
    SubscribePublicTopics,
    SendSubscription,
    SendUnsubscription,
    SubscribeUserTopics,
    Ping,
    OnMessage,
    SendWithDiagnostics,
    EmitReceiveDiagnostics,
    HandleAllMids,
    HandleFastAssetCtxs,
    ListenOHLCV,
    ListenOrderBook,
}
#[derive(Clone, Debug)]
pub struct PublicSocketCapture<V> {
    pub kind: PublicSocketKind,
    pub class: V,
    pub receiver: V,
    pub body: PublicSocketArrow,
}
pub trait PublicConstructorHost: WsPingHost {
    fn number(&self, value: f64) -> Self::Value;
    fn construct(
        &mut self,
        constructor: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    /// Native super(parent): observe the defining class's current superclass,
    /// original newTarget and any replacement object from the base constructor.
    fn public_super(
        &mut self,
        class: &Self::Value,
        parent: &Self::Value,
        new_target: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// A fresh lexical arrow bound to the named mapped body, capturing receiver
    /// and defining class. Full native class dispatch is a separate composition
    /// root; this constructor does not implement those body bindings.
    fn public_arrow(
        &mut self,
        capture: PublicSocketCapture<Self::Value>,
        name: &str,
        length: u8,
    ) -> Result<Self::Value, Self::Error>;
}
/// Source HyperliquidPublicWebSocket.constructor, including instance fields.
pub fn new_core_public_socket<H: PublicConstructorHost>(
    h: &mut H,
    class: &H::Value,
    parent: &H::Value,
    new_target: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let native = h.public_super(class, parent, new_target.unwrap_or(class))?;
    let value = h.object()?;
    h.define_data(&native, "topics", &value)?;
    let value = h.object()?;
    h.define_data(&native, "messageHandlers", &value)?;
    let value = {
        let constructor = h.global("Set")?;
        h.construct(&constructor, &[])?
    };
    h.define_data(&native, "activeSubscriptions", &value)?;
    let value = h.undefined();
    h.define_data(&native, "hlPingTimeoutId", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::ConnectAndSubscribe,
        },
        "connectAndSubscribe",
        0,
    )?;
    h.define_data(&native, "connectAndSubscribe", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::OnOpen,
        },
        "onOpen",
        0,
    )?;
    h.define_data(&native, "onOpen", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::StartPingLoop,
        },
        "startPingLoop",
        0,
    )?;
    h.define_data(&native, "startPingLoop", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SchedulePing,
        },
        "schedulePing",
        0,
    )?;
    h.define_data(&native, "schedulePing", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::StopPingLoop,
        },
        "stopPingLoop",
        0,
    )?;
    h.define_data(&native, "stopPingLoop", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SubscribePublicTopics,
        },
        "subscribePublicTopics",
        0,
    )?;
    h.define_data(&native, "subscribePublicTopics", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SendSubscription,
        },
        "sendSubscription",
        1,
    )?;
    h.define_data(&native, "sendSubscription", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SendUnsubscription,
        },
        "sendUnsubscription",
        1,
    )?;
    h.define_data(&native, "sendUnsubscription", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SubscribeUserTopics,
        },
        "subscribeUserTopics",
        0,
    )?;
    h.define_data(&native, "subscribeUserTopics", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::Ping,
        },
        "ping",
        0,
    )?;
    h.define_data(&native, "ping", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::OnMessage,
        },
        "onMessage",
        1,
    )?;
    h.define_data(&native, "onMessage", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SendWithDiagnostics,
        },
        "sendWithDiagnostics",
        1,
    )?;
    h.define_data(&native, "sendWithDiagnostics", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::EmitReceiveDiagnostics,
        },
        "emitReceiveDiagnostics",
        2,
    )?;
    h.define_data(&native, "emitReceiveDiagnostics", &value)?;
    let value = {
        let value = h.object()?;
        let zero = h.number(0.0);
        h.define_data(&value, "last", &zero)?;
        let zero = h.number(0.0);
        h.define_data(&value, "midPrice", &zero)?;
        value
    };
    h.define_data(&native, "_midsUpdateBuf", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::HandleAllMids,
        },
        "handleAllMids",
        1,
    )?;
    h.define_data(&native, "handleAllMids", &value)?;
    let value = {
        let value = h.object()?;
        let zero = h.number(0.0);
        h.define_data(&value, "mark", &zero)?;
        value
    };
    h.define_data(&native, "_fastMarkUpdateBuf", &value)?;
    let value = {
        let value = h.object()?;
        let zero = h.number(0.0);
        h.define_data(&value, "last", &zero)?;
        let zero = h.number(0.0);
        h.define_data(&value, "midPrice", &zero)?;
        value
    };
    h.define_data(&native, "_fastMidUpdateBuf", &value)?;
    let value = {
        let value = h.object()?;
        let zero = h.number(0.0);
        h.define_data(&value, "mark", &zero)?;
        let zero = h.number(0.0);
        h.define_data(&value, "last", &zero)?;
        let zero = h.number(0.0);
        h.define_data(&value, "midPrice", &zero)?;
        value
    };
    h.define_data(&native, "_fastMarkAndMidUpdateBuf", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::HandleFastAssetCtxs,
        },
        "handleFastAssetCtxs",
        1,
    )?;
    h.define_data(&native, "handleFastAssetCtxs", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::ListenOHLCV,
        },
        "listenOHLCV",
        2,
    )?;
    h.define_data(&native, "listenOHLCV", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Core,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::ListenOrderBook,
        },
        "listenOrderBook",
        2,
    )?;
    h.define_data(&native, "listenOrderBook", &value)?;
    Ok(native)
}
/// Source HyperliquidSpotPublicWebSocket.constructor, including instance fields.
pub fn new_spot_public_socket<H: PublicConstructorHost>(
    h: &mut H,
    class: &H::Value,
    parent: &H::Value,
    new_target: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let native = h.public_super(class, parent, new_target.unwrap_or(class))?;
    let value = h.object()?;
    h.define_data(&native, "topics", &value)?;
    let value = h.object()?;
    h.define_data(&native, "messageHandlers", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::ConnectAndSubscribe,
        },
        "connectAndSubscribe",
        0,
    )?;
    h.define_data(&native, "connectAndSubscribe", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::OnOpen,
        },
        "onOpen",
        0,
    )?;
    h.define_data(&native, "onOpen", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SubscribePublicTopics,
        },
        "subscribePublicTopics",
        0,
    )?;
    h.define_data(&native, "subscribePublicTopics", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::SubscribeUserTopics,
        },
        "subscribeUserTopics",
        0,
    )?;
    h.define_data(&native, "subscribeUserTopics", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::Ping,
        },
        "ping",
        0,
    )?;
    h.define_data(&native, "ping", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::OnMessage,
        },
        "onMessage",
        1,
    )?;
    h.define_data(&native, "onMessage", &value)?;
    let value = {
        let value = h.object()?;
        let zero = h.number(0.0);
        h.define_data(&value, "last", &zero)?;
        let zero = h.number(0.0);
        h.define_data(&value, "midPrice", &zero)?;
        value
    };
    h.define_data(&native, "_midsUpdateBuf", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::HandleAllMids,
        },
        "handleAllMids",
        1,
    )?;
    h.define_data(&native, "handleAllMids", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::ListenOHLCV,
        },
        "listenOHLCV",
        2,
    )?;
    h.define_data(&native, "listenOHLCV", &value)?;
    let value = h.public_arrow(
        PublicSocketCapture {
            kind: PublicSocketKind::Spot,
            class: class.clone(),
            receiver: native.clone(),
            body: PublicSocketArrow::ListenOrderBook,
        },
        "listenOrderBook",
        2,
    )?;
    h.define_data(&native, "listenOrderBook", &value)?;
    Ok(native)
}
