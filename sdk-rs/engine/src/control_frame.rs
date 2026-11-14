//! Explicit venue declarations for socket control traffic.

/// A subscription, authentication or heartbeat frame, declared by the venue
/// helper that encodes it. The engine does not inspect protocol method strings.
/// Order and setting mutations must use [`crate::WebSocketWrite`] and its live
/// mutation Context instead. There is no generic text conversion or constructor.
///
/// Raw order text cannot enter either control-send API:
///
/// ```compile_fail,E0308
/// use safe_cex_engine::{Cx, SocketId};
/// fn unsigned<P>(cx: &mut Cx<'_, P>) {
///     cx.send(SocketId(1), r#"{"op":"order"}"#);
/// }
/// ```
///
/// ```compile_fail,E0308
/// use safe_cex_engine::{Cx, SignStep, SocketId};
/// fn signed<P>(cx: &mut Cx<'_, P>, recipe: Box<dyn SignStep>) {
///     cx.send_signed(SocketId(1), String::from(r#"{"op":"order"}"#), recipe);
/// }
/// ```
///
/// A typed write cannot enter the signed control path either:
///
/// ```compile_fail,E0308
/// use safe_cex_engine::{Cx, SignStep, SocketId, WebSocketWrite};
/// fn signed<P>(cx: &mut Cx<'_, P>, write: WebSocketWrite, recipe: Box<dyn SignStep>) {
///     cx.send_signed(SocketId(1), write, recipe);
/// }
/// ```
///
/// Control helpers cannot consume a typed mutation:
///
/// ```compile_fail,E0277
/// use safe_cex_engine::{ControlFrame, WebSocketWrite};
/// fn subscription(write: WebSocketWrite) -> ControlFrame {
///     ControlFrame::subscription(write)
/// }
/// ```
pub struct ControlFrame(String);

impl ControlFrame {
    /// Declare a subscription, unsubscription or subscription snapshot request.
    /// The venue helper must encode only feed control, never a mutation.
    pub fn subscription(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Declare session authentication (including a signed subscription).
    /// Signing recipes must preserve that purpose when producing final bytes.
    pub fn authentication(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Declare a ping, pong or protocol liveness probe.
    pub fn heartbeat(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub(crate) fn into_text(self) -> String {
        self.0
    }
}
