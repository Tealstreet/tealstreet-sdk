//! WooPublicWebsocket / WooPrivateWebscoket members ported in
//! `crate::legacy_socket_control`, and the two socket class definitions.
use super::{
    data_reads::{Callee, Iter, iterate, next},
    fill_recovery,
};
use crate::{
    binding::{
        classes,
        ops::{self, Runtime},
        promise::{Promise, await_js},
        rt::{Initialize, Rt, Shared},
        value::{Js, Thrown, arg, to_json},
    },
    legacy_metadata::{JsString, Task},
    legacy_socket_control::{self as body, Callee as Member, Channel, Listener},
    socket_classes::{self, PrivateClassOps, SocketClassOps},
};
use safe_cex_exchange::js_runtime::ops as js_ops;
use std::rc::{Rc, Weak};

pub(crate) struct Frame {
    pub(crate) rt: Result<Rt, Thrown>,
}

impl Frame {
    pub(crate) fn over(weak: &Weak<Shared>, receiver: &Js) -> Self {
        Self {
            rt: safe_cex_exchange::js_runtime::scope::scope_of(weak)
                .map(|rt| rt.with_this(receiver.clone())),
        }
    }
    fn rt(&self) -> Result<&Rt, Thrown> {
        self.rt.as_ref().map_err(Clone::clone)
    }
    fn this(&self) -> Result<Js, Thrown> {
        Ok(self.rt()?.this.clone())
    }
    fn own(&self, member: &str) -> Result<Callee, Thrown> {
        Callee::of(self.this()?, member, &format!("this.{member}"))
    }
    fn call_own(&self, member: &str, args: Vec<Js>) -> Result<(), Thrown> {
        self.own(member)?.invoke(args)?;
        Ok(())
    }
    fn global(&self, path: &str, member: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        Callee::of(
            self.rt()?.global(path)?,
            member,
            &format!("{path}.{member}"),
        )?
        .invoke(args)
    }
}

fn text(v: &Js) -> Result<JsString, Thrown> {
    Ok(v.to_js_string()?.to_vec())
}

impl body::Host for Frame {
    type Value = Js;
    type Error = Thrown;
    type Socket = Js;
    type Call = Callee;
    type Signer = Callee;
    type Iterator = Iter;
    type Callback = Js;
    fn prepare(&mut self, callee: Member) -> Result<Callee, Thrown> {
        self.own(match callee {
            Member::CreateSocket => "createWebsocket",
            Member::Send(false) => "send",
            Member::Send(true) => "sendWithLog",
            Member::EmitError => "emitAndLogError",
            Member::Execution(false) => "handleExecutionReport",
            Member::Execution(true) => "handleAlgoExecutionReport",
        })
    }
    fn disposed(&mut self) -> Result<bool, Thrown> {
        Ok(self.this()?.get("isDisposed")?.truthy())
    }
    fn option(&mut self, key: &str) -> Result<Js, Thrown> {
        self.this()?.get("parent")?.get("options")?.get(key)
    }
    fn truthy(&mut self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&mut self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_string(&mut self, v: &Js) -> bool {
        v.is_string()
    }
    fn equals(&mut self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn text(&mut self, v: &Js) -> Result<JsString, Thrown> {
        text(v)
    }
    fn field(&mut self, v: &Js, key: &str) -> Result<Js, Thrown> {
        v.get(key)
    }
    fn string(&self, v: JsString) -> Js {
        Js::utf16(&v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn stringify(&mut self, v: &Js) -> Result<JsString, Thrown> {
        text(&self.global("JSON", "stringify", vec![v.clone()])?)
    }
    // ts: jsonParse — JSON.parse inside try/catch, null on any failure.
    fn parse(&mut self, data: Js) -> Result<Js, Thrown> {
        Ok(self.global("JSON", "parse", vec![data]).unwrap_or(Js::Null))
    }
    fn create_socket(&mut self, call: Callee, url: JsString) -> Result<Js, Thrown> {
        call.invoke(vec![Js::utf16(&url)])
    }
    fn set_socket(&mut self, socket: Js) {
        if let Ok(this) = self.this() {
            // An ordinary data property: [[Set]] cannot fail here.
            let _ = this.set("ws", socket);
        }
    }
    fn socket(&mut self) -> Result<Js, Thrown> {
        self.this()?.get("ws")
    }
    fn add_listener(&mut self, socket: Js, event: &str, listener: Listener) -> Result<(), Thrown> {
        let callback = self.this()?.get(match listener {
            Listener::Open => "onOpen",
            Listener::Message => "onMessage",
            Listener::Close => "onClose",
        })?;
        Callee::of(socket, "addEventListener", "this.ws.addEventListener")?
            .invoke(vec![Js::str(event), callback])?;
        Ok(())
    }
    fn send(&mut self, call: Callee, text: JsString, _: bool) -> Result<(), Thrown> {
        call.invoke(vec![Js::utf16(&text)])?;
        Ok(())
    }
    fn ping(&mut self, _: Channel) -> Result<(), Thrown> {
        self.call_own("ping", vec![])
    }
    fn pong(&mut self) -> Result<(), Thrown> {
        self.call_own("handlePongEvent", vec![])
    }
    fn reset_orderbook(&mut self) -> Result<(), Thrown> {
        self.call_own("resetOrderbook", vec![])
    }
    fn subscribe(&mut self, _: Channel) -> Result<(), Thrown> {
        self.call_own("subscribe", vec![])
    }
    fn reconnecting_enabled(&mut self) -> bool {
        self.rt()
            .is_ok_and(|rt| rt.shared.runtime.enable_reconnecting())
    }
    fn emit_reconnected(&mut self) -> Result<(), Thrown> {
        self.call_own("emitWsReconnected", vec![])
    }
    fn topics(&mut self) -> Result<Js, Thrown> {
        self.this()?.get("topics")
    }
    fn object_values(&mut self, v: Js) -> Result<Vec<Js>, Thrown> {
        if v.is_nullish() {
            return Err(Thrown::type_error(
                "Cannot convert undefined or null to object",
            ));
        }
        Ok(v.own_entries()?.into_iter().map(|(_, v)| v).collect())
    }
    fn flattenable_values(&mut self, v: &Js) -> Option<Vec<Js>> {
        v.array_values()
    }
    fn track_last_received(&mut self) -> Result<(), Thrown> {
        self.call_own("trackLastReceived", vec![])
    }
    fn diagnostic_receive(&mut self, data: &Js) -> Result<(), Thrown> {
        self.call_own("emitWsDiagnosticReceive", vec![data.clone()])
    }
    fn handlers(&mut self, key: &[u16]) -> Result<Js, Thrown> {
        self.this()?.get("messageHandlers")?.get_key(key)
    }
    fn stats(&mut self, key: JsString) -> Result<(), Thrown> {
        self.call_own("recordMessageHandlerStats", vec![Js::utf16(&key)])
    }
    fn iterator(&mut self, handlers: Js) -> Result<Iter, Thrown> {
        iterate(handlers)
    }
    fn next(&mut self, iterator: &mut Iter) -> Result<Option<Js>, Thrown> {
        Ok(next(iterator))
    }
    fn callback(&mut self, callback: Js, json: &Js) -> Result<(), Thrown> {
        callback.call(Js::Undefined, vec![json.clone()])?;
        Ok(())
    }
    fn execution_report(&mut self, call: Callee, data: Js, _: bool) -> Result<(), Thrown> {
        call.invoke(vec![data])?;
        Ok(())
    }
    fn emit_error(&mut self, call: Callee, error: Js) -> Result<(), Thrown> {
        call.invoke(vec![error])?;
        Ok(())
    }
    fn timestamp(&mut self) -> Result<f64, Thrown> {
        self.global("virtualClock", "getCurrentTimeMs", vec![])?
            .to_number()
    }
    fn nanoid(&mut self) -> Result<JsString, Thrown> {
        text(&Callee::global(self.rt()?, "nanoid")?.invoke(vec![])?)
    }
    fn prepare_signer(&mut self, signer: Js) -> Result<Callee, Thrown> {
        Callee::of(signer, "sign", "this.parent.options.remoteSigner.sign")
    }
    fn sign(&mut self, signer: Callee, request: Js) -> Result<Task<Js, Thrown>, Thrown> {
        Ok(Box::pin(await_js(signer.invoke(vec![request])?)))
    }
    fn hmac_sha256_hex(&mut self, secret: Js, to_sign: JsString) -> Result<JsString, Thrown> {
        let hmac =
            Callee::global(self.rt()?, "createHmac")?.invoke(vec![Js::str("sha256"), secret])?;
        let hmac = Callee::of(hmac, "update", "hmac.update")?.invoke(vec![Js::utf16(&to_sign)])?;
        text(&Callee::of(hmac, "digest", "hmac.digest")?.invoke(vec![Js::str("hex")])?)
    }
    // ts: this.parent.fetchAndSetPrivateSnapshot() — the base.ts member, owned by the
    // Rust core; not awaited by the source, so a rejection is reported as unhandled.
    fn private_snapshot(&mut self) -> Result<(), Thrown> {
        let rt = self.rt()?;
        let venue = rt
            .shared
            .sockets
            .venue
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Thrown::error("Error", "exchange is not assembled"))?;
        let reporter = rt.shared.runtime.clone();
        rt.spawn(Box::pin(async move {
            if let Err(e) = venue.fetch_and_set_private_snapshot(1).await {
                reporter.report_exception(
                    &e,
                    &serde_json::json!({ "source": "fetchAndSetPrivateSnapshot" }).into(),
                );
            }
            Ok(Js::Undefined)
        }));
        Ok(())
    }
}

/// `BaseWebSocket.orderBookInnerState` / `resetOrderbook` (base.ws.ts field
/// initializers), which the shared JS socket base does not carry.
fn base_fields(receiver: &Js) -> Result<(), Thrown> {
    receiver.define("orderBookInnerState", Js::object(vec![]))?;
    let owner = receiver.clone();
    receiver.define(
        "resetOrderbook",
        Js::function("resetOrderbook", move |_, _| {
            let state = owner.get("orderBookInnerState")?;
            for hash in state.own_keys() {
                let inner = state.get_key(&hash)?;
                inner.set("updates", Js::array(vec![]))?;
                inner.set("isSnapshotLoaded", Js::Bool(false))?;
            }
            Ok(Js::Undefined)
        }),
    )
}

pub(crate) fn define_socket_classes(
    rt: &Rt,
    weak: Weak<Shared>,
    runtime: Runtime,
) -> Result<(), Thrown> {
    let store = rt.shared.store.clone();
    let frames = weak.clone();
    let common = Rc::new(SocketClassOps {
        stream: ops::stream(weak.clone(), runtime.clone()),
        derive: Rc::new(
            |name: &str, superclass: Js, _arity, initialize: Initialize| {
                let fields: Initialize = Rc::new(move |receiver: Js| {
                    base_fields(&receiver)?;
                    initialize(receiver)
                });
                classes::derive_socket(name, superclass, fields)
            },
        ),
        frame: Rc::new(move |receiver: &Js| Frame::over(&frames, receiver)),
        store: Rc::new(move |_: &Js| store.clone()),
        json: Rc::new(|v: &Js| to_json(v).map(Rc::new)),
        ticker_data: Rc::new(safe_cex_exchange::js_runtime::value::to_retained_input),
        exchange_error: Rc::new(Thrown::from),
    });
    let public = socket_classes::define_public_class(common.clone())?;
    rt.shared.globals.define("WooPublicWebsocket", public)?;
    let recovery = weak;
    let private = socket_classes::define_private_class(Rc::new(PrivateClassOps {
        common,
        recovery: Rc::new(move |receiver: &Js| fill_recovery::Frame::over(&recovery, receiver)),
        coercion_error: Rc::new(Thrown::from),
        async_arrow: Rc::new(move |name: &str, _arity, task| {
            let runtime = runtime.clone();
            Js::function(name, move |_, args| {
                Ok(Js::Promise(Promise::spawn(&runtime, task(args))))
            })
        }),
    }))?;
    rt.shared.globals.define("WooPrivateWebscoket", private)
}

// ts: updateOrderBookByPrice (utils/updateOrderBook.ts) — strict price equality.
fn update_order_book_by_price(a: Vec<Js>) -> Result<Js, Thrown> {
    let (book, side, price, amount) = (arg(&a, 0), arg(&a, 1), arg(&a, 2), arg(&a, 3));
    let upsert = arg(&a, 4);
    let upsert = upsert.is_undefined() || upsert.truthy();
    let levels = book.get_key(&side.to_js_string()?)?;
    let descending = side.strict_equals(&Js::str("bids"));
    let (mut left, mut right) = (0i64, super::data_reads::length(&levels)? as i64 - 1);
    let mut found = None;
    while left <= right {
        let mid = (left + right) / 2;
        let mid_price = levels.get(&mid.to_string())?.get("price")?;
        if mid_price.strict_equals(&price) {
            found = Some(mid);
            break;
        }
        let after = if descending {
            js_ops::greater_than(&mid_price, &price)?
        } else {
            js_ops::less_than(&mid_price, &price)?
        };
        if after {
            left = mid + 1;
        } else {
            right = mid - 1;
        }
    }
    let zero = amount.strict_equals(&Js::Number(0.0));
    let Js::Array(cells) = &levels else {
        return Err(Thrown::type_error("levels.splice is not a function"));
    };
    Ok(Js::Number(match found {
        None if !upsert || zero => -1.0,
        None => {
            let level = Js::object(vec![
                ("price", price),
                ("amount", amount),
                ("total", Js::Number(0.0)),
            ]);
            cells.borrow_mut().insert(left as usize, Some(level));
            left as f64
        }
        Some(i) if zero => {
            cells.borrow_mut().remove(i as usize);
            -1.0
        }
        Some(i) => {
            levels.get(&i.to_string())?.set("amount", amount)?;
            i as f64
        }
    }))
}

pub(crate) fn register(g: &Js, weak: Weak<Shared>) {
    let _ = g.define(
        "updateOrderBookByPrice",
        Js::function("updateOrderBookByPrice", |_, a| {
            update_order_book_by_price(a)
        }),
    );
    if let Ok(console) = g.get("console") {
        let _ = console.define(
            "warn",
            Js::function("warn", move |_, a| {
                let message = a.iter().map(Js::to_display).collect::<Vec<_>>().join(" ");
                safe_cex_exchange::js_runtime::scope::scope_of(&weak)?
                    .shared
                    .runtime
                    .console("warn", &message);
                Ok(Js::Undefined)
            }),
        );
    }
}
