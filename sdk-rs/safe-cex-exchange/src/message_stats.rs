use crate::{
    exchange::Exchange, runtime::ExchangeError, socket_lifecycle::SocketSlot,
    websocket::BaseWebSocket,
};
use safe_cex_types::StoreData;
use serde_json::{Map, Value, json};

pub fn websocket_has_stats(socket: Option<&BaseWebSocket>) -> bool {
    socket.is_some_and(|socket| {
        socket.diagnostics().initialized || !socket.message_handlers_stats.borrow().is_empty()
    })
}

fn snapshot(socket: &BaseWebSocket) -> Map<String, Value> {
    socket
        .message_handlers_stats
        .borrow()
        .iter()
        .map(|(topic, stat)| {
            let mut value = Map::new();
            value.insert(
                "lastMessageTimestamp".into(),
                json!(stat.last_message_timestamp),
            );
            if !matches!(
                &stat.last_callbacks_count,
                safe_cex_num::PrototypeScalar::Undefined
            ) {
                value.insert(
                    "lastCallbacksCount".into(),
                    json!(&stat.last_callbacks_count),
                );
            }
            if let Some(had_handlers) = stat.had_handlers {
                value.insert("hadHandlers".into(), json!(had_handlers));
            }
            (topic.clone(), Value::Object(value))
        })
        .collect()
}
fn assign(store: &mut StoreData, group: &str, value: Value) -> Result<(), ExchangeError> {
    let Some(Value::Object(stats)) = &mut store.message_handler_stats else {
        return Err(ExchangeError::new(
            "Message-handler statistics require a JSON object",
        ));
    };
    stats.insert(group.to_owned(), value);
    Ok(())
}

pub struct MessageHandlerStatsPostprocessor;
impl MessageHandlerStatsPostprocessor {
    pub fn process(
        &self,
        store: &mut StoreData,
        exchange: &dyn Exchange,
    ) -> Result<(), ExchangeError> {
        if !safe_cex_num::js_truthy(store.message_handler_stats.as_ref()) {
            store.message_handler_stats = Some(Value::Object(Map::new()));
        }
        for (slot, group) in [
            (SocketSlot::Public, "public"),
            (SocketSlot::Business, "business"),
            (SocketSlot::Futures, "futures"),
            (SocketSlot::Private, "private"),
        ] {
            if let Some(socket) = exchange.base().get_socket(slot)
                && websocket_has_stats(Some(&socket))
            {
                assign(store, group, Value::Object(snapshot(&socket)))?;
            }
        }
        if let Some(socket) = exchange.base().get_socket(SocketSlot::PublicInverse) {
            assign(store, "public", Value::Object(snapshot(&socket)))?;
        }
        if let Some(socket) = exchange.base().get_socket(SocketSlot::PublicSecondary) {
            let primary = store
                .message_handler_stats
                .as_ref()
                .and_then(|value| value.get("public"));
            if safe_cex_num::js_truthy(primary) {
                let Some(Value::Object(primary)) = primary else {
                    return Err(ExchangeError::new(
                        "Public message-handler statistics require a JSON object",
                    ));
                };
                let mut combined = snapshot(&socket);
                combined.extend(primary.clone());
                assign(store, "public", Value::Object(combined))?;
            }
        }
        Ok(())
    }
}
