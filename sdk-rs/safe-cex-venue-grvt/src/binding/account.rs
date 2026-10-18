//! Session and account operation bindings; algorithm bodies remain in account_runtime.
use super::runtime::{Host, thrown, value};
use crate::{
    account_runtime::{AccountHost, Iteration},
    builder_workflow::{JsPromise, JsResult},
};
use safe_cex_exchange::{
    js_runtime::{
        ops,
        value::{Js, Thrown, from_typed, to_json},
    },
    runtime::LogSeverity,
};
use std::rc::Rc;

fn failure(message: impl AsRef<str>) -> Js {
    Thrown::error("Error", message.as_ref()).value
}

impl AccountHost for Host {
    fn binding(&self) -> Rc<Self> {
        Rc::new(self.clone())
    }
    fn set_indexed(&self, target: &Js, key: Js, field: Js) -> JsResult<Js, ()> {
        let key = value(key.to_js_string())?;
        value(target.set_key(&key, field))
    }
    fn number_value(&self, number: Js) -> JsResult<Js, f64> {
        self.global_call("Number", vec![number])?
            .as_f64()
            .ok_or_else(|| Thrown::type_error("Number returned a non-number").value)
    }
    fn greater_than_zero(&self, number: Js) -> JsResult<Js, bool> {
        value(ops::greater_than(&number, &Js::Number(0.0)))
    }
    fn symbol_set(&self, values: Js) -> JsResult<Js> {
        value(value(self.0.global("Set"))?.construct(vec![values], None))
    }
    fn has_symbol(&self, set: &Js, key: Js) -> JsResult<Js, bool> {
        value(set.call_method("has", vec![key])).map(|v| v.truthy())
    }
    fn append_iterable(&self, target: &Js, values: Js) -> JsResult<Js, ()> {
        self.for_of(values, &mut |item| {
            value(target.call_method("push", vec![item]))?;
            Ok(false)
        })
    }
    fn exchange(&self) -> Js {
        self.0.this.clone()
    }
    fn endpoint(&self, name: &str) -> Js {
        crate::protocol::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == name)
            .map_or(Js::Undefined, |(_, url)| Js::str(url))
    }
    fn severity(&self, level: LogSeverity) -> Js {
        Js::str(level.as_str())
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn indexed(&self, object: &Js, key: &Js) -> JsResult<Js> {
        value(object.get_key(&value(key.to_js_string())?))
    }
    fn parse_float(&self, v: Js) -> JsResult<Js, f64> {
        self.global_call("parseFloat", vec![v])?
            .as_f64()
            .ok_or_else(|| failure("parseFloat returned a non-number"))
    }
    fn parse_int(&self, v: Js) -> JsResult<Js, f64> {
        self.global_call("parseInt", vec![v])?
            .as_f64()
            .ok_or_else(|| failure("parseInt returned a non-number"))
    }
    fn js_string(&self, v: Js) -> JsResult<Js> {
        self.global_call("String", vec![v])
    }
    fn add(&self, a: Js, b: Js) -> JsResult<Js> {
        value(ops::add(&a, &b))
    }
    fn now(&self) -> JsResult<Js, f64> {
        Ok(self.0.now())
    }
    fn virtual_now(&self) -> JsResult<Js, f64> {
        self.global_call("virtualClock.getCurrentTimeMs", vec![])?
            .as_f64()
            .ok_or_else(|| failure("virtual clock returned a non-number"))
    }
    fn for_of(&self, iterable: Js, visitor: &mut Iteration<'_, Js>) -> JsResult<Js, ()> {
        match &iterable {
            Js::Array(items) => {
                let mut index = 0;
                loop {
                    let next = {
                        let items = items.borrow();
                        if index >= items.len() {
                            None
                        } else {
                            Some(items[index].clone().unwrap_or(Js::Undefined))
                        }
                    };
                    let Some(item) = next else {
                        break;
                    };
                    if visitor(item)? {
                        break;
                    }
                    index += 1;
                }
                Ok(())
            }
            _ => {
                let values = self.global_call("Array.from", vec![iterable])?;
                let values = values
                    .array_values()
                    .ok_or_else(|| Thrown::type_error("value is not iterable").value)?;
                for item in values {
                    if visitor(item)? {
                        break;
                    }
                }
                Ok(())
            }
        }
    }
    fn loop_after(&self, callback: Rc<dyn Fn() -> JsResult<Js>>, delay: f64) -> JsResult<Js, ()> {
        self.0
            .set_timeout(Rc::new(move || thrown(callback()).map(|_| ())), delay);
        Ok(())
    }
    fn sleep(&self, delay: f64) -> JsResult<Js, JsPromise<Js>> {
        let runtime = self.0.exchange_runtime();
        Ok(Box::pin(async move {
            runtime
                .sleep(delay.max(0.0) as u64)
                .await
                .map_err(|e| Thrown::from(e).value)?;
            Ok(Js::Undefined)
        }))
    }
    fn builder_address(&self, testnet: Js) -> JsResult<Js> {
        Ok(Js::str(crate::builder::get_grvt_builder_address(
            testnet.truthy(),
        )))
    }
    fn fee_to_raw(&self, fee: Js) -> JsResult<Js, f64> {
        let text = value(fee.to_js_string())?;
        crate::builder::grvt_builder_fee_rate_to_raw(&String::from_utf16_lossy(&text))
            .map_err(failure)
    }
    fn builder_info(&self, testnet: Js) -> JsResult<Js> {
        let info = crate::builder::create_grvt_builder_info(Some(testnet.truthy()), None, None)
            .map_err(failure)?;
        value(from_typed(&info))
    }
    fn builder_error(&self, opts: Js) -> JsResult<Js> {
        let reason = value(opts.get("reason"))?;
        let reason = if reason.strict_equals(&Js::str("cap-too-low")) {
            crate::types::GrvtBuilderUnauthorizedReason::CapTooLow
        } else {
            crate::types::GrvtBuilderUnauthorizedReason::Unauthorized
        };
        let required = self.number_value(value(opts.get("requiredRaw"))?)?;
        let authorized = value(opts.get("authorizedRaw"))?;
        let authorized = if authorized.is_undefined() {
            None
        } else {
            Some(self.number_value(authorized)?)
        };
        let error =
            crate::builder::GrvtBuilderUnauthorizedError::new(reason, required, authorized, None);
        let object = safe_cex_exchange::js_runtime::value::error_object(error.name, &error.message);
        value(object.define("code", Js::str(error.code)))?;
        value(object.define("reason", value(from_typed(&error.reason))?))?;
        value(object.define("requiredRaw", Js::Number(required)))?;
        value(object.define(
            "authorizedRaw",
            authorized.map_or(Js::Undefined, Js::Number),
        ))?;
        if let Js::Object(data) = &object {
            *data.proto.borrow_mut() = Some(self.0.shared.extra.builder_error_proto.clone());
        }
        Ok(object)
    }
    fn position_id(&self, symbol: Js) -> JsResult<Js> {
        self.global_call(
            "createPositionId",
            vec![
                symbol,
                value(from_typed(&safe_cex_types::PositionSide::Long))?,
                value(from_typed(&safe_cex_types::PositionMode::OneWay))?,
            ],
        )
    }
    fn trigger_metadata(&self, order: Js) -> JsResult<Js> {
        let input = value(safe_cex_exchange::js_runtime::value::to_retained_input(
            &order,
        ))?;
        if !input.is_object() {
            return Err(Thrown::type_error("order must be an object").value);
        }
        match crate::order_semantics::get_grvt_trigger_metadata(&input)
            .map_err(|e| failure(e.message))?
        {
            Some(metadata) => value(from_typed(&metadata)),
            None => Ok(Js::Undefined),
        }
    }
    fn time_in_force(&self, order: Js) -> JsResult<Js> {
        let json = value(to_json(&order))?;
        let object = json
            .as_object()
            .ok_or_else(|| Thrown::type_error("order must be an object").value)?;
        match crate::order_semantics::get_grvt_order_time_in_force(object) {
            Some(tif) => value(from_typed(&tif)),
            None => Ok(Js::Undefined),
        }
    }
    fn keys_join(&self, object: Js) -> JsResult<Js> {
        let keys = object.own_keys().into_iter().map(Js::String).collect();
        value(Js::array(keys).call_method("join", vec![Js::str(",")]))
    }
}

#[cfg(test)]
mod accepted_contract_tests {
    #[test]
    fn accepted_account_error_factory() {
        for message in ["", "protocol denied", "café\n\0💠"] {
            let actual = super::failure(message);
            let separate = super::failure(message);
            let name = actual.get("name").expect("native error name");
            let text = actual.get("message").expect("native error text");
            assert_eq!(
                name.as_str().unwrap().as_ref(),
                "Error".encode_utf16().collect::<Vec<_>>(),
                "native UTF16 error name"
            );
            assert_eq!(
                text.as_str().unwrap().as_ref(),
                message.encode_utf16().collect::<Vec<_>>(),
                "native UTF16 error text"
            );
            assert!(
                !actual.strict_equals(&separate),
                "fresh error object identity"
            );
        }
    }
}
