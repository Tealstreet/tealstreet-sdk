//! WOOXExchange members ported in `crate::legacy_metadata`, over js_runtime values.
use super::market_reads::{awaited, call_member, invoke, mode_js, opt, spawn};
use crate::{
    binding::{
        promise::Promise,
        rt::{Rt, Shared},
        value::{Js, Thrown, arg, from_typed, to_typed},
    },
    legacy_metadata::{self as meta, Field, Fields, Host, JsString, Rows, State, Task},
};
use safe_cex_types::PositionMode;
use std::{
    cell::RefCell,
    future::poll_fn,
    rc::{Rc, Weak},
    task::Poll,
};

/// The metadata cache/in-flight/backoff maps; one receiver per exchange scope.
fn receiver_state(rt: &Rt) -> Rc<State<Thrown>> {
    rt.shared.extra.metadata.get_or_init(Rc::default).clone()
}

/// `this._accountPositionMode` is the source of truth other members read.
fn read_mode(this: &Js) -> Option<PositionMode> {
    match this.get("_accountPositionMode") {
        Ok(Js::String(s)) if *s == *crate::binding::value::utf16("oneway") => {
            Some(PositionMode::OneWay)
        }
        Ok(Js::String(s)) if *s == *crate::binding::value::utf16("hedged") => {
            Some(PositionMode::Hedged)
        }
        _ => None,
    }
}

fn write_mode(rt: &Rt, state: &State<Thrown>) -> Result<(), Thrown> {
    let value = state.mode.get().map_or(Js::Null, mode_js);
    rt.this.set("_accountPositionMode", value)
}

/// `isWooRateLimitError` / `isWooPositionModeAlreadySetError`: the shared message read.
fn error_text(e: &Js) -> Result<String, Thrown> {
    let message = opt(&opt(&opt(e, "response")?, "data")?, "message")?;
    let message = if message.truthy() {
        message
    } else {
        opt(e, "message")?
    };
    let message = if message.truthy() {
        message
    } else {
        Js::str("")
    };
    Ok(String::from_utf16_lossy(&message.to_js_string()?).to_lowercase())
}

/// Yields one scheduler turn so the caller's host borrow ends before polling.
fn deferred(request: meta::Shared<Rows, Thrown>) -> Task<Js, Thrown> {
    let mut yielded = false;
    Box::pin(async move {
        poll_fn(|cx| {
            if yielded {
                Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        })
        .await;
        let _ = request.await;
        Ok(Js::Undefined)
    })
}

pub(crate) fn rows_js(rows: &Rows) -> Result<Js, Thrown> {
    let mut out = vec![];
    for m in rows.borrow().iter() {
        out.push(Js::object(vec![
            ("symbol", Js::utf16(&m.symbol)),
            ("side", from_typed(&m.side)?),
            ("leverage", Js::Number(m.leverage)),
            ("positionMode", from_typed(&m.position_mode)?),
            ("marginType", from_typed(&m.margin_type)?),
        ]));
    }
    Ok(Js::array(out))
}

fn fields_js(fields: Fields) -> Js {
    let out = Js::object(vec![]);
    for (k, v) in fields {
        let _ = out.define(
            k,
            match v {
                Field::Text(s) => Js::utf16(&s),
                Field::Number(n) => Js::Number(n),
            },
        );
    }
    out
}

pub(crate) struct Frame {
    pub(crate) rt: Rt,
}

impl Frame {
    fn shared(rt: &Rt) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self { rt: rt.clone() }))
    }
    fn sync(&self) -> Result<(), Thrown> {
        write_mode(&self.rt, &receiver_state(&self.rt))
    }
    fn xhr(&self, method: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let client = self.rt.this.get("xhr")?;
        let f = client.get(method)?;
        invoke(&f, client, args)
    }
}

impl Host for Frame {
    type Error = Thrown;
    type Value = Js;
    type Positions = Js;
    fn state(&self) -> Rc<State<Thrown>> {
        let state = receiver_state(&self.rt);
        state.mode.set(read_mode(&self.rt.this));
        state
    }
    fn get(&mut self, endpoint: &str, params: Fields) -> Result<Task<Js, Thrown>, Thrown> {
        let mut args = vec![Js::str(endpoint)];
        if !params.is_empty() {
            args.push(Js::object(vec![("params", fields_js(params))]));
        }
        let response = awaited(&self.rt, self.xhr("get", args)?);
        Ok(Box::pin(async move { response.await?.get("data") }))
    }
    fn post(&mut self, endpoint: &str, body: Fields) -> Result<Task<(), Thrown>, Thrown> {
        let response = awaited(
            &self.rt,
            self.xhr("post", vec![Js::str(endpoint), fields_js(body)])?,
        );
        Ok(Box::pin(async move { response.await.map(|_| ()) }))
    }
    fn object(&mut self, v: &Js) -> bool {
        v.type_of() == "object"
    }
    fn truthy(&mut self, v: &Js) -> bool {
        v.truthy()
    }
    fn field(&mut self, v: &Js, key: &str) -> Result<Js, Thrown> {
        v.get(key)
    }
    fn index(&mut self, v: &Js, index: usize) -> Result<Js, Thrown> {
        v.get(&index.to_string())
    }
    fn strict_true(&mut self, v: &Js) -> bool {
        v.strict_equals(&Js::Bool(true))
    }
    fn equals(&mut self, v: &Js, text: &str) -> bool {
        v.strict_equals(&Js::str(text))
    }
    fn text(&mut self, v: &Js) -> Result<JsString, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn response_message(&mut self, v: &Js, fallback: &str) -> Result<JsString, Thrown> {
        let message = call_member(
            &self.rt,
            "getWooResponseMessage",
            vec![v.clone(), Js::str(fallback)],
        )?;
        Ok(message.to_js_string()?.to_vec())
    }
    fn error(&mut self, message: JsString) -> Thrown {
        Thrown::error("Error", &String::from_utf16_lossy(&message))
    }
    fn now(&mut self) -> Result<f64, Thrown> {
        let date = self.rt.global("Date")?;
        let now = date.get("now")?;
        invoke(&now, date, vec![])?.to_number()
    }
    fn reverse_symbol(&mut self, symbol: &[u16]) -> Result<JsString, Thrown> {
        let reverse = self.rt.global("reverseSymbol")?;
        Ok(invoke(&reverse, Js::Undefined, vec![Js::utf16(symbol)])?
            .to_js_string()?
            .to_vec())
    }
    fn rate_limit_error(&mut self, error: &Thrown) -> Result<bool, Thrown> {
        let e = &error.value;
        let response = opt(e, "response")?;
        let data = opt(&response, "data")?;
        Ok(opt(&response, "status")?.strict_equals(&Js::Number(429.0))
            || opt(&data, "code")?.strict_equals(&Js::Number(-1003.0))
            || opt(&data, "error_code")?.strict_equals(&Js::Number(90003.0))
            || error_text(e)?.contains("rate limit"))
    }
    fn already_set_error(&mut self, error: &Thrown) -> Result<bool, Thrown> {
        Ok(error_text(&error.value)?.contains("position mode remains unchanged"))
    }
    fn emit_error(&mut self, error: &Thrown, full: bool, field: &str) -> Result<(), Thrown> {
        let e = &error.value;
        let message = opt(&opt(&opt(e, "response")?, "data")?, field)?;
        let message = if message.truthy() {
            message
        } else {
            opt(e, "message")?
        };
        let args = if full {
            vec![e.clone(), message]
        } else {
            vec![message]
        };
        call_member(&self.rt, "emitAndLogError", args).map(|_| ())
    }
    fn sleep(&mut self, delay: u64) -> Result<Task<(), Thrown>, Thrown> {
        let sleep = self.rt.global("sleep")?;
        let done = awaited(
            &self.rt,
            invoke(&sleep, Js::Undefined, vec![Js::Number(delay as f64)])?,
        );
        Ok(Box::pin(async move { done.await.map(|_| ()) }))
    }
    fn fetch_positions(&mut self) -> Result<Task<Js, Thrown>, Thrown> {
        Ok(awaited(
            &self.rt,
            call_member(&self.rt, "fetchPositions", vec![])?,
        ))
    }
    fn update_positions(&mut self, positions: Js) -> Result<(), Thrown> {
        let store = self.rt.this.get("store")?;
        let update = store.get("update")?;
        invoke(
            &update,
            store,
            vec![Js::object(vec![("positions", positions)])],
        )
        .map(|_| ())
    }
    fn account_info(&mut self) -> Result<Task<Js, Thrown>, Thrown> {
        Ok(awaited(
            &self.rt,
            call_member(&self.rt, "fetchAccountInfo", vec![])?,
        ))
    }
    fn position_mode(&mut self) -> Result<Task<PositionMode, Thrown>, Thrown> {
        self.sync()?;
        let mode = awaited(&self.rt, call_member(&self.rt, "getPositionMode", vec![])?);
        Ok(Box::pin(async move { to_typed(&mode.await?) }))
    }
    fn spawn_metadata(&mut self, request: meta::Shared<Rows, Thrown>) {
        let _ = Promise::spawn(&self.rt.exchange_runtime(), deferred(request));
    }
    fn confirm_mode(&mut self, mode: PositionMode) -> Result<Task<(), Thrown>, Thrown> {
        let done = awaited(
            &self.rt,
            call_member(
                &self.rt,
                "confirmPositionModeAndRefreshPositions",
                vec![mode_js(mode)],
            )?,
        );
        Ok(Box::pin(async move { done.await.map(|_| ()) }))
    }
}

fn text(v: &Js) -> Result<JsString, Thrown> {
    Ok(v.to_js_string()?.to_vec())
}

/// Runs a mode-writing async body, then publishes the mode it left behind.
fn publish_mode(rt: &Rt, task: Task<Js, Thrown>) -> Js {
    let owner = rt.clone();
    spawn(
        rt,
        Ok(Box::pin(async move {
            let result = task.await;
            write_mode(&owner, &receiver_state(&owner))?;
            result
        })),
    )
}

type Body = fn(&Rt, Vec<Js>) -> Result<Js, Thrown>;

fn member(rt: &Rt, name: &'static str, body: Body) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, args| body(&rt, args))
}

fn fetch_account_info(rt: &Rt, _: Vec<Js>) -> Result<Js, Thrown> {
    Ok(spawn(rt, Ok(meta::fetch_account_info(Frame::shared(rt)))))
}

fn get_position_mode(rt: &Rt, _: Vec<Js>) -> Result<Js, Thrown> {
    let mode = meta::get_position_mode(Frame::shared(rt));
    Ok(publish_mode(
        rt,
        Box::pin(async move { Ok(mode_js(mode.await?)) }),
    ))
}

fn create_position_metadata_cache_key(_: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let symbol = text(&arg(&args, 0))?;
    let mode: PositionMode = to_typed(&arg(&args, 1))?;
    Ok(Js::utf16(&meta::create_position_metadata_cache_key(
        &symbol, mode,
    )))
}

fn build_position_metadata_from_leverage(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let symbol = text(&arg(&args, 0))?;
    let mode: PositionMode = to_typed(&arg(&args, 1))?;
    let frame = Frame::shared(rt);
    let rows = meta::build_position_metadata_from_leverage(
        &mut *frame.borrow_mut(),
        symbol,
        mode,
        arg(&args, 2),
    )?;
    rows_js(&rows)
}

fn fetch_position_metadata_for_symbol(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let start = || -> Result<Task<Js, Thrown>, Thrown> {
        let symbol = text(&arg(&args, 0))?;
        let mode: PositionMode = to_typed(&arg(&args, 1))?;
        let rows = meta::fetch_position_metadata_for_symbol(Frame::shared(rt), symbol, mode);
        Ok(Box::pin(async move { rows_js(&rows.await?) }) as Task<Js, Thrown>)
    };
    Ok(spawn(rt, start()))
}

fn update_cached_position_metadata_leverage(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let symbol = text(&arg(&args, 0))?;
    let leverage = arg(&args, 1).to_number()?;
    let frame = Frame::shared(rt);
    meta::update_cached_position_metadata_leverage(&mut *frame.borrow_mut(), &symbol, leverage)?;
    Ok(Js::Undefined)
}

fn confirm_position_mode_and_refresh_positions(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let expected: PositionMode = match to_typed(&arg(&args, 0)) {
        Ok(mode) => mode,
        Err(e) => return Ok(spawn(rt, Err(e))),
    };
    let done = meta::confirm_position_mode_and_refresh_positions(Frame::shared(rt), expected);
    Ok(publish_mode(
        rt,
        Box::pin(async move { done.await.map(|()| Js::Undefined) }),
    ))
}

fn set_leverage(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let start = || -> Result<Task<Js, Thrown>, Thrown> {
        let symbol = text(&arg(&args, 0))?;
        let leverage = arg(&args, 1).to_number()?;
        let done = meta::set_leverage(Frame::shared(rt), symbol, leverage);
        Ok(Box::pin(async move { done.await.map(|()| Js::Undefined) }) as Task<Js, Thrown>)
    };
    Ok(spawn(rt, start()))
}

fn change_position_mode(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let done = meta::change_position_mode(Frame::shared(rt), arg(&args, 0).truthy());
    Ok(spawn(
        rt,
        Ok(Box::pin(async move { done.await.map(|()| Js::Undefined) })),
    ))
}

fn after_method_called_in_worker(rt: &Rt, args: Vec<Js>) -> Result<Js, Thrown> {
    let method = match arg(&args, 1) {
        Js::String(s) => String::from_utf16_lossy(&s),
        _ => String::new(),
    };
    let frame = Frame::shared(rt);
    let argument = (args.len() > 2).then(|| arg(&args, 2));
    meta::after_method_called_in_worker(&mut *frame.borrow_mut(), &method, argument);
    frame.borrow().sync()?;
    Ok(Js::Undefined)
}

pub(crate) fn bind(rt: &Rt, name: &str) -> Option<Result<Js, Thrown>> {
    let (name, body): (&'static str, Body) = match name {
        "fetchAccountInfo" => ("fetchAccountInfo", fetch_account_info),
        "getPositionMode" => ("getPositionMode", get_position_mode),
        "createPositionMetadataCacheKey" => (
            "createPositionMetadataCacheKey",
            create_position_metadata_cache_key,
        ),
        "buildPositionMetadataFromLeverage" => (
            "buildPositionMetadataFromLeverage",
            build_position_metadata_from_leverage,
        ),
        "fetchPositionMetadataForSymbol" => (
            "fetchPositionMetadataForSymbol",
            fetch_position_metadata_for_symbol,
        ),
        "updateCachedPositionMetadataLeverage" => (
            "updateCachedPositionMetadataLeverage",
            update_cached_position_metadata_leverage,
        ),
        "confirmPositionModeAndRefreshPositions" => (
            "confirmPositionModeAndRefreshPositions",
            confirm_position_mode_and_refresh_positions,
        ),
        "setLeverage" => ("setLeverage", set_leverage),
        "changePositionMode" => ("changePositionMode", change_position_mode),
        "afterMethodCalledInWorker" => ("afterMethodCalledInWorker", after_method_called_in_worker),
        _ => return None,
    };
    Some(Ok(member(rt, name, body)))
}

pub(crate) fn register(_g: &Js, _weak: Weak<Shared>) {}
