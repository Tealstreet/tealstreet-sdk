//! Core private websocket class allocation, statics, methods and callable assembly.
//! Native class/property/function allocation is a binding; every constructor,
//! arrow and prototype execution path is dispatched to a Rust body here.
use crate::{
    core_account_snapshots::{CoreSnapshotHost, SnapshotPredicate},
    core_position_order_state::{self, PositionCallback, PositionOrderHost},
    cross_cache::CrossCacheHost,
    json_boundaries::JsPayloadHost,
    lifecycle_delegates::{self, LifecycleCallback, LifecycleHost},
    market_data_selectors::{self, MarketDataHost},
    private_constructor::{self, CorePrivateArrow, CorePrivateCapture, PrivateConstructorHost},
    private_message_dispatch::{self, DispatchOperator, PrivateMessageHost},
    private_position_helpers::{self, PrivatePositionHost},
    spot_json::JsString,
    ws_emission::{self, DiagnosticScope, WsEmissionHost},
    ws_ping_lifecycle::{self, PingCallback, WsPingHost},
    ws_transport_leaves::{self, WsTransportHost, WsVenue},
};
use std::future::Future;
/// One realm's values and thrown values are shared by every mapped body family.
pub type PrivateValue<H> = <H as WsPingHost>::Value;
pub type PrivateError<H> = <H as WsPingHost>::Error;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorePrivateMethod {
    GetStaleCheckChannelKey,
    GetMessageTimestamp,
    ExtractCoreBalanceSummary,
    CollectHip3DexesFromFills,
    ResolvePositionMode,
    IsStaleMessage,
    ProcessMessage,
    HandleHip3AllMids,
    HandleServerError,
    ApplyCoreClearinghouseState,
    ApplyOptimisticCorePositionFromFill,
    GetFillPositionSide,
    SafeParseNumber,
    TrackProcessedRealizedPnlFill,
    GetRealizedPnlKey,
    ResolveSymbolFromCoin,
    SetRealizedPnlForPosition,
    IncrementRealizedPnlForPosition,
    ApplyRealizedPnlToCurrentPosition,
    GetRealizedPnlForPosition,
}
#[derive(Clone, Debug)]
pub struct CorePrivateMethodCapture<V> {
    pub class: V,
    pub body: CorePrivateMethod,
}
pub trait CorePrivateClassHost:
    PrivateConstructorHost
    + WsTransportHost
    + WsEmissionHost
    + PrivateMessageHost
    + MarketDataHost
    + PrivatePositionHost
    + PositionOrderHost
    + LifecycleHost
    + JsPayloadHost<Value = PrivateValue<Self>, Error = PrivateError<Self>>
{
    /// Native `class HyperliquidPrivateWebSocket extends base` constructor and
    /// prototype allocation: name/length, [[Prototype]] links, native constructor
    /// property, strict call-without-new rejection and current super/newTarget.
    /// Constructor calls must enter `construct_core_private_class` with this
    /// defining constructor, raw parent argument and the original newTarget.
    fn allocate_private_class(
        &mut self,
        base: &PrivateValue<Self>,
    ) -> Result<(PrivateValue<Self>, PrivateValue<Self>), PrivateError<Self>>;
    /// Fresh nonconstructible strict method, name/length and lexical class only.
    /// Invocation enters `call_core_private_method` with the call-time receiver;
    /// it must not capture the receiver or substitute this.constructor.
    fn private_method(
        &mut self,
        capture: CorePrivateMethodCapture<PrivateValue<Self>>,
        name: &str,
        length: u8,
    ) -> Result<PrivateValue<Self>, PrivateError<Self>>;
    /// Class prototype method descriptor: writable/configurable, nonenumerable.
    fn define_private_method(
        &mut self,
        prototype: &PrivateValue<Self>,
        name: &str,
        value: &PrivateValue<Self>,
    ) -> Result<(), PrivateError<Self>>;
}
fn literal<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
/// Source class expression evaluation. All prototype methods exist before the
/// first static initializer; the four writable static fields follow source order.
pub fn create_core_private_class<H: CorePrivateClassHost>(
    h: &mut H,
    base: &PrivateValue<H>,
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let (class, prototype) = h.allocate_private_class(base)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::GetStaleCheckChannelKey,
        },
        "getStaleCheckChannelKey",
        2,
    )?;
    h.define_private_method(&prototype, "getStaleCheckChannelKey", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::GetMessageTimestamp,
        },
        "getMessageTimestamp",
        1,
    )?;
    h.define_private_method(&prototype, "getMessageTimestamp", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ExtractCoreBalanceSummary,
        },
        "extractCoreBalanceSummary",
        1,
    )?;
    h.define_private_method(&prototype, "extractCoreBalanceSummary", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::CollectHip3DexesFromFills,
        },
        "collectHip3DexesFromFills",
        1,
    )?;
    h.define_private_method(&prototype, "collectHip3DexesFromFills", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ResolvePositionMode,
        },
        "resolvePositionMode",
        1,
    )?;
    h.define_private_method(&prototype, "resolvePositionMode", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::IsStaleMessage,
        },
        "isStaleMessage",
        1,
    )?;
    h.define_private_method(&prototype, "isStaleMessage", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ProcessMessage,
        },
        "processMessage",
        1,
    )?;
    h.define_private_method(&prototype, "processMessage", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::HandleHip3AllMids,
        },
        "handleHip3AllMids",
        1,
    )?;
    h.define_private_method(&prototype, "handleHip3AllMids", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::HandleServerError,
        },
        "handleServerError",
        1,
    )?;
    h.define_private_method(&prototype, "handleServerError", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ApplyCoreClearinghouseState,
        },
        "applyCoreClearinghouseState",
        1,
    )?;
    h.define_private_method(&prototype, "applyCoreClearinghouseState", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ApplyOptimisticCorePositionFromFill,
        },
        "applyOptimisticCorePositionFromFill",
        2,
    )?;
    h.define_private_method(&prototype, "applyOptimisticCorePositionFromFill", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::GetFillPositionSide,
        },
        "getFillPositionSide",
        1,
    )?;
    h.define_private_method(&prototype, "getFillPositionSide", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::SafeParseNumber,
        },
        "safeParseNumber",
        1,
    )?;
    h.define_private_method(&prototype, "safeParseNumber", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::TrackProcessedRealizedPnlFill,
        },
        "trackProcessedRealizedPnlFill",
        1,
    )?;
    h.define_private_method(&prototype, "trackProcessedRealizedPnlFill", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::GetRealizedPnlKey,
        },
        "getRealizedPnlKey",
        2,
    )?;
    h.define_private_method(&prototype, "getRealizedPnlKey", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ResolveSymbolFromCoin,
        },
        "resolveSymbolFromCoin",
        2,
    )?;
    h.define_private_method(&prototype, "resolveSymbolFromCoin", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::SetRealizedPnlForPosition,
        },
        "setRealizedPnlForPosition",
        4,
    )?;
    h.define_private_method(&prototype, "setRealizedPnlForPosition", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::IncrementRealizedPnlForPosition,
        },
        "incrementRealizedPnlForPosition",
        4,
    )?;
    h.define_private_method(&prototype, "incrementRealizedPnlForPosition", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::ApplyRealizedPnlToCurrentPosition,
        },
        "applyRealizedPnlToCurrentPosition",
        3,
    )?;
    h.define_private_method(&prototype, "applyRealizedPnlToCurrentPosition", &function)?;
    let function = h.private_method(
        CorePrivateMethodCapture {
            class: class.clone(),
            body: CorePrivateMethod::GetRealizedPnlForPosition,
        },
        "getRealizedPnlForPosition",
        2,
    )?;
    h.define_private_method(&prototype, "getRealizedPnlForPosition", &function)?;
    let value = PrivateConstructorHost::number(h, 20_000.0);
    WsPingHost::define_data(h, &class, "MAX_TRACKED_REALIZED_PNL_FILL_IDS", &value)?;
    let value = PrivateConstructorHost::number(h, 1e-10);
    WsPingHost::define_data(h, &class, "CORE_FILL_POSITION_EPSILON", &value)?;
    let value = PrivateConstructorHost::number(h, 15_000.0);
    WsPingHost::define_data(h, &class, "PING_INTERVAL_MS", &value)?;
    let set = WsPingHost::global(h, "Set")?;
    let entries = PrivateConstructorHost::array(h)?;
    let value = literal(h, "userFills");
    CoreSnapshotHost::array_define(h, &entries, 0, &value)?;
    let value = literal(h, "userFundings");
    CoreSnapshotHost::array_define(h, &entries, 1, &value)?;
    let value = literal(h, "userNonFundingLedgerUpdates");
    CoreSnapshotHost::array_define(h, &entries, 2, &value)?;
    let value = literal(h, "notification");
    CoreSnapshotHost::array_define(h, &entries, 3, &value)?;
    let value = literal(h, "orderUpdates");
    CoreSnapshotHost::array_define(h, &entries, 4, &value)?;
    let value = literal(h, "userEvents");
    CoreSnapshotHost::array_define(h, &entries, 5, &value)?;
    let value = PrivateConstructorHost::construct(h, &set, &[entries])?;
    WsPingHost::define_data(h, &class, "SKIP_TIMESTAMP_CHECK_CHANNELS", &value)?;
    Ok(class)
}
/// Source derived constructor, including the existing ordered instance fields.
pub fn construct_core_private_class<H: CorePrivateClassHost>(
    h: &mut H,
    class: &PrivateValue<H>,
    parent: &PrivateValue<H>,
    new_target: Option<&PrivateValue<H>>,
) -> Result<PrivateValue<H>, PrivateError<H>> {
    private_constructor::new_core_private_socket(h, class, parent, new_target)
}
/// Borrowed lexical environment. Delegation preserves native realm operations;
/// the defining class lookup is local to this invocation, never a global write.
struct PrivateClassScope<'a, H: WsPingHost> {
    inner: &'a mut H,
    class: PrivateValue<H>,
}
impl<H: CorePrivateClassHost> WsPingHost for PrivateClassScope<'_, H> {
    type Value = PrivateValue<H>;
    type Error = PrivateError<H>;
    fn undefined(&self) -> PrivateValue<H> {
        WsPingHost::undefined(self.inner)
    }
    fn literal(&self, text: &JsString) -> PrivateValue<H> {
        WsPingHost::literal(self.inner, text)
    }
    fn truthy(&self, value: &PrivateValue<H>) -> bool {
        WsPingHost::truthy(self.inner, value)
    }
    fn global(&mut self, name: &str) -> Result<PrivateValue<H>, PrivateError<H>> {
        if name == "HyperliquidPrivateWebSocket" {
            Ok(self.class.clone())
        } else {
            WsPingHost::global(self.inner, name)
        }
    }
    fn get(
        &mut self,
        object: &PrivateValue<H>,
        name: &str,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsPingHost::get(self.inner, object, name)
    }
    fn set(
        &mut self,
        object: &PrivateValue<H>,
        name: &str,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        WsPingHost::set(self.inner, object, name, value)
    }
    fn call(
        &mut self,
        function: &PrivateValue<H>,
        receiver: &PrivateValue<H>,
        args: &[PrivateValue<H>],
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsPingHost::call(self.inner, function, receiver, args)
    }
    fn object(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsPingHost::object(self.inner)
    }
    fn define_data(
        &mut self,
        object: &PrivateValue<H>,
        name: &str,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        WsPingHost::define_data(self.inner, object, name, value)
    }
    fn callback(
        &mut self,
        capture: PingCallback<PrivateValue<H>>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsPingHost::callback(self.inner, capture)
    }
}
impl<H: CorePrivateClassHost> PrivateConstructorHost for PrivateClassScope<'_, H> {
    fn number(&self, value: f64) -> PrivateValue<H> {
        PrivateConstructorHost::number(self.inner, value)
    }
    fn array(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateConstructorHost::array(self.inner)
    }
    fn construct(
        &mut self,
        constructor: &PrivateValue<H>,
        args: &[PrivateValue<H>],
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateConstructorHost::construct(self.inner, constructor, args)
    }
    fn private_super(
        &mut self,
        class: &PrivateValue<H>,
        parent: &PrivateValue<H>,
        new_target: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateConstructorHost::private_super(self.inner, class, parent, new_target)
    }
    fn private_arrow(
        &mut self,
        capture: CorePrivateCapture<PrivateValue<H>>,
        name: &str,
        length: u8,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateConstructorHost::private_arrow(self.inner, capture, name, length)
    }
}
impl<H: CorePrivateClassHost> WsTransportHost for PrivateClassScope<'_, H> {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsTransportHost::urls_binding(self.inner, venue)
    }
    fn boolean(&self, value: bool) -> PrivateValue<H> {
        WsTransportHost::boolean(self.inner, value)
    }
}
impl<H: CorePrivateClassHost> WsEmissionHost for PrivateClassScope<'_, H> {
    fn boolean(&self, value: bool) -> PrivateValue<H> {
        WsEmissionHost::boolean(self.inner, value)
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsEmissionHost::system_types(self.inner, venue)
    }
    fn iterator(&mut self, value: &PrivateValue<H>) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsEmissionHost::iterator(self.inner, value)
    }
    fn next(
        &mut self,
        iterator: &PrivateValue<H>,
    ) -> Result<Option<PrivateValue<H>>, PrivateError<H>> {
        WsEmissionHost::next(self.inner, iterator)
    }
    fn close_after_throw(&mut self, iterator: &PrivateValue<H>) {
        WsEmissionHost::close_after_throw(self.inner, iterator)
    }
    fn triple(&mut self, value: &PrivateValue<H>) -> Result<[PrivateValue<H>; 3], PrivateError<H>> {
        WsEmissionHost::triple(self.inner, value)
    }
    fn string_units<'a>(&self, value: &'a PrivateValue<H>) -> Option<&'a [u16]> {
        WsEmissionHost::string_units(self.inner, value)
    }
    fn spread_into(
        &mut self,
        target: &PrivateValue<H>,
        source: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        WsEmissionHost::spread_into(self.inner, target, source)
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsEmissionHost::formatter(self.inner, scope)
    }
}
impl<H: CorePrivateClassHost> PrivateMessageHost for PrivateClassScope<'_, H> {
    fn typeof_value<'a>(&self, value: &'a PrivateValue<H>) -> &'a str {
        PrivateMessageHost::typeof_value(self.inner, value)
    }
    fn has_property(
        &mut self,
        name: &str,
        object: &PrivateValue<H>,
    ) -> Result<bool, PrivateError<H>> {
        PrivateMessageHost::has_property(self.inner, name, object)
    }
    fn valid_fill_predicate(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::valid_fill_predicate(self.inner)
    }
    fn for_in(&mut self, object: &PrivateValue<H>) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::for_in(self.inner, object)
    }
    fn for_in_next(
        &mut self,
        iterator: &PrivateValue<H>,
    ) -> Result<Option<PrivateValue<H>>, PrivateError<H>> {
        PrivateMessageHost::for_in_next(self.inner, iterator)
    }
    fn construct(
        &mut self,
        function: &PrivateValue<H>,
        args: &[PrivateValue<H>],
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::construct(self.inner, function, args)
    }
    fn regex(&mut self, pattern: &str) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::regex(self.inner, pattern)
    }
    fn number(&self, value: f64) -> PrivateValue<H> {
        PrivateMessageHost::number(self.inner, value)
    }
    fn boolean(&self, value: bool) -> PrivateValue<H> {
        PrivateMessageHost::boolean(self.inner, value)
    }
    fn is_nullish(&self, value: &PrivateValue<H>) -> bool {
        PrivateMessageHost::is_nullish(self.inner, value)
    }
    fn is_null(&self, value: &PrivateValue<H>) -> bool {
        PrivateMessageHost::is_null(self.inner, value)
    }
    fn strict_equal(&self, a: &PrivateValue<H>, b: &PrivateValue<H>) -> bool {
        PrivateMessageHost::strict_equal(self.inner, a, b)
    }
    fn index(
        &mut self,
        object: &PrivateValue<H>,
        key: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::index(self.inner, object, key)
    }
    fn array(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::array(self.inner)
    }
    fn template_string(&mut self, value: &PrivateValue<H>) -> Result<JsString, PrivateError<H>> {
        PrivateMessageHost::template_string(self.inner, value)
    }
    fn operator(
        &mut self,
        op: DispatchOperator,
        a: &PrivateValue<H>,
        b: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivateMessageHost::operator(self.inner, op, a, b)
    }
}
impl<H: CorePrivateClassHost> MarketDataHost for PrivateClassScope<'_, H> {
    fn null(&self) -> PrivateValue<H> {
        MarketDataHost::null(self.inner)
    }
    fn number(&self, value: f64) -> PrivateValue<H> {
        MarketDataHost::number(self.inner, value)
    }
    fn is_nullish(&self, value: &PrivateValue<H>) -> bool {
        MarketDataHost::is_nullish(self.inner, value)
    }
    fn is_string(&self, value: &PrivateValue<H>) -> bool {
        MarketDataHost::is_string(self.inner, value)
    }
    fn strict_equal(&self, left: &PrivateValue<H>, right: &PrivateValue<H>) -> bool {
        MarketDataHost::strict_equal(self.inner, left, right)
    }
    fn index(
        &mut self,
        object: &PrivateValue<H>,
        key: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        MarketDataHost::index(self.inner, object, key)
    }
    fn iterator(&mut self, value: &PrivateValue<H>) -> Result<PrivateValue<H>, PrivateError<H>> {
        MarketDataHost::iterator(self.inner, value)
    }
    fn next(
        &mut self,
        iterator: &PrivateValue<H>,
    ) -> Result<Option<PrivateValue<H>>, PrivateError<H>> {
        MarketDataHost::next(self.inner, iterator)
    }
    fn close_after_throw(&mut self, iterator: &PrivateValue<H>) {
        MarketDataHost::close_after_throw(self.inner, iterator)
    }
    fn construct(
        &mut self,
        function: &PrivateValue<H>,
        args: &[PrivateValue<H>],
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        MarketDataHost::construct(self.inner, function, args)
    }
    fn first_binding(
        &mut self,
        value: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        MarketDataHost::first_binding(self.inner, value)
    }
    fn unit_regex(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        MarketDataHost::unit_regex(self.inner)
    }
    fn template_string(&mut self, value: &PrivateValue<H>) -> Result<JsString, PrivateError<H>> {
        MarketDataHost::template_string(self.inner, value)
    }
    fn subtract(
        &mut self,
        left: &PrivateValue<H>,
        right: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        MarketDataHost::subtract(self.inner, left, right)
    }
}
impl<H: CorePrivateClassHost> JsPayloadHost for PrivateClassScope<'_, H> {
    type Value = PrivateValue<H>;
    type Error = PrivateError<H>;
    fn string_units<'a>(&self, value: &'a PrivateValue<H>) -> Option<&'a [u16]> {
        JsPayloadHost::string_units(self.inner, value)
    }
    fn is_object(&self, value: &PrivateValue<H>) -> bool {
        JsPayloadHost::is_object(self.inner, value)
    }
    fn is_array(&mut self, value: &PrivateValue<H>) -> Result<bool, PrivateError<H>> {
        JsPayloadHost::is_array(self.inner, value)
    }
    fn is_nullish(&self, value: &PrivateValue<H>) -> bool {
        JsPayloadHost::is_nullish(self.inner, value)
    }
    fn undefined(&self) -> PrivateValue<H> {
        JsPayloadHost::undefined(self.inner)
    }
    fn parse_json(&mut self, text: &[u16]) -> Result<PrivateValue<H>, PrivateError<H>> {
        JsPayloadHost::parse_json(self.inner, text)
    }
    fn get(
        &mut self,
        value: &PrivateValue<H>,
        property: &str,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        JsPayloadHost::get(self.inner, value, property)
    }
    fn json_stringify(
        &mut self,
        value: &PrivateValue<H>,
    ) -> Result<Option<JsString>, PrivateError<H>> {
        JsPayloadHost::json_stringify(self.inner, value)
    }
    fn to_string(&mut self, value: &PrivateValue<H>) -> Result<JsString, PrivateError<H>> {
        JsPayloadHost::to_string(self.inner, value)
    }
}
impl<H: CorePrivateClassHost> CrossCacheHost for PrivateClassScope<'_, H> {
    fn null(&self) -> PrivateValue<H> {
        CrossCacheHost::null(self.inner)
    }
    fn is_undefined(&self, value: &PrivateValue<H>) -> bool {
        CrossCacheHost::is_undefined(self.inner, value)
    }
    fn truthy(&self, value: &PrivateValue<H>) -> bool {
        CrossCacheHost::truthy(self.inner, value)
    }
    fn session_storage(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::session_storage(self.inner)
    }
    fn literal(&self, text: &JsString) -> PrivateValue<H> {
        CrossCacheHost::literal(self.inner, text)
    }
    fn object(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::object(self.inner)
    }
    fn set(
        &mut self,
        object: &PrivateValue<H>,
        key: &PrivateValue<H>,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        CrossCacheHost::set(self.inner, object, key, value)
    }
    fn define_data(
        &mut self,
        object: &PrivateValue<H>,
        key: &JsString,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        CrossCacheHost::define_data(self.inner, object, key, value)
    }
    fn call(
        &mut self,
        function: &PrivateValue<H>,
        receiver: &PrivateValue<H>,
        args: &[PrivateValue<H>],
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::call(self.inner, function, receiver, args)
    }
    fn parse_json_value(
        &mut self,
        value: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::parse_json_value(self.inner, value)
    }
    fn property_key(
        &mut self,
        value: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::property_key(self.inner, value)
    }
    fn entries(&mut self, map: &PrivateValue<H>) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::entries(self.inner, map)
    }
    fn iterator_step(
        &mut self,
        iterator: &PrivateValue<H>,
    ) -> Result<Option<PrivateValue<H>>, PrivateError<H>> {
        CrossCacheHost::iterator_step(self.inner, iterator)
    }
    fn entry_pair(
        &mut self,
        entry: &PrivateValue<H>,
    ) -> Result<[PrivateValue<H>; 2], PrivateError<H>> {
        CrossCacheHost::entry_pair(self.inner, entry)
    }
    fn close_after_throw(&mut self, iterator: &PrivateValue<H>) {
        CrossCacheHost::close_after_throw(self.inner, iterator)
    }
    fn spread(&mut self, value: &PrivateValue<H>) -> Result<PrivateValue<H>, PrivateError<H>> {
        CrossCacheHost::spread(self.inner, value)
    }
    fn greater_than_500(&mut self, value: &PrivateValue<H>) -> Result<bool, PrivateError<H>> {
        CrossCacheHost::greater_than_500(self.inner, value)
    }
}
impl<H: CorePrivateClassHost> PrivatePositionHost for PrivateClassScope<'_, H> {
    fn number(&mut self, value: &PrivateValue<H>) -> Result<f64, PrivateError<H>> {
        PrivatePositionHost::number(self.inner, value)
    }
    fn number_value(&self, value: f64) -> PrivateValue<H> {
        PrivatePositionHost::number_value(self.inner, value)
    }
    fn lowercase(&mut self, value: &JsString) -> Result<JsString, PrivateError<H>> {
        PrivatePositionHost::lowercase(self.inner, value)
    }
    fn index(
        &mut self,
        object: &PrivateValue<H>,
        key: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivatePositionHost::index(self.inner, object, key)
    }
    fn add(
        &mut self,
        left: &PrivateValue<H>,
        right: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PrivatePositionHost::add(self.inner, left, right)
    }
    fn max_tracked_fill_ids(&mut self) -> Result<PrivateValue<H>, PrivateError<H>> {
        WsPingHost::get(self.inner, &self.class, "MAX_TRACKED_REALIZED_PNL_FILL_IDS")
    }
    fn greater_than(
        &mut self,
        value: &PrivateValue<H>,
        limit: &PrivateValue<H>,
    ) -> Result<bool, PrivateError<H>> {
        PrivatePositionHost::greater_than(self.inner, value, limit)
    }
}
impl<H: CorePrivateClassHost> CoreSnapshotHost for PrivateClassScope<'_, H> {
    fn null_value(&self) -> PrivateValue<H> {
        CoreSnapshotHost::null_value(self.inner)
    }
    fn add_values(
        &mut self,
        a: &PrivateValue<H>,
        b: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        CoreSnapshotHost::add_values(self.inner, a, b)
    }
    fn return_await_value(
        &mut self,
        value: PrivateValue<H>,
    ) -> impl Future<Output = Result<PrivateValue<H>, PrivateError<H>>> {
        CoreSnapshotHost::return_await_value(self.inner, value)
    }
    fn await_value(
        &mut self,
        value: PrivateValue<H>,
    ) -> impl Future<Output = Result<PrivateValue<H>, PrivateError<H>>> {
        CoreSnapshotHost::await_value(self.inner, value)
    }
    fn error_value(&self, error: &PrivateError<H>) -> PrivateValue<H> {
        CoreSnapshotHost::error_value(self.inner, error)
    }
    fn construct_error(
        &mut self,
        constructor: &PrivateValue<H>,
        args: &[PrivateValue<H>],
    ) -> Result<PrivateError<H>, PrivateError<H>> {
        CoreSnapshotHost::construct_error(self.inner, constructor, args)
    }
    fn instance_of(
        &mut self,
        value: &PrivateError<H>,
        constructor: &PrivateValue<H>,
    ) -> Result<bool, PrivateError<H>> {
        CoreSnapshotHost::instance_of(self.inner, value, constructor)
    }
    fn snapshot_predicate(
        &mut self,
        capture: SnapshotPredicate<PrivateValue<H>>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        CoreSnapshotHost::snapshot_predicate(self.inner, capture)
    }
    fn array_define(
        &mut self,
        array: &PrivateValue<H>,
        index: usize,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        CoreSnapshotHost::array_define(self.inner, array, index, value)
    }
    fn spread_into(
        &mut self,
        object: &PrivateValue<H>,
        source: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        CoreSnapshotHost::spread_into(self.inner, object, source)
    }
    fn iterator(&mut self, value: &PrivateValue<H>) -> Result<PrivateValue<H>, PrivateError<H>> {
        CoreSnapshotHost::iterator(self.inner, value)
    }
    fn next(
        &mut self,
        iterator: &PrivateValue<H>,
    ) -> Result<Option<PrivateValue<H>>, PrivateError<H>> {
        CoreSnapshotHost::next(self.inner, iterator)
    }
    fn close_after_throw(&mut self, iterator: &PrivateValue<H>) {
        CoreSnapshotHost::close_after_throw(self.inner, iterator)
    }
    fn close_after_return(&mut self, iterator: &PrivateValue<H>) -> Result<(), PrivateError<H>> {
        CoreSnapshotHost::close_after_return(self.inner, iterator)
    }
    fn pair(&mut self, value: &PrivateValue<H>) -> Result<[PrivateValue<H>; 2], PrivateError<H>> {
        CoreSnapshotHost::pair(self.inner, value)
    }
}
impl<H: CorePrivateClassHost> PositionOrderHost for PrivateClassScope<'_, H> {
    fn position_operator(
        &mut self,
        operator: &str,
        left: &PrivateValue<H>,
        right: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PositionOrderHost::position_operator(self.inner, operator, left, right)
    }
    fn position_unary(
        &mut self,
        operator: &str,
        value: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PositionOrderHost::position_unary(self.inner, operator, value)
    }
    fn position_callback(
        &mut self,
        callback: PositionCallback<PrivateValue<H>>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        PositionOrderHost::position_callback(self.inner, callback)
    }
    fn set_position_index(
        &mut self,
        object: &PrivateValue<H>,
        key: &PrivateValue<H>,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        PositionOrderHost::set_position_index(self.inner, object, key, value)
    }
    fn append_position_spread(
        &mut self,
        array: &PrivateValue<H>,
        value: &PrivateValue<H>,
    ) -> Result<(), PrivateError<H>> {
        PositionOrderHost::append_position_spread(self.inner, array, value)
    }
}
impl<H: CorePrivateClassHost> LifecycleHost for PrivateClassScope<'_, H> {
    fn null(&self) -> PrivateValue<H> {
        LifecycleHost::null(self.inner)
    }
    fn boolean(&self, value: bool) -> PrivateValue<H> {
        LifecycleHost::boolean(self.inner, value)
    }
    fn number(&self, value: f64) -> PrivateValue<H> {
        LifecycleHost::number(self.inner, value)
    }
    fn is_nullish(&self, value: &PrivateValue<H>) -> bool {
        LifecycleHost::is_nullish(self.inner, value)
    }
    fn super_get(
        &mut self,
        receiver: &PrivateValue<H>,
        name: &str,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        LifecycleHost::super_get(self.inner, receiver, name)
    }
    fn lifecycle_callback(
        &mut self,
        capture: LifecycleCallback<PrivateValue<H>>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        LifecycleHost::lifecycle_callback(self.inner, capture)
    }
    fn subtract(
        &mut self,
        left: &PrivateValue<H>,
        right: &PrivateValue<H>,
    ) -> Result<PrivateValue<H>, PrivateError<H>> {
        LifecycleHost::subtract(self.inner, left, right)
    }
    fn greater_than(
        &mut self,
        left: &PrivateValue<H>,
        right: &PrivateValue<H>,
    ) -> Result<bool, PrivateError<H>> {
        LifecycleHost::greater_than(self.inner, left, right)
    }
    fn template_string(&mut self, value: &PrivateValue<H>) -> Result<JsString, PrivateError<H>> {
        LifecycleHost::template_string(self.inner, value)
    }
}
/// Lexical arrows ignore the supplied call receiver; source returns are
/// discarded by void bodies, while the lowered ingestion bodies return native U.
pub fn call_core_private_arrow<H: CorePrivateClassHost>(
    h: &mut H,
    capture: &CorePrivateCapture<PrivateValue<H>>,
    args: &[PrivateValue<H>],
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let receiver = &capture.receiver;
    let mut scope = PrivateClassScope {
        inner: h,
        class: capture.class.clone(),
    };
    let h = &mut scope;
    let u = WsPingHost::undefined(h);
    let arg = |i| args.get(i).cloned().unwrap_or_else(|| u.clone());
    match capture.body {
        CorePrivateArrow::ConnectAndSubscribe => {
            ws_transport_leaves::private_connect_and_subscribe(h, receiver)?;
        }
        CorePrivateArrow::OnOpen => {
            ws_ping_lifecycle::private_on_open(h, receiver)?;
        }
        CorePrivateArrow::StartPingLoop => {
            ws_ping_lifecycle::private_start_ping_loop(h, receiver)?;
        }
        CorePrivateArrow::SchedulePing => {
            ws_ping_lifecycle::private_schedule_ping(h, receiver)?;
        }
        CorePrivateArrow::StopPingLoop => {
            ws_ping_lifecycle::private_stop_ping_loop(h, receiver)?;
        }
        CorePrivateArrow::Subscribe => {
            ws_transport_leaves::private_subscribe(h, receiver)?;
        }
        CorePrivateArrow::Ping => {
            ws_ping_lifecycle::private_ping(h, receiver)?;
        }
        CorePrivateArrow::SubscribeSystemTopics => {
            ws_emission::private_subscribe_system_topics(h, receiver)?;
        }
        CorePrivateArrow::SubscribeAllActiveAssetData => {
            ws_emission::private_subscribe_all_active_asset_data(h, receiver)?;
        }
        CorePrivateArrow::OnMessage => {
            private_message_dispatch::core_on_message(h, receiver, &arg(0))?;
        }
        CorePrivateArrow::SendWithDiagnostics => {
            ws_emission::private_send_with_diagnostics(h, receiver, &arg(0))?;
        }
        CorePrivateArrow::EmitReceiveDiagnostics => {
            ws_emission::private_emit_receive_diagnostics(h, receiver, &arg(0), &arg(1))?;
        }
        CorePrivateArrow::HandleUserEvents => {
            private_message_dispatch::core_handle_user_events(h, receiver, &arg(0))?;
        }
        CorePrivateArrow::Dispose => {
            lifecycle_delegates::private_dispose(h, receiver)?;
        }
        CorePrivateArrow::HandleHip3ClearinghouseState => {
            core_position_order_state::handle_hip3_clearinghouse_state(
                h,
                receiver,
                &arg(0),
                &arg(1),
            )?;
        }
        CorePrivateArrow::HandleFills => {
            core_position_order_state::handle_fills(h, receiver, &arg(0))?;
        }
        CorePrivateArrow::HandleFundings => {
            private_message_dispatch::core_handle_fundings(h, receiver, &arg(0))?;
        }
        CorePrivateArrow::HandleUserNonFundingLedgerUpdates => {
            private_message_dispatch::core_handle_ledger_updates(h, receiver, &arg(0))?;
        }
        CorePrivateArrow::HandleOrderUpdates => {
            core_position_order_state::handle_order_updates(h, receiver, &arg(0))?;
        }
    }
    Ok(u)
}
/// Prototype methods preserve raw arguments and their call-time receiver.
pub fn call_core_private_method<H: CorePrivateClassHost>(
    h: &mut H,
    capture: &CorePrivateMethodCapture<PrivateValue<H>>,
    receiver: &PrivateValue<H>,
    args: &[PrivateValue<H>],
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let mut scope = PrivateClassScope {
        inner: h,
        class: capture.class.clone(),
    };
    let h = &mut scope;
    let u = WsPingHost::undefined(h);
    let arg = |i| args.get(i).cloned().unwrap_or_else(|| u.clone());
    Ok(match capture.body {
        CorePrivateMethod::GetStaleCheckChannelKey => {
            market_data_selectors::get_stale_check_channel_key(h, &arg(0), &arg(1))?
        }
        CorePrivateMethod::CollectHip3DexesFromFills => {
            market_data_selectors::collect_hip3_dexes_from_fills(h, receiver, &arg(0))?
        }
        CorePrivateMethod::ResolvePositionMode => native_resolve_position_mode(h, &arg(0))?,
        CorePrivateMethod::IsStaleMessage => {
            private_message_dispatch::core_is_stale_message(h, receiver, &arg(0))?
        }
        CorePrivateMethod::ProcessMessage => {
            private_message_dispatch::core_process_message(h, receiver, &arg(0))?;
            u
        }
        CorePrivateMethod::HandleHip3AllMids => {
            private_message_dispatch::core_handle_hip3_all_mids(h, receiver, &arg(0))?;
            u
        }
        CorePrivateMethod::HandleServerError => {
            private_message_dispatch::core_handle_server_error(h, receiver, &arg(0))?;
            u
        }
        CorePrivateMethod::ApplyCoreClearinghouseState => {
            core_position_order_state::apply_core_clearinghouse_state(
                h,
                receiver,
                &arg(0),
                &arg(1),
            )?
        }
        CorePrivateMethod::ApplyOptimisticCorePositionFromFill => {
            core_position_order_state::apply_optimistic_core_position_from_fill(
                h,
                receiver,
                &arg(0),
                &arg(1),
            )?
        }
        CorePrivateMethod::TrackProcessedRealizedPnlFill => {
            private_position_helpers::track_processed_realized_pnl_fill(h, receiver, &arg(0))?;
            u
        }
        CorePrivateMethod::SetRealizedPnlForPosition => {
            private_position_helpers::set_realized_pnl_for_position(
                h,
                receiver,
                &arg(0),
                &arg(1),
                &arg(2),
                &arg(3),
            )?;
            u
        }
        CorePrivateMethod::IncrementRealizedPnlForPosition => {
            private_position_helpers::increment_realized_pnl_for_position(
                h,
                receiver,
                &arg(0),
                &arg(1),
                &arg(2),
                &arg(3),
            )?;
            u
        }
        CorePrivateMethod::ApplyRealizedPnlToCurrentPosition => {
            core_position_order_state::apply_realized_pnl_to_current_position(
                h,
                receiver,
                &arg(0),
                &arg(1),
                &arg(2),
            )?
        }
        CorePrivateMethod::GetRealizedPnlForPosition => {
            private_position_helpers::get_realized_pnl_for_position(h, receiver, &arg(0), &arg(1))?
        }
        CorePrivateMethod::GetMessageTimestamp => native_message_timestamp(h, &arg(0))?,
        CorePrivateMethod::ExtractCoreBalanceSummary => native_balance_summary(h, &arg(0))?,
        CorePrivateMethod::SafeParseNumber => native_safe_number(h, &arg(0))?,
        CorePrivateMethod::GetFillPositionSide => {
            let dir = arg(0);
            let mut result = u;
            for (needle, side) in [("long", "Long"), ("short", "Short")] {
                let includes = WsPingHost::get(h, &dir, "includes")?;
                let yes = WsPingHost::call(h, &includes, &dir, &[literal(h, needle)])?;
                if WsPingHost::truthy(h, &yes) {
                    let sides = WsPingHost::global(h, "PositionSide")?;
                    result = WsPingHost::get(h, &sides, side)?;
                    break;
                }
            }
            result
        }
        CorePrivateMethod::GetRealizedPnlKey => {
            let mut key = PrivateMessageHost::template_string(h, &arg(0))?.0;
            key.push(b':' as u16);
            key.extend(PrivateMessageHost::template_string(h, &arg(1))?.0);
            WsPingHost::literal(h, &JsString(key))
        }
        CorePrivateMethod::ResolveSymbolFromCoin => {
            let coin = arg(0);
            let mut market = MarketDataHost::index(h, &arg(1), &coin)?;
            if !WsPingHost::truthy(h, &market) {
                let includes = WsPingHost::get(h, &coin, "includes")?;
                let colon = literal(h, ":");
                let has_colon = WsPingHost::call(h, &includes, &coin, &[colon])?;
                if WsPingHost::truthy(h, &has_colon) {
                    let parent = WsPingHost::get(h, receiver, "parent")?;
                    let map = WsPingHost::get(h, &parent, "marketsLowerCaseMap")?;
                    let get = WsPingHost::get(h, &map, "get")?;
                    let lower = WsPingHost::get(h, &coin, "toLowerCase")?;
                    let lower = WsPingHost::call(h, &lower, &coin, &[])?;
                    market = WsPingHost::call(h, &get, &map, &[lower])?;
                }
            }
            let symbol = if MarketDataHost::is_nullish(h, &market) {
                WsPingHost::undefined(h)
            } else {
                WsPingHost::get(h, &market, "symbol")?
            };
            if WsPingHost::truthy(h, &symbol) {
                symbol
            } else {
                coin
            }
        }
    })
}

fn native_method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
fn native_optional<H: PrivateMessageHost>(
    h: &mut H,
    value: &H::Value,
    name: &str,
) -> Result<H::Value, H::Error> {
    if h.is_nullish(value) {
        Ok(h.undefined())
    } else {
        h.get(value, name)
    }
}
fn native_finite<H: PrivateMessageHost>(h: &mut H, value: &H::Value) -> Result<bool, H::Error> {
    let number = h.global("Number")?;
    let result = native_method(h, &number, "isFinite", std::slice::from_ref(value))?;
    Ok(h.truthy(&result))
}
fn native_number<H: WsPingHost>(h: &mut H, value: &H::Value) -> Result<H::Value, H::Error> {
    let function = h.global("Number")?;
    let receiver = h.undefined();
    h.call(&function, &receiver, std::slice::from_ref(value))
}
fn native_safe_number<H: CorePrivateClassHost>(
    h: &mut PrivateClassScope<'_, H>,
    value: &PrivateValue<H>,
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let parsed = native_number(h, value)?;
    if native_finite(h, &parsed)? {
        Ok(parsed)
    } else {
        Ok(PrivateConstructorHost::number(h, 0.0))
    }
}
fn native_message_timestamp<H: CorePrivateClassHost>(
    h: &mut PrivateClassScope<'_, H>,
    value: &PrivateValue<H>,
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let mut raw = native_optional(h, value, "serverTime")?;
    if PrivateMessageHost::is_nullish(h, &raw) {
        raw = native_optional(h, value, "time")?;
        if PrivateMessageHost::is_nullish(h, &raw) {
            let nested = native_optional(h, value, "clearinghouseState")?;
            raw = native_optional(h, &nested, "time")?;
        }
    }
    let parsed = native_number(h, &raw)?;
    if !native_finite(h, &parsed)? {
        return Ok(MarketDataHost::null(h));
    }
    let zero = PrivateConstructorHost::number(h, 0.0);
    let invalid = PrivateMessageHost::operator(h, DispatchOperator::LessEqual, &parsed, &zero)?;
    if WsPingHost::truthy(h, &invalid) {
        Ok(MarketDataHost::null(h))
    } else {
        Ok(parsed)
    }
}
fn native_balance_summary<H: CorePrivateClassHost>(
    h: &mut PrivateClassScope<'_, H>,
    value: &PrivateValue<H>,
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let margin = native_optional(h, value, "marginSummary")?;
    let field = if WsPingHost::truthy(h, &margin) {
        "marginSummary"
    } else {
        let cross = native_optional(h, value, "crossMarginSummary")?;
        if WsPingHost::truthy(h, &cross) {
            "crossMarginSummary"
        } else {
            return Ok(MarketDataHost::null(h));
        }
    };
    let number = WsPingHost::global(h, "Number")?;
    let summary = native_optional(h, value, field)?;
    let raw = native_optional(h, &summary, "accountValue")?;
    let u = WsPingHost::undefined(h);
    let account = WsPingHost::call(h, &number, &u, &[raw])?;
    let number = WsPingHost::global(h, "Number")?;
    let summary = native_optional(h, value, field)?;
    let raw = native_optional(h, &summary, "totalMarginUsed")?;
    let used = WsPingHost::call(h, &number, &u, &[raw])?;
    if !native_finite(h, &account)? || !native_finite(h, &used)? {
        return Ok(MarketDataHost::null(h));
    }
    let result = WsPingHost::object(h)?;
    WsPingHost::define_data(h, &result, "accountValue", &account)?;
    WsPingHost::define_data(h, &result, "totalMarginUsed", &used)?;
    let field = literal(h, field);
    WsPingHost::define_data(h, &result, "sourceField", &field)?;
    Ok(result)
}
fn native_resolve_position_mode<H: CorePrivateClassHost>(
    h: &mut PrivateClassScope<'_, H>,
    value: &PrivateValue<H>,
) -> Result<PrivateValue<H>, PrivateError<H>> {
    let string = WsPingHost::global(h, "String")?;
    let raw = native_optional(h, value, "type")?;
    let raw = if WsPingHost::truthy(h, &raw) {
        raw
    } else {
        literal(h, "")
    };
    let u = WsPingHost::undefined(h);
    let raw = WsPingHost::call(h, &string, &u, &[raw])?;
    let kind = native_method(h, &raw, "toLowerCase", &[])?;
    let hedged = PrivateMessageHost::strict_equal(h, &kind, &literal(h, "twoway"))
        || PrivateMessageHost::strict_equal(h, &kind, &literal(h, "hedged"));
    let modes = WsPingHost::global(h, "PositionMode")?;
    WsPingHost::get(h, &modes, if hedged { "Hedged" } else { "OneWay" })
}
