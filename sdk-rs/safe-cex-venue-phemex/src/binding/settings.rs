use super::rt::{Host, optional, rows};
use crate::rest_account_control::{ContextHost, LeverageContext, SettingsHost};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, Thrown},
};
impl ContextHost for Host {
    type Symbol = Js;
    type Row = Js;
    type Rows = Js;
    type Reply = Js;
    type Call = Js;
    type Value = Js;
    type Error = Thrown;
    fn ends_usdc(&mut self, s: &Js) -> Result<bool, Thrown> {
        Ok(!s.is_nullish() && s.call_method("endsWith", vec![Js::str("USDC")])?.truthy())
    }
    fn format_out(&mut self, s: &Js) -> Result<Js, Thrown> {
        self.call("formatSymbolOut", vec![s.clone()])
    }
    fn get_target(&mut self) -> Result<Js, Thrown> {
        Host::get(self, "xhr")
    }
    async fn get(
        &mut self,
        c: Js,
        e: &'static str,
        currency: &'static str,
        s: Js,
    ) -> Result<Js, Thrown> {
        await_js(c.call_method(
            "get",
            vec![
                Js::str(e),
                Js::object(vec![(
                    "params",
                    Js::object(vec![("currency", Js::str(currency)), ("symbol", s)]),
                )]),
            ],
        )?)
        .await
    }
    fn raw_rows(&mut self, r: Js) -> Result<Js, Thrown> {
        let p = optional(&r.get("data")?.get("data")?, "positions")?;
        Ok(if p.truthy() { p } else { Js::array(vec![]) })
    }
    fn stored_rows(&mut self) -> Result<Js, Thrown> {
        self.store("positions")
    }
    fn length(&mut self, r: &Js) -> Result<usize, Thrown> {
        Ok(rows(r)?.len())
    }
    fn row(&mut self, r: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        Ok(r.array_items()
            .ok_or_else(|| Thrown::type_error("positions.filter is not a function"))?
            .get(i)
            .cloned()
            .flatten())
    }
    fn matches(&mut self, r: &Js, s: &Js, formatted: bool) -> Result<bool, Thrown> {
        let mut a = r.get("symbol")?;
        if formatted {
            a = self.call("formatSymbolIn", vec![a])?;
        }
        Ok(a.strict_equals(s))
    }
    fn side_is(&mut self, r: &Js, k: &'static str, s: &'static str) -> Result<bool, Thrown> {
        Ok(self
            .global("v", vec![r.clone(), Js::str(k)])?
            .strict_equals(&Js::str(s)))
    }
    fn stored_margin(&mut self, r: &Js) -> Result<Option<Js>, Thrown> {
        let v = r.get("marginType")?;
        Ok((!v.is_undefined()).then_some(v))
    }
    fn stored_mode(&mut self, r: &Js) -> Result<Option<Js>, Thrown> {
        let v = r.get("positionMode")?;
        Ok((!v.is_undefined()).then_some(v))
    }
    fn stored_leverage(&mut self, r: &Js) -> Result<f64, Thrown> {
        r.get("leverage")?.to_number()
    }
    fn raw_leverage(&mut self, r: &Js) -> Result<f64, Thrown> {
        self.global("v", vec![r.clone(), Js::str("leverageRr")])?
            .parse_float()
    }
    fn cross_margin(&mut self, r: &Js) -> Result<bool, Thrown> {
        Ok(self
            .global("v", vec![r.clone(), Js::str("crossMargin")])?
            .truthy())
    }
    fn raw_hedged(&mut self, r: &Js) -> Result<bool, Thrown> {
        Ok(self
            .global("v", vec![r.clone(), Js::str("posMode")])?
            .strict_equals(&Js::str("Hedged")))
    }
    fn value(&mut self, s: &'static str) -> Result<Js, Thrown> {
        Ok(Js::str(s))
    }
    fn report(&mut self, e: &Thrown) -> Result<(), Thrown> {
        Host::report(self, e)
    }
}
fn context_value(c: &LeverageContext<Js>) -> Js {
    Js::object(vec![
        ("marginType", c.margin_type.clone()),
        ("positionMode", c.position_mode.clone()),
        ("longLeverage", Js::Number(c.long_leverage)),
        ("shortLeverage", Js::Number(c.short_leverage)),
    ])
}
pub(crate) async fn context(h: &mut Host, s: Js) -> Result<Js, Thrown> {
    Ok(crate::rest_account_control::resolve_leverage_context(h, s)
        .await?
        .as_ref()
        .map_or(Js::Undefined, context_value))
}
impl SettingsHost for Host {
    type Symbol = Js;
    type Value = Js;
    type Market = Js;
    type Context = LeverageContext<Js>;
    type Reply = Js;
    type Positions = Js;
    type Call = Js;
    type Error = Thrown;
    fn symbol_truthy(&mut self, s: &Js) -> Result<bool, Thrown> {
        Ok(s.truthy())
    }
    fn symbol_text(&mut self, s: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(s.to_js_string()?.to_vec())
    }
    fn message(&mut self, s: Vec<u16>) -> Result<Js, Thrown> {
        Ok(Js::utf16(&s))
    }
    fn emit(&mut self, m: Js) -> Result<(), Thrown> {
        Host::emit(self, m)
    }
    fn error(&mut self, m: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::error("Error", &super::rt::text(&m)?))
    }
    fn format(&mut self, s: &Js) -> Result<Js, Thrown> {
        self.call("formatSymbolOut", vec![s.clone()])
    }
    fn market(&mut self, s: &Js) -> Result<Option<Js>, Thrown> {
        let m = self.store("marketsBySymbol")?.get_key(&s.to_js_string()?)?;
        Ok(m.truthy().then_some(m))
    }
    async fn context(&mut self, s: &Js) -> Result<Option<Self::Context>, Thrown> {
        crate::rest_account_control::resolve_leverage_context(self, s.clone()).await
    }
    fn long(&mut self, c: &Self::Context) -> Result<f64, Thrown> {
        Ok(c.long_leverage)
    }
    fn short(&mut self, c: &Self::Context) -> Result<f64, Thrown> {
        Ok(c.short_leverage)
    }
    fn cross(&mut self, c: &Self::Context) -> Result<bool, Thrown> {
        Ok(c.margin_type.strict_equals(&Js::str("cross")))
    }
    fn hedged(&mut self, c: &Self::Context) -> Result<bool, Thrown> {
        Ok(c.position_mode.strict_equals(&Js::str("hedged")))
    }
    fn min(&mut self, m: &Js) -> Result<f64, Thrown> {
        m.get("limits")?.get("leverage")?.get("min")?.to_number()
    }
    fn max(&mut self, m: &Js) -> Result<f64, Thrown> {
        m.get("limits")?.get("leverage")?.get("max")?.to_number()
    }
    fn put_target(&mut self) -> Result<Js, Thrown> {
        Host::get(self, "xhr")
    }
    async fn put(
        &mut self,
        c: Js,
        e: &'static str,
        s: Js,
        mode: Option<&'static str>,
        leverages: Vec<(&'static str, f64)>,
    ) -> Result<Js, Thrown> {
        let mut p = vec![("symbol", s)];
        if let Some(m) = mode {
            p.push(("targetPosMode", Js::str(m)));
        }
        p.extend(leverages.into_iter().map(|(k, v)| (k, Js::Number(v))));
        await_js(c.call_method(
            "put",
            vec![
                Js::str(e),
                Js::Undefined,
                Js::object(vec![("params", Js::object(p))]),
            ],
        )?)
        .await
    }
    fn zero_code(&mut self, r: &Js, opt: bool) -> Result<bool, Thrown> {
        let d = r.get("data")?;
        let c = if opt {
            optional(&d, "code")?
        } else {
            d.get("code")?
        };
        Ok(c.strict_equals(&Js::Number(0.0)))
    }
    fn reply_message(&mut self, r: &Js, opt: bool) -> Result<Js, Thrown> {
        let d = r.get("data")?;
        if opt {
            optional(&d, "msg")
        } else {
            d.get("msg")
        }
    }
    async fn fetch_positions(&mut self, s: &Js) -> Result<Js, Thrown> {
        self.invoke("fetchPositions", vec![Js::array(vec![s.clone()])])
            .await
    }
    fn replace_symbol_positions(&mut self, s: &Js, p: Js) -> Result<(), Thrown> {
        let mut all = vec![];
        for row in rows(&self.store("positions")?)? {
            if !row.get("symbol")?.strict_equals(s) {
                all.push(row)
            }
        }
        all.extend(rows(&p)?);
        Host::get(self, "store")?.call_method(
            "update",
            vec![Js::object(vec![("positions", Js::array(all))])],
        )?;
        Ok(())
    }
    fn add_positions(&mut self, p: Js) -> Result<(), Thrown> {
        Host::get(self, "store")?.call_method("addOrUpdatePositions", vec![p])?;
        Ok(())
    }
    fn record_mode(&mut self, s: &Js, c: &Self::Context) -> Result<(), Thrown> {
        record(self, s, c, None, None)
    }
    fn record_leverage(
        &mut self,
        s: &Js,
        c: &Self::Context,
        side: Option<&Js>,
        applied: f64,
        signed: f64,
    ) -> Result<(), Thrown> {
        record(self, s, c, side, Some((applied, signed)))
    }
    fn side_truthy(&mut self, s: Option<&Js>) -> Result<bool, Thrown> {
        Ok(s.is_some_and(Js::truthy))
    }
    fn side_long(&mut self, s: Option<&Js>) -> Result<bool, Thrown> {
        Ok(s.is_some_and(|s| s.strict_equals(&Js::str("long"))))
    }
    async fn helper(
        &mut self,
        s: Js,
        l: f64,
        side: Option<Js>,
        allow: Option<bool>,
    ) -> Result<(), Thrown> {
        crate::rest_account_control::set_leverage_helper(self, s, l, side, allow.unwrap_or(false))
            .await
    }
    fn report(&mut self, e: &Thrown) -> Result<(), Thrown> {
        Host::report(self, e)
    }
}
fn record(
    h: &Host,
    s: &Js,
    c: &LeverageContext<Js>,
    side: Option<&Js>,
    leverage: Option<(f64, f64)>,
) -> Result<(), Thrown> {
    let fetched = h.store("fetchedLeverage")?;
    let existing = fetched.get_key(&s.to_js_string()?)?;
    let old = |key, default| -> Result<Js, Thrown> {
        let v = optional(&existing, key)?;
        Ok(if v.is_nullish() {
            Js::Number(default)
        } else {
            v
        })
    };
    let (buy, sell, margin) = if let Some((n, signed)) = leverage {
        (
            if side.is_some_and(|s| s.strict_equals(&Js::str("short"))) {
                old("buyLeverage", n)?
            } else {
                Js::Number(n)
            },
            if side.is_some_and(|s| s.strict_equals(&Js::str("long"))) {
                old("sellLeverage", n)?
            } else {
                Js::Number(n)
            },
            Js::str(if signed < 0.0 { "cross" } else { "isolated" }),
        )
    } else {
        (
            old("buyLeverage", c.long_leverage)?,
            old("sellLeverage", c.short_leverage)?,
            c.margin_type.clone(),
        )
    };
    let next = fetched.spread_object()?;
    next.define_key(
        &s.to_js_string()?,
        Js::object(vec![
            ("buyLeverage", buy),
            ("sellLeverage", sell),
            ("marginType", margin),
            ("positionMode", c.position_mode.clone()),
        ]),
    )?;
    h.get("store")?
        .call_method("update", vec![Js::object(vec![("fetchedLeverage", next)])])?;
    Ok(())
}
