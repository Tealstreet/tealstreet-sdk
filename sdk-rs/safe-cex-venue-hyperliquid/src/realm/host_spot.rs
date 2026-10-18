//! Host operations for the existing Spot class, trading and socket bodies.
use super::{
    Realm,
    value::{IterSource, Kind, PropKey, Value},
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

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or(Value::Undefined)
}
fn class_pair(realm: &mut Realm, name: &str) -> Result<(Value, Value), Value> {
    let class = realm.global_binding(name)?;
    let prototype = realm.get_str(&class, "prototype")?;
    Ok((class, prototype))
}
impl SpotClientHost for Realm {
    fn client_callback(&mut self, body: ClientCallback<Value>) -> Result<Value, Value> {
        self.native_method("", 1, body)
    }
    fn class_pair(
        &mut self,
        name: &str,
        length: u8,
        base: Option<&str>,
    ) -> Result<(Value, Value), Value> {
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
        if base.is_none() {
            pair.0.object().unwrap().borrow_mut().proto =
                Some(self.intrinsics().function_proto.clone());
        }
        self.bind_import(name, pair.0.clone());
        Ok(pair)
    }
    fn define_hidden(&mut self, object: &Value, key: &str, value: &Value) -> Result<(), Value> {
        Realm::define_hidden(self, object, PropKey::str(key), value.clone());
        Ok(())
    }
    fn native_method(
        &mut self,
        name: &str,
        length: u8,
        body: ClientCallback<Value>,
    ) -> Result<Value, Value> {
        Ok(self.native(name, length, move |r, this, args| {
            spot_client::run_client_callback(r, this, &body, args)
        }))
    }
    fn allocate_class(&mut self, prototype: &Value, target: &Value) -> Result<Value, Value> {
        let fallback = prototype
            .object()
            .ok_or_else(|| self.type_error("class prototype must be an object"))?;
        let prototype = self.prototype_of_target(target, fallback)?;
        Ok(self.alloc(Kind::Ordinary, Some(&prototype)))
    }
}
impl SpotRuntimeHost for Realm {
    fn base_exchange(
        &mut self,
        args: &[Value; 3],
        target: &Value,
        _: &Value,
    ) -> Result<Value, Value> {
        let base = self.global_binding("BaseExchange")?;
        self.construct(&base, args, Some(target))
    }
    fn exchange_arrow(
        &mut self,
        owner: &Value,
        name: &str,
        length: u8,
        body: ExchangeMethod,
    ) -> Result<Value, Value> {
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
    ) -> Result<Value, Value> {
        let class = self.global_binding("HyperliquidSpotExchange")?;
        Ok(self.native(name, length, move |r, this, args| {
            spot_runtime::run_exchange_method(&mut r.with_lexical(&class), this, body, args)
        }))
    }
}
impl SpotTradingHost for Realm {
    fn trading_callback(&mut self, capture: TradingCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            spot_trading::run_trading_callback(r, &capture, args)
        }))
    }
    fn unary_number(&mut self, value: &Value) -> Result<Value, Value> {
        self.unary("+", value)
    }
    fn has_property(&mut self, key: &str, value: &Value) -> Result<bool, Value> {
        Realm::has_property(self, value, &PropKey::str(key))
    }
    fn trading_binding(&mut self, value: &Value, keys: &[&str]) -> Result<Vec<Value>, Value> {
        self.object_pattern(value, keys)
    }
}
impl SpotWsHost for Realm {
    fn event_data(&mut self, event: &Value) -> Result<Value, Value> {
        Ok(self.object_pattern(event, &["data"])?[0].clone())
    }
    fn is_nullish(&self, value: &Value) -> bool {
        value.is_nullish()
    }
    fn is_array(&mut self, value: &Value) -> Result<bool, Value> {
        Ok(value
            .object()
            .is_some_and(|object| matches!(object.borrow().kind, Kind::Array(_))))
    }
    fn equal(&self, left: &Value, right: &Value) -> bool {
        left.strict_equal(right)
    }
    fn number(&self, value: f64) -> Value {
        Value::Number(value)
    }
    fn less(&mut self, left: &Value, right: &Value) -> Result<bool, Value> {
        Ok(self.binary("<", left, right)?.truthy())
    }
    fn greater(&mut self, left: &Value, right: &Value) -> Result<bool, Value> {
        Ok(self.binary(">", left, right)?.truthy())
    }
    fn index(&mut self, value: &Value, key: &Value) -> Result<Value, Value> {
        Realm::index(self, value, key)
    }
    fn array(&mut self) -> Result<Value, Value> {
        Ok(self.new_array(vec![]))
    }
    fn parse_float(&mut self, value: &Value) -> Result<f64, Value> {
        let parser = self.global_binding("parseFloat")?;
        let parsed = self.call(&parser, &Value::Undefined, &[value.clone()])?;
        self.to_number(&parsed)
    }
    fn template_string(&mut self, value: &Value) -> Result<JsString, Value> {
        self.to_string(value).map(|units| JsString(units.to_vec()))
    }
    fn json_parse(&mut self, data: &Value) -> Result<Value, Value> {
        let parser = self.global_binding("jsonParse")?;
        self.call(&parser, &Value::Undefined, &[data.clone()])
    }
    fn order_side(&mut self, buy: bool) -> Result<Value, Value> {
        let side = self.global_binding("OrderSide")?;
        self.get_str(&side, if buy { "Buy" } else { "Sell" })
    }
    fn for_in(&mut self, value: &Value) -> Result<Value, Value> {
        let keys = self.for_in_keys(value)?;
        Ok(self.iterator_object(IterSource::Keys { keys, index: 0 }))
    }
    fn for_in_next(&mut self, iterator: &Value) -> Result<Option<Value>, Value> {
        self.iterator_step(iterator)
    }
    fn private_class(&mut self) -> Result<(Value, Value), Value> {
        SpotClientHost::class_pair(
            self,
            "HyperliquidSpotPrivateWebSocket",
            1,
            Some("BaseWebSocket"),
        )
    }
    fn construct_base(
        &mut self,
        parent: &Value,
        target: &Value,
        _: &Value,
    ) -> Result<Value, Value> {
        let base = self.global_binding("BaseWebSocket")?;
        self.construct(&base, &[parent.clone()], Some(target))
    }
    fn arrow(
        &mut self,
        capture: &Value,
        name: &str,
        length: u8,
        body: PrivateMethod,
    ) -> Result<Value, Value> {
        let capture = capture.clone();
        let class = self.global_binding("HyperliquidSpotPrivateWebSocket")?;
        Ok(self.native(name, length, move |r, _, args| {
            private::run_private_callback(&mut r.with_lexical(&class), &capture, body, args)
        }))
    }
}
impl SpotPublicHost for Realm {
    fn append_literal(&mut self, array: &Value, value: &Value) -> Result<(), Value> {
        let length = self.get_str(array, "length")?;
        self.set_index(array, &length, value.clone())
    }
    fn decrement(&mut self, value: &Value) -> Result<Value, Value> {
        self.binary("-", value, &Value::Number(1.0))
    }
    fn negate(&mut self, value: &Value) -> Result<Value, Value> {
        self.unary("-", value)
    }
    fn public_callback(&mut self, capture: PublicCallback<Value>) -> Result<Value, Value> {
        Ok(self.native("", 1, move |r, _, args| {
            public::run_public_callback(r, &capture, args)
        }))
    }
    fn public_arrow(
        &mut self,
        owner: &Value,
        name: &str,
        length: u8,
        body: PublicMethod,
    ) -> Result<Value, Value> {
        let owner = owner.clone();
        let class = self.global_binding("HyperliquidSpotPublicWebSocket")?;
        Ok(self.native(name, length, move |r, _, args| {
            public::run_public_method(&mut r.with_lexical(&class), &owner, body, args)
        }))
    }
    fn base_socket(
        &mut self,
        parent: &Value,
        target: &Value,
        prototype: &Value,
    ) -> Result<Value, Value> {
        SpotWsHost::construct_base(self, parent, target, prototype)
    }
    fn assign_index(&mut self, object: &Value, key: &Value, value: &Value) -> Result<(), Value> {
        self.set_index(object, key, value.clone())
    }
    fn delete_index(&mut self, object: &Value, key: &Value) -> Result<(), Value> {
        let key = self.to_property_key(key)?;
        self.delete(object, &key).map(|_| ())
    }
    fn record_binding(&mut self, value: &Value, keys: &[&str]) -> Result<Vec<Value>, Value> {
        self.object_pattern(value, keys)
    }
}

// The Spot candle reader uses only dayjs(input), subtract and valueOf. Keep
// calendar arithmetic in the shared library; no second venue date algorithm.
fn dayjs_object(realm: &Realm, day: safe_cex_exchange::js_runtime::value::Js) -> Value {
    let result = realm.new_object();
    let value = day.clone();
    let value_of = realm.native("valueOf", 0, move |realm, _, _| {
        let value = value
            .call_method("valueOf", vec![])
            .map_err(|error| realm.typed_error(error))?;
        realm.restore_typed_value(&value)
    });
    realm.define_hidden(&result, PropKey::str("valueOf"), value_of);
    let subtract = realm.native("subtract", 2, move |realm, _, args| {
        let args = args
            .iter()
            .map(|value| realm.capture_typed_value(value, 0))
            .collect::<Result<Vec<_>, _>>()?;
        let shifted = day
            .call_method("subtract", args)
            .map_err(|error| realm.typed_error(error))?;
        Ok(dayjs_object(realm, shifted))
    });
    realm.define_hidden(&result, PropKey::str("subtract"), subtract);
    result
}

pub(super) fn bind_spot_imports(realm: &Realm) -> Result<(), Value> {
    use safe_cex_exchange::js_runtime::{library::install_library, value::Js};
    let globals = Js::object(vec![]);
    install_library::<()>(&globals, std::rc::Weak::new());
    let dayjs = globals
        .get("dayjs")
        .map_err(|error| realm.typed_error(error))?;
    let function = realm.native("dayjs", 1, move |realm, _, args| {
        let input = arg(args, 0);
        let input = if matches!(input, Value::Undefined) {
            // Supplying now explicitly lets this library import remain scoped
            // to the existing family realm's runtime and clock owner.
            Js::Number(realm.runtime().now_ms().trunc())
        } else {
            realm.capture_typed_value(&input, 0)?
        };
        let day = dayjs
            .call(Js::Undefined, vec![input])
            .map_err(|error| realm.typed_error(error))?;
        Ok(dayjs_object(realm, day))
    });
    realm.bind_import("dayjs", function);
    Ok(())
}
