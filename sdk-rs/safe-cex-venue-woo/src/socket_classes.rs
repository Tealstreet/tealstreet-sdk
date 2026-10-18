use crate::{legacy_lifecycle, legacy_socket_control as control, public_socket, public_streams};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::Store;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

pub type Initializer<V, E> = Rc<dyn Fn(V) -> Result<(), E>>;
pub struct SocketClassOps<V, E, H> {
    pub stream: Rc<public_streams::StreamOps<V, E>>,
    /// Implicit derived class: forward original arguments/newTarget, resolve live super,
    /// then initialize the returned receiver with ordinary class-field definitions.
    pub derive: Rc<dyn Fn(&str, V, usize, Initializer<V, E>) -> Result<V, E>>,
    /// Pure acquisition of a fresh adapter over the same authoritative receiver state.
    pub frame: Rc<dyn Fn(&V) -> H>,
    /// Pure acquisition of the existing shared store; no source property reads.
    pub store: Rc<dyn Fn(&V) -> Rc<dyn Store>>,
    /// Existing immutable JSON ingress; caller-owned mutable objects are outside this adapter.
    /// Never walk or coerce V to manufacture this view.
    pub json: Rc<dyn Fn(&V) -> Result<Rc<Value>, E>>,
    pub ticker_data: Rc<dyn Fn(&V) -> Result<safe_cex_types::retained_json::RetainedJsonInput, E>>,
    pub exchange_error: Rc<dyn Fn(ExchangeError) -> E>,
}
#[derive(Clone, Copy)]
enum PublicMember {
    Connect,
    Open,
    Subscribe,
    Message,
    Ping,
    PingEvent,
    Tickers,
    Bbo,
    Marks,
    Candles,
    Book,
    UpdateBook,
}
fn invoke<V: Clone + 'static, E: 'static, H: control::Host<Value = V, Error = E>>(
    h: &Rc<SocketClassOps<V, E, H>>,
    receiver: &V,
    member: PublicMember,
    args: Vec<V>,
) -> Result<V, E> {
    let value = &h.stream.value;
    let arg = |index| {
        args.get(index)
            .cloned()
            .unwrap_or_else(|| (value.undefined)())
    };
    match member {
        PublicMember::Connect => control::public_connect_and_subscribe(&mut (h.frame)(receiver))?,
        PublicMember::Open => control::public_on_open(&mut (h.frame)(receiver))?,
        PublicMember::Subscribe => control::public_subscribe(&mut (h.frame)(receiver))?,
        PublicMember::Message => {
            let data = (value.get)(&arg(0), "data")?;
            control::public_on_message(&mut (h.frame)(receiver), data)?;
        }
        PublicMember::Ping => return ping_public(&h.stream, receiver),
        PublicMember::PingEvent => return handle_ping_event(&h.stream, receiver),
        PublicMember::Tickers | PublicMember::Bbo | PublicMember::Marks => {
            let data = (h.ticker_data)(&arg(0))?;
            let store = (h.store)(receiver);
            match member {
                PublicMember::Tickers => {
                    public_socket::handle_tickers_stream_events(store.as_ref(), &data)
                }
                PublicMember::Bbo => public_socket::handle_bbo_stream_events(store.as_ref(), &data),
                PublicMember::Marks => {
                    public_socket::handle_mark_prices_stream_events(store.as_ref(), &data)
                }
                _ => unreachable!(),
            }
            .map_err(|e| (h.exchange_error)(e))?;
        }
        PublicMember::Candles => {
            return public_streams::listen_ohlcv(
                h.stream.clone(),
                receiver.clone(),
                arg(0),
                arg(1),
            );
        }
        PublicMember::Book => {
            return public_streams::listen_order_book(
                h.stream.clone(),
                receiver.clone(),
                arg(0),
                arg(1),
            );
        }
        PublicMember::UpdateBook => {
            return legacy_lifecycle::process_order_book_update(
                &h.stream.lifecycle,
                arg(0),
                arg(1),
            );
        }
    }
    Ok((value.undefined)())
}
fn initialize_public<V: Clone + 'static, E: 'static, H: control::Host<Value = V, Error = E>>(
    h: Rc<SocketClassOps<V, E, H>>,
    receiver: V,
) -> Result<(), E> {
    let value = &h.stream.value;
    (h.stream.define)(receiver.clone(), "topics", (value.object)(vec![]))?;
    let mut handlers = Vec::new();
    for (key, method, data) in [
        ("ping", "handlePingEvent", false),
        ("pong", "handlePongEvent", false),
        ("tickers", "handleTickersStreamEvents", true),
        ("bbos", "handleBBOStreamEvents", true),
        ("markprices", "handleMarkPricesStreamEvents", true),
    ] {
        let ops = h.clone();
        let instance = receiver.clone();
        let callback = (value.function)(
            "",
            usize::from(data),
            Rc::new(move |args| {
                let argument = if data {
                    let input = args
                        .first()
                        .cloned()
                        .unwrap_or_else(|| (ops.stream.value.undefined)());
                    Some((ops.stream.value.get)(&input, "data")?)
                } else {
                    None
                };
                let callee = (ops.stream.value.get)(&instance, method)?;
                (ops.stream.value.invoke)(callee, instance.clone(), argument.into_iter().collect())
            }),
        );
        handlers.push((key, (value.array)(vec![callback])));
    }
    (h.stream.define)(
        receiver.clone(),
        "messageHandlers",
        (value.object)(handlers),
    )?;
    for (name, arity, member) in [
        ("connectAndSubscribe", 0, PublicMember::Connect),
        ("onOpen", 0, PublicMember::Open),
        ("subscribe", 0, PublicMember::Subscribe),
        ("onMessage", 1, PublicMember::Message),
        ("ping", 0, PublicMember::Ping),
        ("handlePingEvent", 0, PublicMember::PingEvent),
        ("handleTickersStreamEvents", 1, PublicMember::Tickers),
        ("handleBBOStreamEvents", 1, PublicMember::Bbo),
        ("handleMarkPricesStreamEvents", 1, PublicMember::Marks),
        ("listenOHLCV", 2, PublicMember::Candles),
        ("listenOrderBook", 2, PublicMember::Book),
        ("processOrderBookUpdate", 2, PublicMember::UpdateBook),
    ] {
        let ops = h.clone();
        let instance = receiver.clone();
        let function = (value.function)(
            name,
            arity,
            Rc::new(move |args| invoke(&ops, &instance, member, args)),
        );
        (h.stream.define)(receiver.clone(), name, function)?;
    }
    Ok(())
}
pub fn define_public_class<
    V: Clone + 'static,
    E: 'static,
    H: control::Host<Value = V, Error = E>,
>(
    h: Rc<SocketClassOps<V, E, H>>,
) -> Result<V, E> {
    let superclass = (h.stream.import)("BaseWebSocket")?;
    let ops = h.clone();
    (h.derive)(
        "WooPublicWebsocket",
        superclass,
        0,
        Rc::new(move |receiver| initialize_public(ops.clone(), receiver)),
    )
}

pub struct PrivateClassOps<V, E, H, F> {
    pub common: Rc<SocketClassOps<V, E, H>>,
    /// Pure acquisition of an independent frame sharing the receiver's existing state.
    pub recovery: Rc<dyn Fn(&V) -> F>,
    pub coercion_error: Rc<dyn Fn(safe_cex_num::JsCoercionError) -> E>,
    /// Creates an AsyncFunction arrow and eagerly retains its task without an extra adoption promise.
    pub async_arrow:
        Rc<dyn Fn(&str, usize, Rc<dyn Fn(Vec<V>) -> crate::legacy_metadata::Task<V, E>>) -> V>,
}
#[derive(Clone, Copy)]
enum PrivateMember {
    Timestamp,
    Connect,
    Subscribe,
    Ping,
    Message,
    Execution,
    Algo,
}
fn invoke_private<
    V: Clone + 'static,
    E: 'static,
    H: control::Host<Value = V, Error = E>,
    F: crate::legacy_fill_recovery::Host<Value = V, Error = E>,
>(
    h: &Rc<PrivateClassOps<V, E, H, F>>,
    receiver: &V,
    member: PrivateMember,
    args: Vec<V>,
) -> Result<V, E> {
    let common = &h.common;
    let value = &common.stream.value;
    let arg = |index| {
        args.get(index)
            .cloned()
            .unwrap_or_else(|| (value.undefined)())
    };
    match member {
        PrivateMember::Timestamp => {
            let argument = arg(0);
            let raw = if (common.stream.is_undefined)(&argument) {
                None
            } else {
                Some((common.json)(&argument)?)
            };
            let timestamp = crate::private_socket::parse_timestamp_to_seconds(raw.as_deref())
                .map_err(|e| (h.coercion_error)(e))?;
            return Ok(timestamp.map_or_else(|| (value.undefined)(), |n| (value.number)(n)));
        }
        PrivateMember::Connect => {
            control::private_connect_and_subscribe(&mut (common.frame)(receiver))?
        }
        PrivateMember::Subscribe => control::private_subscribe(&mut (common.frame)(receiver))?,
        PrivateMember::Ping => return ping_private(&common.stream, receiver),
        PrivateMember::Message => {
            let data = (value.get)(&arg(0), "data")?;
            control::private_on_message(&mut (common.frame)(receiver), data)?;
        }
        PrivateMember::Execution => crate::legacy_fill_recovery::handle_execution_report(
            &mut (h.recovery)(receiver),
            arg(0),
        )?,
        PrivateMember::Algo => crate::legacy_fill_recovery::handle_algo_execution_report(
            &mut (h.recovery)(receiver),
            arg(0),
        )?,
    }
    Ok((value.undefined)())
}
fn initialize_private<
    V: Clone + 'static,
    E: 'static,
    H: control::Host<Value = V, Error = E>,
    F: crate::legacy_fill_recovery::Host<Value = V, Error = E>,
>(
    h: Rc<PrivateClassOps<V, E, H, F>>,
    receiver: V,
) -> Result<(), E> {
    for (name, arity, member) in [
        ("parseTimestampToSeconds", 1, Some(PrivateMember::Timestamp)),
        ("connectAndSubscribe", 0, Some(PrivateMember::Connect)),
        ("onOpen", 0, None),
        ("subscribe", 0, Some(PrivateMember::Subscribe)),
        ("ping", 0, Some(PrivateMember::Ping)),
        ("onMessage", 1, Some(PrivateMember::Message)),
        ("handleExecutionReport", 1, Some(PrivateMember::Execution)),
        ("handleAlgoExecutionReport", 1, Some(PrivateMember::Algo)),
    ] {
        let ops = h.clone();
        let instance = receiver.clone();
        let function = if let Some(member) = member {
            (h.common.stream.value.function)(
                name,
                arity,
                Rc::new(move |args| invoke_private(&ops, &instance, member, args)),
            )
        } else {
            (h.async_arrow)(
                name,
                arity,
                Rc::new(move |_| {
                    let ops = ops.common.clone();
                    let instance = instance.clone();
                    Box::pin(async move {
                        let frame = (ops.frame)(&instance);
                        control::private_on_open(Rc::new(RefCell::new(frame))).await?;
                        Ok((ops.stream.value.undefined)())
                    })
                }),
            )
        };
        (h.common.stream.define)(receiver.clone(), name, function)?;
    }
    Ok(())
}
pub fn define_private_class<
    V: Clone + 'static,
    E: 'static,
    H: control::Host<Value = V, Error = E>,
    F: crate::legacy_fill_recovery::Host<Value = V, Error = E>,
>(
    h: Rc<PrivateClassOps<V, E, H, F>>,
) -> Result<V, E> {
    let superclass = (h.common.stream.import)("BaseWebSocket")?;
    let ops = h.clone();
    (h.common.derive)(
        "WooPrivateWebscoket",
        superclass,
        0,
        Rc::new(move |receiver| initialize_private(ops.clone(), receiver)),
    )
}

fn heartbeat<V: Clone, E>(
    h: &public_streams::StreamOps<V, E>,
    receiver: &V,
    ping: bool,
    logged: bool,
) -> Result<V, E> {
    let value = &h.value;
    if ping {
        if (value.truthy)(&(value.get)(receiver, "isDisposed")?) {
            return Ok((value.undefined)());
        }
        let performance = (value.global)("performance")?;
        let now = (value.get)(&performance, "now")?;
        let timestamp = (value.invoke)(now, performance, vec![])?;
        (h.set)(receiver.clone(), (value.string)("pingAt"), timestamp)?;
    }
    let send = (value.get)(receiver, if logged { "sendWithLog" } else { "send" })?;
    let json = (value.global)("JSON")?;
    let stringify = (value.get)(&json, "stringify")?;
    let payload = (value.object)(vec![(
        "event",
        (value.string)(if ping { "ping" } else { "pong" }),
    )]);
    let payload = (value.invoke)(stringify, json, vec![payload])?;
    (value.invoke)(send, receiver.clone(), vec![payload])?;
    Ok((value.undefined)())
}
pub fn ping_public<V: Clone, E>(h: &public_streams::StreamOps<V, E>, receiver: &V) -> Result<V, E> {
    heartbeat(h, receiver, true, false)
}
pub fn ping_private<V: Clone, E>(
    h: &public_streams::StreamOps<V, E>,
    receiver: &V,
) -> Result<V, E> {
    heartbeat(h, receiver, true, true)
}
pub fn handle_ping_event<V: Clone, E>(
    h: &public_streams::StreamOps<V, E>,
    receiver: &V,
) -> Result<V, E> {
    heartbeat(h, receiver, false, false)
}
