use super::rt::{Host, optional, rows};
use crate::rest_data::DataHost;
use safe_cex_exchange::js_runtime::{
    promise::{JsFuture, await_js},
    value::{Js, Thrown},
};
impl DataHost for Host {
    type V = Js;
    type E = Thrown;
    type Call = Js;
    type Read = JsFuture<'static>;
    fn get(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn optional(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        optional(v, k)
    }
    fn key(&mut self, v: &Js, k: &str) -> Result<Js, Thrown> {
        self.global("v", vec![v.clone(), Js::str(k)])
    }
    fn truthy(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.truthy())
    }
    fn equals(&mut self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        Ok(a.strict_equals(b))
    }
    fn is_text(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(matches!(v, Js::String(_)))
    }
    fn coerce(&mut self, v: Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn number_value(&mut self, v: &Js) -> Result<Option<f64>, Thrown> {
        Ok(if let Js::Number(n) = v {
            Some(*n)
        } else {
            None
        })
    }
    fn parse(&mut self, v: Js) -> Result<f64, Thrown> {
        v.parse_float()
    }
    fn text(&mut self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn upper(&mut self, v: Vec<u16>) -> Result<Vec<u16>, Thrown> {
        Ok(Js::utf16(&v)
            .call_method("toUpperCase", vec![])?
            .to_js_string()?
            .to_vec())
    }
    fn lower(&mut self, v: Vec<u16>) -> Result<Vec<u16>, Thrown> {
        Ok(Js::utf16(&v)
            .call_method("toLowerCase", vec![])?
            .to_js_string()?
            .to_vec())
    }
    fn interval_text(&mut self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.call_method("toString", vec![])?.to_js_string()?.to_vec())
    }
    fn new_text(&mut self, v: Vec<u16>) -> Js {
        Js::utf16(&v)
    }
    fn num(&mut self, n: f64) -> Js {
        Js::Number(n)
    }
    fn boolean(&mut self, b: bool) -> Js {
        Js::Bool(b)
    }
    fn undefined(&mut self) -> Js {
        Js::Undefined
    }
    fn array(&mut self, v: Vec<Js>) -> Result<Js, Thrown> {
        Ok(Js::array(v))
    }
    fn object(&mut self, v: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        Ok(Js::object(v))
    }
    fn spread(&mut self, v: &Js, fields: Vec<(&'static str, Js)>) -> Result<Js, Thrown> {
        let out = v.spread_object()?;
        for (k, v) in fields {
            out.define(k, v)?;
        }
        Ok(out)
    }
    fn rows(&mut self, v: Js) -> Result<Vec<Js>, Thrown> {
        rows(&v)
    }
    fn is_array(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.is_array())
    }
    fn length(&mut self, v: &Js) -> Result<usize, Thrown> {
        Ok(v.get("length")?.to_number()? as usize)
    }
    fn at(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn store(&mut self, k: &str) -> Result<Js, Thrown> {
        Host::store(self, k)
    }
    fn cached(&mut self, table: &str, symbol: &Js) -> Result<Js, Thrown> {
        Host::store(self, table)?.get_key(&symbol.to_js_string()?)
    }
    fn format_in(&mut self, symbol: Js) -> Result<Js, Thrown> {
        self.call("formatSymbolIn", vec![symbol])
    }
    fn format_out(&mut self, symbol: Js) -> Result<Js, Thrown> {
        self.call("formatSymbolOut", vec![symbol])
    }
    fn table(&mut self, table: &str, key: Js) -> Result<Js, Thrown> {
        let table = if table == "timeframes" {
            Host::get(self, table)?
        } else {
            super::methods::table(table)?
        };
        table.get_key(&key.to_js_string()?)
    }
    fn position_id(&mut self, symbol: &Js, side: &str, mode: &str) -> Result<Js, Thrown> {
        self.global(
            "createPositionId",
            vec![symbol.clone(), Js::str(side), Js::str(mode)],
        )
    }
    fn price_diff(&mut self, entry: f64, last: f64, side: &str) -> Result<f64, Thrown> {
        Ok(if side == "long" {
            safe_cex_num::subtract(last, entry)
        } else {
            safe_cex_num::subtract(entry, last)
        })
    }
    fn map_position(&mut self, row: Js) -> Result<Js, Thrown> {
        crate::rest_data::map_position(self, row)
    }
    async fn account_positions(&mut self) -> Result<Js, Thrown> {
        self.invoke("fetchPositionsAndBalance", vec![]).await
    }
    async fn symbol_positions(&mut self, s: Js) -> Result<Js, Thrown> {
        self.invoke("fetchPositionForSymbol", vec![s]).await
    }
    fn flatten(&mut self, arrays: Vec<Js>) -> Result<Js, Thrown> {
        let mut out = vec![];
        for a in arrays {
            if a.is_array() {
                out.extend(rows(&a)?)
            } else {
                out.push(a)
            }
        }
        Ok(Js::array(out))
    }
    fn read_target(&mut self) -> Result<Js, Thrown> {
        Host::get(self, "xhr")
    }
    fn read(
        &mut self,
        c: Js,
        e: &'static str,
        p: Vec<(&'static str, Js)>,
    ) -> Result<JsFuture<'static>, Thrown> {
        let result = c.call_method(
            "get",
            vec![Js::str(e), Js::object(vec![("params", Js::object(p))])],
        )?;
        Ok(Box::pin(await_js(result)))
    }
    fn error_value(&mut self, e: &Thrown) -> Js {
        e.value.clone()
    }
    fn error(&mut self, m: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::error("Error", &super::rt::text(&m)?))
    }
    fn emit(&mut self, m: Js) -> Result<(), Thrown> {
        Host::emit(self, m)
    }
    fn emit_candle_error(&mut self, m: Js) -> Result<(), Thrown> {
        Host::get(self, "emitter")?.call_method("emit", vec![Js::str("error"), m])?;
        Ok(())
    }
    fn now(&mut self) -> Result<Js, Thrown> {
        self.global("virtualClock.getCurrentTimeMs", vec![])
    }
    fn includes(&mut self, v: &Js, n: &str) -> Result<bool, Thrown> {
        Ok(v.call_method("includes", vec![Js::str(n)])?.truthy())
    }
    fn nullish(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.is_nullish())
    }
    fn public_only(&mut self) -> Result<bool, Thrown> {
        Ok(Host::get(self, "isPublicDataOnly")?.truthy())
    }
    fn date_seconds(&mut self, v: Js) -> Result<Option<f64>, Thrown> {
        // Source handles numeric seconds/milliseconds/nanoseconds, numeric strings,
        // and then dayjs date strings; preserve each branch before date parsing.
        super::methods::date_seconds(self, v)
    }
    fn end_time(&mut self, to: Js, daily: bool) -> Result<Js, Thrown> {
        let date = self.global("dayjs", if to.truthy() { vec![to] } else { vec![] })?;
        if daily {
            date.call_method("startOf", vec![Js::str("day")])?
                .call_method("subtract", vec![Js::Number(1.0), Js::str("day")])
        } else {
            Ok(date)
        }
    }
    fn start_time(
        &mut self,
        end: &Js,
        from: Js,
        use_from: bool,
        amount: f64,
        unit: Vec<u16>,
    ) -> Result<Js, Thrown> {
        if use_from {
            self.global("dayjs", vec![from])
        } else {
            end.call_method("subtract", vec![Js::Number(amount), Js::utf16(&unit)])
        }
    }
    fn milliseconds(&mut self, d: &Js) -> Result<f64, Thrown> {
        d.call_method("valueOf", vec![])?.to_number()
    }
    fn round_usd(&mut self, n: f64) -> Result<f64, Thrown> {
        self.global("roundUSD", vec![Js::Number(n)])?.to_number()
    }
}
