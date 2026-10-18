//! WOOXExchange members ported in `crate::legacy_market_reads`, over js_runtime values.
use crate::{
    binding::{
        promise::{JsFuture, await_js},
        rt::{Rt, Shared},
        value::{Js, Thrown, arg},
    },
    legacy_market_reads::{self as reads, Call, Host, Transport},
    legacy_metadata::{JsString, Task},
};
use safe_cex_types::PositionMode;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// `v?.k`
pub(super) fn opt(v: &Js, k: &str) -> Result<Js, Thrown> {
    if v.is_nullish() {
        Ok(Js::Undefined)
    } else {
        v.get(k)
    }
}

/// `f.call(this, ...args)` with the TypeError a non-callable callee throws.
pub(super) fn invoke(f: &Js, this: Js, args: Vec<Js>) -> Result<Js, Thrown> {
    if !f.is_callable() {
        return Err(Thrown::type_error(&format!(
            "{} is not a function",
            f.to_display()
        )));
    }
    f.call(this, args)
}

/// `this.<name>(...args)`
pub(super) fn call_member(rt: &Rt, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    let f = rt.this.get(name)?;
    if !f.is_callable() {
        return Err(Thrown::type_error(&format!(
            "this.{name} is not a function"
        )));
    }
    f.call(rt.this.clone(), args)
}

pub(super) fn awaited(rt: &Rt, value: Js) -> Task<Js, Thrown> {
    Box::pin(await_js(&rt.exchange_runtime(), value))
}

/// An async member's body: a synchronous throw becomes a rejected promise.
pub(super) fn spawn(rt: &Rt, body: Result<JsFuture<'static>, Thrown>) -> Js {
    rt.spawn(body.unwrap_or_else(|e| Box::pin(std::future::ready(Err(e)))))
}

pub(super) fn mode_js(mode: PositionMode) -> Js {
    Js::str(match mode {
        PositionMode::OneWay => "oneway",
        PositionMode::Hedged => "hedged",
    })
}

pub(crate) struct Frame {
    pub(crate) rt: Rt,
}

impl Frame {
    fn shared(rt: &Rt) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self { rt: rt.clone() }))
    }
    fn store(&self, key: &str) -> Result<Js, Thrown> {
        self.rt.this.get("store")?.get(key)
    }
}

fn not_a_function(v: &Js, method: &str) -> Thrown {
    if v.is_nullish() {
        Thrown::type_error(&format!(
            "Cannot read properties of {} (reading '{method}')",
            v.to_display()
        ))
    } else {
        Thrown::type_error(&format!("rows.{method} is not a function"))
    }
}

impl Host for Frame {
    type Value = Js;
    type Error = Thrown;
    type Prepared = Js;
    fn prepare(&mut self, call: Call) -> Result<Js, Thrown> {
        match call {
            Call::AccountInfo => self.rt.this.get("fetchAccountInfo"),
            Call::Balance => self.rt.this.get("fetchBalance"),
            Call::Positions => self.rt.this.get("fetchPositions"),
            Call::TickersRaw => self.rt.this.get("fetchTickersRaw"),
            Call::ParseTickers => self.rt.this.get("parseTickers"),
            Call::EmitError => self.rt.this.get("emitAndLogError"),
            Call::ObjectError => self.rt.global("Error"),
            Call::TypeError => self.rt.global("TypeError"),
        }
    }
    fn invoke(&mut self, call: Js) -> Result<Task<Js, Thrown>, Thrown> {
        Ok(awaited(
            &self.rt,
            invoke(&call, self.rt.this.clone(), vec![])?,
        ))
    }
    fn invoke_parse(&mut self, call: Js, rows: Js) -> Result<Js, Thrown> {
        invoke(&call, self.rt.this.clone(), vec![rows])
    }
    fn get(&mut self, transport: Transport, endpoint: &str) -> Result<Task<Js, Thrown>, Thrown> {
        let client = self.rt.this.get(match transport {
            Transport::Main => "xhr",
            Transport::Primary => "_primaryXhr",
            Transport::Secondary => "_secondaryXhr",
        })?;
        let get = client.get("get")?;
        Ok(awaited(
            &self.rt,
            invoke(&get, client, vec![Js::str(endpoint)])?,
        ))
    }
    fn field(&mut self, value: &Js, key: &str) -> Result<Js, Thrown> {
        value.get(key)
    }
    fn v(&mut self, value: &Js, key: &str) -> Result<Js, Thrown> {
        invoke(
            &self.rt.global("v")?,
            Js::Undefined,
            vec![value.clone(), Js::str(key)],
        )
    }
    fn set_field(&mut self, value: &Js, key: &str, new: Js) -> Result<(), Thrown> {
        value.set(key, new)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn boolean(&self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn string(&self, s: JsString) -> Js {
        Js::utf16(&s)
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn array(&self, items: Vec<Option<Js>>) -> Js {
        Js::holey_array(items)
    }
    fn to_number(&mut self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn text(&mut self, v: &Js) -> Result<JsString, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn equals(&self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn starts_with(&mut self, v: &Js, prefix: &str) -> Result<bool, Thrown> {
        let method = v.get("startsWith")?;
        if !method.is_callable() {
            return Err(Thrown::type_error("r.symbol.startsWith is not a function"));
        }
        Ok(method.call(v.clone(), vec![Js::str(prefix)])?.truthy())
    }
    fn split(&mut self, v: &Js, separator: &str) -> Result<Vec<Js>, Thrown> {
        let method = v.get("split")?;
        if !method.is_callable() {
            return Err(Thrown::type_error("r.symbol.split is not a function"));
        }
        let parts = method.call(v.clone(), vec![Js::str(separator)])?;
        Ok(parts.array_values()?.unwrap_or_default())
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn length(&mut self, v: &Js, method: &str) -> Result<usize, Thrown> {
        if let Js::Array(items) = v {
            return Ok(items.len());
        }
        if method != "length" || v.is_nullish() {
            return Err(not_a_function(v, method));
        }
        let length = v.get("length")?.to_number()?;
        Ok(if length.is_finite() && length > 0.0 {
            length as usize
        } else {
            0
        })
    }
    fn has_index(&mut self, v: &Js, index: usize) -> Result<bool, Thrown> {
        Ok(v.has_index(index))
    }
    fn index(&mut self, v: &Js, index: usize) -> Result<Js, Thrown> {
        v.get(&index.to_string())
    }
    fn normalize_symbol(&mut self, v: Js) -> Result<Js, Thrown> {
        invoke(&self.rt.global("normalizeSymbol")?, Js::Undefined, vec![v])
    }
    fn market(&mut self, symbol: &Js) -> Result<Js, Thrown> {
        self.store("marketsBySymbol")?
            .get_key(&symbol.to_js_string()?)
    }
    fn stored(&mut self, key: &str) -> Result<Js, Thrown> {
        self.store(key)
    }
    fn set_position_mode(&mut self, mode: PositionMode) -> Result<(), Thrown> {
        self.rt.this.set("_accountPositionMode", mode_js(mode))
    }
    fn secondary_key(&mut self) -> Result<Js, Thrown> {
        self.rt.this.get("options")?.get("secondary_key")
    }
    fn sum_by(&mut self, v: &Js, property: &str) -> Result<Js, Thrown> {
        invoke(
            &self.rt.global("sumBy")?,
            Js::Undefined,
            vec![v.clone(), Js::str(property)],
        )
    }
    fn error(&mut self, constructor: Js, message: JsString) -> Thrown {
        match invoke(&constructor, Js::Undefined, vec![Js::utf16(&message)]) {
            Ok(error) => Thrown::new(error),
            Err(thrown) => thrown,
        }
    }
    fn error_field(&mut self, error: &Thrown, path: &[&str]) -> Result<Js, Thrown> {
        let mut value = error.value.clone();
        for key in path {
            value = opt(&value, key)?;
        }
        Ok(value)
    }
    fn emit(&mut self, call: Js, message: Js) -> Result<(), Thrown> {
        invoke(&call, self.rt.this.clone(), vec![message]).map(|_| ())
    }
}

type Body = fn(&Rt, Vec<Js>) -> Result<Js, Thrown>;

fn member(rt: &Rt, name: &'static str, body: Body) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, args| body(&rt, args))
}

fn async_member(
    rt: &Rt,
    name: &'static str,
    body: fn(Rc<RefCell<Frame>>) -> Task<Js, Thrown>,
) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, _| {
        Ok(spawn(&rt, Ok(body(Frame::shared(&rt)))))
    })
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    Some(Ok(match name {
        "getAccount" => async_member(rt, "getAccount", reads::get_account),
        "fetchBalance" => async_member(rt, "fetchBalance", reads::fetch_balance),
        "validateAccount" => async_member(rt, "validateAccount", reads::validate_account),
        "fetchMarkets" => async_member(rt, "fetchMarkets", reads::fetch_markets),
        "fetchTickersRaw" => async_member(rt, "fetchTickersRaw", reads::fetch_tickers_raw),
        "fetchTickers" => async_member(rt, "fetchTickers", reads::fetch_tickers),
        "fetchPositionsAndBalance" => async_member(
            rt,
            "fetchPositionsAndBalance",
            reads::fetch_positions_and_balance,
        ),
        "parseTickers" => member(rt, "parseTickers", |rt, args| {
            let frame = Frame::shared(rt);
            reads::parse_tickers(&mut *frame.borrow_mut(), arg(&args, 0))
        }),
        _ => return None,
    }))
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
