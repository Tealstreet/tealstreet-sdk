//! The host-trait implementation the generic ported bodies run on: js_runtime values,
//! promises and module-scope names, with lexical callbacks dispatched to their bodies.
use super::{
    promise::{Promise, all, await_js},
    rt::Rt,
    value::{Js, Thrown, error_object},
};
use crate::core_snapshot_readers::{Operation, SnapshotHost};
use safe_cex_exchange::js_runtime::scope::bound_member;
use std::rc::Rc;

pub(crate) fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn arg(args: &[Js], i: usize) -> Js {
    args.get(i).cloned().unwrap_or(Js::Undefined)
}

/// get-key `v`: the key, then its lodash snake_case and camelCase spellings, by `in`.
pub(crate) fn get_key(obj: &Js, key: &str) -> Result<Js, Thrown> {
    if obj.is_nullish() {
        return Ok(Js::Undefined);
    }
    if !obj.is_object() && !obj.is_array() {
        return Err(Thrown::type_error(&format!(
            "Cannot use 'in' operator to search for '{key}' in {}",
            obj.to_display()
        )));
    }
    for candidate in [
        key.to_owned(),
        safe_cex_num::lodash_snake_case(key),
        safe_cex_num::lodash_camel_case(key),
    ] {
        if obj.has_own(&candidate) {
            return obj.get(&candidate);
        }
    }
    Ok(Js::Undefined)
}

// ts: mergeUpdatesUsingLastUpdate — keep an existing row updated within the cutoff.
fn merge_updates_using_last_update(rt: &Rt, updates: &Js, existing: &Js) -> Result<Js, Thrown> {
    let now = rt.now();
    let existing = existing.array_values().unwrap_or_default();
    let mut merged = vec![];
    for update in updates.array_values().unwrap_or_default() {
        let id = update.get("id")?;
        let found = existing
            .iter()
            .find(|e| e.get("id").is_ok_and(|v| v.strict_equals(&id)))
            .cloned();
        let keep = match &found {
            Some(found) => {
                let last = found.get("lastUpdate")?;
                last.truthy() && now - last.to_number()? <= 1000.
            }
            None => false,
        };
        merged.push(if keep {
            found.expect("kept row")
        } else {
            update
        });
    }
    merged.extend(existing);
    // lodash uniqBy(…, 'id'): first occurrence by SameValueZero.
    let mut out: Vec<Js> = vec![];
    let mut seen: Vec<Js> = vec![];
    for row in merged {
        let id = row.get("id")?;
        if !seen.iter().any(|s| s.same_value_zero(&id)) {
            seen.push(id);
            out.push(row);
        }
    }
    Ok(Js::array(out))
}

// ts: syncUnifiedBalanceUpnlFromPositions — the same balance object unless upnl moved.
fn sync_unified_balance_upnl(
    balance: &Js,
    positions: &Js,
    account_type: &Js,
) -> Result<Js, Thrown> {
    if !balance.truthy() || text(account_type)? != "UNIFIED" {
        return Ok(balance.clone());
    }
    let mut upnl = 0.;
    for position in positions.array_values().unwrap_or_default() {
        let typed: safe_cex_types::Position = super::value::to_typed(&position)?;
        upnl += crate::inverse_symbol::position_upnl_in_quote(&typed);
    }
    if balance.get("upnl")?.strict_equals(&Js::Number(upnl)) {
        return Ok(balance.clone());
    }
    let next = balance.spread_object()?;
    next.set("upnl", Js::Number(upnl))?;
    Ok(next)
}

// ts: loop — sleep the interval (5000ms fallback), then run fn on a zero timer.
fn schedule_loop(rt: &Rt, callback: Js, interval: &Js) -> Result<Js, Thrown> {
    let interval = if interval.is_undefined() {
        0.
    } else {
        interval.to_number()?
    };
    if interval == f64::INFINITY {
        return Ok(Js::Undefined);
    }
    let effective = if interval > 0. {
        interval
    } else {
        rt.shared.runtime.console(
            "warn",
            "[safe-cex] loop() called without a minInterval — defaulting to 5000ms.",
        );
        5000.
    };
    let inner = rt.clone();
    rt.set_timeout(
        Rc::new(move || {
            let callback = callback.clone();
            inner.set_timeout(
                Rc::new(move || callback.call(Js::Undefined, vec![]).map(|_| ())),
                0.,
            );
            Ok(())
        }),
        effective,
    );
    Ok(Js::Undefined)
}

fn dispatch_callback(
    h: Rc<Rt>,
    owner: Js,
    code: &'static str,
    captures: Vec<Js>,
    args: Vec<Js>,
) -> Result<Js, Thrown> {
    if let Some(code) = crate::hyro_private_reads::Callback::from_name(code) {
        return crate::hyro_private_reads::dispatch(h.as_ref(), &owner, code, &captures, args);
    }
    if let Some(code) = crate::hyro_snapshot::Callback::from_name(code) {
        return crate::hyro_snapshot::dispatch(h.as_ref(), code, args);
    }
    if let Some(code) = crate::hyro_order_publication::Callback::from_name(code) {
        return crate::hyro_order_publication::dispatch(h.as_ref(), &owner, code, &captures, args);
    }
    if let Some(code) = crate::inverse_read_surface::Callback::from_name(code) {
        return crate::inverse_read_surface::dispatch(h.as_ref(), code, args);
    }
    if let Some(code) = crate::variant_account_refresh::Callback::from_name(code) {
        return crate::variant_account_refresh::dispatch(h.as_ref(), &owner, code, args);
    }
    if code.starts_with("start_") {
        return crate::primary_startup::start_callback(h, owner, code, args);
    }
    if code == "reconciliation_order_matches" {
        let arg = |v: &[Js]| v.first().cloned().unwrap_or(Js::Undefined);
        return crate::local_order_reconciliation::reconciliation_order_matches(
            h.as_ref(),
            &arg(&captures),
            &arg(&args),
        );
    }
    if code.starts_with("history_") {
        return crate::ledger_history::dispatch(h.as_ref(), &owner, code, &captures, &args);
    }
    crate::core_snapshot_readers::dispatch(h, owner, code, captures, args)
}

impl SnapshotHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn get(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {
        owner.get(key)
    }
    fn optional(&self, owner: &Js, key: &str) -> Result<Js, Thrown> {
        if owner.is_nullish() {
            Ok(Js::Undefined)
        } else {
            owner.get(key)
        }
    }
    fn set(&self, owner: &Js, key: &str, value: Js) -> Result<(), Thrown> {
        owner.set(key, value)
    }
    fn select(&self, owner: Js, key: &str) -> Result<Js, Thrown> {
        if owner.is_nullish() {
            return owner.get(key);
        }
        // Function.prototype.bind, which the shared builtins do not carry.
        if key == "bind" && owner.is_callable() {
            return Ok(Js::function("bind", move |_, a| {
                let target = owner.clone();
                let receiver = arg(&a, 0);
                let leading: Vec<Js> = a.iter().skip(1).cloned().collect();
                Ok(Js::function("bound", move |_, rest| {
                    let mut args = leading.clone();
                    args.extend(rest);
                    target.call(receiver.clone(), args)
                }))
            }));
        }
        // Array.prototype.flat() at its default depth of 1, also absent from the builtins.
        if key == "flat" && owner.is_array() {
            return Ok(Js::function("flat", move |_, _| {
                let mut out = vec![];
                for item in owner
                    .array_items()
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                {
                    match item.array_items() {
                        Some(inner) => out.extend(inner.into_iter().flatten()),
                        None => out.push(item),
                    }
                }
                Ok(Js::array(out))
            }));
        }
        bound_member(&owner, key)
    }
    fn apply(&self, selected: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        if !selected.is_callable() {
            return Err(Thrown::type_error(&format!(
                "{} is not a function",
                selected.to_display()
            )));
        }
        selected.call(Js::Undefined, args)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn literal(&self, s: &str) -> Js {
        Js::str(s)
    }
    fn boolean(&self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn number(&self, n: f64) -> Js {
        Js::Number(n)
    }
    fn numeric(&self, v: &Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        let object = Js::object(fields);
        if let Js::Object(data) = &object {
            *data.proto.borrow_mut() = Some(Js::Builtin(
                safe_cex_num::PrototypeRealm::current()
                    .reference(safe_cex_num::ProtoMember::Prototype),
            ));
        }
        object
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn values(&self, array: &Js) -> Result<Vec<Js>, Thrown> {
        array
            .array_values()
            .ok_or_else(|| Thrown::type_error(&format!("{} is not iterable", array.to_display())))
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn spread(&self, v: Js, fields: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        let out = if v.is_nullish() {
            Js::object(vec![])
        } else {
            v.spread_object()?
        };
        for (k, v) in fields {
            out.define(k, v)?;
        }
        Ok(out)
    }
    fn intrinsic(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        match name {
            "null" => Ok(Js::Null),
            "globalThis" => Ok(self.shared.globals.clone()),
            "Set" | "Map" => self.global(name)?.construct(vec![], None),
            "String" => self.global("String")?.call(Js::Undefined, args),
            "select:aggregateBybitClosedPnl" => {
                Ok(Js::function("aggregateBybitClosedPnl", |_, args| {
                    crate::journal_fold::aggregate_bybit_closed_pnl(
                        &arg(&args, 0),
                        &crate::journal_fold::BybitPositionAdapterDeps(arg(&args, 1)),
                    )
                }))
            }
            "select:mapBybitHistoricalOrder" => {
                Ok(Js::function("mapBybitHistoricalOrder", |_, args| {
                    crate::ledger_mapping::map_bybit_historical_order(&arg(&args, 0))
                }))
            }
            "select:mapBybitTransactionToIncomeRecords" => Ok(Js::function(
                "mapBybitTransactionToIncomeRecords",
                |_, args| {
                    crate::ledger_mapping::map_bybit_transaction_to_income_records(&arg(&args, 0))
                },
            )),
            "select:fetchBybitIncomeHistory" => self.global("fetchBybitIncomeHistory"),
            "select:fetchBybitIncomeHistoryRange" => self.global("fetchBybitIncomeHistoryRange"),
            "select:splitTimeRange" => self.global("splitTimeRange"),
            "select:sleep" => self.global("sleep"),
            "get-key" => arg(&args, 0).get_property(&arg(&args, 1)),
            "spreadArgs" => Ok(Js::array(arg(&args, 0).iterable_values()?)),
            "add" => safe_cex_exchange::js_runtime::ops::add(&arg(&args, 0), &arg(&args, 1)),
            "less" => Ok(Js::Bool(safe_cex_exchange::js_runtime::ops::less_than(
                &arg(&args, 0),
                &arg(&args, 1),
            )?)),
            "select:mapBybitClosedPnl" => Ok(Js::function("mapBybitClosedPnl", |_, args| {
                crate::ledger_mapping::map_bybit_closed_pnl(
                    &arg(&args, 0),
                    arg(&args, 1),
                    arg(&args, 2).truthy(),
                )
            })),
            "isBybitOptionSymbol" => Ok(Js::Bool(crate::options::is_bybit_option_symbol(&text(
                &arg(&args, 0),
            )?))),
            "isBybitInverseSymbol" => Ok(Js::Bool(crate::inverse_symbol::is_bybit_inverse_symbol(
                &text(&arg(&args, 0))?,
            ))),
            "omitUndefined" => self.global("omitUndefined")?.call(Js::Undefined, args),
            "subtract" => Ok(Js::Number(
                arg(&args, 0).to_number()? - arg(&args, 1).to_number()?,
            )),
            "iterator" => arg(&args, 0).iterator(),
            "iterator.next" => {
                let step = arg(&args, 0).call_method("next", vec![])?;
                if !step.is_object() {
                    return Err(Thrown::type_error("Iterator result is not an object"));
                }
                Ok(step)
            }
            "iterator.close" => {
                let iterator = arg(&args, 0);
                let close = iterator.get("return")?;
                if !close.is_nullish() {
                    let result = close.call(iterator, vec![])?;
                    if !result.is_object() {
                        return Err(Thrown::type_error("Iterator result is not an object"));
                    }
                }
                Ok(Js::Undefined)
            }
            "template" | "string.concat" => {
                let mut units = vec![];
                for a in &args {
                    units.extend_from_slice(&a.to_js_string()?);
                }
                Ok(Js::utf16(&units))
            }
            "iterator.bindings" => {
                let count = arg(&args, 1).to_number()? as usize;
                let mut values = self.values(&arg(&args, 0))?;
                values.resize(count, Js::Undefined);
                Ok(Js::array(values))
            }
            "Date.now" => Ok(Js::Number(self.now().floor())),
            "virtualClock.getCurrentTimeMs" => self
                .global("virtualClock")?
                .call_method("getCurrentTimeMs", vec![]),
            "LogSeverity.Warning" => Ok(Js::str("warning")),
            "v" => get_key(&arg(&args, 0), &text(&arg(&args, 1))?),
            "Math.min" => {
                let mut least = f64::INFINITY;
                for a in &args {
                    least = safe_cex_num::math_min(least, a.to_number()?);
                }
                Ok(Js::Number(least))
            }
            "Promise.all" => {
                let values = self.values(&arg(&args, 0))?;
                Ok(Js::Promise(all(&self.exchange_runtime(), values)))
            }
            "mapSeries" => self.global("mapSeries")?.call(Js::Undefined, args),
            "loop" => schedule_loop(self, arg(&args, 0), &arg(&args, 1)),
            "mergeUpdatesUsingLastUpdate" => {
                merge_updates_using_last_update(self, &arg(&args, 0), &arg(&args, 1))
            }
            "syncUnifiedBalanceUpnlFromPositions" => {
                sync_unified_balance_upnl(&arg(&args, 0), &arg(&args, 1), &arg(&args, 2))
            }
            other => Err(Thrown::error(
                "Error",
                &format!("Bybit binding has no intrinsic {other}"),
            )),
        }
    }
    fn callback(&self, owner: Js, code: &'static str, captures: Vec<Js>) -> Result<Js, Thrown> {
        let h = Rc::new(self.clone());
        Ok(Js::function(code, move |_, args| {
            dispatch_callback(h.clone(), owner.clone(), code, captures.clone(), args)
        }))
    }
    fn launch(&self, body: Operation<Js, Thrown>) -> Js {
        Js::Promise(Promise::spawn(&self.exchange_runtime(), body))
    }
    fn wait(&self, value: Js) -> Operation<Js, Thrown> {
        Box::pin(await_js(&self.exchange_runtime(), value))
    }
    fn error(&self, message: Js) -> Result<Thrown, Thrown> {
        let message = if message.is_undefined() {
            String::new()
        } else {
            text(&message)?
        };
        Ok(Thrown::new(error_object("Error", &message)))
    }
    fn error_value(&self, error: Thrown) -> Js {
        error.value
    }
}
