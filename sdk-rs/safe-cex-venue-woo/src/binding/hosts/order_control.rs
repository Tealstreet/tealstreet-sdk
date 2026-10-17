//! WOOXExchange members ported in `crate::legacy_order_control`, over js_runtime values.
use crate::binding::{
    promise::await_js,
    rt::{Rt, Shared},
    value::{Js, Thrown, arg},
};
use crate::legacy_metadata::Task;
use crate::legacy_order_control::{self as body, Call, Host, Operation, Options};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// A callee resolved before its arguments are evaluated, as `this.a.b(...)` is.
#[derive(Clone)]
pub(crate) enum Callee {
    Function {
        owner: Js,
        function: Js,
        label: &'static str,
    },
    /// `inverseObj(TABLE)[key]`: the table is built first, then indexed.
    Table(Js),
    /// A module-scope `const` helper of woo.exchange.ts.
    Native(fn(&Rt, Vec<Js>) -> Result<Js, Thrown>),
}

impl Callee {
    pub(crate) fn call(&self, rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
        match self {
            Callee::Function {
                owner,
                function,
                label,
            } => {
                if !function.is_callable() {
                    return Err(Thrown::type_error(&format!("{label} is not a function")));
                }
                function.call(owner.clone(), args)
            }
            Callee::Table(table) => table.get_key(&arg(&args, 0).to_js_string()?),
            Callee::Native(f) => f(rt, args),
        }
    }
}

/// `owner.name` read now; called later with `owner` as receiver.
pub(crate) fn member(owner: Js, name: &str, label: &'static str) -> Result<Callee, Thrown> {
    let function = owner.get(name)?;
    Ok(Callee::Function {
        owner,
        function,
        label,
    })
}

/// A module-scope import (`reverseSymbol`, `adjust`, ...), called without a receiver.
pub(crate) fn module(rt: &Rt, name: &str, label: &'static str) -> Result<Callee, Thrown> {
    Ok(Callee::Function {
        owner: Js::Undefined,
        function: rt.global(name)?,
        label,
    })
}

pub(crate) fn xhr(rt: &Rt, method: &str, label: &'static str) -> Result<Callee, Thrown> {
    member(rt.this.get("xhr")?, method, label)
}

/// `value?.a?.b`
pub(crate) fn optional_path(value: &Js, path: &[&str]) -> Result<Js, Thrown> {
    let mut out = value.clone();
    for key in path {
        if out.is_nullish() {
            return Ok(Js::Undefined);
        }
        out = out.get(key)?;
    }
    Ok(out)
}

/// `this.emitAndLogError(err, message)` / `this.emitAndLogError(message)`.
pub(crate) fn emit_with(
    rt: &Rt,
    callee: &Callee,
    original: Option<&Thrown>,
    message: Js,
) -> Result<(), Thrown> {
    let args = match original {
        Some(e) => vec![e.value.clone(), message],
        None => vec![message],
    };
    callee.call(rt, args).map(|_| ())
}

/// `array.length` read as ToLength.
pub(crate) fn length(value: &Js) -> Result<usize, Thrown> {
    let n = value.get("length")?.to_number()?;
    Ok(if n.is_nan() || n <= 0.0 {
        0
    } else {
        n.floor() as usize
    })
}

/// The iteration protocol for `...value` over the values this source spreads.
pub(crate) fn iterate(value: &Js) -> Result<Vec<Js>, Thrown> {
    match value {
        Js::Array(_) => Ok(value.array_values().unwrap_or_default()),
        Js::String(s) => Ok(char::decode_utf16(s.iter().copied())
            .map(|c| match c {
                Ok(c) => Js::str(&c.to_string()),
                Err(e) => Js::utf16(&[e.unpaired_surrogate()]),
            })
            .collect()),
        other => Err(Thrown::type_error(&format!(
            "{} is not iterable",
            other.to_display()
        ))),
    }
}

pub(crate) fn settle(value: Js) -> Task<Js, Thrown> {
    Box::pin(await_js(value))
}

pub(crate) struct Frame {
    pub(crate) rt: Rt,
}

impl Host for Frame {
    type Value = Js;
    type Error = Thrown;
    type Prepared = Callee;
    fn prepare(&mut self, op: Operation) -> Result<Callee, Thrown> {
        let rt = &self.rt;
        let this = rt.this.clone();
        match op {
            Operation::Delete => xhr(rt, "delete", "this.xhr.delete"),
            Operation::MapLimit => member(this, "mapLimitOrder", "this.mapLimitOrder"),
            Operation::MapAlgo => member(this, "mapAlgoOrder", "this.mapAlgoOrder"),
            Operation::IsAlgo => member(this, "isAlgoOrder", "this.isAlgoOrder"),
            Operation::Place => member(
                this,
                "placePositionalAlgoOrder",
                "this.placePositionalAlgoOrder",
            ),
            Operation::Emit => member(this, "emitAndLogError", "this.emitAndLogError"),
            Operation::ParseInt => module(rt, "parseInt", "parseInt"),
            Operation::ReverseSymbol => module(rt, "reverseSymbol", "reverseSymbol"),
            Operation::UpdateCanceled => member(
                this.get("store")?,
                "updateOrderStatus",
                "this.store.updateOrderStatus",
            ),
        }
    }
    fn await_element(&mut self, v: Js) -> Task<Js, Thrown> {
        settle(v)
    }
    fn call(&mut self, call: Call) -> Result<Task<Js, Thrown>, Thrown> {
        let name = match call {
            Call::LimitOrders => "fetchLimitOrders",
            Call::AlgoOrders => "fetchAlgoOrders",
            Call::PositionMode => "getPositionMode",
        };
        Ok(settle(self.rt.call_path(name, vec![])?))
    }
    fn recursive_orders(&mut self, endpoint: &str) -> Result<Task<Js, Thrown>, Thrown> {
        Ok(settle(self.rt.call_path(
            "fetchOrdersRecursively",
            vec![Js::str(endpoint)],
        )?))
    }
    fn delete(
        &mut self,
        call: Callee,
        endpoint: Js,
        options: Options,
        body: Js,
    ) -> Result<Task<(), Thrown>, Thrown> {
        let args = match options {
            Options::None => vec![endpoint],
            Options::Params => vec![endpoint, Js::object(vec![("params", body)])],
            Options::Data => vec![endpoint, Js::object(vec![("data", body)])],
        };
        let pending = settle(call.call(&self.rt, args)?);
        Ok(Box::pin(async move { pending.await.map(|_| ()) }))
    }
    fn field(&mut self, v: &Js, key: &str) -> Result<Js, Thrown> {
        v.get(key)
    }
    fn index(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn length(&mut self, v: &Js) -> Result<usize, Thrown> {
        length(v)
    }
    fn has_index(&mut self, v: &Js, i: usize) -> Result<bool, Thrown> {
        Ok(v.has_own(&i.to_string()))
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn is_null(&self, v: &Js) -> bool {
        matches!(v, Js::Null)
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    /// Only ever applied to a fresh data-property literal, whose reads cannot throw.
    fn omit_undefined(&self, v: Js) -> Js {
        let out = Js::object(vec![]);
        for (key, value) in v.own_entries().unwrap_or_default() {
            if !value.is_undefined() {
                let _ = out.define_key(&key, value);
            }
        }
        out
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn same(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn same_zero(&self, a: &Js, b: &Js) -> bool {
        a.same_value_zero(b)
    }
    fn equals(&self, v: &Js, s: &str) -> bool {
        v.strict_equals(&Js::str(s))
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn string(&self, s: &str) -> Js {
        Js::str(s)
    }
    fn utf16(&self, s: Vec<u16>) -> Js {
        Js::utf16(&s)
    }
    fn stringify(&mut self, v: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn spread(&mut self, v: &Js) -> Result<Vec<Js>, Thrown> {
        iterate(v)
    }
    fn parse_int(&mut self, call: Callee, v: &Js) -> Result<Js, Thrown> {
        call.call(&self.rt, vec![v.clone(), Js::Number(10.0)])
    }
    fn reverse_symbol(&mut self, call: Callee, v: Js) -> Result<Js, Thrown> {
        call.call(&self.rt, vec![v])
    }
    fn map_limit(&mut self, call: Callee, v: Js) -> Result<Js, Thrown> {
        call.call(&self.rt, vec![v])
    }
    fn map_algo(&mut self, call: Callee, v: Js) -> Result<Js, Thrown> {
        call.call(&self.rt, vec![v])
    }
    fn is_algo(&mut self, call: Callee, v: Js) -> Result<bool, Thrown> {
        Ok(call.call(&self.rt, vec![v])?.truthy())
    }
    fn stored_orders(&mut self) -> Result<Js, Thrown> {
        self.rt.path("store.orders")
    }
    fn place_positional(&mut self, call: Callee, opts: Js) -> Result<Task<(), Thrown>, Thrown> {
        let pending = settle(call.call(&self.rt, vec![opts])?);
        Ok(Box::pin(async move { pending.await.map(|_| ()) }))
    }
    fn update_canceled(&mut self, call: Callee, id: Js) -> Result<(), Thrown> {
        let status = self.rt.global("OrderStatus")?.get("Canceled")?;
        call.call(&self.rt, vec![Js::object(vec![("id", id)]), status])
            .map(|_| ())
    }
    fn type_error(&mut self, message: &str) -> Thrown {
        Thrown::type_error(message)
    }
    fn error_field(&mut self, e: &Thrown, path: &[&str]) -> Result<Js, Thrown> {
        optional_path(&e.value, path)
    }
    /// `message?.includes(text)` on whatever the venue put in `message`.
    fn includes(&mut self, v: &Js, text: &str) -> Result<bool, Thrown> {
        let includes = v.get("includes")?;
        if !includes.is_callable() {
            return Err(Thrown::type_error(
                "err?.response?.data?.message?.includes is not a function",
            ));
        }
        Ok(includes.call(v.clone(), vec![Js::str(text)])?.truthy())
    }
    fn emit(&mut self, call: Callee, original: Option<&Thrown>, message: Js) -> Result<(), Thrown> {
        emit_with(&self.rt, &call, original, message)
    }
}

type Frames = Rc<RefCell<Frame>>;

fn async_member(
    rt: &Rt,
    name: &str,
    run: impl Fn(Frames, Vec<Js>) -> Task<Js, Thrown> + 'static,
) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, args| {
        let frame = Rc::new(RefCell::new(Frame { rt: rt.clone() }));
        Ok(rt.spawn(run(frame, args)))
    })
}

fn unit(task: Task<(), Thrown>) -> Task<Js, Thrown> {
    Box::pin(async move { task.await.map(|()| Js::Undefined) })
}

// The public native Order contract keeps numeric/bool fields strict. Validate
// each mapped row before fetchOrders can feed either the trait or Store facade.
// Keep the original Js row so admitted values and property presence are retained.
fn admit_order_rows(rt: &Rt, rows: Js) -> Result<Js, Thrown> {
    let rows = rows
        .array_values()
        .ok_or_else(|| Thrown::type_error("WOO orders must be an array"))?;
    let mut admitted = Vec::with_capacity(rows.len());
    for row in rows {
        let mut refusal = None;
        for field in ["amount", "filled", "reduceOnly"] {
            let value = row.get(field)?;
            let valid = if field == "reduceOnly" {
                value.is_undefined() || matches!(value, Js::Bool(_))
            } else {
                matches!(value, Js::Number(_))
            };
            if !valid {
                refusal = Some(format!(
                    "WOO order {} refused {field}={value:?}: expected {}",
                    row.get("id")?.to_display(),
                    if field == "reduceOnly" {
                        "boolean or undefined"
                    } else {
                        "number"
                    }
                ));
                break;
            }
        }
        if refusal.is_none()
            && let Err(error) = crate::binding::value::to_typed::<safe_cex_types::Order>(&row)
        {
            refusal = Some(format!(
                "WOO order {} refused: {error:?}; row={row:?}",
                row.get("id")?.to_display()
            ));
        }
        if let Some(message) = refusal {
            rt.call_path("emitAndLogError", vec![Js::str(&message)])?;
        } else {
            admitted.push(row);
        }
    }
    Ok(Js::array(admitted))
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    let member = match name {
        "fetchOrders" => async_member(rt, name, |h, _| {
            let rt = h.borrow().rt.clone();
            let task = body::fetch_orders(h);
            Box::pin(async move { admit_order_rows(&rt, task.await?) })
        }),
        "fetchLimitOrders" => async_member(rt, name, |h, _| body::fetch_limit_orders(h)),
        "fetchAlgoOrders" => async_member(rt, name, |h, _| body::fetch_algo_orders(h)),
        "cancelOrders" => async_member(rt, name, |h, a| unit(body::cancel_orders(h, arg(&a, 0)))),
        "cancelAlgoOrder" => async_member(rt, name, |h, a| {
            let recreate = arg(&a, 1);
            let recreate = recreate.is_undefined() || recreate.truthy();
            unit(body::cancel_algo_order(h, arg(&a, 0), recreate))
        }),
        "getPositionSideForOrder" => async_member(rt, name, |h, a| {
            body::get_position_side_for_order(h, arg(&a, 0))
        }),
        "cancelAllOrders" => async_member(rt, name, |h, _| unit(body::cancel_all_orders(h))),
        "cancelSymbolOrders" => async_member(rt, name, |h, a| {
            unit(body::cancel_symbol_orders(h, arg(&a, 0)))
        }),
        _ => return None,
    };
    Some(Ok(member))
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
