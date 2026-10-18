use super::rt::{Host, optional, rows};
use crate::position_control::{OrderSideHost, PositionQueryHost};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_typed};
use safe_cex_types::PositionMode;
impl PositionQueryHost for Host {
    type Value = Js;
    type Response = Js;
    type Rows = Js;
    type Record = Js;
    type Position = Js;
    type Mapper = Js;
    type Error = Thrown;
    fn ends_usdc(&mut self, s: &Js) -> Result<bool, Thrown> {
        Ok(s.call_method("endsWith", vec![Js::str("USDC")])?.truthy())
    }
    fn format_out(&mut self, s: &Js) -> Result<Js, Thrown> {
        self.call("formatSymbolOut", vec![s.clone()])
    }
    async fn get(&mut self, e: &'static str, c: &'static str, s: Js) -> Result<Js, Thrown> {
        self.http(
            "xhr",
            "get",
            e,
            vec![Js::object(vec![(
                "params",
                Js::object(vec![("currency", Js::str(c)), ("symbol", s)]),
            )])],
        )
        .await
    }
    fn response_data(&mut self, r: Js) -> Result<Option<Js>, Thrown> {
        let p = optional(&optional(&optional(&r, "data")?, "data")?, "positions")?;
        Ok(if p.is_array() { Some(p) } else { None })
    }
    fn length(&mut self, r: &Js) -> Result<usize, Thrown> {
        Ok(r.get("length")?.to_number()? as usize)
    }
    fn row(&mut self, r: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        let v = r.get(&i.to_string())?;
        Ok(if v.is_nullish() { None } else { Some(v) })
    }
    fn bind_mapper(&mut self) -> Result<Js, Thrown> {
        Host::get(self, "mapPosition")
    }
    fn map_position(&mut self, m: &Js, r: Js) -> Result<Option<Js>, Thrown> {
        let v = m.call(self.rt.this.clone(), vec![r])?;
        Ok(if v.truthy() { Some(v) } else { None })
    }
    fn formatted_row_symbol(&mut self, r: Option<&Js>) -> Result<Js, Thrown> {
        self.call(
            "formatSymbolIn",
            vec![optional(r.unwrap_or(&Js::Undefined), "symbol")?],
        )
    }
    fn same_symbol(&mut self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        Ok(a.strict_equals(b))
    }
    fn mode_is(&mut self, r: &Js, m: &'static str) -> Result<bool, Thrown> {
        Ok(r.get("posMode")?.strict_equals(&Js::str(m)))
    }
    fn report(&mut self, e: &Thrown) -> Result<(), Thrown> {
        Host::report(self, e)
    }
}
impl OrderSideHost for Host {
    type Options = Js;
    type Value = Js;
    type Positions = Js;
    type Error = Thrown;
    fn info_side(&mut self, o: &Js, opt: bool) -> Result<Js, Thrown> {
        let info = o.get("info")?;
        if opt {
            optional(&info, "posSide")
        } else {
            info.get("posSide")
        }
    }
    fn truthy(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.truthy())
    }
    fn symbol(&mut self, o: &Js) -> Result<Js, Thrown> {
        o.get("symbol")
    }
    async fn positions(&mut self, s: Js) -> Result<Js, Thrown> {
        self.invoke("getOrFetchPositionsForSymbol", vec![s]).await
    }
    fn length(&mut self, p: &Js) -> Result<usize, Thrown> {
        Ok(rows(p)?.len())
    }
    fn position_oneway(&mut self, p: &Js, i: usize) -> Result<Option<bool>, Thrown> {
        let p = p.get(&i.to_string())?;
        Ok(if p.is_nullish() {
            None
        } else {
            Some(p.get("positionMode")?.strict_equals(&Js::str("oneway")))
        })
    }
    async fn raw_mode(&mut self, s: Js) -> Result<Option<PositionMode>, Thrown> {
        crate::position_control::fetch_raw_position_mode(self, s).await
    }
    fn is_buy(&mut self, o: &Js) -> Result<bool, Thrown> {
        Ok(o.get("side")?.strict_equals(&Js::str("buy")))
    }
    fn is_type(&mut self, o: &Js, k: &'static str) -> Result<bool, Thrown> {
        Ok(o.get("type")?.strict_equals(&Js::str(k)))
    }
    fn reduce_only(&mut self, o: &Js) -> Result<bool, Thrown> {
        Ok(o.get("reduceOnly")?.truthy())
    }
    fn value(&mut self, s: &'static str) -> Result<Js, Thrown> {
        Ok(Js::str(s))
    }
}
pub(crate) async fn raw_mode(h: &mut Host, s: Js) -> Result<Js, Thrown> {
    match crate::position_control::fetch_raw_position_mode(h, s).await? {
        Some(m) => from_typed(&m),
        None => Ok(Js::Undefined),
    }
}
