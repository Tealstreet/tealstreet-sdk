use super::rt::{Host, rows, text};
use crate::{
    start_order_formatting::{ExchangeState, OrderHost},
    websocket_data::WsHost,
};
use safe_cex_exchange::js_runtime::{
    base_exchange::{self, BaseExchangeParts},
    promise::{JsFuture, await_js},
    value::{Js, Thrown},
};
use std::rc::Rc;

impl WsHost for Host {
    type V = Js;
    type E = Thrown;
    fn get(&mut self, v: &Js, k: &[u16]) -> Result<Js, Thrown> {
        v.get_key(k)
    }
    fn optional(&mut self, v: &Js, k: &[u16]) -> Result<Js, Thrown> {
        if v.is_nullish() {
            Ok(Js::Undefined)
        } else {
            v.get_key(k)
        }
    }
    fn set(&mut self, v: &Js, k: &[u16], x: Js) -> Result<(), Thrown> {
        v.set_key(k, x)
    }
    fn remove(&mut self, v: &Js, k: &[u16]) -> Result<(), Thrown> {
        v.delete(k)?;
        Ok(())
    }
    fn truthy(&mut self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&mut self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn same(&mut self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn set_same(&mut self, a: &Js, b: &Js) -> bool {
        a.same_value_zero(b)
    }
    fn number(&mut self, v: Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn parse(&mut self, v: Js) -> Result<f64, Thrown> {
        v.parse_float()
    }
    fn text(&mut self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.to_js_string()?.to_vec())
    }
    fn lower(&mut self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.call_method("toLowerCase", vec![])?
            .to_js_string()?
            .to_vec())
    }
    fn num(&mut self, v: f64) -> Js {
        Js::Number(v)
    }
    fn string(&mut self, v: Vec<u16>) -> Js {
        Js::utf16(&v)
    }
    fn undefined(&mut self) -> Js {
        Js::Undefined
    }
    fn boolean(&mut self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn array(&mut self, v: Vec<Js>) -> Js {
        Js::array(v)
    }
    fn object(&mut self, v: Vec<(&'static str, Js)>) -> Js {
        Js::object(v)
    }
    fn spread(&mut self, v: &Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn rows(&mut self, v: Js) -> Result<Vec<Js>, Thrown> {
        rows(&v)
    }
    fn values(&mut self, v: &Js) -> Result<Vec<Js>, Thrown> {
        Ok(v.own_entries()?.into_iter().map(|(_, v)| v).collect())
    }
    fn at(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn is_array(&mut self, v: &Js) -> bool {
        v.is_array()
    }
    fn store(&mut self, name: &'static str) -> Result<Js, Thrown> {
        Host::store(self, name)
    }
    fn field(&mut self, name: &'static str) -> Result<Js, Thrown> {
        if name == "timeframes" {
            return Host::get(self, "parent")?.get("timeframes");
        }
        Host::get(
            self,
            match name {
                "disposed" => "isDisposed",
                "publicOnly" => "isPublicDataOnly",
                "connected" => "isConnected",
                "snapshotLogs" => "orderBookSnapshotErrorLogAt",
                other => other,
            },
        )
    }
    fn call(&mut self, name: &'static str, args: Vec<Js>) -> Result<Js, Thrown> {
        super::methods::call(self, name, args)
    }
    fn read(&mut self, endpoint: &'static str, params: Js) -> Result<JsFuture<'static>, Thrown> {
        // The public socket snapshot reads this.parent.xhr in the source.
        let parent = self.child(Host::get(self, "parent")?);
        let request = parent.http(
            "xhr",
            "get",
            endpoint,
            vec![Js::object(vec![("params", params)])],
        );
        Ok(Box::pin(async move {
            let result = request.await;
            // JS resumes after await in a later job even for an already
            // settled HTTP response. Let the parser buffer its first delta
            // before the snapshot continuation consumes that buffer.
            let mut yielded = false;
            std::future::poll_fn(|cx| {
                if yielded {
                    std::task::Poll::Ready(())
                } else {
                    yielded = true;
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await;
            result
        }))
    }
    fn spawn(
        &mut self,
        task: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Thrown>>>>,
    ) -> Result<(), Thrown> {
        self.rt.spawn(Box::pin(async move {
            task.await?;
            Ok(Js::Undefined)
        }));
        Ok(())
    }
    fn timer(
        &mut self,
        delay: f64,
        task: Box<dyn FnOnce() -> Result<(), Thrown>>,
    ) -> Result<Js, Thrown> {
        let callback = std::cell::RefCell::new(Some(task));
        Ok(self.rt.set_timeout(
            Rc::new(move || match callback.borrow_mut().take() {
                Some(task) => task(),
                None => Ok(()),
            }),
            delay,
        ))
    }
    fn error_text(&mut self, e: Thrown) -> Result<Vec<u16>, Thrown> {
        // Source uses Error.message, and String(value) for other rejections.
        let message = if matches!(&e.value, Js::Object(object) if matches!(object.class, "Error" | "AxiosError"))
        {
            e.value.get("message")?
        } else {
            e.value
        };
        Ok(message.to_js_string()?.to_vec())
    }
}

impl OrderHost for Host {
    fn async_call(
        &mut self,
        name: &'static str,
        args: Vec<Js>,
    ) -> Result<JsFuture<'static>, Thrown> {
        let name = match name {
            "initExternal" => "initExternalPublicData",
            "market" => "getOrFetchMarket",
            "positions" => "getOrFetchPositionsForSymbol",
            "positionSide" => "getOrderPositionSide",
            "formatCreate" => "formatCreateOrder",
            "formatTrailing" => "formatCreateTrailingStopLossOrder",
            "startPrivate" => "_startPrivate",
            "startPublicOnly" => "_startPublicDataOnly",
            other => other,
        };
        if name == "put" {
            let endpoint = text(&args[0])?;
            return Ok(self.http(
                "unlimitedXHR",
                "put",
                &endpoint,
                vec![Js::Undefined, Js::object(vec![("params", args[1].clone())])],
            ));
        }
        let pending = Host::call(self, name, args)?;
        Ok(Box::pin(await_js(&self.rt.exchange_runtime(), pending)))
    }
    fn error(&mut self, message: Js) -> Result<Thrown, Thrown> {
        Ok(Thrown::error("Error", &text(&message)?))
    }
    fn has(&mut self, v: &Js, key: &'static str) -> Result<bool, Thrown> {
        Ok(!v.get(key)?.is_undefined() || v.has_own(key))
    }
    fn method_text(&mut self, v: Js) -> Result<Vec<u16>, Thrown> {
        Ok(v.call_method("toString", vec![])?.to_js_string()?.to_vec())
    }
    fn table(&mut self, name: &'static str, key: Js, inverse: bool) -> Result<Js, Thrown> {
        let table = super::methods::table(match name {
            "timeInForce" => "TIME_IN_FORCE",
            "side" => "ORDER_SIDE",
            "type" => "ORDER_TYPE",
            other => other,
        })?;
        let table = if inverse {
            self.global("inverseObj", vec![table])?
        } else {
            table
        };
        table.get_key(&key.to_js_string()?)
    }
    fn times_count(&mut self, n: f64) -> Result<usize, Thrown> {
        Ok(if !n.is_finite() || n < 1.0 {
            0
        } else {
            n.trunc() as usize
        })
    }
    fn base_construct(&mut self, name: Js, opts: Js, store: Js) -> Result<(), Thrown> {
        let parts = BaseExchangeParts {
            name: self.rt.shared.extra.name.clone(),
            options: self.rt.shared.extra.options.clone(),
            store: self.rt.shared.store.clone(),
            settings: super::exchange::settings(&self.rt)?,
            external_public_data: self.rt.shared.extra.external_public_data.clone(),
        };
        let (this, base) = base_exchange::create(
            &self.rt.shared.sockets,
            parts,
            name,
            opts,
            store,
            |_| Ok(()),
        )?;
        self.rt
            .shared
            .base
            .set(base)
            .map_err(|_| Thrown::error("Error", "Phemex base already constructed"))?;
        *self.rt.shared.exchange.borrow_mut() = this.clone();
        self.rt.this = this;
        Ok(())
    }
    fn initialize(&mut self) -> Result<ExchangeState<Js>, Thrown> {
        super::methods::install(self)?;
        let state = ExchangeState::initialize(self)?;
        for (name, value) in [
            ("clientOrderIdRule", state.client_order_id_rule.clone()),
            (
                "grafanaEndpointsPeriodic",
                state.grafana_endpoints_periodic.clone(),
            ),
            (
                "grafanaEndpointsEvery",
                state.grafana_endpoints_every.clone(),
            ),
            ("timeframes", state.timeframes.clone()),
        ] {
            self.rt.this.define(name, value)?;
        }
        Ok(state)
    }
    fn assign(&mut self, name: &'static str, value: Js) -> Result<(), Thrown> {
        self.rt.this.define(name, value)
    }
    fn secondary_xhr(&mut self, opts: Js, max_rps: f64) -> Result<Js, Thrown> {
        let weak = Rc::downgrade(&self.rt.shared);
        let factory = Js::function("createAPI", move |_, a| {
            super::api::create(
                &Host {
                    rt: safe_cex_exchange::js_runtime::scope::scope_of(&weak)?,
                },
                a[0].clone(),
            )
        });
        let limiter = Js::function("rateLimit", move |_, a| {
            a[0].call_method(
                "__configureRateLimit",
                vec![Js::object(vec![("maxRPS", Js::Number(max_rps))])],
            )?;
            Ok(a[0].clone())
        });
        Host::call(
            self,
            "createXhrWithSecondaryKey",
            vec![opts, factory, Js::object(vec![("rateLimit", limiter)])],
        )
    }
}
