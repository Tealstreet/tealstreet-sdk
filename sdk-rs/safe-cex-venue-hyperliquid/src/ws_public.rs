//! Public subscription state; shared socket lifecycle remains to be composed.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, JsJsonValue, js_json_stringify};
use std::cell::RefCell;

#[derive(Debug)]
pub enum SubscriptionError {
    Coercion(JsCoercionError),
    Send(ExchangeError),
}

pub trait SubscriptionInput {
    fn evaluate(&self) -> Result<JsJsonValue, JsCoercionError>;
}
impl SubscriptionInput for JsJsonValue {
    fn evaluate(&self) -> Result<JsJsonValue, JsCoercionError> {
        Ok(self.clone())
    }
}

#[derive(Default)]
pub struct HyperliquidPublicWebSocket {
    active_subscriptions: RefCell<Vec<String>>,
}
impl HyperliquidPublicWebSocket {
    pub fn active_subscriptions(&self) -> Vec<String> {
        self.active_subscriptions.borrow().clone()
    }
    pub fn subscribe_user_topics(
        &self,
        topic_keys: Vec<String>,
        subscribe: &dyn Fn(&JsJsonValue) -> Result<bool, SubscriptionError>,
    ) -> Result<(), SubscriptionError> {
        for key in safe_cex_num::js_own_keys(topic_keys.iter().map(String::as_str)) {
            let mut parts = key.split('.');
            let id = parts.next().unwrap_or_default();
            let coin = parts.next().map_or(JsJsonValue::Undefined, |part| {
                JsJsonValue::String(part.into())
            });
            let interval = parts.next().map_or(JsJsonValue::Undefined, |part| {
                JsJsonValue::String(part.into())
            });
            match id {
                "candle" => {
                    subscribe(&JsJsonValue::Object(vec![
                        ("type".into(), JsJsonValue::String("candle".into())),
                        ("coin".into(), coin),
                        ("interval".into(), interval),
                    ]))?;
                }
                "l2Book" => {
                    for topic in ["l2Book", "bbo"] {
                        subscribe(&JsJsonValue::Object(vec![
                            ("type".into(), JsJsonValue::String(topic.into())),
                            ("coin".into(), coin.clone()),
                        ]))?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub fn subscribe_public_topics(
        &self,
        send: &dyn Fn(&str) -> Result<(), ExchangeError>,
        subscribe_user_topics: &dyn Fn() -> Result<(), ExchangeError>,
    ) -> Result<(), SubscriptionError> {
        for topic in ["allMids", "fastAssetCtxs"] {
            let subscription =
                JsJsonValue::Object(vec![("type".into(), JsJsonValue::String(topic.into()))]);
            let message = Self::envelope("subscribe", &subscription)?;
            send(&message).map_err(SubscriptionError::Send)?;
        }
        subscribe_user_topics().map_err(SubscriptionError::Send)
    }
    pub fn send_subscription(
        &self,
        subscription: &dyn SubscriptionInput,
        send: &dyn Fn(&str) -> Result<(), ExchangeError>,
    ) -> Result<bool, SubscriptionError> {
        let key = Self::subscription_key(
            &subscription
                .evaluate()
                .map_err(SubscriptionError::Coercion)?,
        )?;
        {
            let mut active = self.active_subscriptions.borrow_mut();
            if active.contains(&key) {
                return Ok(false);
            }
            active.push(key);
        }
        let message = Self::envelope(
            "subscribe",
            &subscription
                .evaluate()
                .map_err(SubscriptionError::Coercion)?,
        )?;
        send(&message).map_err(SubscriptionError::Send)?;
        Ok(true)
    }
    pub fn send_unsubscription(
        &self,
        subscription: &dyn SubscriptionInput,
        send: &dyn Fn(&str) -> Result<(), ExchangeError>,
    ) -> Result<(), SubscriptionError> {
        let key = Self::subscription_key(
            &subscription
                .evaluate()
                .map_err(SubscriptionError::Coercion)?,
        )?;
        self.active_subscriptions
            .borrow_mut()
            .retain(|value| value != &key);
        let message = Self::envelope(
            "unsubscribe",
            &subscription
                .evaluate()
                .map_err(SubscriptionError::Coercion)?,
        )?;
        send(&message).map_err(SubscriptionError::Send)
    }
    fn subscription_key(subscription: &JsJsonValue) -> Result<String, SubscriptionError> {
        js_json_stringify(subscription)
            .map_err(SubscriptionError::Coercion)?
            .ok_or_else(|| {
                SubscriptionError::Coercion(JsCoercionError {
                    name: "TypeError",
                    message: "Subscription must have a JSON key",
                })
            })
    }
    fn envelope(method: &str, subscription: &JsJsonValue) -> Result<String, SubscriptionError> {
        Self::subscription_key(&JsJsonValue::Object(vec![
            ("method".into(), JsJsonValue::String(method.into())),
            ("subscription".into(), subscription.clone()),
        ]))
    }
}
