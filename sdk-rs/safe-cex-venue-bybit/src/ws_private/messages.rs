//! One private event consumer over the variant's live source receiver.
use super::protocol::PrivateProtocol;
use crate::{
    binding::{
        host::{get_key, text},
        rt::Rt,
        value::{Js, Thrown, from_typed, to_json},
    },
    exchange::BybitExchange,
};
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::ExchangeError,
    websocket::{BaseWebSocket, WsDiagnosticReceiveOptions},
};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, OrderStatus};
use std::rc::Rc;

fn optional(owner: &Js, key: &str) -> Result<Js, Thrown> {
    if owner.is_nullish() {
        Ok(Js::Undefined)
    } else {
        owner.get(key)
    }
}
fn status(order: &Js, expected: &str) -> Result<bool, Thrown> {
    Ok(order.get("orderStatus")?.strict_equals(&Js::str(expected)))
}
fn rt(parent: &BybitExchange) -> Result<&Rt, Thrown> {
    parent
        .js
        .get()
        .ok_or_else(|| Thrown::error("Error", "Bybit receiver is not assembled"))
}

/// ts: BybitExchange.tradesCategoryOf — retain the receiver's live selectors.
pub(crate) fn trades_category_of(rt: &Rt, symbol: Js) -> Result<Js, Thrown> {
    let category = rt.this.call_method("getCategoryForSymbol", vec![symbol])?;
    if category.strict_equals(&Js::str("option")) {
        rt.this.call_method("optionsEnabled", vec![])
    } else if category.strict_equals(&Js::str("inverse")) {
        rt.this.call_method("inverseEnabled", vec![])
    } else {
        Ok(Js::Bool(true))
    }
}

impl PrivateProtocol {
    /// ts: BybitPrivateWebsocket.onMessage (Spot omits position handling).
    pub(crate) fn message(
        &self,
        socket: &Rc<BaseWebSocket>,
        frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        if socket.is_disposed() {
            return Ok(());
        }
        socket.track_last_received()?;
        socket.direct_fallback.borrow_mut().on_message();
        let SocketFrame::Text(data) = frame else {
            return Ok(());
        };
        socket.emit_ws_diagnostic_receive(
            &serde_json::Value::String(data.clone()),
            &WsDiagnosticReceiveOptions::default(),
        )?;
        let Some(parent) = self.parent.upgrade() else {
            return Ok(());
        };
        self.message_body(socket, &parent, data)
            .map_err(|e| e.to_exchange_error())
    }

    fn message_body(
        &self,
        socket: &Rc<BaseWebSocket>,
        parent: &BybitExchange,
        data: &str,
    ) -> Result<(), Thrown> {
        let rt = rt(parent)?;
        // jsonParse catches only JSON.parse, and leaves subsequent handler errors visible.
        let json = rt
            .shared
            .globals
            .get("JSON")?
            .call_method("parse", vec![Js::str(data)])
            .unwrap_or(Js::Null);
        let topic = optional(&json, "topic")?;
        let spot = parent.base().name == ExchangeName::Bybitspot;
        if topic.strict_equals(&Js::str("order")) {
            socket.record_message_handler_stats("order")?;
            let rows = self.rows_this_connection_trades(parent, json.get("data")?)?;
            if spot || parent.base().name == ExchangeName::Bybitinverse || !rows.is_empty() {
                if parent.base().name == ExchangeName::Hyrotrader {
                    rt.this
                        .get("privateWebsocket")?
                        .call_method("handleOrderTopic", vec![Js::array(rows)])?;
                } else {
                    self.handle_order_topic(parent, rows)?;
                }
            }
        }
        if !spot && topic.strict_equals(&Js::str("position")) {
            socket.record_message_handler_stats("position")?;
            let rows = self.rows_this_connection_trades(parent, json.get("data")?)?;
            if parent.base().name == ExchangeName::Bybitinverse || !rows.is_empty() {
                self.handle_position_topic(parent, rows)?;
            }
        }
        if optional(&json, "op")?.strict_equals(&Js::str("pong")) {
            socket.handle_pong_event(5000)?;
        }
        if optional(&json, "success")?.strict_equals(&Js::Bool(false)) {
            let mut message = String::from("[WS]");
            let op = optional(&json, "op")?;
            if op.truthy() {
                message.push_str(&text(&op)?);
                message.push(':');
            }
            let reason = optional(&json, "ret_msg")?;
            if reason.truthy() {
                message.push_str(&text(&reason)?);
            }
            socket.emit_and_log_error(&message, None, None)?;
            if op.strict_equals(&Js::str("auth")) && self.auth_retries.get() <= 3 {
                socket.emit_and_log_error(
                    if parent.base().name == ExchangeName::Bybitinverse {
                        "[WS] Syncing time with BybitInverse..."
                    } else {
                        "[WS] Syncing time with Bybit..."
                    },
                    None,
                    None,
                )?;
                self.auth_retries.set(self.auth_retries.get() + 1);
                // open starts the owned reconnect sequence immediately, without awaiting it.
                drop(self.open(socket.clone()));
            }
        }
        Ok(())
    }

    /// Primary/Demo/Hyro gate rows by the parent's opted-in categories; dedicated
    /// Spot/Inverse source handlers receive the array without the primary gate.
    fn rows_this_connection_trades(
        &self,
        parent: &BybitExchange,
        rows: Js,
    ) -> Result<Vec<Js>, Thrown> {
        let dedicated = matches!(
            parent.base().name,
            ExchangeName::Bybitspot | ExchangeName::Bybitinverse
        );
        let Some(rows) = rows.array_values()? else {
            return if dedicated {
                Err(Thrown::type_error("data.forEach is not a function"))
            } else {
                Ok(vec![])
            };
        };
        if dedicated {
            return Ok(rows);
        }
        let this = &rt(parent)?.this;
        let mut accepted = vec![];
        for row in rows {
            let symbol = optional(&row, "symbol")?;
            let symbol = if symbol.is_nullish() {
                Js::str("")
            } else {
                Js::String(symbol.to_js_string()?)
            };
            if this.call_method("tradesCategoryOf", vec![symbol])?.truthy() {
                accepted.push(row);
            }
        }
        Ok(accepted)
    }

    /// The three source variants share this order/fill body and use their own mapOrder.
    pub(crate) fn handle_order_topic(
        &self,
        parent: &BybitExchange,
        rows: Vec<Js>,
    ) -> Result<(), Thrown> {
        let rt = rt(parent)?;
        let this = &rt.this;
        for order in rows {
            let orders = this.call_method("mapOrder", vec![order.clone()])?;
            for mapped in orders
                .array_values()?
                .ok_or_else(|| Thrown::type_error("orders is not iterable"))?
            {
                this.call_method("markOrderSeen", vec![mapped.get("id")?])?;
            }
            let price = get_key(&order, "price")?.parse_float()?;
            let amount = get_key(&order, "qty")?.parse_float()?;
            if status(&order, "PartiallyFilled")? && price <= 0. && amount <= 0. {
                continue;
            }
            if status(&order, "Filled")? || status(&order, "PartiallyFilled")? {
                let raw = to_json(&order)?;
                let executed = crate::fill_event::resolve_bybit_execution(raw.as_object())
                    .map_err(Thrown::type_error)?;
                let cumulative = get_key(&order, "cumExecValue")?.parse_float()?;
                let first = orders.get("0")?;
                let id = first.get("id")?;
                let fill = (self.fills.borrow_mut())(
                    &text(&id)?,
                    executed.amount,
                    cumulative.is_finite().then_some(cumulative),
                );
                if fill.qty > 0. {
                    let price = fill.value.filter(|v| *v != 0.).map_or(executed.price, |v| {
                        safe_cex_store::incremental_fills::clean_float(
                            if parent.base().name == ExchangeName::Bybitinverse {
                                fill.qty / v
                            } else {
                                v / fill.qty
                            },
                        )
                    });
                    let event = Js::object(vec![
                        ("id", id),
                        ("side", first.get("side")?),
                        ("symbol", first.get("symbol")?),
                        ("price", Js::Number(price)),
                        ("amount", Js::Number(fill.qty)),
                    ]);
                    this.get("emitter")?
                        .call_method("emit", vec![Js::str("fill"), event])?;
                }
            }
            if status(&order, "Cancelled")?
                || status(&order, "Filled")?
                || status(&order, "Deactivated")?
                || status(&order, "Rejected")?
            {
                let reason = order.get("rejectReason")?;
                if !status(&order, "Filled")?
                    && reason.truthy()
                    && !reason.strict_equals(&Js::str("EC_PerCancelRequest"))
                    && !reason.strict_equals(&Js::str("EC_NoError"))
                {
                    this.call_method("emitAndLogError", vec![reason])?;
                }
                let next = from_typed(&if status(&order, "Filled")? {
                    OrderStatus::Closed
                } else {
                    OrderStatus::Canceled
                })?;
                for mapped in orders.array_values()?.unwrap_or_default() {
                    mapped.set("status", next.clone())?;
                }
                this.get("store")?
                    .call_method("addOrUpdateOrders", vec![orders.clone()])?;
            }
            if status(&order, "New")?
                || status(&order, "Untriggered")?
                || status(&order, "PartiallyFilled")?
            {
                this.get("store")?
                    .call_method("addOrUpdateOrders", vec![orders])?;
            }
        }
        Ok(())
    }

    fn handle_position_topic(&self, parent: &BybitExchange, rows: Vec<Js>) -> Result<(), Thrown> {
        let rt = rt(parent)?;
        let this = &rt.this;
        let mut positions = vec![];
        for row in rows {
            positions.push(this.call_method("mapPosition", vec![row])?);
        }
        if parent.base().name == ExchangeName::Bybitinverse {
            let mut updates = vec![];
            for position in positions {
                updates.push(Js::array(vec![position.clone(), position]));
            }
            this.get("store")?
                .call_method("updatePositions", vec![Js::array(updates)])?;
        } else {
            this.get("store")?
                .call_method("addOrUpdatePositions", vec![Js::array(positions)])?;
            this.call_method("syncBalanceUpnlFromPositions", vec![])?;
        }
        this.set(
            "lastWebsocketPositionsUpdate",
            rt.shared
                .globals
                .get("virtualClock")?
                .call_method("getCurrentTimeMs", vec![])?,
        )?;
        Ok(())
    }
}
