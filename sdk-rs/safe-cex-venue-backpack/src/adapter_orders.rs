//! Exchange account and batch entrypoints over the owned transport and store.
use crate::native_value::NativeValue;
use crate::{adapter::BackpackExchange, order_utils as mapping};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::JsJsonValue;
use safe_cex_types::{Order, PlaceOrderOpts};
use std::rc::Rc;
impl BackpackExchange {
    pub fn validate_account(
        self: &Rc<Self>,
    ) -> ExchangeFuture<'static, crate::account_reads::ValidationResult<safe_cex_types::Balance>>
    {
        let xhr = self.xhr.clone();
        let balance = self.clone();
        let emit = self.clone();
        crate::account_reads::validate_account(
            move |url| {
                let f = xhr.request("GET", url, JsJsonValue::Undefined, JsJsonValue::Undefined);
                Ok(async move { f.await.map(|_| ()).map_err(validation_failure) })
            },
            move || {
                let f = balance.fetch_balance();
                Ok(async move { f.await.map_err(validation_failure) })
            },
            move |m| {
                emit.base.emit_and_log_error(
                    safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(
                        crate::native_value::Value::from_prepared(m.clone()).utf8()?,
                    ),
                    None,
                    None,
                )
            },
        )
    }
    pub fn cancel_orders(self: &Rc<Self>, orders: Vec<Order>) -> ExchangeFuture<'static, ()> {
        let orders = Rc::new(orders);
        let len = orders.clone();
        let owner = self.clone();
        crate::account_reads::cancel_orders(
            move || Ok(len.len()),
            move |i| Ok(orders.get(i).cloned().map(|o| std::future::ready(Ok(o)))),
            move |o| Ok(owner.cancel_order(o)),
        )
    }
    pub fn cancel_all_orders(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let len = self.clone();
        let element = self.clone();
        let xhr = self.xhr.clone();
        let log = self.clone();
        crate::account_reads::cancel_all_orders(
            move || Ok(len.base.store.markets()?.len()),
            move |i| {
                Ok(element
                    .base
                    .store
                    .markets()?
                    .get(i)
                    .cloned()
                    .map(|m| std::future::ready(Ok(m))))
            },
            |m: safe_cex_types::Market| Ok(m.id),
            move |url, config| {
                let JsJsonValue::Object(c) = config else {
                    return Err(ExchangeError::new("Cancellation config mismatch"));
                };
                let data = c
                    .into_iter()
                    .find(|(k, _)| k == "data")
                    .map(|(_, v)| v)
                    .unwrap_or(JsJsonValue::Undefined);
                let f = xhr.request("DELETE", url, JsJsonValue::Undefined, data);
                Ok(async move {
                    f.await?;
                    Ok(())
                })
            },
            move |m, s| log.base.log(m, s),
        )
    }
    pub fn set_leverage(self: &Rc<Self>, leverage: f64) -> ExchangeFuture<'static, ()> {
        let f = self.xhr.request(
            "PATCH",
            "/api/v1/account",
            JsJsonValue::Undefined,
            JsJsonValue::Object(vec![(
                "leverageLimit".into(),
                JsJsonValue::String(safe_cex_num::js_number_string(leverage)),
            )]),
        );
        let owner = self.clone();
        let log = self.clone();
        let emit = self.clone();
        let fetch = self.clone();
        let update = self.clone();
        Box::pin(async move {
            crate::account_reads::set_leverage(
                &owner.account_leverage_limit,
                leverage,
                move |_, _| {
                    Ok(async move {
                        f.await?;
                        Ok(())
                    })
                },
                move |m, s| log.base.log(m, s),
                move |ev, e| {
                    emit.base.emit(
                        ev,
                        safe_cex_exchange::js_runtime::value::Thrown::from(e).value,
                    )
                },
                move || {
                    let f = fetch.fetch_positions();
                    Ok(async move {
                        f.await?
                            .into_iter()
                            .map(crate::model_bridge::position)
                            .collect::<Result<Vec<_>, _>>()
                    })
                },
                move |p| {
                    update
                        .base
                        .store
                        .update(
                            safe_cex_store::StoreUpdate {
                                positions: Some(p.positions),
                                ..Default::default()
                            },
                            false,
                        )
                        .map_err(Into::into)
                },
            )
            .await
        })
    }
    pub fn change_position_mode(&self, hedged: bool) -> ExchangeFuture<'static, ()> {
        crate::account_reads::change_position_mode(
            hedged,
            |ev, e| {
                self.base.emit(
                    ev,
                    safe_cex_exchange::js_runtime::value::Js::str(&e.message),
                )
            },
            |m, s| self.base.log(m, s),
        )
    }
    pub fn change_margin_type(
        &self,
        isolated: bool,
        symbol: Option<&str>,
    ) -> ExchangeFuture<'static, ()> {
        crate::account_reads::change_margin_type(
            isolated,
            symbol,
            |ev, e| {
                self.base.emit(
                    ev,
                    safe_cex_exchange::js_runtime::value::Js::str(&e.message),
                )
            },
            |m, s| self.base.log(m, s),
        )
    }
    pub fn is_ordinary_limit_order(&self, o: &PlaceOrderOpts) -> bool {
        mapping::is_ordinary_limit_order(o)
    }
    pub fn build_batch_limit_order_request(
        &self,
        o: &PlaceOrderOpts,
        id: f64,
    ) -> Result<crate::types::BackpackOrderRequest, ExchangeError> {
        crate::batch_orders::build_batch_limit_order_request(o, id, |s| {
            Ok(self
                .base
                .store
                .markets()?
                .iter()
                .find(|m| m.symbol == s)
                .map(|m| crate::batch_orders::BatchMarket {
                    id: m.id.clone(),
                    active: m.active,
                    read_only: m.is_read_only.unwrap_or(false),
                    amount_min: m.limits.amount.min,
                    amount_max: safe_cex_num::numeric_value_to_number(&m.limits.amount.max),
                    price_precision: m.precision.price,
                    amount_precision: m.precision.amount,
                }))
        })
    }
    pub fn can_use_native_batch(&self, o: &PlaceOrderOpts) -> Result<bool, ExchangeError> {
        mapping::can_use_native_batch(
            o,
            |o| Ok(mapping::is_ordinary_limit_order(o)),
            |o| Ok(o.client_order_id.is_some()),
            |o, id| self.build_batch_limit_order_request(o, id),
        )
    }
    pub fn next_batch_client_id(&self) -> Result<f64, ExchangeError> {
        mapping::next_batch_client_id(
            None::<fn(&mut [u32; 1]) -> Result<[u32; 1], ExchangeError>>,
            || Ok(self.runtime.random()),
        )
    }
    pub fn create_batch_client_ids(&self, count: f64) -> Result<Vec<f64>, ExchangeError> {
        mapping::create_batch_client_ids(count, || self.next_batch_client_id())
    }
    pub fn decode_batch_order_response(
        &self,
        data: &JsJsonValue,
        requests: &[crate::types::BackpackOrderRequest],
    ) -> Result<Vec<String>, ExchangeError> {
        mapping::decode_batch_order_response(data, requests)
    }
    pub fn place_batch_pair(
        self: &Rc<Self>,
        orders: [PlaceOrderOpts; 2],
    ) -> ExchangeFuture<'static, Vec<String>> {
        let ids = self.clone();
        let build = self.clone();
        let xhr = self.xhr.clone();
        let decode = self.clone();
        mapping::place_batch_pair(
            &orders,
            move |n| ids.create_batch_client_ids(n as f64),
            move |o, id| build.build_batch_limit_order_request(o, id.unwrap_or(f64::NAN)),
            move |url, requests| {
                let wire = serde_json::to_value(&*requests.borrow())
                    .map_err(|e| ExchangeError::new(e.to_string()))?;
                let f = xhr.request("POST", url, JsJsonValue::Undefined, wire.into());
                Ok(async move { f.await?.prepared() })
            },
            move |data, requests| decode.decode_batch_order_response(&data, &requests.borrow()),
        )
    }
    pub fn place_orders(
        self: &Rc<Self>,
        orders: Vec<PlaceOrderOpts>,
    ) -> ExchangeFuture<'static, Vec<String>> {
        let orders = Rc::new(orders);
        let pre = self.clone();
        let check = orders.clone();
        let len = orders.clone();
        let batch = self.clone();
        let pair = self.clone();
        let single = self.clone();
        let emit = self.clone();
        let log = self.clone();
        crate::batch_orders::place_orders(
            move || pre.base.assert_caller_client_order_ids(&check),
            move || Ok(len.len()),
            move |i| Ok(orders.get(i).cloned()),
            move |o| {
                o.as_ref()
                    .map_or(Ok(false), |o| batch.can_use_native_batch(o))
            },
            move |o| {
                Ok(pair.place_batch_pair([
                    o[0].clone()
                        .ok_or_else(|| ExchangeError::new("Missing first pair"))?,
                    o[1].clone()
                        .ok_or_else(|| ExchangeError::new("Missing second pair"))?,
                ]))
            },
            move |o| {
                let o = o.ok_or_else(|| ExchangeError::new("Missing order"))?;
                let f = single.place_order(crate::model_bridge::action(o));
                Ok(Box::pin(async move {
                    f.await?
                        .into_iter()
                        .map(|s| {
                            String::from_utf16(&s).map_err(|e| ExchangeError::new(e.to_string()))
                        })
                        .collect::<Result<_, _>>()
                }) as ExchangeFuture<'static, Vec<String>>)
            },
            move |e, m| {
                emit.base.emit_and_log_error(
                    safe_cex_exchange::error_reporting::ExchangeErrorInput::Error(Box::new(e)),
                    Some(m),
                    None,
                )
            },
            |e| Ok(e.js_error_string().encode_utf16().collect()),
            move |m, s| {
                log.base.log(
                    &String::from_utf16(m).map_err(|e| ExchangeError::new(e.to_string()))?,
                    s,
                )
            },
        )
    }
}

fn validation_failure(e: ExchangeError) -> crate::account_reads::ValidationFailure {
    std::rc::Rc::new(std::cell::RefCell::new(JsJsonValue::Object(vec![
        ("message".into(), JsJsonValue::String(e.message.clone())),
        (
            "response".into(),
            e.response_json_view()
                .map(Into::into)
                .unwrap_or(JsJsonValue::Undefined),
        ),
    ])))
}
