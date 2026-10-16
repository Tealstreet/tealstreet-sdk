use crate::types::OkxInstrumentType;
use std::{cell::Cell, rc::Rc};

pub fn on_open_public<E>(
    is_disposed: &Cell<bool>,
    mut invoke: impl FnMut(&str) -> Result<(), E>,
    reconnect_enabled: impl FnOnce() -> bool,
) -> Result<(), E> {
    if !is_disposed.get() {
        for action in [
            "orderBookSequenceByHandler.clear",
            "recoveringOrderBooks.clear",
            "subscribe",
            "ping",
            "emitter.off:markets:changed",
            "emitter.on:markets:changed",
        ] {
            invoke(action)?;
        }
        if reconnect_enabled() {
            invoke("emitWsReconnected")?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum SubscriptionTopic<T> {
    Account,
    Product {
        channel: &'static str,
        instrument_type: OkxInstrumentType,
    },
    Existing(Rc<T>),
}

pub fn subscribe_private<T, E>(
    instrument_types: impl FnOnce() -> Result<Vec<OkxInstrumentType>, E>,
    stringify_and_send_with_log: impl FnOnce(&str, &[SubscriptionTopic<T>]) -> Result<(), E>,
) -> Result<(), E> {
    let instruments = instrument_types()?;
    let mut topics = vec![SubscriptionTopic::Account];
    for instrument_type in instruments {
        for channel in ["positions", "orders"] {
            topics.push(SubscriptionTopic::Product {
                channel,
                instrument_type,
            });
        }
    }
    stringify_and_send_with_log("subscribe", &topics)?;
    Ok(())
}

/// Rc preserves topic identity; host callbacks own live JSON hooks.
pub fn subscribe_business<T, E>(
    instrument_types: impl FnOnce() -> Result<Vec<OkxInstrumentType>, E>,
    flattened_live_topics: impl FnOnce() -> Result<Vec<Rc<T>>, E>,
    mut stringify_and_send_with_log: impl FnMut(&str, &[SubscriptionTopic<T>]) -> Result<(), E>,
) -> Result<(), E> {
    let instruments = instrument_types()?;
    let mut order_topics = vec![];
    for instrument_type in instruments {
        for channel in ["orders-algo", "algo-advance"] {
            order_topics.push(SubscriptionTopic::Product {
                channel,
                instrument_type,
            });
        }
    }
    stringify_and_send_with_log("subscribe", &order_topics)?;
    let topics = flattened_live_topics()?;
    for batch in topics.chunks(100) {
        let batch = batch
            .iter()
            .cloned()
            .map(SubscriptionTopic::Existing)
            .collect::<Vec<_>>();
        stringify_and_send_with_log("subscribe", &batch)?;
    }
    Ok(())
}

pub fn contracts_to_notional<E>(
    contracts_native: f64,
    mark_price: f64,
    contract_size: impl FnOnce() -> Result<Option<f64>, E>,
    ct_val_ccy: impl FnOnce() -> Result<Vec<u16>, E>,
    is_inverse: impl FnOnce() -> Result<bool, E>,
    quote_upper: impl FnOnce() -> Result<Vec<u16>, E>,
) -> Result<f64, E> {
    let contract_size = contract_size()?
        .filter(|value| *value != 0.0 && !value.is_nan())
        .unwrap_or(1.0);
    let ct_val_ccy = ct_val_ccy()?;
    if is_inverse()? && ct_val_ccy == quote_upper()? {
        return Ok(contracts_native * contract_size);
    }
    Ok(contracts_native * mark_price * contract_size)
}
