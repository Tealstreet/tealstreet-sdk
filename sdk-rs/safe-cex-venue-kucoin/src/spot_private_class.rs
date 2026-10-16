use crate::{order_lifecycle, public_streams::StreamOps};
use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};
pub type Task<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
pub type Initializer<V, E> = Rc<dyn Fn(V) -> Result<(), E>>;
pub struct PrivateClassOps<V, E> {
    pub value: Rc<StreamOps<V, E>>,
    /// Pure fresh view over the original receiver; no reads, snapshots or failure.
    pub frame: Rc<dyn Fn(&V) -> Rc<StreamOps<V, E>>>,
    /// Pure acquisition of the same live receiver and existing module bindings.
    pub orders: Rc<dyn Fn(&V) -> Rc<order_lifecycle::OrderLifecycleOps<V, E>>>,
    /// Standard implicit derived constructor, forwarding original arguments/newTarget.
    pub derive: Rc<dyn Fn(&str, V, Initializer<V, E>) -> Result<V, E>>,
    pub define: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub arrow: Rc<dyn Fn(&str, usize, Rc<dyn Fn(Vec<V>) -> Result<V, E>>) -> V>,
    /// AsyncFunction arrow: eager execution and retained completion on promise jobs.
    pub async_arrow: Rc<dyn Fn(&str, usize, Rc<dyn Fn(Vec<V>) -> Task<V, E>>) -> V>,
    pub method: Rc<dyn Fn(&str, usize, Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>) -> V>,
    pub define_prototype: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    /// Standard super-property Reference, retaining home object and actual receiver.
    pub super_member: Rc<dyn Fn(&V, &V, &str) -> Result<V, E>>,
    pub template: Rc<dyn Fn(&V) -> Result<Vec<u16>, E>>,
    pub string_value: Rc<dyn Fn(Vec<u16>) -> V>,
    pub boolean: Rc<dyn Fn(bool) -> V>,
    pub null: Rc<dyn Fn() -> V>,
    pub error: Rc<dyn Fn(&str) -> E>,
}
pub(crate) fn call<V, E>(
    h: &StreamOps<V, E>,
    name: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let callee = (h.capture)(name)?;
    (h.invoke)(callee, args()?)
}
pub(crate) fn optional<V, E>(h: &StreamOps<V, E>, value: &V, key: &str) -> Result<V, E> {
    if (h.nullish)(value) {
        Ok((h.undefined)())
    } else {
        (h.get)(value, key)
    }
}
pub(crate) fn fallback<V, E>(
    h: &StreamOps<V, E>,
    value: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if (h.truthy)(&value) {
        Ok(value)
    } else {
        next()
    }
}
async fn connect<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    crate::socket_setup::connect_spot_private(
        || Ok((h.truthy)(&(h.get)(&receiver, "isDisposed")?)),
        |key| {
            let response = call(&h, "this.parent.xhr.post", || {
                let table = (h.capture)("KUCOIN_SPOT_ENDPOINTS")?;
                Ok(vec![(h.get)(&table, key)?])
            })?;
            Ok((h.await_value)(response))
        },
        |value, key| match key {
            "instanceServers?.0" => {
                let servers = (h.get)(value, "instanceServers")?;
                optional(&h, &servers, "0")
            }
            "?.endpoint" => optional(&h, value, "endpoint"),
            _ => (h.get)(value, key),
        },
        |code| (h.equal)(code, &(h.literal)("200000")),
        |value| (h.truthy)(value),
        |key| (h.get)(&receiver, key),
        |effect, arg, text| {
            match effect {
                "set:token" | "set:pingInterval" | "set:socketEndpoint" | "set:ws" => {
                    let key = effect.strip_prefix("set:").unwrap();
                    (h.set)(&receiver, key, arg.unwrap().clone())?;
                }
                "emit-token-error" => {
                    call(&h, "this.parent.emitAndLogError", || {
                        Ok(vec![fallback(&h, (h.get)(arg.unwrap(), "msg")?, || {
                            Ok((h.literal)(text.unwrap()))
                        })?])
                    })?;
                }
                "emit-error" => {
                    call(&h, "this.parent.emitAndLogError", || {
                        Ok(vec![(h.literal)(text.unwrap())])
                    })?;
                }
                "create-current" => {
                    return call(&h, "this.createWebsocket", || {
                        Ok(vec![(h.get)(&receiver, "socketEndpoint")?])
                    });
                }
                "add-listener" => {
                    let event = text.unwrap();
                    let field = match event {
                        "open" => "onOpen",
                        "message" => "onMessage",
                        "close" => "onClose",
                        _ => return Err((ops.error)("unknown websocket event")),
                    };
                    call(&h, "this.ws.addEventListener", || {
                        Ok(vec![(h.literal)(event), (h.get)(&receiver, field)?])
                    })?;
                }
                _ => return Err((ops.error)("unknown socket connection effect")),
            }
            Ok((h.undefined)())
        },
        |value| (ops.template)(value),
        || call(&h, "Date.now", || Ok(vec![])),
        |units| (ops.string_value)(units),
        |error, message| {
            call(&h, "this.retryFailedConnect", || {
                let value = (h.error_value)(error);
                let response = optional(&h, &value, "response")?;
                let data = optional(&h, &response, "data")?;
                let cause = fallback(&h, optional(&h, &data, "msg")?, || {
                    fallback(&h, optional(&h, &value, "message")?, || {
                        Ok((h.literal)(message))
                    })
                })?;
                Ok(vec![cause])
            })?;
            Ok(())
        },
    )
    .await?;
    Ok((h.undefined)())
}
fn on_open<V: Clone, E>(ops: &PrivateClassOps<V, E>, receiver: &V) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    if (h.truthy)(&(h.get)(receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    call(&h, "this.subscribe", || Ok(vec![]))?;
    call(&h, "this.ping", || Ok(vec![]))?;
    if (h.truthy)(&(h.capture)("ENABLE_RECONNECTING_WEBSOCKET")?) {
        call(&h, "this.parent.fetchAndSetPrivateSnapshot", || {
            Ok(vec![(h.number)(10.)])
        })?;
    }
    Ok((h.undefined)())
}
pub(crate) fn ping<V: Clone, E>(ops: &PrivateClassOps<V, E>, receiver: &V) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    if (h.truthy)(&(h.get)(receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    let now = call(&h, "performance.now", || Ok(vec![]))?;
    (h.set)(receiver, "pingAt", now)?;
    call(&h, "this.sendWithLog", || {
        Ok(vec![call(&h, "JSON.stringify", || {
            Ok(vec![(h.object)(vec![
                ("id", call(&h, "Date.now", || Ok(vec![]))?),
                ("type", (h.literal)("ping")),
            ])])
        })?])
    })?;
    Ok((h.undefined)())
}
fn on_message<V: Clone + 'static, E: 'static>(
    ops: &PrivateClassOps<V, E>,
    receiver: &V,
    input: V,
) -> Result<V, E> {
    let h = (ops.frame)(receiver);
    let data = (h.get)(&input, "data")?;
    let disposed = Cell::new((h.truthy)(&(h.get)(receiver, "isDisposed")?));
    crate::message_routing::spot_private_message(
        &disposed,
        |effect, json: Option<&Rc<V>>, text| {
            match effect {
                "track" => {
                    call(&h, "this.trackLastReceived", || Ok(vec![]))?;
                }
                "diagnostic" => {
                    call(&h, "this.emitWsDiagnosticReceive", || {
                        Ok(vec![data.clone()])
                    })?;
                }
                "pong-interval" => {
                    call(&h, "this.handlePongEvent", || {
                        Ok(vec![(h.get)(receiver, "pingInterval")?])
                    })?;
                }
                "stats" => {
                    call(&h, "this.recordMessageHandlerStats", || {
                        Ok(vec![(ops.string_value)(text.unwrap().to_vec())])
                    })?;
                }
                "order" => {
                    call(&h, "this.handleOrderTopic", || {
                        Ok(vec![(**json.unwrap()).clone()])
                    })?;
                }
                "refresh-balance" => {
                    call(&h, "this.refreshBalanceSoon", || Ok(vec![]))?;
                }
                _ => return Err((ops.error)("unknown private routing effect")),
            }
            Ok(())
        },
        || {
            let value = call(&h, "jsonParse", || Ok(vec![data.clone()]))?;
            Ok((h.truthy)(&value).then_some(Rc::new(value)))
        },
        |json, selector| {
            if selector == "type:pong" {
                return Ok((h.equal)(&(h.get)(json, "type")?, &(h.literal)("pong")));
            }
            let topic = (h.get)(json, "topic")?;
            if (h.nullish)(&topic) {
                return Ok(false);
            }
            let prefix = selector.strip_prefix("optional:").unwrap();
            let result = (h.intrinsic)(
                "call-method",
                vec![topic, (h.literal)("startsWith"), (h.literal)(prefix)],
            )?;
            Ok((h.truthy)(&result))
        },
    )?;
    Ok((h.undefined)())
}
fn subscribe<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    let topics = (h.array)(vec![
        (h.literal)("/spotMarket/tradeOrdersV2"),
        (h.literal)("/account/balance"),
    ]);
    let send = h.clone();
    let owner = ops.clone();
    (h.for_of)(
        topics,
        Rc::new(move |topic| {
            call(&send, "this.sendWithLog", || {
                Ok(vec![call(&send, "JSON.stringify", || {
                    Ok(vec![(send.object)(vec![
                        ("id", call(&send, "Date.now", || Ok(vec![]))?),
                        ("type", (send.literal)("subscribe")),
                        ("topic", topic),
                        ("privateChannel", (owner.boolean)(true)),
                        ("response", (owner.boolean)(true)),
                    ])])
                })?])
            })?;
            Ok(())
        }),
    )?;
    Ok((h.undefined)())
}
fn refresh<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
    balance: bool,
) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    let field = if balance {
        "balanceRefreshTimeout"
    } else {
        "orderRefreshTimeout"
    };
    if (h.truthy)(&(h.get)(&receiver, field)?) || (h.truthy)(&(h.get)(&receiver, "isDisposed")?) {
        return Ok((h.undefined)());
    }
    let callee = (h.capture)("setTimeout")?;
    let owner = ops.clone();
    let instance = receiver.clone();
    let callback = (ops.async_arrow)(
        "",
        0,
        Rc::new(move |_| {
            let ops = owner.clone();
            let receiver = instance.clone();
            Box::pin(async move {
                let h = (ops.frame)(&receiver);
                (h.set)(&receiver, field, (h.undefined)())?;
                if (h.truthy)(&(h.get)(&receiver, "isDisposed")?) {
                    return Ok((h.undefined)());
                }
                let task = call(
                    &h,
                    if balance {
                        "this.parent.fetchBalance"
                    } else {
                        "this.parent.fetchAndSetOrders"
                    },
                    || Ok(vec![]),
                )?;
                let result = (h.await_value)(task).await?;
                if balance {
                    if (h.truthy)(&(h.get)(&receiver, "isDisposed")?) {
                        return Ok((h.undefined)());
                    }
                    call(&h, "this.store.update", || {
                        let loaded = (h.capture)("this.store.loaded")?;
                        let copy = (h.object)(vec![]);
                        (h.intrinsic)("spread-into", vec![copy.clone(), loaded])?;
                        (ops.define)(&copy, "balance", (ops.boolean)(true))?;
                        Ok(vec![(h.object)(vec![
                            ("balance", result),
                            ("loaded", copy),
                        ])])
                    })?;
                }
                Ok((h.undefined)())
            })
        }),
    );
    let handle = (h.invoke)(callee, vec![callback, (h.number)(250.)])?;
    (h.set)(&receiver, field, handle)?;
    Ok((h.undefined)())
}
fn dispose<V: Clone, E>(ops: &PrivateClassOps<V, E>, class: &V, receiver: V) -> Result<V, E> {
    let h = (ops.frame)(&receiver);
    for key in ["orderRefreshTimeout", "balanceRefreshTimeout"] {
        if (h.truthy)(&(h.get)(&receiver, key)?) {
            call(&h, "clearTimeout", || Ok(vec![(h.get)(&receiver, key)?]))?;
            (h.set)(&receiver, key, (h.undefined)())?;
        }
    }
    let member = (ops.super_member)(class, &receiver, "dispose")?;
    (h.invoke)(member, vec![])?;
    Ok((h.undefined)())
}
fn initialize<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
    receiver: V,
) -> Result<(), E> {
    for key in ["token", "socketEndpoint"] {
        (ops.define)(&receiver, key, (ops.null)())?;
    }
    (ops.define)(&receiver, "pingInterval", (ops.value.number)(5000.))?;
    for key in ["orderRefreshTimeout", "balanceRefreshTimeout"] {
        (ops.define)(&receiver, key, (ops.value.undefined)())?;
    }
    let owner = ops.clone();
    let instance = receiver.clone();
    let connect = (ops.async_arrow)(
        "connectAndSubscribe",
        0,
        Rc::new(move |_| Box::pin(connect(owner.clone(), instance.clone()))),
    );
    (ops.define)(&receiver, "connectAndSubscribe", connect)?;
    for (name, arity) in [
        ("onOpen", 0),
        ("ping", 0),
        ("onMessage", 1),
        ("handleOrderTopic", 1),
        ("subscribe", 0),
        ("refreshOrdersSoon", 0),
        ("refreshBalanceSoon", 0),
    ] {
        let owner = ops.clone();
        let instance = receiver.clone();
        let body = (ops.arrow)(
            name,
            arity,
            Rc::new(move |args| {
                let argument = || {
                    args.first()
                        .cloned()
                        .unwrap_or_else(|| (owner.value.undefined)())
                };
                match name {
                    "onOpen" => on_open(&owner, &instance),
                    "ping" => ping(&owner, &instance),
                    "onMessage" => on_message(&owner, &instance, argument()),
                    "handleOrderTopic" => order_lifecycle::handle_spot_order_topic(
                        &(owner.orders)(&instance),
                        argument(),
                    ),
                    "subscribe" => subscribe(owner.clone(), instance.clone()),
                    "refreshOrdersSoon" => refresh(owner.clone(), instance.clone(), false),
                    "refreshBalanceSoon" => refresh(owner.clone(), instance.clone(), true),
                    _ => Err((owner.error)("unknown spot private native member")),
                }
            }),
        );
        (ops.define)(&receiver, name, body)?;
    }
    Ok(())
}
pub fn define_spot_private_class<V: Clone + 'static, E: 'static>(
    ops: Rc<PrivateClassOps<V, E>>,
) -> Result<V, E> {
    let base = (ops.value.capture)("BaseWebSocket")?;
    let owner = ops.clone();
    let class = (ops.derive)(
        "KucoinSpotPrivateWebsocket",
        base,
        Rc::new(move |r| initialize(owner.clone(), r)),
    )?;
    let owner = ops.clone();
    let home = class.clone();
    let dispose = (ops.method)(
        "dispose",
        0,
        Rc::new(move |receiver, _| dispose(&owner, &home, receiver)),
    );
    (ops.define_prototype)(&class, "dispose", dispose)?;
    Ok(class)
}
