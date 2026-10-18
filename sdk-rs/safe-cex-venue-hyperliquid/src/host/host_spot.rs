//! Host operations for the existing Spot class, trading and socket bodies.
use safe_cex_exchange::js_runtime::{value::{Js, Thrown}};
use super::{
    Host,
};
use crate::{
    spot_client::{self, ClientCallback, SpotClientHost},
    spot_json::JsString,
    spot_public_subscriptions::{self as public, PublicCallback, PublicMethod, SpotPublicHost},
    spot_runtime::{self, ExchangeMethod, SpotRuntimeHost},
    spot_trading::{self, SpotTradingHost, TradingCallback},
    spot_ws_dispatch::{self as private, PrivateMethod, SpotWsHost},
};
use std::rc::Rc;

fn arg(args: &[Js], index: usize) -> Js {
    args.get(index).cloned().unwrap_or(Js::Undefined)
}
fn class_pair(realm: &mut Host, name: &str) -> Result<(Js, Js), Thrown> {
    let class = realm.global_binding(name)?;
    let prototype = realm.get_str(&class, "prototype")?;
    Ok((class, prototype))
}
impl SpotClientHost for Host {
    fn client_callback(&mut self, body: ClientCallback<Js>) -> Result<Js, Thrown> {
        self.native_method("", 1, body)
    }
    fn class_pair(
        &mut self,
        name: &str,
        length: u8,
        base: Option<&str>,
    ) -> Result<(Js, Js), Thrown> {
        let parent = self.global_binding(base.unwrap_or("Object"))?;
        let kind = name.to_owned();
        let pair = self.derived_class(
            name,
            length,
            &parent,
            Rc::new(move |r, class, args, target| {
                let prototype = r.get_str(class, "prototype")?;
                match kind.as_str() {
                    "HyperliquidSpotExchange" => {
                        let (public_ctor, public_proto) =
                            class_pair(r, "HyperliquidSpotPublicWebSocket")?;
                        let (private_ctor, private_proto) =
                            class_pair(r, "HyperliquidSpotPrivateWebSocket")?;
                        spot_runtime::new_spot_exchange(
                            r,
                            &spot_runtime::HyperliquidSpotExchange {
                                constructor: class.clone(),
                                prototype,
                                public: public::HyperliquidSpotPublicWebSocket {
                                    constructor: public_ctor,
                                    prototype: public_proto,
                                },
                                private: private::PrivateSocketClass {
                                    constructor: private_ctor,
                                    prototype: private_proto,
                                },
                            },
                            &[arg(args, 0), arg(args, 1), arg(args, 2)],
                            Some(target),
                        )
                    }
                    "HyperliquidSpotPublicWebSocket" => public::new_public_socket(
                        r,
                        &public::HyperliquidSpotPublicWebSocket {
                            constructor: class.clone(),
                            prototype,
                        },
                        &arg(args, 0),
                        Some(target),
                    ),
                    "HyperliquidSpotPrivateWebSocket" => private::new_private_socket(
                        r,
                        &private::PrivateSocketClass {
                            constructor: class.clone(),
                            prototype,
                        },
                        &arg(args, 0),
                        Some(target),
                    )
                    .map(|socket| socket.native),
                    "RateLimiter" => spot_client::new_rate_limiter(
                        r,
                        &spot_client::RateLimiter {
                            constructor: class.clone(),
                            prototype,
                        },
                        Some(target),
                    ),
                    _ => Err(r.type_error("unknown Spot class binding")),
                }
            }),
        )?;
        self.bind_import(name, pair.0.clone());
        Ok(pair)
    }
    fn define_hidden(&mut self, object: &Js, key: &str, value: &Js) -> Result<(), Thrown> {
        Host::define_hidden(self, object, Js::str(key), value.clone());
        Ok(())
    }
    fn native_method(
        &mut self,
        name: &str,
        length: u8,
        body: ClientCallback<Js>,
    ) -> Result<Js, Thrown> {
        Ok(self.native(name, length, move |r, this, args| {
            spot_client::run_client_callback(r, this, &body, args)
        }))
    }
    fn allocate_class(&mut self, prototype: &Js, target: &Js) -> Result<Js, Thrown> {
        let _ = prototype;
        target.allocate_instance()
    }
}
impl SpotRuntimeHost for Host {
    fn base_exchange(
        &mut self,
        args: &[Js; 3],
        target: &Js,
        _: &Js,
    ) -> Result<Js, Thrown> {
        let base = self.global_binding("BaseExchange")?;
        self.construct(&base, args, Some(target))
    }
    fn exchange_arrow(
        &mut self,
        owner: &Js,
        name: &str,
        length: u8,
        body: ExchangeMethod,
    ) -> Result<Js, Thrown> {
        let owner = owner.clone();
        let class = self.global_binding("HyperliquidSpotExchange")?;
        Ok(self.native(name, length, move |r, _, args| {
            spot_runtime::run_exchange_method(&mut r.with_lexical(&class), &owner, body, args)
        }))
    }
    fn exchange_method(
        &mut self,
        name: &str,
        length: u8,
        body: ExchangeMethod,
    ) -> Result<Js, Thrown> {
        let class = self.global_binding("HyperliquidSpotExchange")?;
        Ok(self.native(name, length, move |r, this, args| {
            spot_runtime::run_exchange_method(&mut r.with_lexical(&class), this, body, args)
        }))
    }
}
impl SpotTradingHost for Host {
    fn trading_callback(&mut self, capture: TradingCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            spot_trading::run_trading_callback(r, &capture, args)
        }))
    }
    fn unary_number(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.unary("+", value)
    }
    fn has_property(&mut self, key: &str, value: &Js) -> Result<bool, Thrown> {
        Host::has_property(self, value, &Js::str(key))
    }
    fn trading_binding(&mut self, value: &Js, keys: &[&str]) -> Result<Vec<Js>, Thrown> {
        self.object_pattern(value, keys)
    }
}
impl SpotWsHost for Host {
    fn event_data(&mut self, event: &Js) -> Result<Js, Thrown> {
        Ok(self.object_pattern(event, &["data"])?[0].clone())
    }
    fn is_nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn is_array(&mut self, value: &Js) -> Result<bool, Thrown> {
        Ok(value.is_array())
    }
    fn equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn less(&mut self, left: &Js, right: &Js) -> Result<bool, Thrown> {
        Ok(self.binary("<", left, right)?.truthy())
    }
    fn greater(&mut self, left: &Js, right: &Js) -> Result<bool, Thrown> {
        Ok(self.binary(">", left, right)?.truthy())
    }
    fn index(&mut self, value: &Js, key: &Js) -> Result<Js, Thrown> {
        Host::index(self, value, key)
    }
    fn array(&mut self) -> Result<Js, Thrown> {
        Ok(self.new_array(vec![]))
    }
    fn parse_float(&mut self, value: &Js) -> Result<f64, Thrown> {
        let parser = self.global_binding("parseFloat")?;
        let parsed = self.call(&parser, &Js::Undefined, &[value.clone()])?;
        self.to_number(&parsed)
    }
    fn template_string(&mut self, value: &Js) -> Result<JsString, Thrown> {
        self.to_string(value).map(|units| JsString(units.to_vec()))
    }
    fn json_parse(&mut self, data: &Js) -> Result<Js, Thrown> {
        let parser = self.global_binding("jsonParse")?;
        self.call(&parser, &Js::Undefined, &[data.clone()])
    }
    fn order_side(&mut self, buy: bool) -> Result<Js, Thrown> {
        let side = self.global_binding("OrderSide")?;
        self.get_str(&side, if buy { "Buy" } else { "Sell" })
    }
    fn for_in(&mut self, value: &Js) -> Result<Js, Thrown> {
        Ok(self.for_in_iterator(value))
    }
    fn for_in_next(&mut self, iterator: &Js) -> Result<Option<Js>, Thrown> {
        self.iterator_step(iterator)
    }
    fn private_class(&mut self) -> Result<(Js, Js), Thrown> {
        SpotClientHost::class_pair(
            self,
            "HyperliquidSpotPrivateWebSocket",
            1,
            Some("BaseWebSocket"),
        )
    }
    fn construct_base(
        &mut self,
        parent: &Js,
        target: &Js,
        _: &Js,
    ) -> Result<Js, Thrown> {
        let base = self.global_binding("BaseWebSocket")?;
        self.construct(&base, &[parent.clone()], Some(target))
    }
    fn arrow(
        &mut self,
        capture: &Js,
        name: &str,
        length: u8,
        body: PrivateMethod,
    ) -> Result<Js, Thrown> {
        let capture = capture.clone();
        let class = self.global_binding("HyperliquidSpotPrivateWebSocket")?;
        Ok(self.native(name, length, move |r, _, args| {
            private::run_private_callback(&mut r.with_lexical(&class), &capture, body, args)
        }))
    }
}
impl SpotPublicHost for Host {
    fn append_literal(&mut self, array: &Js, value: &Js) -> Result<(), Thrown> {
        let length = self.get_str(array, "length")?;
        self.set_index(array, &length, value.clone())
    }
    fn decrement(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.binary("-", value, &Js::Number(1.0))
    }
    fn negate(&mut self, value: &Js) -> Result<Js, Thrown> {
        self.unary("-", value)
    }
    fn public_callback(&mut self, capture: PublicCallback<Js>) -> Result<Js, Thrown> {
        Ok(self.native("", 1, move |r, _, args| {
            public::run_public_callback(r, &capture, args)
        }))
    }
    fn public_arrow(
        &mut self,
        owner: &Js,
        name: &str,
        length: u8,
        body: PublicMethod,
    ) -> Result<Js, Thrown> {
        let owner = owner.clone();
        let class = self.global_binding("HyperliquidSpotPublicWebSocket")?;
        Ok(self.native(name, length, move |r, _, args| {
            public::run_public_method(&mut r.with_lexical(&class), &owner, body, args)
        }))
    }
    fn base_socket(
        &mut self,
        parent: &Js,
        target: &Js,
        prototype: &Js,
    ) -> Result<Js, Thrown> {
        SpotWsHost::construct_base(self, parent, target, prototype)
    }
    fn assign_index(&mut self, object: &Js, key: &Js, value: &Js) -> Result<(), Thrown> {
        self.set_index(object, key, value.clone())
    }
    fn delete_index(&mut self, object: &Js, key: &Js) -> Result<(), Thrown> {
        let key = self.to_property_key(key)?;
        self.delete(object, &key).map(|_| ())
    }
    fn record_binding(&mut self, value: &Js, keys: &[&str]) -> Result<Vec<Js>, Thrown> {
        self.object_pattern(value, keys)
    }
}

pub(super) fn bind_spot_imports(realm: &Host) -> Result<(), Thrown> {
    realm.bind_import("dayjs", realm.shared.globals.get("dayjs")?);
    Ok(())
}
