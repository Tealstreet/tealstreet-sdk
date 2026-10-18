//! Order preparation, transaction signing, sequential submission and replacement.
use crate::{
    api::serialize_apex_payload_legacy,
    data::{get, truthy},
    decimal::{Decimal, rounded_ratio},
    exchange::{ApexOmniExchange, ApexRequest, failure, text},
    exchange_utils,
    order_records::Iso8601,
    protocol,
    transaction_records::SignContractPayload,
    zklink_host::ZkLinkHost,
};
use num_bigint::BigUint;
use safe_cex_exchange::{error_reporting::ExchangeErrorInput, runtime::ExchangeError};
use safe_cex_num::{JsJsonValue as J, adjust, js_number_string};
use safe_cex_types::retained_json::RetainedJsonInput as R;
use safe_cex_types::{
    Order, OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PreparedOrderOpts,
    UpdateOrderOpts, UpdateOrderOptsUpdate,
};
fn number(value: f64) -> J {
    J::Number(value)
}
fn string(value: impl Into<String>) -> J {
    J::String(value.into())
}
fn present(value: Option<f64>) -> Option<f64> {
    value.filter(|v| *v != 0. && !v.is_nan())
}
fn fields(values: Vec<(&str, J)>) -> J {
    J::Object(
        values
            .into_iter()
            .filter(|(_, v)| !matches!(v, J::Undefined))
            .map(|(k, v)| (k.into(), v))
            .collect(),
    )
}
fn add(record: &mut J, key: &str, value: J) {
    let J::Object(fields) = record else {
        unreachable!()
    };
    fields.push((key.into(), value));
}
fn from_order(order: &Order) -> PreparedOrderOpts {
    PreparedOrderOpts {
        symbol: order.symbol.clone(),
        r#type: order.r#type.into_property(),
        side: order.side.into_property(),
        amount: order.amount,
        price: order.price.into_value(),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: order.reduce_only.into_property(),
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        max_slippage_pct: None,
        close: None,
        implied_volatility: None,
    }
}
impl ApexOmniExchange {
    fn new_client_id(&self, account: &str) -> Result<String, ExchangeError> {
        let services = self.host.services()?;
        Ok(exchange_utils::generate_apex_random_client_id(
            account,
            || services.date_now_ms(),
            || self.base.runtime.random(),
        ))
    }
    async fn order_signature(
        &self,
        signer: &dyn ZkLinkHost,
        config: &R,
        pair: f64,
        id: &str,
        price: &J,
        size: &J,
        direction: bool,
    ) -> Result<J, ExchangeError> {
        let digest = exchange_utils::hash_client_order_id(&id.encode_utf16().collect::<Vec<_>>());
        let nonce = BigUint::from_bytes_be(&digest);
        let u32max = u32::MAX as u64;
        let low32 = (&nonce % BigUint::from(u32max))
            .to_string()
            .parse()
            .expect("u32 nonce");
        let low64 = &nonce % BigUint::from(u64::MAX);
        let account = crate::response::get(config, "contractAccount");
        let payload = SignContractPayload {
            direction,
            fee_rates: [
                (crate::response::float(crate::response::get(account, "makerFeeRate"))? * 1e4)
                    .ceil(),
                (crate::response::float(crate::response::get(account, "takerFeeRate"))? * 1e4)
                    .ceil(),
            ],
            nonce: low32,
            pair_id: pair,
            price: Decimal::from_value(if truthy(price) { price } else { &J::Number(0.) })
                .scaled_integer(18),
            size: Decimal::from_value(if truthy(size) { size } else { &J::Number(0.) })
                .scaled_integer(18),
            slot_id: rounded_ratio(low64, u32max),
            // Only the scalar consumed by the decimal signer crosses its strict prepared boundary.
            account_id: Decimal::from_value(&if crate::response::get(config, "id").is_truthy() {
                J::String(crate::response::text(crate::response::get(config, "id"))?)
            } else {
                J::Number(0.)
            })
            .modulo_number(u32max),
            sub_account_id: 0.,
            has_subsidy: Some(false),
        };
        Self::sign_order(signer, &payload, &J::Undefined).await
    }
    fn limit_fee(&self, price: &J, size: &J, rate: &J) -> Result<String, ExchangeError> {
        let zero = J::String("0".into());
        let product = Decimal::from_value(if truthy(price) { price } else { &zero })
            .multiply(&Decimal::from_value(if truthy(rate) {
                rate
            } else {
                &zero
            }))
            .multiply(&Decimal::from_value(if truthy(size) {
                size
            } else {
                &zero
            }))
            .to_number();
        exchange_utils::fee_helper(&Iso8601::Number(product), Some(6.), |v| Ok(v.ceil()))
            .map_err(failure)
    }
    // ts: ApexOmniExchange.placeOrder
    pub async fn place_order(
        &self,
        opts: impl Into<PreparedOrderOpts>,
    ) -> Result<Vec<String>, ExchangeError> {
        let opts = opts.into();
        let caller = self.base.resolve_caller_client_order_id(&opts)?;
        let market = self
            .base
            .store
            .markets_by_symbol()
            .get(&opts.symbol)
            .cloned()
            .ok_or_else(|| failure(format!("Market {} not found", opts.symbol)))?;
        let max_size = safe_cex_num::numeric_value_to_number(&market.limits.amount.max);
        let p_price = market.precision.price;
        let p_amount = market.precision.amount;
        let amount = adjust(opts.amount, p_amount);
        let price = present(opts.price).map(|v| adjust(v, p_price));
        let stop_loss = present(opts.stop_loss).map(|v| adjust(v, p_price));
        let take_profit = present(opts.take_profit).map(|v| adjust(v, p_price));
        if amount > max_size && (present(stop_loss).is_some() || present(take_profit).is_some()) {
            let message = "ApeX Omni cannot atomically attach TP/SL to split orders";
            self.base.emit_and_log_error(
                ExchangeErrorInput::Message(message.into()),
                None,
                None,
            )?;
            return Err(failure(message.into()));
        }
        let services = self.host.services()?;
        services
            .sdk_initialization()
            .init_sdk(services.sdk())
            .await?;
        let signer = services
            .new_signer(self.base.options().omni_key_seed.clone())
            .await?;
        let side = if opts.side == OrderSide::Buy {
            "BUY"
        } else {
            "SELL"
        };
        let trigger = present(opts.trigger_price).map(|v| adjust(v, p_price));
        let execution = match opts.r#type.into_value() {
            Some(OrderType::Limit) => price.map_or_else(|| "null".into(), js_number_string),
            Some(OrderType::Market) => {
                let ticker = self
                    .base
                    .store
                    .tickers_by_symbol()
                    .get(&opts.symbol)
                    .cloned()
                    .ok_or_else(|| failure("typed market order requires a ticker".into()))?;
                js_number_string(adjust(
                    ticker.mark
                        * if opts.side == OrderSide::Buy {
                            1.02
                        } else {
                            0.98
                        },
                    p_price,
                ))
            }
            Some(OrderType::StopLoss | OrderType::TakeProfit) => {
                let effective = present(trigger).or(present(price)).unwrap_or_else(|| {
                    self.base
                        .store
                        .tickers_by_symbol()
                        .get(&opts.symbol)
                        .map_or(f64::NAN, |t| t.mark)
                });
                js_number_string(exchange_utils::calculate_tp_sl_price(
                    side, effective, p_price,
                ))
            }
            _ => js_number_string(p_price),
        };
        let mapped_type = protocol::ORDER_TYPE
            .iter()
            .rev()
            .find(|(_, t)| *t == opts.r#type)
            .map_or(J::Undefined, |(s, _)| string(*s));
        let tif = opts
            .time_in_force
            .unwrap_or(OrderTimeInForce::GoodTillCancel);
        let tif = protocol::ORDER_TIME_IN_FORCE
            .iter()
            .rev()
            .find(|(_, t)| *t == tif)
            .map_or(J::Undefined, |(s, _)| string(*s));
        let trigger_price = if matches!(
            opts.r#type.into_value(),
            Some(OrderType::StopLoss | OrderType::TakeProfit)
        ) {
            present(trigger)
                .or(price)
                .map_or_else(|| string("null"), |v| string(js_number_string(v)))
        } else {
            J::Undefined
        };
        let trigger_type = match opts.r#type.into_value() {
            Some(OrderType::StopLoss) => string(
                if opts.sl_trigger_type == Some(OrderStopTriggerType::LastPrice) {
                    "MARKET"
                } else {
                    "INDEX"
                },
            ),
            Some(OrderType::TakeProfit) => string(
                if opts.tp_trigger_type == Some(OrderStopTriggerType::IndexPrice) {
                    "INDEX"
                } else {
                    "MARKET"
                },
            ),
            _ => J::Undefined,
        };
        let req = fields(vec![
            (
                "symbol",
                string(if opts.symbol.contains('-') {
                    opts.symbol.clone()
                } else {
                    market.id
                }),
            ),
            (
                "side",
                opts.side.into_value().map_or(J::Undefined, |side| {
                    string(match side {
                        OrderSide::Buy => "BUY",
                        OrderSide::Sell => "SELL",
                    })
                }),
            ),
            ("type", mapped_type),
            ("size", string(js_number_string(amount))),
            ("price", string(execution)),
            ("reduceOnly", J::Bool(opts.reduce_only.unwrap_or(false))),
            ("triggerPrice", trigger_price),
            ("triggerPriceType", trigger_type),
            ("timeInForce", tif),
            ("brokerId", string(protocol::BROKER_ID)),
        ]);
        let rendered = safe_cex_num::js_json_stringify(&req)
            .map_err(|e| failure(e.message.into()))?
            .ok_or_else(|| failure("order request did not stringify".into()))?;
        self.base
            .log(&format!("Order request: {rendered}"), "info")?;
        let lots = if amount > max_size {
            (amount / max_size).ceil()
        } else {
            1.
        };
        let rest = if amount > max_size {
            adjust(amount % max_size, p_amount)
        } else {
            0.
        };
        let lot_size = adjust((amount - rest) / lots, p_amount);
        // lodash times returns [] for counts outside 1..=MAX_SAFE_INTEGER.
        let count = if (1. ..=9_007_199_254_740_991.).contains(&lots) {
            lots as usize
        } else {
            0
        };
        let mut payloads = Vec::new();
        for _ in 0..count {
            let mut payload = req.clone();
            let J::Object(fields) = &mut payload else {
                unreachable!()
            };
            fields.iter_mut().find(|(k, _)| k == "size").unwrap().1 =
                string(js_number_string(lot_size));
            payloads.push(payload);
        }
        if rest != 0. && !rest.is_nan() {
            let mut payload = req;
            let J::Object(fields) = &mut payload else {
                unreachable!()
            };
            fields.iter_mut().find(|(k, _)| k == "size").unwrap().1 =
                string(js_number_string(rest));
            payloads.push(payload);
        }
        let mut responses = Vec::new();
        for (index, mut payload) in payloads.into_iter().enumerate() {
            let config = self
                .base
                .store
                .zk_link_config()
                .as_ref()
                .map_or(R::Undefined, |v| (**v).clone());
            let account_id = crate::response::text(crate::response::get(&config, "id"))?;
            let client = match &caller {
                Some(id) => id.clone(),
                None => self.new_client_id(&account_id)?,
            };
            let expiry = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
                + 30. * 24. * 60. * 60. * 1000.;
            let mut pair = 0.;
            for contract in crate::response::array(crate::response::get(
                crate::response::get(&config, "contractConfig"),
                "perpetualContract",
            ))
            .map_err(failure)?
            {
                if crate::response::get(contract, "symbol")
                    .text_equals(&text(get(&payload, "symbol"))?)
                {
                    pair = crate::response::numeric(crate::response::get(contract, "l2PairId"))?;
                }
            }
            let rate = crate::response::get(
                crate::response::get(&config, "contractAccount"),
                "takerFeeRate",
            );
            let rate = if rate.is_truthy() {
                J::String(crate::response::text(rate)?)
            } else {
                J::Number(0.)
            };
            let limit_fee = self.limit_fee(get(&payload, "price"), get(&payload, "size"), &rate)?;
            let signature = self
                .order_signature(
                    signer.as_ref(),
                    &config,
                    pair,
                    &client,
                    get(&payload, "price"),
                    get(&payload, "size"),
                    opts.side == OrderSide::Buy,
                )
                .await?;
            let expiration = exchange_utils::add_order_expiration_buffer_hours(
                exchange_utils::iso_timestamp_to_epoch_hours(&js_number_string(expiry)),
            ) * 60.
                * 60.
                * 1000.;
            add(&mut payload, "clientId", string(&client));
            add(&mut payload, "signature", signature);
            add(&mut payload, "limitFee", string(limit_fee));
            add(&mut payload, "expiration", number(expiration));
            add(&mut payload, "clientOrderId", string(&client));
            if index == 0 && (present(take_profit).is_some() || present(stop_loss).is_some()) {
                add(&mut payload, "isOpenTpslOrder", J::Bool(true));
            }
            if index == 0 {
                for (prefix, value, is_tp) in [("tp", take_profit, true), ("sl", stop_loss, false)]
                {
                    if let Some(value) = present(value) {
                        add(
                            &mut payload,
                            if is_tp { "isSetOpenTp" } else { "isSetOpenSl" },
                            J::Bool(true),
                        );
                        let client = self.new_client_id(&account_id)?;
                        let side = if get(&payload, "side") == &string("BUY") {
                            "SELL"
                        } else {
                            "BUY"
                        };
                        let price = exchange_utils::calculate_tp_sl_price(side, value, p_price);
                        let size = get(&payload, "size").clone();
                        let fee = self.limit_fee(&string(js_number_string(price)), &size, &rate)?;
                        let sig = self
                            .order_signature(
                                signer.as_ref(),
                                &config,
                                pair,
                                &client,
                                &number(if price == 0. || price.is_nan() {
                                    0.
                                } else {
                                    price
                                }),
                                &size,
                                opts.side != OrderSide::Buy,
                            )
                            .await?;
                        let trigger = if is_tp {
                            if opts.tp_trigger_type == Some(OrderStopTriggerType::IndexPrice) {
                                "INDEX"
                            } else {
                                "MARKET"
                            }
                        } else if opts.sl_trigger_type == Some(OrderStopTriggerType::LastPrice) {
                            "MARKET"
                        } else {
                            "INDEX"
                        };
                        for (suffix, v) in [
                            ("ClientOrderId", string(client)),
                            ("Expiration", number(expiration)),
                            ("LimitFee", string(fee)),
                            ("Price", string(js_number_string(price))),
                            ("Side", string(side)),
                            ("Signature", sig),
                            ("Size", size),
                            ("TriggerPrice", number(value)),
                            ("TriggerPriceType", string(trigger)),
                        ] {
                            add(&mut payload, &format!("{prefix}{suffix}"), v);
                        }
                    }
                }
            }
            let data = serialize_apex_payload_legacy(&payload).map_err(failure)?;
            let response = self
                .host
                .request(ApexRequest {
                    method: "post",
                    path: "/v3/order",
                    params: J::Undefined,
                    data: string(data),
                    content_type: Some("application/x-www-form-urlencoded; charset=UTF-8"),
                    timeout_ms: None,
                    unlimited: true,
                })
                .await?;
            responses.push(response);
        }
        // The source maps IDs only after mapSeries finishes all submissions.
        let ids = responses
            .iter()
            .filter(|response| {
                !matches!(
                    response,
                    safe_cex_types::retained_json::RetainedJsonInput::Undefined
                )
            })
            .map(|response| {
                crate::response::property(crate::response::get(response, "data"), "id").cloned()
            })
            .collect::<Result<Vec<_>, _>>()?;
        services.throttle_fetch_orders()?;
        ids.into_iter()
            .map(|id| {
                if matches!(
                    id,
                    safe_cex_types::retained_json::RetainedJsonInput::Undefined
                ) {
                    return Err(failure("ApexOmni placement response is missing id".into()));
                }
                crate::response::text(&id)
            })
            .collect()
    }
    // ts: ApexOmniExchange.updateOrder
    pub async fn update_order(&self, opts: &UpdateOrderOpts) -> Result<Vec<String>, ExchangeError> {
        let order = &opts.order;
        let composite = order.id.contains("__take_profit") || order.id.contains("__stop_loss");
        let (base, mut next) = if composite {
            let base_id = order.id.split("__").next().unwrap();
            let orders = self.base.store.orders();
            let base = orders
                .iter()
                .find(|o| {
                    o.info
                        .as_ref()
                        .and_then(|v| v.get("id"))
                        .is_some_and(|value| value.text_equals(base_id))
                })
                .cloned()
                .ok_or_else(|| failure(format!("Base order {base_id} not found")))?;
            let mut next = from_order(&base);
            if let Some(sl) = orders
                .iter()
                .find(|o| o.id == format!("{base_id}__stop_loss"))
            {
                next.stop_loss = sl.price.into_value();
                next.sl_trigger_type = (sl.sl_trigger_type).into_value();
            }
            if let Some(tp) = orders
                .iter()
                .find(|o| o.id == format!("{base_id}__take_profit"))
            {
                next.take_profit = tp.price.into_value();
                next.tp_trigger_type = (tp.tp_trigger_type).into_value();
            }
            (base, next)
        } else {
            let mut next = from_order(order);
            next.tp_trigger_type = (order.tp_trigger_type).into_value();
            next.sl_trigger_type = (order.sl_trigger_type).into_value();
            (order.clone(), next)
        };
        let (price, amount) = match &opts.update {
            UpdateOrderOptsUpdate::Variant0(v) => (v.price, Some(v.amount)),
            UpdateOrderOptsUpdate::Variant1(v) => (Some(v.price), v.amount),
        };
        if let Some(price) = price {
            if composite && order.id.contains("__take_profit") {
                next.take_profit = Some(price);
            } else if composite && order.id.contains("__stop_loss") {
                next.stop_loss = Some(price);
            } else {
                next.price = Some(price);
            }
        }
        if let Some(amount) = amount {
            next.amount = amount;
        }
        self.cancel_orders(&[base]).await?;
        self.place_order(&next).await
    }
    // ts: ApexOmniExchange.cancelSymbolOrders
    pub async fn cancel_symbol_orders(
        &self,
        symbol: &str,
        cancel_tp_sl: bool,
    ) -> Result<(), ExchangeError> {
        let retained = if cancel_tp_sl {
            Vec::new()
        } else {
            self.base
                .store
                .orders()
                .iter()
                .filter(|o| o.symbol == symbol && o.r#type != OrderType::Limit)
                .cloned()
                .collect::<Vec<_>>()
        };
        let id = if symbol.contains('-') {
            symbol.to_owned()
        } else {
            self.base
                .store
                .markets_by_symbol()
                .get(symbol)
                .map(|m| m.id.clone())
                .ok_or_else(|| failure("typed cancellation requires a known market".into()))?
        };
        self.host
            .request(ApexRequest {
                method: "post",
                path: "/v3/delete-open-orders",
                params: J::Undefined,
                data: fields(vec![("symbol", string(id))]),
                content_type: Some("application/x-www-form-urlencoded; charset=UTF-8"),
                timeout_ms: None,
                unlimited: true,
            })
            .await?;
        if !retained.is_empty() {
            let orders = retained
                .iter()
                .map(|o| {
                    let mut p = from_order(o);
                    p.side = (if o.side == OrderSide::Buy {
                        OrderSide::Sell
                    } else {
                        OrderSide::Buy
                    })
                    .into();
                    p.time_in_force = (o.time_in_force).into_value();
                    p.tp_trigger_type = (o.tp_trigger_type).into_value();
                    p.sl_trigger_type = (o.sl_trigger_type).into_value();
                    p
                })
                .collect();
            self.host.services()?.place_orders(orders).await?;
        }
        Ok(())
    }
}
