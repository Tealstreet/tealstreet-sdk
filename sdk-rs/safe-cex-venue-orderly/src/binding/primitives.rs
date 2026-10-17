use super::{
    host::{Host, R, V, v},
    text,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_typed};
use safe_cex_types::{QueryValue, object_map::JsObjectMap};
use std::{cell::RefCell, rc::Rc};
pub(super) fn table<T: serde::Serialize>(entries: &[(&str, T)], key: &Js, inverse: bool) -> R<V> {
    let result = Js::object(vec![]);
    for (name, value) in entries {
        let value = from_typed(value)?;
        if inverse {
            result.set(&text(&value)?, Js::str(name))?;
        } else {
            result.set(name, value)?;
        }
    }
    Ok(v(result.get(&text(key)?)?))
}
pub(super) fn deep_clone(value: &Js) -> R<Js> {
    fn copy(value: &Js, seen: &mut Vec<(Js, Js)>) -> R<Js> {
        if let Some((_, target)) = seen.iter().find(|(old, _)| old.same_reference(value)) {
            return Ok(target.clone());
        }
        match value {
            Js::Array(items) => {
                let target = Js::Array(Rc::new(RefCell::new(vec![])));
                seen.push((value.clone(), target.clone()));
                if let Js::Array(out) = &target {
                    for item in items.borrow().iter() {
                        out.borrow_mut()
                            .push(item.as_ref().map(|item| copy(item, seen)).transpose()?);
                    }
                }
                Ok(target)
            }
            Js::Object(object) => {
                let target = Js::object(vec![]);
                if let Js::Object(out) = &target {
                    *out.proto.borrow_mut() = object.proto.borrow().clone();
                }
                seen.push((value.clone(), target.clone()));
                for (key, item) in value.own_entries()? {
                    target.define_key(&key, copy(&item, seen)?)?;
                }
                Ok(target)
            }
            _ => Ok(value.clone()),
        }
    }
    copy(value, &mut vec![])
}
fn query_value(value: &Js) -> R<QueryValue> {
    Ok(match value {
        Js::Undefined => QueryValue::Undefined,
        Js::Null => QueryValue::Null,
        Js::Bool(x) => QueryValue::Bool(*x),
        Js::Number(x) => QueryValue::Number(*x),
        Js::String(_) => QueryValue::String(text(value)?),
        Js::Array(_) => QueryValue::Array(
            value
                .array_values()
                .unwrap()
                .iter()
                .map(query_value)
                .collect::<R<_>>()?,
        ),
        Js::Object(_) => {
            let mut entries = JsObjectMap::new();
            for (key, value) in value.own_entries()? {
                entries.insert(String::from_utf16_lossy(&key), query_value(&value)?);
            }
            QueryValue::Object(entries)
        }
        _ => return Err(Thrown::type_error("Unsupported Orderly query value")),
    })
}
pub(super) fn query(value: &Js, sorted: bool) -> R<Js> {
    use safe_cex_transport::query::*;
    let profile = QueryProfile {
        arrays: QueryArrayFormat::Indexed,
        encode: true,
        key_order: if sorted {
            QueryKeyOrder::JsRoot
        } else {
            QueryKeyOrder::Preserve
        },
    };
    Ok(Js::str(
        &render_query(&query_value(value)?, profile)
            .map_err(|e| Thrown::error(e.name, e.message))?,
    ))
}
impl crate::api::ApiHost for Host {
    fn spread_object(&self, value: V) -> R<V> {
        Ok(v(value.spread_object()?))
    }
    fn delete(&self, value: V, key: &str) -> R<()> {
        value.delete(&key.encode_utf16().collect::<Vec<_>>());
        Ok(())
    }
    fn uppercase_optional(&self, value: V) -> R<V> {
        if value.is_nullish() {
            Ok(v(Js::Undefined))
        } else {
            let function = value.get("toUpperCase")?;
            if function.is_nullish() {
                Ok(v(Js::Undefined))
            } else {
                Ok(v(function.call((*value).clone(), vec![])?))
            }
        }
    }
    fn includes_optional(&self, value: V, needle: &str) -> R<bool> {
        if value.is_nullish() {
            return Ok(false);
        }
        let function = value.get("includes")?;
        if function.is_nullish() {
            Ok(false)
        } else {
            Ok(function
                .call((*value).clone(), vec![Js::str(needle)])?
                .truthy())
        }
    }
    fn keys_len(&self, value: V) -> R<usize> {
        Ok(value.own_keys().len())
    }
    fn query(&self, value: V, sorted: bool) -> R<V> {
        Ok(v(query(&value, sorted)?))
    }
    fn join(&self, values: Vec<V>) -> R<V> {
        let mut out = vec![];
        for value in values {
            if !value.is_nullish() {
                out.extend(value.to_js_string()?.iter().copied());
            }
        }
        Ok(v(Js::utf16(&out)))
    }
    fn axios_headers(&self, fields: V) -> R<V> {
        let headers = Js::object_with_class("AxiosHeaders", vec![]);
        for (key, value) in fields.own_entries()? {
            let name = String::from_utf16_lossy(&key);
            let existing = headers
                .own_keys()
                .into_iter()
                .find(|key| String::from_utf16_lossy(key).eq_ignore_ascii_case(&name));
            headers.define_key(existing.as_deref().unwrap_or(&key), value)?;
        }
        Ok(v(headers))
    }
    fn function(&self, name: &str, body: crate::classes::BoundMember<Self>) -> V {
        Host::function(self, name, body)
    }
    fn axios_create(&self, config: V, options: V, _: V) -> R<V> {
        let rt = self.rt()?;
        let typed = safe_cex_exchange::js_runtime::base_exchange::exchange_options(&options)?;
        Ok(v(safe_cex_exchange::js_runtime::client::factory::create(
            (rt.shared.extra.deps.http)(&typed).map_err(Thrown::from)?,
            &*rt.shared.runtime.transport(),
            (*config).clone(),
            &options,
            rt.base(),
        )?))
    }
    fn network_retry_condition(&self) -> R<V> {
        Ok(v(Js::function("networkErrorOnRead", |_, args| {
            use safe_cex_transport::{RestFailure, RestRequest, retry};
            let error = safe_cex_exchange::js_runtime::value::arg(&args, 0);
            let config = error.get("config")?;
            if config.is_nullish() {
                return Ok(Js::Bool(false));
            }
            let method = config.get("method")?;
            let method = if method.is_undefined() {
                "get".into()
            } else {
                text(&method)?.to_lowercase()
            };
            if !matches!(method.as_str(), "get" | "head" | "options") {
                return Ok(Js::Bool(false));
            }
            let code = error.get("code")?;
            let response = error.get("response")?;
            let failure = RestFailure {
                request: Box::new(RestRequest {
                    method,
                    ..Default::default()
                }),
                code: if code.is_nullish() {
                    None
                } else {
                    Some(text(&code)?)
                },
                message: text(&error.get("message")?)?,
                response: if response.is_nullish() {
                    None
                } else {
                    Some(Box::new(safe_cex_transport::RestResponse {
                        status: response.get("status")?.to_number()? as u16,
                        reason: None,
                        headers: Default::default(),
                        body: safe_cex_transport::RestResponseBody::Untransformed(
                            serde_json::Value::Null,
                        ),
                    }))
                },
                transport_error: None,
                context: None,
            };
            Ok(Js::Bool(retry::is_network_error(&failure)))
        })))
    }
    fn retry(&self, client: V, options: V) -> R<()> {
        client.call_method("__configureRetry", vec![(*options).clone()])?;
        Ok(())
    }
    fn transport(&self, client: V, options: V, name: V) -> R<()> {
        let rt = self.rt()?;
        safe_cex_exchange::js_runtime::client::factory::install_transport_adapter(
            rt.shared.http.clone(),
            &*rt.shared.runtime.transport(),
            &client,
            &options,
            safe_cex_exchange::js_runtime::value::to_typed(&name)?,
            safe_cex_transport::RestAdapterVariant::Default,
        )
    }
}
impl crate::runtime_actions::RuntimeHost for Host {
    fn strict_eq(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn same(&self, a: &Js, b: &Js) -> bool {
        match (a, b) {
            (Js::Number(a), Js::Number(b)) => {
                (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
            }
            _ => a.strict_equals(b),
        }
    }
    fn is_number(&self, value: &Js) -> bool {
        matches!(value, Js::Number(_))
    }
    fn is_object(&self, value: &Js) -> bool {
        value.type_of() == "object"
    }
    fn own(&self, value: V, key: &str) -> R<bool> {
        Ok(value.has_own(key))
    }
    fn keys(&self, value: V) -> R<Vec<Vec<u16>>> {
        Ok(value.own_keys().into_iter().map(|x| x.to_vec()).collect())
    }
    fn find(&self, value: V, mut predicate: impl FnMut(V) -> R<bool>) -> R<V> {
        for item in self.values(&value)? {
            let item = v(item);
            if predicate(item.clone())? {
                return Ok(item);
            }
        }
        Ok(v(Js::Undefined))
    }
    fn adjust(&self, value: V, step: V) -> R<V> {
        self.global("adjust", vec![value, step])
    }
    fn inverse_type(&self, value: V) -> R<V> {
        table(&crate::types::ORDER_TYPE, &value, true)
    }
    fn inverse_side(&self, value: V) -> R<V> {
        table(&crate::types::ORDER_SIDE, &value, true)
    }
    fn omit_undefined(&self, value: V) -> R<V> {
        self.global("omitUndefined", vec![value])
    }
    fn ceil(&self, value: V) -> R<V> {
        Ok(v(Js::Number(value.to_number()?.ceil())))
    }
    fn remainder(&self, a: V, b: V) -> R<V> {
        Ok(v(safe_cex_exchange::js_runtime::ops::remainder(&a, &b)?))
    }
    fn add(&self, a: V, b: V) -> R<V> {
        Ok(v(safe_cex_exchange::js_runtime::ops::add(&a, &b)?))
    }
    fn times(&self, n: V, mut f: impl FnMut() -> R<V>) -> R<V> {
        let count = n.to_number()?;
        if count.is_nan() || !(1.0..=9_007_199_254_740_991.0).contains(&count) {
            return Ok(v(Js::array(vec![])));
        }
        let mut out = vec![];
        for _ in 0..(count.trunc() as usize) {
            out.push((*f()?).clone());
        }
        Ok(v(Js::array(out)))
    }
    async fn map_series<F: std::future::Future<Output = R<V>>>(
        &self,
        rows: V,
        mut f: impl FnMut(V) -> F,
    ) -> R<V> {
        let mut out = vec![];
        for item in self.values(&rows)? {
            out.push((*f(v(item)).await?).clone());
        }
        Ok(v(Js::array(out)))
    }
    fn sum_by(&self, rows: V, key: &str) -> R<V> {
        self.global("sumBy", vec![rows, v(Js::str(key))])
    }
    fn new_error(&self, message: V) -> Thrown {
        match text(&message) {
            Ok(message) => Thrown::error("Error", &message),
            Err(error) => error,
        }
    }
    fn error_log_options(&self, message: V) -> R<V> {
        let message = if message.is_nullish() {
            None
        } else {
            Some(text(&message)?)
        };
        let options = crate::errors::get_orderly_order_error_log_options(message.as_deref());
        Ok(v(if let Some(options) = options {
            from_typed(&options)?
        } else {
            Js::Undefined
        }))
    }
    fn attach_catch(
        &self,
        promise: V,
        callback: crate::runtime_actions::LeverageCatch<Self>,
    ) -> R<()> {
        let rt = self.rt()?;
        let runtime = rt.shared.runtime.clone();
        rt.spawn(Box::pin(async move {
            if let Err(error) =
                safe_cex_exchange::js_runtime::promise::await_js((*promise).clone()).await
            {
                if let Err(error) = callback.invoke(error) {
                    runtime.report_exception(
                        &error.to_exchange_error(),
                        &serde_json::json!({"source":"Orderly leverage catch"}),
                    );
                }
            }
            Ok(Js::Undefined)
        }));
        Ok(())
    }
    fn schedule(&self, callback: crate::runtime_actions::TickCallback<Self>, interval: V) {
        let result = (|| -> R<()> {
            let rt = self.rt()?;
            let timer = Js::function("loop", move |_, _| Ok((*callback.invoke()?).clone()));
            rt.global("loop")?
                .call(Js::Undefined, vec![timer, (*interval).clone()])?;
            Ok(())
        })();
        if let Err(error) = result {
            if let Ok(rt) = self.rt() {
                rt.shared.runtime.report_exception(
                    &error.to_exchange_error(),
                    &serde_json::json!({"source":"Orderly loop"}),
                );
            }
        }
    }
}
impl crate::trading_actions::ActionHost for Host {
    fn has(&self, value: &V, key: &str) -> R<bool> {
        Ok(value.has_own(key))
    }
    fn strict_eq(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn finite_number(&self, value: &Js) -> bool {
        matches!(value,Js::Number(x)if x.is_finite())
    }
    fn true_value(&self) -> V {
        v(Js::Bool(true))
    }
    fn numeric(&self, value: V) -> R<V> {
        Ok(v(Js::Number(value.to_number()?)))
    }
    fn parse_int(&self, value: V, radix: u32) -> R<V> {
        self.global("parseInt", vec![value, v(Js::Number(radix as f64))])
    }
    fn math_min(&self, a: V, b: V) -> R<V> {
        self.global("Math.min", vec![a, b])
    }
    fn math_max(&self, a: V, b: V) -> R<V> {
        self.global("Math.max", vec![a, b])
    }
    fn math_round(&self, value: V) -> R<V> {
        Ok(v(Js::Number(safe_cex_num::math_round(value.to_number()?))))
    }
    fn reverse_symbol(&self, value: V) -> R<V> {
        Ok(v(Js::str(&crate::utils::reverse_symbol(&text(&value)?))))
    }
    fn new_error(&self, message: V) -> Thrown {
        crate::runtime_actions::RuntimeHost::new_error(self, message)
    }
    fn filter(&self, rows: V, f: impl FnMut(V) -> R<bool>) -> R<V> {
        crate::data_mappers::DataHost::filter(self, rows, f)
    }
    fn map(&self, rows: V, f: impl FnMut(V) -> R<V>) -> R<V> {
        crate::data_mappers::DataHost::map(self, rows, f)
    }
    fn find(&self, rows: V, f: impl FnMut(V) -> R<bool>) -> R<V> {
        crate::runtime_actions::RuntimeHost::find(self, rows, f)
    }
    async fn series<F: std::future::Future<Output = R<()>>>(
        &self,
        rows: V,
        mut callback: impl FnMut(V) -> F,
    ) -> R<()> {
        for row in self.values(&rows)? {
            callback(v(row)).await?;
        }
        Ok(())
    }
}
impl crate::classes::ClassHost for Host {
    fn spread_object(&self, value: V) -> R<V> {
        Ok(v(value.spread_object()?))
    }
    fn starts_with(&self, value: V, prefix: &str) -> R<bool> {
        Ok(value
            .call_method("startsWith", vec![Js::str(prefix)])?
            .truthy())
    }
    fn super_exchange(&self, _: crate::classes::Kind, name: V, options: V, store: V) -> R<V> {
        let rt = self.rt()?;
        let mut parts = rt
            .shared
            .extra
            .parts
            .borrow_mut()
            .take()
            .ok_or_else(|| Thrown::error("Error", "Orderly base already constructed"))?;
        parts.options = safe_cex_exchange::js_runtime::base_exchange::exchange_options(&options)?;
        let (this, base) = safe_cex_exchange::js_runtime::base_exchange::create(
            &rt.shared.sockets,
            parts,
            (*name).clone(),
            (*options).clone(),
            (*store).clone(),
            |_| Ok(()),
        )?;
        if let Js::Object(object) = &this {
            *object.proto.borrow_mut() = Some(rt.shared.extra.prototype.clone());
        }
        let _ = rt.shared.base.set(base);
        let _ = rt.shared.extra.owner.set(this.clone());
        Ok(v(this))
    }
    fn define(&self, receiver: V, name: &str, value: V) -> R<()> {
        receiver.define(name, (*value).clone())
    }
    fn fresh_map(&self) -> R<V> {
        Ok(v(Js::new_map()))
    }
    fn client_rule(&self, name: &str, pattern: &str) -> R<V> {
        let label = name.to_owned();
        let predicate = self
            .rt()?
            .base()
            .settings
            .client_order_id_rule
            .as_ref()
            .expect("Orderly client ID rule")
            .is_valid
            .clone();
        Ok(v(Js::object(vec![
            ("label", Js::str(&label)),
            ("requirement", Js::str(&format!("match /{pattern}/"))),
            (
                "isValid",
                Js::function("isValid", move |_, args| {
                    Ok(Js::Bool(predicate(&text(
                        &safe_cex_exchange::js_runtime::value::arg(&args, 0),
                    )?)))
                }),
            ),
        ])))
    }
    fn imported(&self, name: &'static str) -> V {
        match name {
            "GRAFANA_ENDPOINTS_EVERY" => {
                v(from_typed(&crate::types::GRAFANA_ENDPOINTS_EVERY).expect("literal table"))
            }
            "GRAFANA_ENDPOINTS_PERIODIC" => {
                v(from_typed(&crate::types::GRAFANA_ENDPOINTS_PERIODIC).expect("literal table"))
            }
            _ => unreachable!("source import"),
        }
    }
    fn member(&self, name: &'static str, _: &'static str) -> crate::classes::NativeMember<Self> {
        let host = self.clone();
        Rc::new(move |receiver, args| super::members::invoke(&host, receiver, name, args))
    }
    fn arrow(&self, name: &'static str, body: crate::classes::BoundMember<Self>) -> V {
        Host::function(self, name, body)
    }
    fn prototype(&self, name: &'static str, body: crate::classes::NativeMember<Self>) {
        if let Ok(rt) = self.rt() {
            if !rt.shared.extra.prototype.has_own(name) {
                let weak = self.weak.clone();
                let function = Js::function(name, move |receiver, args| {
                    let rt = super::scope(&weak, receiver.clone())?;
                    match body(v(receiver), args.into_iter().map(v).collect())? {
                        crate::classes::Return::Value(value) => Ok((*value).clone()),
                        crate::classes::Return::Future(future) => {
                            Ok(rt.spawn(Box::pin(async move { Ok((*future.await?).clone()) })))
                        }
                    }
                });
                rt.shared
                    .extra
                    .prototype
                    .define(name, function)
                    .expect("fresh prototype property");
            }
        }
    }
    fn create_api(&self, options: V, receiver: V) -> R<V> {
        crate::api::create_api(self, options, receiver)
    }
    fn rate_limit(&self, client: V, max_rps: f64) -> R<V> {
        client.call_method(
            "__configureRateLimit",
            vec![Js::object(vec![("maxRPS", Js::Number(max_rps))])],
        )?;
        Ok(client)
    }
    fn construct_socket(&self, private: bool, parent: V) -> R<V> {
        crate::socket_classes::construct(self, private, vec![parent])
    }
}
impl crate::public_feeds::FeedHost for Host {
    fn function(&self, name: &str, body: crate::classes::BoundMember<Self>) -> V {
        Host::function(self, name, body)
    }
    fn timeout(&self, callback: V, delay: f64) -> R<V> {
        self.global("setTimeout", vec![callback, v(Js::Number(delay))])
    }
    fn clear_timeout(&self, id: V) -> R<()> {
        self.global("clearTimeout", vec![id])?;
        Ok(())
    }
    fn spawn(&self, future: crate::classes::BodyFuture<Self>) {
        if let Ok(rt) = self.rt() {
            let runtime = rt.shared.runtime.clone();
            rt.spawn(Box::pin(async move {
                if let Err(error) = future.await {
                    runtime.report_exception(
                        &error.to_exchange_error(),
                        &serde_json::json!({"source":"Orderly public feed"}),
                    );
                }
                Ok(Js::Undefined)
            }));
        }
    }
    fn strict_eq(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn delete(&self, object: V, key: &[u16]) -> R<()> {
        object.delete(key);
        Ok(())
    }
    fn for_each(&self, rows: V, mut body: impl FnMut(V) -> R<()>) -> R<()> {
        for row in self.values(&rows)? {
            body(v(row))?;
        }
        Ok(())
    }
    fn pair(&self, value: V) -> R<(V, V)> {
        let values = self.values(&value)?;
        Ok((
            v(values.first().cloned().unwrap_or(Js::Undefined)),
            v(values.get(1).cloned().unwrap_or(Js::Undefined)),
        ))
    }
    fn update_level(&self, book: V, side: &str, update: V) -> R<()> {
        self.global("updateOrderBook", vec![book, v(Js::str(side)), update])?;
        Ok(())
    }
    fn calc_total(&self, book: V) -> R<()> {
        self.global("calcOrderBookTotal", vec![book])?;
        Ok(())
    }
}
impl crate::stream_reports::ReportHost for Host {
    fn ticker_patch_refusal(&self, symbol: &[u16], fields: &[(&str, V)]) -> R<Option<String>> {
        for (field, value) in fields {
            if !matches!(&**value, Js::Number(_)) {
                let rendered = self.global("JSON.stringify", vec![value.clone()])?;
                return Ok(Some(format!(
                    "Orderly ticker {} typed field {field} requires a number; received {}",
                    String::from_utf16_lossy(symbol),
                    text(&rendered)?
                )));
            }
        }
        Ok(None)
    }
    fn ticker_refusal_error(&self, messages: Vec<String>) -> Thrown {
        Thrown::error("Error", &messages.join("; "))
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn undefined(&self) -> V {
        v(Js::Undefined)
    }
    fn text_eq(&self, value: &Js, expected: &str) -> bool {
        value.strict_equals(&Js::str(expected))
    }
    fn is_array(&self, value: &Js) -> bool {
        matches!(value, Js::Array(_))
    }
    fn array(&self, values: Vec<V>) -> V {
        v(Js::array(
            values.into_iter().map(|x| (*x).clone()).collect(),
        ))
    }
    fn for_each(&self, value: V, mut callback: impl FnMut(V) -> R<()>) -> R<()> {
        for item in self.values(&value)? {
            callback(v(item))?;
        }
        Ok(())
    }
    fn starts_with(&self, value: V, prefix: &str) -> R<bool> {
        Ok(value
            .call_method("startsWith", vec![Js::str(prefix)])?
            .truthy())
    }
    fn normalize_symbol(&self, value: V) -> R<Vec<u16>> {
        Ok(crate::utils::normalize_symbol(&text(&value)?)
            .encode_utf16()
            .collect())
    }
    fn get_key(&self, value: V, key: &str) -> R<V> {
        get_key(&value, key)
    }
    fn parse_float(&self, value: V) -> R<V> {
        self.global("parseFloat", vec![value])
    }
    fn order_side(&self, value: V) -> R<V> {
        table(&crate::types::ORDER_SIDE, &value, false)
    }
}
pub(super) fn get_key(value: &Js, key: &str) -> R<V> {
    if value.is_nullish() {
        return Ok(v(Js::Undefined));
    }
    if !value.is_object() {
        return Err(Thrown::type_error(
            "Cannot use 'in' operator on a primitive value",
        ));
    }
    fn present(value: &Js, key: &str) -> R<bool> {
        if value.has_own(key) {
            return Ok(true);
        }
        if let Js::Object(object) = value {
            if let Some(proto) = object.proto.borrow().as_ref() {
                return present(proto, key);
            }
        }
        Ok(false)
    }
    for key in [
        key.to_owned(),
        safe_cex_num::lodash_snake_case(key),
        safe_cex_num::lodash_camel_case(key),
    ] {
        if present(value, &key)? {
            return Ok(v(value.get(&key)?));
        }
    }
    Ok(v(Js::Undefined))
}
