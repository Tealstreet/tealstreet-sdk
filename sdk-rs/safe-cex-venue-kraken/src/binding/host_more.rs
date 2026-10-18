//! Private-account, income-paging and OHLCV-polling hosts on the same receiver scope.
use super::{
    host::{h, iso, lossy, units},
    promise::{Promise, await_js},
    rt::Rt,
    value::{Js, Thrown, from_typed, to_typed},
};
use crate::{
    ohlcv_polling::{PollCallback, PollFuture, PollHost, Slot},
    private_accounts::{AccountHost, IncomeHost, PageCallback},
    rest_account::{Handle, ReadResult},
};
use safe_cex_exchange::income_paging::{
    IncomePageFetcher, IncomePageParams, IncomeRangeOptions, paginate_income_range,
};
use safe_cex_types::IncomeRecord;
use std::{future::Future, rc::Rc};

/// The exchange that owns the preference caches: the receiver itself, or a socket's parent.
pub(crate) fn owner(rt: &Rt) -> Result<Js, Thrown> {
    if rt.this.has_own("parent") {
        rt.this.get("parent")
    } else {
        Ok(rt.this.clone())
    }
}

fn optional_map(rt: &Rt, cache: &str) -> Result<Js, Thrown> {
    owner(rt)?.get(if cache == "margin" {
        "marginTypePreferences"
    } else {
        "leverageSettings"
    })
}

fn store(rt: &Rt) -> Result<Js, Thrown> {
    rt.this.get("store")
}

impl AccountHost for Rt {
    // ts: binary `+` (ToPrimitive, then string concatenation or numeric addition).
    fn add(&self, left: &Js, right: &Js) -> ReadResult<Self, Handle<Self>> {
        let primitive = |v: &Js| -> Result<Js, Thrown> {
            Ok(if v.is_object() {
                Js::utf16(&v.to_js_string()?)
            } else {
                v.clone()
            })
        };
        let (a, b) = (primitive(left)?, primitive(right)?);
        if a.is_string() || b.is_string() {
            let mut out = a.to_js_string()?.to_vec();
            out.extend_from_slice(&b.to_js_string()?);
            return Ok(h(Js::utf16(&out)));
        }
        Ok(h(Js::Number(a.to_number()? + b.to_number()?)))
    }
    fn to_string_method(&self, value: &Js) -> ReadResult<Self, Handle<Self>> {
        Ok(h(value.call_method("toString", vec![])?))
    }
    // ts: `new Date(value).getTime()`.
    fn date_milliseconds(&self, value: &Js) -> ReadResult<Self, f64> {
        let n = match value {
            Js::String(s) => safe_cex_num::js_date_parse(&lossy(s), |n| n),
            other => other.to_number()?,
        };
        Ok(if !n.is_finite() || n.abs() > 8_640_000_000_000_000.0 {
            f64::NAN
        } else {
            n.trunc() + 0.0
        })
    }
    // ts: utils/createPositionId(symbol, side) with no mode: `${symbol}:${side}`.
    fn position_id(&self, symbol: &Js, side: &str) -> ReadResult<Self, Handle<Self>> {
        let mut out = symbol.to_js_string()?.to_vec();
        out.extend(units(":"));
        out.extend(units(side));
        Ok(h(Js::utf16(&out)))
    }
    // ts: `(this.parent as any).marginTypePreferences?.get(symbol)`.
    fn optional_preference(&self, cache: &str, symbol: &Js) -> ReadResult<Self, Handle<Self>> {
        let map = optional_map(self, cache)?;
        if map.is_nullish() {
            return Ok(h(Js::Undefined));
        }
        Ok(h(map.call_method("get", vec![symbol.clone()])?))
    }
    fn optional_set_margin(
        &self,
        symbol: Handle<Self>,
        margin: Handle<Self>,
    ) -> ReadResult<Self, ()> {
        let map = optional_map(self, "margin")?;
        if !map.is_nullish() {
            map.call_method("set", vec![(*symbol).clone(), (*margin).clone()])?;
        }
        Ok(())
    }
    fn update_order(&self, selector: Handle<Self>, patch: Handle<Self>) -> ReadResult<Self, ()> {
        store(self)?.call_method("updateOrder", vec![(*selector).clone(), (*patch).clone()])?;
        Ok(())
    }
    fn order_status(&self, selector: Handle<Self>, status: Handle<Self>) -> ReadResult<Self, ()> {
        store(self)?.call_method(
            "updateOrderStatus",
            vec![(*selector).clone(), (*status).clone()],
        )?;
        Ok(())
    }
    fn add_order(&self, order: Handle<Self>) -> ReadResult<Self, ()> {
        store(self)?.call_method("addOrUpdateOrder", vec![(*order).clone()])?;
        Ok(())
    }
    fn full_orders(&self, orders: Handle<Self>) -> ReadResult<Self, ()> {
        store(self)?.call_method("addOrUpdateOrdersFull", vec![(*orders).clone()])?;
        Ok(())
    }
    fn full_positions(&self, positions: Handle<Self>) -> ReadResult<Self, ()> {
        store(self)?.call_method("addOrUpdatePositionsFull", vec![(*positions).clone()])?;
        Ok(())
    }
    // ts: `this.parent.emitter.emit('fill', payload)`.
    fn fill(&self, payload: Handle<Self>) -> ReadResult<Self, ()> {
        owner(self)?
            .get("emitter")?
            .call_method("emit", vec![Js::str("fill"), (*payload).clone()])?;
        Ok(())
    }
    fn throttle(&self) -> ReadResult<Self, ()> {
        self.this.call_method("throttledFetchOrders", vec![])?;
        Ok(())
    }
    fn parse_order(&self, item: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        Ok(h(self
            .this
            .call_method("parseOrder", vec![(*item).clone()])?))
    }
    fn null(&self) -> Handle<Self> {
        h(Js::Null)
    }
}

fn number(v: &Js, key: &str) -> Result<f64, Thrown> {
    v.get(key)?.to_number()
}

impl IncomeHost for Rt {
    fn date_iso(&self, value: &Js) -> ReadResult<Self, Vec<u16>> {
        Ok(units(&iso(self.date_milliseconds(value)?)?))
    }
    // ts: utils/income-paging paginateIncomeRange(fetchPage, options).
    fn paginate<'a>(
        &'a self,
        options: Handle<Self>,
        page: PageCallback<'a, Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>> + 'a> {
        let opts = IncomeRangeOptions {
            start_time: number(&options, "startTime")?,
            end_time: number(&options, "endTime")?,
            chunk_ms: number(&options, "chunkMs")?,
            page_limit: number(&options, "pageLimit")?,
            chunk_delay_ms: None,
            max_pages_per_chunk: None,
        };
        let runtime = self.exchange_runtime();
        Ok(async move {
            let mut fetch: Box<IncomePageFetcher<'a>> = Box::new(move |p: IncomePageParams| {
                let params = Js::object(vec![
                    ("startTime", Js::Number(p.start_time)),
                    ("endTime", Js::Number(p.end_time)),
                    ("limit", Js::Number(p.limit)),
                ]);
                let future = page(h(params));
                Box::pin(async move {
                    let records = future?.await?;
                    let typed: Vec<IncomeRecord> = to_typed(&records)?;
                    Ok(typed)
                })
            });
            let mut delay = move |ms: f64| runtime.sleep(if ms > 0.0 { ms as u64 } else { 0 });
            let result = paginate_income_range(&mut *fetch, opts, &mut delay).await?;
            Ok(h(Js::object(vec![
                ("records", from_typed(&result.records)?),
                ("truncated", Js::Bool(result.truncated)),
                ("exhaustedChunks", Js::Number(result.exhausted_chunks)),
                ("requests", Js::Number(result.requests)),
            ])))
        })
    }
    fn account_log_page(
        &self,
        params: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let promise = self
            .this
            .call_method("_fetchAccountLogPage", vec![(*params).clone()])?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })
    }
    fn income_range(
        &self,
        start: Handle<Self>,
        end: Handle<Self>,
        income_type: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let promise = self.this.call_method(
            "fetchIncomeHistoryRange",
            vec![(*start).clone(), (*end).clone(), (*income_type).clone()],
        )?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })
    }
    fn warning(&self, message: Handle<Self>) -> ReadResult<Self, ()> {
        self.this
            .call_method("log", vec![(*message).clone(), Js::str("warning")])?;
        Ok(())
    }
    fn income_error(&self, message: Handle<Self>) -> ReadResult<Self, ()> {
        self.this
            .call_method("emitAndLogError", vec![(*message).clone()])?;
        Ok(())
    }
}

fn slot_map(rt: &Rt, slot: Slot) -> Result<Js, Thrown> {
    rt.this.get(match slot {
        Slot::Timeout => "ohlcvPollingTimeouts",
        Slot::Active => "ohlcvPollingActive",
        Slot::Candle => "lastCandleCache",
    })
}

impl PollHost for Rt {
    fn map_has(&self, slot: Slot, key: &[u16]) -> ReadResult<Self, bool> {
        Ok(slot_map(self, slot)?
            .call_method("has", vec![Js::utf16(key)])?
            .truthy())
    }
    fn map_get(&self, slot: Slot, key: &[u16]) -> ReadResult<Self, Handle<Self>> {
        Ok(h(
            slot_map(self, slot)?.call_method("get", vec![Js::utf16(key)])?
        ))
    }
    fn map_set(&self, slot: Slot, key: &[u16], value: Handle<Self>) -> ReadResult<Self, ()> {
        slot_map(self, slot)?.call_method("set", vec![Js::utf16(key), (*value).clone()])?;
        Ok(())
    }
    fn map_delete(&self, slot: Slot, key: &[u16]) -> ReadResult<Self, ()> {
        slot_map(self, slot)?.call_method("delete", vec![Js::utf16(key)])?;
        Ok(())
    }
    fn clear_timeout(&self, timer: Handle<Self>) -> ReadResult<Self, ()> {
        Rt::clear_timeout(self, &timer);
        Ok(())
    }
    fn set_timeout(
        &self,
        callback: PollCallback<Self>,
        delay: f64,
    ) -> ReadResult<Self, Handle<Self>> {
        let rt = self.clone();
        let fire: Rc<dyn Fn() -> Result<(), Thrown>> = Rc::new(move || {
            PollHost::spawn(&rt, callback());
            Ok(())
        });
        Ok(h(Rt::set_timeout(self, fire, delay)))
    }
    fn fetch_candles(
        &self,
        options: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let promise = self
            .this
            .call_method("fetchOHLCV", vec![(*options).clone()])?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })
    }
    // ts: `console.error(prefix, err)`.
    fn console_error(&self, prefix: &'static str, error: Thrown) -> ReadResult<Self, ()> {
        self.shared
            .runtime
            .console_values("error", &[Js::str(prefix), error.value]);
        Ok(())
    }
    fn spawn(&self, future: PollFuture<Self>) {
        let rt = self.clone();
        let promise = Promise::spawn(
            &self.exchange_runtime(),
            Box::pin(async move {
                if let Err(error) = future.await {
                    rt.report(&error, "kraken ohlcv poll");
                }
                Ok(Js::Undefined)
            }),
        );
        drop(promise);
    }
}
