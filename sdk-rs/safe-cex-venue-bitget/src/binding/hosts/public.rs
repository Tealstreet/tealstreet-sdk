//! Concrete PublicDataHost / SocketRuntimeHost plus the public-group adapters:
//! public REST reads, public and private socket members, ticker and subscriptions.
use crate::binding::{
    globals::{arg, shared},
    promise::{JsFuture, await_js},
    rt::{Rt, Shared},
    sockets,
    value::{Js, Str, Thrown, from_typed, to_json, to_typed, utf16},
};
use crate::{
    numeric::FiniteNumberInput,
    public_data::{self, PublicDataHost, PublicRead},
    socket_ping,
    socket_runtime::{self, SocketFuture, SocketRuntimeHost},
    subscriptions, ticker,
};
use safe_cex_exchange::{
    external::ExternalPublicData,
    runtime::{ExchangeError, ListenerCleanup},
};
use safe_cex_types::{Candle, ExchangeName, ExchangeOptions, OHLCVOptions, OrderBook};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

/// Native array iterator state: index into the live receiver, length re-read per step.
pub(crate) struct JsIter {
    value: Js,
    index: usize,
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// `owner.member` with the owner kept as receiver, read along the dotted path.
fn capture_path(rt: &Rt, path: &str) -> Result<Js, Thrown> {
    let (owner, member) = rt.owner_and_member(path)?;
    Ok(bind_owner(
        owner,
        member,
        path.rsplit('.').next().unwrap_or(path),
    ))
}

fn bind_owner(owner: Js, value: Js, name: &str) -> Js {
    if !value.is_callable() {
        return value;
    }
    Js::function(name, move |_, a| value.call(owner.clone(), a))
}

fn capture_member_of(value: &Js, name: &str) -> Result<Js, Thrown> {
    let member = value.get(name)?;
    Ok(bind_owner(value.clone(), member, name))
}

fn array_cell(value: &Js, method: &str) -> Result<Rc<RefCell<Vec<Option<Js>>>>, Thrown> {
    match value {
        Js::Array(items) => Ok(items.clone()),
        other if other.is_nullish() => Err(other
            .get(method)
            .err()
            .unwrap_or_else(|| Thrown::type_error(method))),
        other => Err(Thrown::type_error(&format!(
            "{}.{method} is not a function",
            other.to_display()
        ))),
    }
}

/// Element `i` if the live array still has it (HasProperty), skipping holes.
fn live(items: &Rc<RefCell<Vec<Option<Js>>>>, i: usize) -> Option<Js> {
    items.borrow().get(i).cloned().flatten()
}

fn native_filter<F>(value: Js, mut f: F) -> Result<Js, Thrown>
where
    F: FnMut(Js, usize) -> Result<bool, Thrown>,
{
    let items = array_cell(&value, "filter")?;
    let len = items.borrow().len();
    let mut out = vec![];
    for i in 0..len {
        if let Some(item) = live(&items, i)
            && f(item.clone(), i)?
        {
            out.push(item);
        }
    }
    Ok(Js::array(out))
}

fn native_map<F>(value: Js, mut f: F) -> Result<Js, Thrown>
where
    F: FnMut(Js, usize) -> Result<Js, Thrown>,
{
    let items = array_cell(&value, "map")?;
    let len = items.borrow().len();
    let mut out = vec![None; len];
    for (i, slot) in out.iter_mut().enumerate() {
        if let Some(item) = live(&items, i) {
            *slot = Some(f(item, i)?);
        }
    }
    Ok(Js::holey_array(out))
}

fn native_flat_map<F>(value: Js, mut f: F) -> Result<Js, Thrown>
where
    F: FnMut(Js, usize) -> Result<Js, Thrown>,
{
    let items = array_cell(&value, "flatMap")?;
    let len = items.borrow().len();
    let mut out = vec![];
    for i in 0..len {
        if let Some(item) = live(&items, i) {
            match f(item, i)? {
                Js::Array(inner) => out.extend(inner.borrow().iter().flatten().cloned()),
                other => out.push(other),
            }
        }
    }
    Ok(Js::array(out))
}

fn native_reduce<F>(value: Js, initial: Js, mut f: F) -> Result<Js, Thrown>
where
    F: FnMut(Js, Js, usize) -> Result<Js, Thrown>,
{
    let items = array_cell(&value, "reduce")?;
    let len = items.borrow().len();
    let mut acc = initial;
    for i in 0..len {
        if let Some(item) = live(&items, i) {
            acc = f(acc, item, i)?;
        }
    }
    Ok(acc)
}

fn iterator_of(value: &Js) -> Result<JsIter, Thrown> {
    match value {
        Js::Array(_) | Js::String(_) | Js::Map(_) | Js::Set(_) => Ok(JsIter {
            value: value.clone(),
            index: 0,
        }),
        other => Err(Thrown::type_error(&format!(
            "{} is not iterable",
            other.to_display()
        ))),
    }
}

fn iterator_next(iter: &mut JsIter) -> Result<Option<Js>, Thrown> {
    let i = iter.index;
    let out = match &iter.value {
        Js::Array(items) => {
            let items = items.borrow();
            if i >= items.len() {
                return Ok(None);
            }
            iter.index += 1;
            items[i].clone().unwrap_or(Js::Undefined)
        }
        Js::String(s) => {
            let Some(&unit) = s.get(i) else {
                return Ok(None);
            };
            let pair = (0xD800..0xDC00).contains(&unit)
                && s.get(i + 1).is_some_and(|n| (0xDC00..0xE000).contains(n));
            let width = if pair { 2 } else { 1 };
            iter.index += width;
            Js::utf16(&s[i..i + width])
        }
        Js::Map(m) => {
            let Some((k, v)) = m.borrow().get(i).cloned() else {
                return Ok(None);
            };
            iter.index += 1;
            Js::array(vec![k, v])
        }
        Js::Set(s) => {
            let Some(v) = s.borrow().get(i).cloned() else {
                return Ok(None);
            };
            iter.index += 1;
            v
        }
        _ => return Ok(None),
    };
    Ok(Some(out))
}

fn property_key(key: &Js) -> Result<Str, Thrown> {
    key.to_js_string()
}

/// Abstract relational `a < b`; None is the undefined (NaN) outcome.
fn less_than(a: &Js, b: &Js) -> Result<Option<bool>, Thrown> {
    if let (Js::String(a), Js::String(b)) = (a, b) {
        return Ok(Some(a.as_ref() < b.as_ref()));
    }
    let (a, b) = (a.to_number()?, b.to_number()?);
    Ok(if a.is_nan() || b.is_nan() {
        None
    } else {
        Some(a < b)
    })
}

/// `text.split(/(\d+)/)`: separators are captured, so digit runs stay as items.
fn split_digits(value: &Js) -> Result<Js, Thrown> {
    let Js::String(s) = value else {
        let split = value.get("split")?;
        return Err(Thrown::type_error(&format!(
            "{}.split is not a function",
            split.to_display()
        )));
    };
    let digit = |c: u16| (b'0' as u16..=b'9' as u16).contains(&c);
    let mut out = vec![];
    let mut start = 0;
    let mut i = 0;
    while i < s.len() {
        if digit(s[i]) {
            let run = i;
            while i < s.len() && digit(s[i]) {
                i += 1;
            }
            out.push(Js::utf16(&s[start..run]));
            out.push(Js::utf16(&s[run..i]));
            start = i;
        } else {
            i += 1;
        }
    }
    out.push(Js::utf16(&s[start..]));
    Ok(Js::array(out))
}

fn new_map(entries: &Js) -> Result<Js, Thrown> {
    let map = Js::new_map();
    if entries.is_nullish() {
        return Ok(map);
    }
    let Js::Map(cells) = &map else {
        unreachable!("new_map is a Map")
    };
    let mut iter = iterator_of(entries)?;
    while let Some(entry) = iterator_next(&mut iter)? {
        if !entry.is_object() {
            return Err(Thrown::type_error(&format!(
                "Iterator value {} is not an entry object",
                entry.to_display()
            )));
        }
        let (k, v) = (entry.get("0")?, entry.get("1")?);
        let mut cells = cells.borrow_mut();
        match cells.iter_mut().find(|(key, _)| key.same_value_zero(&k)) {
            Some(slot) => slot.1 = v,
            None => cells.push((k, v)),
        }
    }
    Ok(map)
}

fn new_set(values: &Js) -> Result<Js, Thrown> {
    let set = Js::new_set();
    if values.is_nullish() {
        return Ok(set);
    }
    let Js::Set(cells) = &set else {
        unreachable!("new_set is a Set")
    };
    let mut iter = iterator_of(values)?;
    while let Some(v) = iterator_next(&mut iter)? {
        let mut cells = cells.borrow_mut();
        if !cells.iter().any(|x| x.same_value_zero(&v)) {
            cells.push(v);
        }
    }
    Ok(set)
}

/// create-hmac: `digest(enc)` returns text, `digest()` a Buffer whose toString(enc) encodes.
fn create_hmac(algorithm: &Js, secret: &Js) -> Result<Js, Thrown> {
    if text(algorithm)?.to_lowercase() != "sha256" {
        return Err(Thrown::error(
            "Error",
            &format!("Digest method not supported: {}", algorithm.to_display()),
        ));
    }
    let Js::String(secret) = secret else {
        return Err(Thrown::type_error(
            "The first argument must be of type string or an instance of Buffer, ArrayBuffer, or Array or an Array-like Object.",
        ));
    };
    let secret = String::from_utf16_lossy(secret);
    let payload = Rc::new(RefCell::new(Vec::<u8>::new()));
    let mac = Js::object_with_class("Hmac", vec![]);
    {
        let payload = payload.clone();
        let receiver = mac.clone();
        mac.define(
            "update",
            Js::function("update", move |_, a| {
                let Js::String(data) = arg(&a, 0) else {
                    return Err(Thrown::type_error("Data must be a string or a buffer"));
                };
                payload
                    .borrow_mut()
                    .extend(String::from_utf16_lossy(&data).into_bytes());
                Ok(receiver.clone())
            }),
        )?;
    }
    mac.define(
        "digest",
        Js::function("digest", move |_, a| {
            let encode = |encoding: &Js| -> Result<Js, Thrown> {
                let encoding = match text(encoding)?.as_str() {
                    "base64" => safe_cex_crypto::Encoding::Base64,
                    "hex" => safe_cex_crypto::Encoding::Hex,
                    other => return Err(Thrown::type_error(&format!("Unknown encoding: {other}"))),
                };
                Ok(Js::str(&safe_cex_crypto::hmac_sha256(
                    secret.as_bytes(),
                    &payload.borrow(),
                    encoding,
                )))
            };
            let encoding = arg(&a, 0);
            if !encoding.is_undefined() {
                return encode(&encoding);
            }
            let digest = encode(&Js::str("base64"))?;
            let hex = encode(&Js::str("hex"))?;
            Ok(Js::object_with_class(
                "Buffer",
                vec![(
                    "toString",
                    Js::function("toString", move |_, b| match text(&arg(&b, 0))?.as_str() {
                        "base64" => Ok(digest.clone()),
                        "hex" => Ok(hex.clone()),
                        other => Err(Thrown::type_error(&format!("Unknown encoding: {other}"))),
                    }),
                )],
            ))
        }),
    )?;
    Ok(mac)
}

/// Module-scope value by dotted path (`Math.min`, `OrderStatus.Open`) with its owner.
fn global_path(rt: &Rt, path: &str) -> Result<(Js, Js), Thrown> {
    let mut parts = path.split('.');
    let mut owner = Js::Undefined;
    let mut value = rt.global(parts.next().unwrap_or_default())?;
    for part in parts {
        let next = value.get(part)?;
        owner = value;
        value = next;
    }
    Ok((owner, value))
}

fn virtual_ms(rt: &Rt) -> Result<Js, Thrown> {
    let (owner, f) = global_path(rt, "virtualClock.getCurrentTimeMs")?;
    f.call(owner, vec![])
}

/// Named imported/native operations of the public_data and socket_runtime bodies.
fn operation(rt: &Rt, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    let a0 = arg(&args, 0);
    let a1 = arg(&args, 1);
    Ok(match name {
        "increment" => Js::Number(a0.to_number()? + 1.0),
        "greater" => Js::Bool(less_than(&a1, &a0)? == Some(true)),
        "greaterEqual" => Js::Bool(less_than(&a0, &a1)? == Some(false)),
        "template" => {
            let mut out: Vec<u16> = vec![];
            for v in &args {
                out.extend_from_slice(&v.to_js_string()?);
            }
            Js::String(out.into())
        }
        "toUpperCase" => a0.call_method("toUpperCase", vec![])?,
        "power" => Js::Number(safe_cex_num::math_pow(a0.to_number()?, a1.to_number()?)),
        "splitDigits" => split_digits(&a0)?,
        "newMap" => new_map(&a0)?,
        "newSet" => new_set(&a0)?,
        "createHmac" => create_hmac(&a0, &a1)?,
        "clock.ms" => virtual_ms(rt)?,
        // ts: virtualClock.getCurrentTime().unix() floors the virtual milliseconds.
        "clock.unix" => Js::Number((virtual_ms(rt)?.to_number()? / 1000.0).floor()),
        "dayjs.valueOf" => a0.call_method("valueOf", vec![])?,
        _ => {
            let (owner, value) = global_path(rt, name)?;
            if value.is_callable() {
                value.call(owner, args)?
            } else if args.is_empty() {
                value
            } else {
                // Imported lookup tables (INTERVAL, timeframeToSeconds) are indexed by the argument.
                value.get_key(&property_key(&a0)?)?
            }
        }
    })
}

impl PublicDataHost for Rt {
    type Value = Js;
    type Error = Thrown;
    type Iterator = JsIter;
    fn receiver(&self, path: &str) -> Result<Js, Thrown> {
        self.path(path)
    }
    fn set_receiver(&self, path: &str, value: Js) -> Result<(), Thrown> {
        self.set_path(path, value)
    }
    fn capture(&self, path: &str) -> Result<Js, Thrown> {
        capture_path(self, path)
    }
    fn invoke(&self, method: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        method.call(Js::Undefined, args)
    }
    fn capture_member(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        capture_member_of(value, name)
    }
    fn flat_map<F>(&self, value: Js, callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<Js, Thrown>,
    {
        native_flat_map(value, callback)
    }
    fn property(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        value.get(name)
    }
    fn key_get(&self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        value.get_key(&property_key(key)?)
    }
    fn assign(&self, value: &Js, key: Js, field: Js) -> Result<(), Thrown> {
        value.set_key(&property_key(&key)?, field)
    }
    fn define(&self, value: &Js, key: Js, field: Js) -> Result<(), Thrown> {
        value.define_key(&property_key(&key)?, field)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn numeric(&self, value: &Js) -> Result<f64, Thrown> {
        value.to_number()
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn string(&self, value: &[u16]) -> Js {
        Js::utf16(value)
    }
    fn array(&self, value: Vec<Js>) -> Js {
        Js::array(value)
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn spread(&self, value: &Js) -> Result<Js, Thrown> {
        value.spread_object()
    }
    fn index(&self, value: &Js, index: usize) -> Result<Js, Thrown> {
        value.get_key(&utf16(&index.to_string()))
    }
    fn iterator(&self, value: &Js) -> Result<JsIter, Thrown> {
        iterator_of(value)
    }
    fn next(&self, iterator: &mut JsIter) -> Result<Option<Js>, Thrown> {
        iterator_next(iterator)
    }
    fn filter<F>(&self, value: Js, callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<bool, Thrown>,
    {
        native_filter(value, callback)
    }
    fn map<F>(&self, value: Js, callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<Js, Thrown>,
    {
        native_map(value, callback)
    }
    fn reduce<F>(&self, value: Js, initial: Js, callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, Js, usize) -> Result<Js, Thrown>,
    {
        native_reduce(value, initial, callback)
    }
    fn operation(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        operation(self, name, args)
    }
    fn read(
        &self,
        client: Js,
        endpoint: &'static str,
        params: Js,
    ) -> Result<PublicRead<'_, Js, Thrown>, Thrown> {
        let path = self.global("ENDPOINTS")?.get(endpoint)?;
        let started = client.call(
            Js::Undefined,
            vec![path, Js::object(vec![("params", params)])],
        )?;
        Ok(Box::pin(async move {
            await_js(&self.exchange_runtime(), started)
                .await?
                .get("data")
        }))
    }
}

impl SocketRuntimeHost for Rt {
    type Value = Js;
    type Error = Thrown;
    type Iterator = JsIter;
    fn receiver(&self, path: &str) -> Result<Js, Thrown> {
        self.path(path)
    }
    fn set_receiver(&self, path: &str, value: Js) -> Result<(), Thrown> {
        self.set_path(path, value)
    }
    fn capture(&self, path: &str) -> Result<Js, Thrown> {
        capture_path(self, path)
    }
    fn capture_member(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        capture_member_of(value, name)
    }
    fn invoke(&self, method: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        method.call(Js::Undefined, args)
    }
    fn invoke_bare(&self, callback: Js, args: Vec<Js>) -> Result<(), Thrown> {
        callback.call(Js::Undefined, args).map(|_| ())
    }
    fn start(&self, method: Js, args: Vec<Js>) -> Result<SocketFuture<'_, Js, Thrown>, Thrown> {
        let started = method.call(Js::Undefined, args)?;
        Ok(Box::pin(await_js(&self.exchange_runtime(), started)))
    }
    fn property(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        value.get(name)
    }
    fn key_get(&self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        value.get_key(&property_key(key)?)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn string(&self, value: &[u16]) -> Js {
        Js::utf16(value)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn array(&self, value: Vec<Js>) -> Js {
        Js::array(value)
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn spread(&self, value: &Js) -> Result<Js, Thrown> {
        value.spread_object()
    }
    fn define(&self, value: &Js, key: &str, field: Js) -> Result<(), Thrown> {
        value.define(key, field)
    }
    fn iterator(&self, value: &Js) -> Result<JsIter, Thrown> {
        iterator_of(value)
    }
    fn next(&self, iter: &mut JsIter) -> Result<Option<Js>, Thrown> {
        iterator_next(iter)
    }
    fn filter<F>(&self, value: Js, callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<bool, Thrown>,
    {
        native_filter(value, callback)
    }
    fn map<F>(&self, value: Js, callback: F) -> Result<Js, Thrown>
    where
        F: FnMut(Js, usize) -> Result<Js, Thrown>,
    {
        native_map(value, callback)
    }
    fn operation(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        operation(self, name, args)
    }
}

// --- dayjs (UTC calendar; the TS runtime uses the host's local zone) ---

const MAX_TIME_MS: f64 = 8.64e15;

fn time_clip(ms: f64) -> f64 {
    if !ms.is_finite() || ms.abs() > MAX_TIME_MS {
        f64::NAN
    } else {
        ms.trunc() + 0.0
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    days_from_civil(ny, nm, 1) - days_from_civil(y, m, 1)
}

/// dayjs Utils.p unit normalization.
fn dayjs_unit(unit: &Js) -> Result<String, Thrown> {
    let raw = if unit.truthy() {
        text(unit)?
    } else {
        String::new()
    };
    Ok(match raw.as_str() {
        "M" => "month".into(),
        "y" => "year".into(),
        "w" => "week".into(),
        "d" => "day".into(),
        "D" => "date".into(),
        "h" => "hour".into(),
        "m" => "minute".into(),
        "s" => "second".into(),
        "ms" => "millisecond".into(),
        "Q" => "quarter".into(),
        other => {
            let lower = other.to_lowercase();
            lower.strip_suffix('s').map(str::to_owned).unwrap_or(lower)
        }
    })
}

// ts: dayjs.prototype.add (subtract is add(-n)); calendar units set month/year/date fields.
fn dayjs_add(ms: f64, number: f64, unit: &str) -> f64 {
    if ms.is_nan() {
        return f64::NAN;
    }
    let day_ms = 86_400_000.0;
    match unit {
        "month" | "year" => {
            let days = (ms / day_ms).floor() as i64;
            let tod = ms - days as f64 * day_ms;
            let (y, m, d) = civil_from_days(days);
            let target = if unit == "month" {
                (m - 1) as f64 + number
            } else {
                y as f64 + number
            };
            if !target.is_finite() || target.abs() > 1e9 {
                return f64::NAN;
            }
            let target = target.trunc() as i64;
            let (ny, nm) = if unit == "month" {
                (y + target.div_euclid(12), target.rem_euclid(12) + 1)
            } else {
                (target, m)
            };
            let nd = d.min(days_in_month(ny, nm));
            time_clip(days_from_civil(ny, nm, nd) as f64 * day_ms + tod)
        }
        "day" | "week" => {
            let step = if unit == "week" { 7.0 } else { 1.0 };
            time_clip(ms + safe_cex_num::math_round(step * number) * day_ms)
        }
        "minute" => time_clip(ms + number * 60_000.0),
        "hour" => time_clip(ms + number * 3_600_000.0),
        "second" => time_clip(ms + number * 1_000.0),
        _ => time_clip(ms + number),
    }
}

/// `YYYY-MM-DD[THH:mm[:ss[.sss]]][Z]`, read on the UTC calendar.
fn parse_date_text(s: &str) -> f64 {
    let s = s.strip_suffix(['Z', 'z']).unwrap_or(s);
    let (date, time) = s.split_once(['T', 't', ' ']).unwrap_or((s, ""));
    let nums: Vec<&str> = date.split(['-', '/']).collect();
    let field = |v: Option<&&str>, default: i64| -> Option<i64> {
        match v {
            None => Some(default),
            Some(&"") => Some(default),
            Some(t) if t.bytes().all(|b| b.is_ascii_digit()) => t.parse().ok(),
            _ => None,
        }
    };
    if nums.first().is_none_or(|y| y.len() != 4) || nums.len() > 3 {
        return f64::NAN;
    }
    let (Some(y), Some(mo), Some(d)) = (
        field(nums.first(), 0),
        field(nums.get(1), 1),
        field(nums.get(2), 1),
    ) else {
        return f64::NAN;
    };
    let (clock, frac) = time.split_once('.').unwrap_or((time, ""));
    let parts: Vec<&str> = if clock.is_empty() {
        vec![]
    } else {
        clock.split(':').collect()
    };
    let (Some(h), Some(mi), Some(sec)) = (
        field(parts.first(), 0),
        field(parts.get(1), 0),
        field(parts.get(2), 0),
    ) else {
        return f64::NAN;
    };
    let millis = if frac.is_empty() {
        0
    } else if frac.bytes().all(|b| b.is_ascii_digit()) {
        frac.chars()
            .take(3)
            .collect::<String>()
            .parse::<i64>()
            .unwrap_or(0)
            * 10_i64.pow(3 - frac.len().min(3) as u32)
    } else {
        return f64::NAN;
    };
    if !(1..=12).contains(&mo) || parts.len() > 3 {
        return f64::NAN;
    }
    let days = days_from_civil(y, mo, 1) + d - 1;
    time_clip(
        days as f64 * 86_400_000.0 + (h * 3_600_000 + mi * 60_000 + sec * 1_000 + millis) as f64,
    )
}

fn dayjs_value(ms: f64) -> Js {
    let value = Js::object_with_class("Dayjs", vec![]);
    let _ = value.define(
        "valueOf",
        Js::function("valueOf", move |_, _| Ok(Js::Number(ms))),
    );
    let _ = value.define(
        "unix",
        Js::function("unix", move |_, _| Ok(Js::Number((ms / 1000.0).floor()))),
    );
    let _ = value.define(
        "add",
        Js::function("add", move |_, a| {
            Ok(dayjs_value(dayjs_add(
                ms,
                arg(&a, 0).to_number()?,
                &dayjs_unit(&arg(&a, 1))?,
            )))
        }),
    );
    let _ = value.define(
        "subtract",
        Js::function("subtract", move |_, a| {
            Ok(dayjs_value(dayjs_add(
                ms,
                -arg(&a, 0).to_number()?,
                &dayjs_unit(&arg(&a, 1))?,
            )))
        }),
    );
    value
}

// ts: dayjs(input) parseDate: undefined is now, null invalid, Dayjs cloned, strings parsed.
fn dayjs_construct(rt: &Rt, input: &Js) -> Result<Js, Thrown> {
    let ms = match input {
        Js::Undefined => rt.now().floor(),
        Js::Null => f64::NAN,
        Js::String(s) => parse_date_text(&String::from_utf16_lossy(s)),
        Js::Object(o) if o.class == "Dayjs" => input.call_method("valueOf", vec![])?.to_number()?,
        other => time_clip(other.to_number()?),
    };
    Ok(dayjs_value(ms))
}

// ts: resolveTransportRouting(name, options) over the shared Rust routing table.
fn resolve_transport_routing(name: &Js, options: &Js) -> Result<Js, Thrown> {
    let name: ExchangeName = to_typed(name)?;
    let options: ExchangeOptions =
        safe_cex_exchange::js_runtime::base_exchange::exchange_options(options)?;
    let routing = safe_cex_transport::resolve_exchange_routing(
        &name,
        &safe_cex_transport::RoutingOptions::from(&options),
    );
    from_typed(&routing)
}

const NANOID_ALPHABET: &[u8; 64] =
    b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

pub(crate) fn register(globals: &Js, weak: Weak<Shared>) {
    let def = |k: &str, v: Js| {
        let _ = globals.define(k, v);
    };
    // config.ts: ENABLE_RECONNECTING_WEBSOCKET = false
    def("ENABLE_RECONNECTING_WEBSOCKET", Js::Bool(false));
    def(
        "timeframeToSeconds",
        Js::object(
            safe_cex_types::TIMEFRAME_TO_SECONDS
                .iter()
                .map(|(k, v)| (*k, Js::Number(*v)))
                .collect(),
        ),
    );
    def(
        "resolveTransportRouting",
        Js::function("resolveTransportRouting", |_, a| {
            resolve_transport_routing(&arg(&a, 0), &arg(&a, 1))
        }),
    );
    def(
        "jsonParse",
        Js::function("jsonParse", |_, a| {
            Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
                safe_cex_transport::how::json_parse(&text(&arg(&a, 0))?),
            ))
        }),
    );
    let w = weak.clone();
    def(
        "nanoid",
        Js::function("nanoid", move |_, _| {
            let rt = shared(&w)?;
            let id: String = (0..21)
                .map(|_| {
                    char::from(NANOID_ALPHABET[(rt.shared.runtime.random() * 64.0) as usize & 63])
                })
                .collect();
            Ok(Js::str(&id))
        }),
    );
    let w = weak;
    def(
        "dayjs",
        Js::function("dayjs", move |_, a| {
            dayjs_construct(&shared(&w)?, &arg(&a, 0))
        }),
    );
}

// --- bindings ---

fn sync_member(
    name: &str,
    rt: &Rt,
    f: impl Fn(&Rt, Vec<Js>) -> Result<Js, Thrown> + 'static,
) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, a| f(&rt, a))
}

fn async_member(name: &str, rt: &Rt, f: impl Fn(Rt, Vec<Js>) -> JsFuture<'static> + 'static) -> Js {
    let rt = rt.clone();
    Js::function(name, move |_, a| Ok(rt.spawn(f(rt.clone(), a))))
}

fn undefined(result: Result<(), Thrown>) -> Result<Js, Thrown> {
    result.map(|()| Js::Undefined)
}

fn exchange_error(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

fn ticker_error(e: ticker::TickerHandlerError) -> Thrown {
    match e {
        ticker::TickerHandlerError::Coercion(e) => Thrown::from(e),
        ticker::TickerHandlerError::Store(e) => Thrown::from(ExchangeError::from(e)),
    }
}

/// A JS-shaped public socket seen through the typed ExternalPublicData contract.
struct JsPublicData {
    rt: Rt,
    socket: Js,
}

impl JsPublicData {
    fn base(&self) -> Option<Rc<safe_cex_exchange::websocket::BaseWebSocket>> {
        sockets::core_of(&self.rt.shared.sockets, &self.socket)
            .and_then(|core| core.base.get().cloned())
    }
    fn cleanup(value: Js) -> ListenerCleanup {
        Box::new(move || {
            value
                .call(Js::Undefined, vec![])
                .map(|_| ())
                .map_err(exchange_error)
        })
    }
}

impl ExternalPublicData for JsPublicData {
    fn connect(&self) {
        if let Err(e) = self.socket.call_method("connectAndSubscribe", vec![]) {
            self.rt.report(&e, "Bitget public socket connect");
        }
    }
    fn dispose(&self) {
        if let Err(e) = self.socket.call_method("dispose", vec![]) {
            self.rt.report(&e, "Bitget public socket dispose");
        }
    }
    fn is_initialized(&self) -> bool {
        self.base().is_some_and(|b| b.diagnostics().initialized)
    }
    fn is_healthy(&self) -> bool {
        self.base().is_some_and(|b| b.is_healthy())
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        self.base().and_then(|b| b.last_message_timestamp())
    }
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let parser = Js::function("callback", move |_, a| {
            callback(to_typed::<Candle>(&arg(&a, 0))?);
            Ok(Js::Undefined)
        });
        let options = from_typed(&options).map_err(exchange_error)?;
        let cleanup = self
            .socket
            .call_method("listenOHLCV", vec![options, parser])
            .map_err(exchange_error)?;
        Ok(Self::cleanup(cleanup))
    }
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        let parser = Js::function("callback", move |_, a| {
            callback(to_typed::<OrderBook>(&arg(&a, 0))?);
            Ok(Js::Undefined)
        });
        let cleanup = self
            .socket
            .call_method("listenOrderBook", vec![Js::str(&symbol), parser])
            .map_err(exchange_error)?;
        Ok(Self::cleanup(cleanup))
    }
}

/// Typed callback over a JS callback; its failure cannot cross `Fn(T)`, so it is reported.
fn typed_callback<T: serde::Serialize + 'static>(
    rt: &Rt,
    callback: Js,
    source: &'static str,
) -> Rc<dyn Fn(T)> {
    let rt = rt.clone();
    Rc::new(move |value: T| {
        if let Err(e) = from_typed(&value).and_then(|v| callback.call(Js::Undefined, vec![v])) {
            rt.report(&e, source);
        }
    })
}

fn cleanup_function(cleanup: ListenerCleanup) -> Js {
    let slot = RefCell::new(Some(cleanup));
    Js::function("cleanup", move |_, _| {
        let taken = slot.borrow_mut().take();
        if let Some(cleanup) = taken {
            cleanup().map_err(Thrown::from)?;
        }
        Ok(Js::Undefined)
    })
}

fn public_data_of(rt: &Rt) -> Result<JsPublicData, Thrown> {
    Ok(JsPublicData {
        rt: rt.clone(),
        socket: rt.this.get("publicWebsocket")?,
    })
}

fn finite_number_input(value: &Js) -> (Option<f64>, Option<String>) {
    match value {
        Js::Number(n) => (Some(*n), None),
        Js::String(s) => (None, Some(String::from_utf16_lossy(s))),
        _ => (None, None),
    }
}

fn optional_json(value: &Js) -> Result<Option<serde_json::Value>, Thrown> {
    if value.is_undefined() {
        Ok(None)
    } else {
        to_json(value).map(Some)
    }
}

/// Socket ping: one performance-clock sample publishes pingAt (JS field and core) before sending.
fn ping(rt: &Rt, private: bool) -> Result<Js, Thrown> {
    let this = rt.this.clone();
    let disposed = this.get("isDisposed")?.truthy();
    let base = if disposed {
        None
    } else {
        let core = sockets::core_of(&rt.shared.sockets, &this)
            .ok_or_else(|| Thrown::error("Error", "Bitget socket has no core"))?;
        Some(core.base()?)
    };
    let ping_at = Cell::new(0.0);
    let runtime = rt.shared.runtime.clone();
    let now = || {
        if let Some(base) = &base {
            base.mark_ping_sent();
        }
        runtime.monotonic_now_ms()
    };
    let send = |message: &str| -> Result<(), Thrown> {
        this.set("pingAt", Js::Number(ping_at.get()))?;
        let member = if private { "sendWithLog" } else { "send" };
        this.call_method(member, vec![Js::str(message)]).map(|_| ())
    };
    undefined(if private {
        socket_ping::ping_private(disposed, &ping_at, now, send)
    } else {
        socket_ping::ping_public(disposed, &ping_at, now, send)
    })
}

const PREFIX: &str = "safe-cex-venue-bitget/src/";

pub(crate) fn bind(
    rt: &Rt,
    owner: &str,
    name: &str,
    targets: &[&str],
) -> Result<Option<Js>, Thrown> {
    let Some(target) = targets.first().and_then(|t| t.strip_prefix(PREFIX)) else {
        return Ok(None);
    };
    let callable = match (owner, target) {
        ("BitgetExchange", "public_data.rs::fetch_markets") => async_member(name, rt, |rt, _| {
            Box::pin(async move { public_data::fetch_markets(&rt).await })
        }),
        ("BitgetInverseExchange", "public_data.rs::fetch_inverse_markets") => {
            async_member(name, rt, |rt, _| {
                Box::pin(async move { public_data::fetch_inverse_markets(&rt).await })
            })
        }
        ("BitgetExchange", "public_data.rs::fetch_tickers") => async_member(name, rt, |rt, _| {
            Box::pin(async move { public_data::fetch_tickers(&rt).await })
        }),
        ("BitgetExchange", "public_data.rs::fetch_ohlcv") => async_member(name, rt, |rt, a| {
            Box::pin(async move { public_data::fetch_ohlcv(&rt, arg(&a, 0)).await })
        }),
        ("BitgetExchange", "subscriptions.rs::listen_ohlcv") => sync_member(name, rt, |rt, a| {
            let socket = public_data_of(rt)?;
            let options: OHLCVOptions = to_typed(&arg(&a, 0))?;
            let callback = typed_callback::<Candle>(rt, arg(&a, 1), "Bitget listenOHLCV callback");
            Ok(cleanup_function(subscriptions::listen_ohlcv(
                &socket, options, callback,
            )?))
        }),
        ("BitgetExchange", "subscriptions.rs::listen_order_book") => {
            sync_member(name, rt, |rt, a| {
                let socket = public_data_of(rt)?;
                let symbol: String = to_typed(&arg(&a, 0))?;
                let callback =
                    typed_callback::<OrderBook>(rt, arg(&a, 1), "Bitget listenOrderBook callback");
                Ok(cleanup_function(subscriptions::listen_order_book(
                    &socket, symbol, callback,
                )?))
            })
        }
        ("BitgetPublicWebsocket", "ticker.rs::to_finite_number") => {
            sync_member(name, rt, |_, a| {
                let (number, string) = finite_number_input(&arg(&a, 0));
                let input = match (number, &string) {
                    (Some(n), _) => FiniteNumberInput::Number(n),
                    (None, Some(s)) => FiniteNumberInput::String(s),
                    _ => FiniteNumberInput::Other,
                };
                Ok(ticker::to_finite_number(input).map_or(Js::Undefined, Js::Number))
            })
        }
        ("BitgetPublicWebsocket", "ticker.rs::build_ticker_update") => {
            sync_member(name, rt, |_, a| {
                let data = optional_json(&arg(&a, 0))?;
                let update = Js::object(vec![]);
                for (field, value) in ticker::build_ticker_update(data.as_ref())? {
                    update.define(field, Js::Number(value))?;
                }
                Ok(update)
            })
        }
        ("BitgetPublicWebsocket", "ticker.rs::handle_ticker_topic") => {
            sync_member(name, rt, |rt, a| {
                let json = optional_json(&arg(&a, 0))?;
                ticker::handle_ticker_topic(rt.shared.store.as_ref(), json.as_ref())
                    .map_err(ticker_error)?;
                Ok(Js::Undefined)
            })
        }
        ("BitgetPublicWebsocket", "public_data.rs::connect_and_subscribe") => {
            sync_member(name, rt, |rt, _| {
                undefined(public_data::connect_and_subscribe(rt))
            })
        }
        ("BitgetPublicWebsocket", "public_data.rs::on_open") => {
            sync_member(name, rt, |rt, _| undefined(public_data::on_open(rt)))
        }
        // ts: onMarketsChanged = ({ added }) => ...
        ("BitgetPublicWebsocket", "public_data.rs::on_markets_changed") => {
            sync_member(name, rt, |rt, a| {
                let added = arg(&a, 0).get("added")?;
                undefined(public_data::on_markets_changed(rt, added))
            })
        }
        ("BitgetPublicWebsocket", "socket_ping.rs::ping_public") => {
            sync_member(name, rt, |rt, _| ping(rt, false))
        }
        ("BitgetPublicWebsocket", "public_data.rs::subscribe") => {
            sync_member(name, rt, |rt, _| undefined(public_data::subscribe(rt)))
        }
        // ts: onMessage = ({ data }: MessageEvent) => ...
        ("BitgetPublicWebsocket", "socket_runtime.rs::on_public_message") => {
            sync_member(name, rt, |rt, a| {
                let data = arg(&a, 0).get("data")?;
                undefined(socket_runtime::on_public_message(rt, data))
            })
        }
        ("BitgetPrivateWebsocket", "socket_runtime.rs::connect_and_subscribe") => {
            sync_member(name, rt, |rt, _| {
                undefined(socket_runtime::connect_and_subscribe(rt))
            })
        }
        ("BitgetPrivateWebsocket", "socket_ping.rs::ping_private") => {
            sync_member(name, rt, |rt, _| ping(rt, true))
        }
        ("BitgetPrivateWebsocket", "socket_runtime.rs::on_private_message") => {
            sync_member(name, rt, |rt, a| {
                let data = arg(&a, 0).get("data")?;
                undefined(socket_runtime::on_private_message(rt, data))
            })
        }
        ("BitgetPrivateWebsocket", "socket_runtime.rs::execute_position_fetch") => {
            async_member(name, rt, |rt, _| {
                Box::pin(
                    async move { undefined(socket_runtime::execute_position_fetch(&rt).await) },
                )
            })
        }
        ("BitgetPrivateWebsocket", "socket_runtime.rs::execute_balance_fetch") => {
            async_member(name, rt, |rt, _| {
                Box::pin(async move { undefined(socket_runtime::execute_balance_fetch(&rt).await) })
            })
        }
        ("BitgetPrivateWebsocket", "socket_runtime.rs::execute_orders_fetch") => {
            async_member(name, rt, |rt, _| {
                Box::pin(async move { undefined(socket_runtime::execute_orders_fetch(&rt).await) })
            })
        }
        ("BitgetPrivateWebsocket", "socket_runtime.rs::auth") => async_member(name, rt, |rt, _| {
            Box::pin(async move { undefined(socket_runtime::auth(&rt).await) })
        }),
        ("BitgetPrivateWebsocket", "socket_runtime.rs::subscribe") => {
            sync_member(name, rt, |rt, _| undefined(socket_runtime::subscribe(rt)))
        }
        _ => return Ok(None),
    };
    Ok(Some(callable))
}
