use super::rt::{Host, rows};
use crate::rest_order_control::{CancellationHost, MarketLookupHost};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
impl CancellationHost for Host {
    type Input = Js;
    type Order = Js;
    type Value = Js;
    type Reply = Js;
    type Call = Js;
    type Error = Thrown;
    fn input_length(&mut self, v: &Js) -> Result<usize, Thrown> {
        Ok(rows(v)?.len())
    }
    fn input_at(&mut self, v: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        Ok(v.array_items()?
            .ok_or_else(|| Thrown::type_error("orders.map is not a function"))?
            .get(i)
            .cloned()
            .flatten())
    }
    fn spread(&mut self, v: &Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn id(&mut self, v: &Js) -> Result<Vec<u16>, Thrown> {
        let id = v.get("id")?;
        if !matches!(id, Js::String(_)) {
            return Err(Thrown::type_error("o.id.split is not a function"));
        }
        Ok(id.to_js_string()?.to_vec())
    }
    fn set_id(&mut self, v: &mut Js, id: Vec<u16>) -> Result<(), Thrown> {
        v.define("id", Js::utf16(&id))
    }
    fn missing_order(&mut self) -> Result<Js, Thrown> {
        Ok(Js::Undefined)
    }
    async fn position_side(&mut self, o: &Js) -> Result<Js, Thrown> {
        self.invoke("getOrderPositionSide", vec![o.clone()]).await
    }
    fn with_side(&mut self, o: &Js, s: Js) -> Result<Js, Thrown> {
        let c = o.spread_object()?;
        c.define("posSide", s)?;
        Ok(c)
    }
    fn side(&mut self, o: &Js) -> Result<Js, Thrown> {
        o.get("posSide")
    }
    fn symbol(&mut self, o: &Js) -> Result<Js, Thrown> {
        o.get("symbol")
    }
    fn value_text(&mut self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn format(&mut self, s: Js) -> Result<Js, Thrown> {
        self.call("formatSymbolOut", vec![s])
    }
    fn format_value_ref(&mut self, s: &Js) -> Result<Js, Thrown> {
        self.call("formatSymbolOut", vec![s.clone()])
    }
    fn clone_value_ref(&mut self, v: &Js) -> Result<Js, Thrown> {
        Ok(v.clone())
    }
    fn delete_target(&mut self) -> Result<Js, Thrown> {
        self.get("unlimitedXHR")
    }
    async fn delete(
        &mut self,
        c: Js,
        e: &'static str,
        s: Js,
        side: Js,
        id: Vec<u16>,
    ) -> Result<Js, Thrown> {
        safe_cex_exchange::js_runtime::promise::await_js(
            &self.rt.exchange_runtime(),
            c.call_method(
                "delete",
                vec![
                    Js::str(e),
                    Js::object(vec![(
                        "params",
                        Js::object(vec![
                            ("symbol", s),
                            ("posSide", side),
                            ("orderID", Js::utf16(&id)),
                        ]),
                    )]),
                ],
            )?,
        )
        .await
    }
    fn flatten(&mut self, replies: Vec<Js>, _batch: bool) -> Result<Vec<Js>, Thrown> {
        // Source flattens arrays of Axios replies, never the envelope's order
        // rows. Both cancellation routes validate response.data.code/msg.
        replies.into_iter().map(|r| r.get("data")).collect()
    }
    fn zero_code(&mut self, r: &Js) -> Result<bool, Thrown> {
        Ok(r.get("code")?.strict_equals(&Js::Number(0.0)))
    }
    fn message(&mut self, r: &Js) -> Result<Js, Thrown> {
        r.get("msg")
    }
    fn error(&mut self, m: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::error("Error", &super::rt::text(&m)?))
    }
    fn status(&mut self, ids: Vec<Option<Vec<u16>>>, s: &'static str) -> Result<(), Thrown> {
        self.get("store")?.call_method(
            "updateOrdersStatus",
            vec![
                Js::array(
                    ids.into_iter()
                        .map(|id| {
                            id.map_or(Js::Undefined, |id| Js::object(vec![("id", Js::utf16(&id))]))
                        })
                        .collect(),
                ),
                Js::str(s),
            ],
        )?;
        Ok(())
    }
    fn report(&mut self, e: &Thrown) -> Result<(), Thrown> {
        Host::report(self, e)
    }
}
impl MarketLookupHost for Host {
    type Symbol = Js;
    type Market = Js;
    type Markets = Js;
    type Loaded = Js;
    type Error = Thrown;
    fn cached(&mut self, s: &Js) -> Result<Option<Js>, Thrown> {
        let v = self.store("marketsBySymbol")?.get_key(&s.to_js_string()?)?;
        Ok(v.truthy().then_some(v))
    }
    async fn fetch(&mut self) -> Result<Js, Thrown> {
        self.invoke("fetchMarkets", vec![]).await
    }
    fn disposed(&mut self) -> Result<bool, Thrown> {
        Ok(self.get("isDisposed")?.truthy())
    }
    fn is_array(&mut self, m: &Js) -> Result<bool, Thrown> {
        Ok(m.is_array())
    }
    fn length(&mut self, m: &Js) -> Result<usize, Thrown> {
        Ok(rows(m)?.len())
    }
    fn loaded(&mut self) -> Result<Js, Thrown> {
        self.store("loaded")
    }
    fn update(&mut self, m: &Js, l: Js, flag: (&'static str, bool)) -> Result<(), Thrown> {
        let l = l.spread_object()?;
        l.define(flag.0, Js::Bool(flag.1))?;
        self.get("store")?.call_method(
            "update",
            vec![Js::object(vec![("markets", m.clone()), ("loaded", l)])],
        )?;
        Ok(())
    }
    fn find(&mut self, m: &Js, s: &Js) -> Result<Option<Js>, Thrown> {
        for row in rows(m)? {
            if row.get("symbol")?.strict_equals(s) {
                return Ok(Some(row));
            }
        }
        Ok(None)
    }
}
