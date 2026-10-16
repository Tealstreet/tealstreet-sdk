//! Core public class allocation and callable assembly over the mapped native bodies.
use crate::{
    candle_listeners::{self, CandleCallback, CandleListenerHost},
    core_book_listener::{self, CoreBookCallback, CoreBookHost},
    core_public_message::{self, CorePublicMessageHost},
    lifecycle_delegates::{self, LifecycleCallback, LifecycleHost},
    private_message_dispatch::{self, DispatchOperator, PrivateMessageHost},
    public_constructor::{
        self, PublicConstructorHost, PublicSocketArrow, PublicSocketCapture, PublicSocketKind,
    },
    spot_json::JsString,
    ws_emission::{self, DiagnosticScope, WsEmissionHost},
    ws_ping_lifecycle::{self, PingCallback, WsPingHost},
    ws_transport_leaves::{self, WsTransportHost, WsVenue},
};
pub type PublicValue<H> = <H as WsPingHost>::Value;
pub type PublicError<H> = <H as WsPingHost>::Error;
pub trait CorePublicClassHost:
    PublicConstructorHost
    + WsTransportHost
    + WsEmissionHost
    + PrivateMessageHost
    + CorePublicMessageHost
    + CoreBookHost
    + LifecycleHost
{
    /// Native class/prototype allocation with strict constructor semantics and
    /// ordinary inheritance links. Construction enters construct_core_public_class.
    fn allocate_public_class(
        &mut self,
        base: &PublicValue<Self>,
    ) -> Result<(PublicValue<Self>, PublicValue<Self>), PublicError<Self>>;
    /// Nonconstructible prototype dispose, name=dispose, length=0. Retains its
    /// defining class; invocation enters call_core_public_dispose with dynamic this.
    fn public_dispose_method(
        &mut self,
        class: &PublicValue<Self>,
    ) -> Result<PublicValue<Self>, PublicError<Self>>;
    fn define_public_method(
        &mut self,
        prototype: &PublicValue<Self>,
        name: &str,
        value: &PublicValue<Self>,
    ) -> Result<(), PublicError<Self>>;
    /// Ordinary Get(super home-object prototype, name, receiver), including
    /// replacement of the defining class's prototype chain after allocation.
    fn public_super_get(
        &mut self,
        class: &PublicValue<Self>,
        receiver: &PublicValue<Self>,
        name: &str,
    ) -> Result<PublicValue<Self>, PublicError<Self>>;
}
pub fn create_core_public_class<H: CorePublicClassHost>(
    h: &mut H,
    base: &PublicValue<H>,
) -> Result<PublicValue<H>, PublicError<H>> {
    let (class, prototype) = h.allocate_public_class(base)?;
    let dispose = h.public_dispose_method(&class)?;
    h.define_public_method(&prototype, "dispose", &dispose)?;
    let interval = PublicConstructorHost::number(h, 15000.0);
    h.define_data(&class, "PING_INTERVAL_MS", &interval)?;
    Ok(class)
}
pub fn construct_core_public_class<H: CorePublicClassHost>(
    h: &mut H,
    class: &PublicValue<H>,
    parent: &PublicValue<H>,
    new_target: Option<&PublicValue<H>>,
) -> Result<PublicValue<H>, PublicError<H>> {
    public_constructor::new_core_public_socket(h, class, parent, new_target)
}
struct PublicClassScope<'a, H: WsPingHost> {
    inner: &'a mut H,
    class: PublicValue<H>,
}
impl<H: CorePublicClassHost> WsPingHost for PublicClassScope<'_, H> {
    type Value = PublicValue<H>;
    type Error = PublicError<H>;
    fn undefined(&self) -> PublicValue<H> {
        WsPingHost::undefined(self.inner)
    }
    fn literal(&self, text: &JsString) -> PublicValue<H> {
        WsPingHost::literal(self.inner, text)
    }
    fn truthy(&self, value: &PublicValue<H>) -> bool {
        WsPingHost::truthy(self.inner, value)
    }
    fn global(&mut self, name: &str) -> Result<PublicValue<H>, PublicError<H>> {
        if name == "HyperliquidPublicWebSocket" {
            Ok(self.class.clone())
        } else {
            WsPingHost::global(self.inner, name)
        }
    }
    fn get(
        &mut self,
        object: &PublicValue<H>,
        name: &str,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        WsPingHost::get(self.inner, object, name)
    }
    fn set(
        &mut self,
        object: &PublicValue<H>,
        name: &str,
        value: &PublicValue<H>,
    ) -> Result<(), PublicError<H>> {
        WsPingHost::set(self.inner, object, name, value)
    }
    fn call(
        &mut self,
        function: &PublicValue<H>,
        receiver: &PublicValue<H>,
        args: &[PublicValue<H>],
    ) -> Result<PublicValue<H>, PublicError<H>> {
        WsPingHost::call(self.inner, function, receiver, args)
    }
    fn object(&mut self) -> Result<PublicValue<H>, PublicError<H>> {
        WsPingHost::object(self.inner)
    }
    fn define_data(
        &mut self,
        object: &PublicValue<H>,
        name: &str,
        value: &PublicValue<H>,
    ) -> Result<(), PublicError<H>> {
        WsPingHost::define_data(self.inner, object, name, value)
    }
    fn callback(
        &mut self,
        capture: PingCallback<PublicValue<H>>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        WsPingHost::callback(self.inner, capture)
    }
}
impl<H: CorePublicClassHost> WsTransportHost for PublicClassScope<'_, H> {
    fn urls_binding(&mut self, venue: WsVenue) -> Result<PublicValue<H>, PublicError<H>> {
        WsTransportHost::urls_binding(self.inner, venue)
    }
    fn boolean(&self, value: bool) -> PublicValue<H> {
        WsTransportHost::boolean(self.inner, value)
    }
}
impl<H: CorePublicClassHost> WsEmissionHost for PublicClassScope<'_, H> {
    fn boolean(&self, value: bool) -> PublicValue<H> {
        WsEmissionHost::boolean(self.inner, value)
    }
    fn system_types(&mut self, venue: WsVenue) -> Result<PublicValue<H>, PublicError<H>> {
        WsEmissionHost::system_types(self.inner, venue)
    }
    fn iterator(&mut self, value: &PublicValue<H>) -> Result<PublicValue<H>, PublicError<H>> {
        WsEmissionHost::iterator(self.inner, value)
    }
    fn next(
        &mut self,
        iterator: &PublicValue<H>,
    ) -> Result<Option<PublicValue<H>>, PublicError<H>> {
        WsEmissionHost::next(self.inner, iterator)
    }
    fn close_after_throw(&mut self, iterator: &PublicValue<H>) {
        WsEmissionHost::close_after_throw(self.inner, iterator)
    }
    fn triple(&mut self, value: &PublicValue<H>) -> Result<[PublicValue<H>; 3], PublicError<H>> {
        WsEmissionHost::triple(self.inner, value)
    }
    fn string_units<'a>(&self, value: &'a PublicValue<H>) -> Option<&'a [u16]> {
        WsEmissionHost::string_units(self.inner, value)
    }
    fn spread_into(
        &mut self,
        target: &PublicValue<H>,
        source: &PublicValue<H>,
    ) -> Result<(), PublicError<H>> {
        WsEmissionHost::spread_into(self.inner, target, source)
    }
    fn formatter(&mut self, scope: DiagnosticScope) -> Result<PublicValue<H>, PublicError<H>> {
        WsEmissionHost::formatter(self.inner, scope)
    }
}
impl<H: CorePublicClassHost> PrivateMessageHost for PublicClassScope<'_, H> {
    fn typeof_value<'a>(&self, value: &'a PublicValue<H>) -> &'a str {
        PrivateMessageHost::typeof_value(self.inner, value)
    }
    fn has_property(
        &mut self,
        name: &str,
        object: &PublicValue<H>,
    ) -> Result<bool, PublicError<H>> {
        PrivateMessageHost::has_property(self.inner, name, object)
    }
    fn valid_fill_predicate(&mut self) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::valid_fill_predicate(self.inner)
    }
    fn for_in(&mut self, object: &PublicValue<H>) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::for_in(self.inner, object)
    }
    fn for_in_next(
        &mut self,
        iterator: &PublicValue<H>,
    ) -> Result<Option<PublicValue<H>>, PublicError<H>> {
        PrivateMessageHost::for_in_next(self.inner, iterator)
    }
    fn construct(
        &mut self,
        function: &PublicValue<H>,
        args: &[PublicValue<H>],
    ) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::construct(self.inner, function, args)
    }
    fn regex(&mut self, pattern: &str) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::regex(self.inner, pattern)
    }
    fn number(&self, value: f64) -> PublicValue<H> {
        PrivateMessageHost::number(self.inner, value)
    }
    fn boolean(&self, value: bool) -> PublicValue<H> {
        PrivateMessageHost::boolean(self.inner, value)
    }
    fn is_nullish(&self, value: &PublicValue<H>) -> bool {
        PrivateMessageHost::is_nullish(self.inner, value)
    }
    fn is_null(&self, value: &PublicValue<H>) -> bool {
        PrivateMessageHost::is_null(self.inner, value)
    }
    fn strict_equal(&self, a: &PublicValue<H>, b: &PublicValue<H>) -> bool {
        PrivateMessageHost::strict_equal(self.inner, a, b)
    }
    fn index(
        &mut self,
        object: &PublicValue<H>,
        key: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::index(self.inner, object, key)
    }
    fn array(&mut self) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::array(self.inner)
    }
    fn template_string(&mut self, value: &PublicValue<H>) -> Result<JsString, PublicError<H>> {
        PrivateMessageHost::template_string(self.inner, value)
    }
    fn operator(
        &mut self,
        op: DispatchOperator,
        a: &PublicValue<H>,
        b: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        PrivateMessageHost::operator(self.inner, op, a, b)
    }
}
impl<H: CorePublicClassHost> CorePublicMessageHost for PublicClassScope<'_, H> {
    fn event_data(&mut self, event: &PublicValue<H>) -> Result<PublicValue<H>, PublicError<H>> {
        CorePublicMessageHost::event_data(self.inner, event)
    }
    fn is_nullish(&self, value: &PublicValue<H>) -> bool {
        CorePublicMessageHost::is_nullish(self.inner, value)
    }
    fn is_array(&mut self, value: &PublicValue<H>) -> Result<bool, PublicError<H>> {
        CorePublicMessageHost::is_array(self.inner, value)
    }
    fn equal(&self, left: &PublicValue<H>, right: &PublicValue<H>) -> bool {
        CorePublicMessageHost::equal(self.inner, left, right)
    }
    fn number(&self, value: f64) -> PublicValue<H> {
        CorePublicMessageHost::number(self.inner, value)
    }
    fn less(
        &mut self,
        left: &PublicValue<H>,
        right: &PublicValue<H>,
    ) -> Result<bool, PublicError<H>> {
        CorePublicMessageHost::less(self.inner, left, right)
    }
    fn subtract(
        &mut self,
        left: &PublicValue<H>,
        right: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CorePublicMessageHost::subtract(self.inner, left, right)
    }
    fn divide(
        &mut self,
        left: &PublicValue<H>,
        right: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CorePublicMessageHost::divide(self.inner, left, right)
    }
    fn index(
        &mut self,
        value: &PublicValue<H>,
        key: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CorePublicMessageHost::index(self.inner, value, key)
    }
    fn array(&mut self) -> Result<PublicValue<H>, PublicError<H>> {
        CorePublicMessageHost::array(self.inner)
    }
    fn template_string(&mut self, value: &PublicValue<H>) -> Result<JsString, PublicError<H>> {
        CorePublicMessageHost::template_string(self.inner, value)
    }
    fn json_parse(&mut self, data: &PublicValue<H>) -> Result<PublicValue<H>, PublicError<H>> {
        CorePublicMessageHost::json_parse(self.inner, data)
    }
}
impl<H: CorePublicClassHost> CandleListenerHost for PublicClassScope<'_, H> {
    fn number(&self, value: f64) -> PublicValue<H> {
        CandleListenerHost::number(self.inner, value)
    }
    fn boolean(&self, value: bool) -> PublicValue<H> {
        CandleListenerHost::boolean(self.inner, value)
    }
    fn nullish(&self, value: &PublicValue<H>) -> bool {
        CandleListenerHost::nullish(self.inner, value)
    }
    fn strict_equal(&self, left: &PublicValue<H>, right: &PublicValue<H>) -> bool {
        CandleListenerHost::strict_equal(self.inner, left, right)
    }
    fn binary(
        &mut self,
        operator: &str,
        left: &PublicValue<H>,
        right: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::binary(self.inner, operator, left, right)
    }
    fn decrement(&mut self, value: &PublicValue<H>) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::decrement(self.inner, value)
    }
    fn template_string(&mut self, value: &PublicValue<H>) -> Result<JsString, PublicError<H>> {
        CandleListenerHost::template_string(self.inner, value)
    }
    fn index(
        &mut self,
        object: &PublicValue<H>,
        key: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::index(self.inner, object, key)
    }
    fn assign_index(
        &mut self,
        object: &PublicValue<H>,
        key: &PublicValue<H>,
        value: &PublicValue<H>,
    ) -> Result<(), PublicError<H>> {
        CandleListenerHost::assign_index(self.inner, object, key, value)
    }
    fn array(&mut self, values: &[PublicValue<H>]) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::array(self.inner, values)
    }
    fn spread_append(
        &mut self,
        iterable: &PublicValue<H>,
        last: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::spread_append(self.inner, iterable, last)
    }
    fn spread_array(
        &mut self,
        iterable: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::spread_array(self.inner, iterable)
    }
    fn candle_callback(
        &mut self,
        capture: CandleCallback<PublicValue<H>>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CandleListenerHost::candle_callback(self.inner, capture)
    }
}
impl<H: CorePublicClassHost> CoreBookHost for PublicClassScope<'_, H> {
    fn imported(&mut self, name: &str) -> Result<PublicValue<H>, PublicError<H>> {
        CoreBookHost::imported(self.inner, name)
    }
    fn spread_object(&mut self, source: &PublicValue<H>) -> Result<PublicValue<H>, PublicError<H>> {
        CoreBookHost::spread_object(self.inner, source)
    }
    fn delete_index(
        &mut self,
        object: &PublicValue<H>,
        key: &PublicValue<H>,
    ) -> Result<(), PublicError<H>> {
        CoreBookHost::delete_index(self.inner, object, key)
    }
    fn construct(
        &mut self,
        constructor: &PublicValue<H>,
        args: &[PublicValue<H>],
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CoreBookHost::construct(self.inner, constructor, args)
    }
    fn throw_value(&self, value: PublicValue<H>) -> PublicError<H> {
        CoreBookHost::throw_value(self.inner, value)
    }
    fn book_callback(
        &mut self,
        capture: CoreBookCallback<PublicValue<H>>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        CoreBookHost::book_callback(self.inner, capture)
    }
}
impl<H: CorePublicClassHost> LifecycleHost for PublicClassScope<'_, H> {
    fn null(&self) -> PublicValue<H> {
        LifecycleHost::null(self.inner)
    }
    fn boolean(&self, value: bool) -> PublicValue<H> {
        LifecycleHost::boolean(self.inner, value)
    }
    fn number(&self, value: f64) -> PublicValue<H> {
        LifecycleHost::number(self.inner, value)
    }
    fn is_nullish(&self, value: &PublicValue<H>) -> bool {
        LifecycleHost::is_nullish(self.inner, value)
    }
    fn super_get(
        &mut self,
        receiver: &PublicValue<H>,
        name: &str,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        self.inner.public_super_get(&self.class, receiver, name)
    }
    fn lifecycle_callback(
        &mut self,
        capture: LifecycleCallback<PublicValue<H>>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        LifecycleHost::lifecycle_callback(self.inner, capture)
    }
    fn subtract(
        &mut self,
        left: &PublicValue<H>,
        right: &PublicValue<H>,
    ) -> Result<PublicValue<H>, PublicError<H>> {
        LifecycleHost::subtract(self.inner, left, right)
    }
    fn greater_than(
        &mut self,
        left: &PublicValue<H>,
        right: &PublicValue<H>,
    ) -> Result<bool, PublicError<H>> {
        LifecycleHost::greater_than(self.inner, left, right)
    }
    fn template_string(&mut self, value: &PublicValue<H>) -> Result<JsString, PublicError<H>> {
        LifecycleHost::template_string(self.inner, value)
    }
}

pub fn call_core_public_dispose<H: CorePublicClassHost>(
    h: &mut H,
    class: &PublicValue<H>,
    receiver: &PublicValue<H>,
) -> Result<PublicValue<H>, PublicError<H>> {
    let mut scope = PublicClassScope {
        inner: h,
        class: class.clone(),
    };
    lifecycle_delegates::public_dispose(&mut scope, receiver)?;
    Ok(WsPingHost::undefined(&scope))
}
/// Each arrow uses its lexical receiver, not the caller's receiver.
pub fn call_core_public_arrow<H: CorePublicClassHost>(
    h: &mut H,
    capture: &PublicSocketCapture<PublicValue<H>>,
    args: &[PublicValue<H>],
) -> Result<PublicValue<H>, PublicError<H>> {
    assert_eq!(
        capture.kind,
        PublicSocketKind::Core,
        "core class dispatch requires core capture"
    );
    let receiver = &capture.receiver;
    let mut scope = PublicClassScope {
        inner: h,
        class: capture.class.clone(),
    };
    let h = &mut scope;
    let u = WsPingHost::undefined(h);
    let arg = |index| args.get(index).cloned().unwrap_or_else(|| u.clone());
    match capture.body {
        PublicSocketArrow::ConnectAndSubscribe => {
            ws_transport_leaves::public_connect_and_subscribe(h, receiver)?
        }
        PublicSocketArrow::OnOpen => ws_ping_lifecycle::public_on_open(h, receiver)?,
        PublicSocketArrow::StartPingLoop => ws_ping_lifecycle::public_start_ping_loop(h, receiver)?,
        PublicSocketArrow::SchedulePing => ws_ping_lifecycle::public_schedule_ping(h, receiver)?,
        PublicSocketArrow::StopPingLoop => ws_ping_lifecycle::public_stop_ping_loop(h, receiver)?,
        PublicSocketArrow::SubscribePublicTopics => {
            ws_emission::core_public_subscribe_public_topics(h, receiver)?
        }
        PublicSocketArrow::SendSubscription => {
            return ws_emission::core_public_send_subscription(h, receiver, &arg(0));
        }
        PublicSocketArrow::SendUnsubscription => {
            ws_emission::core_public_send_unsubscription(h, receiver, &arg(0))?
        }
        PublicSocketArrow::SubscribeUserTopics => {
            ws_emission::core_public_subscribe_user_topics(h, receiver)?
        }
        PublicSocketArrow::Ping => ws_ping_lifecycle::public_ping(h, receiver)?,
        PublicSocketArrow::OnMessage => {
            core_public_message::core_public_on_message(h, receiver, &arg(0))?
        }
        PublicSocketArrow::SendWithDiagnostics => {
            ws_emission::public_send_with_diagnostics(h, receiver, &arg(0))?
        }
        PublicSocketArrow::EmitReceiveDiagnostics => {
            ws_emission::public_emit_receive_diagnostics(h, receiver, &arg(0), &arg(1))?
        }
        PublicSocketArrow::HandleAllMids => {
            return private_message_dispatch::public_handle_all_mids(h, receiver, &arg(0));
        }
        PublicSocketArrow::HandleFastAssetCtxs => {
            return private_message_dispatch::public_handle_fast_asset_ctxs(h, receiver, &arg(0));
        }
        PublicSocketArrow::ListenOHLCV => {
            return candle_listeners::core_listen_ohlcv(h, receiver.clone(), arg(0), arg(1));
        }
        PublicSocketArrow::ListenOrderBook => {
            return core_book_listener::core_listen_order_book(h, receiver.clone(), arg(0), arg(1));
        }
    }
    Ok(u)
}
