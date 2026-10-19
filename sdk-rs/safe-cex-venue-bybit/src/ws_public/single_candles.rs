use super::{
    candle_recovery::Recovery,
    streams::{Topics, append, remove},
};
use safe_cex_exchange::{
    OwnedOperation,
    exchange::BaseExchange,
    js_runtime::value::Js,
    runtime::{ExchangeError, ListenerCleanup},
    websocket::{BaseWebSocket, MessageHandler},
};
use safe_cex_types::{Candle, ExchangeName, OHLCVOptions, OrderSide};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

struct Subscription {
    socket: Weak<BaseWebSocket>,
    parent: Weak<BaseExchange>,
    retained: Topics,
    topic: String,
    secondary: String,
    parser: MessageHandler,
    trades: MessageHandler,
    primary: bool,
    trade_derived: bool,
    spot: bool,
    active: Cell<bool>,
    retry: RefCell<Option<OwnedOperation>>,
    recovery: Option<Rc<Recovery>>,
}
impl Subscription {
    fn send(&self, socket: &Rc<BaseWebSocket>, op: &str, topic: &str) -> Result<(), ExchangeError> {
        let payload = json!({"op":op,"args":[topic]}).to_string();
        if self.primary {
            socket.send_with_log(&payload)
        } else {
            socket.send(&payload)
        }
    }
    fn connect(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let Some(socket) = self.socket.upgrade() else {
            return Ok(());
        };
        if self.primary && (!self.active.get() || socket.is_disposed()) {
            return Ok(());
        }
        if socket.is_connected() {
            if !socket.is_disposed() {
                append(&socket, &self.topic, self.parser.clone());
                self.send(&socket, "subscribe", &self.topic)?;
                self.retained
                    .borrow_mut()
                    .insert(self.topic.clone(), json!(self.topic));
                let trade_derived = self.primary
                    || self
                        .parent
                        .upgrade()
                        .is_some_and(|parent| !parent.options().public_data_only.unwrap_or(false));
                if self.trade_derived && trade_derived {
                    append(&socket, &self.secondary, self.trades.clone());
                    self.send(&socket, "subscribe", &self.secondary)?;
                    self.retained
                        .borrow_mut()
                        .insert(self.secondary.clone(), json!(self.secondary));
                }
                if let Some(recovery) = &self.recovery {
                    recovery.arm(&self.topic, Some(&self.secondary))?;
                }
            }
        } else {
            let subscription = self.clone();
            let timer = socket.start_timeout(100, move || subscription.connect())?;
            if self.primary {
                *self.retry.borrow_mut() = Some(timer);
            }
        }
        Ok(())
    }
    fn cleanup(&self) -> Result<(), ExchangeError> {
        self.active.set(false);
        if let Some(timer) = self.retry.borrow_mut().take() {
            timer.cancel();
        }
        let Some(socket) = self.socket.upgrade() else {
            return Ok(());
        };
        let primary_empty = remove(&socket, &self.topic, &self.parser);
        let secondary_empty = self.trade_derived && remove(&socket, &self.secondary, &self.trades);
        if primary_empty {
            self.retained.borrow_mut().shift_remove(&self.topic);
            if let Some(recovery) = &self.recovery {
                recovery.clear(&self.topic)?;
                recovery.forget(&self.topic);
            }
            if !self.primary {
                self.retained.borrow_mut().shift_remove(&self.secondary);
            }
        }
        if self.primary && secondary_empty {
            self.retained.borrow_mut().shift_remove(&self.secondary);
        }
        if socket.is_connected() && primary_empty {
            if !self.spot {
                socket.cleanup_message_handler_stats(&self.topic)?;
            }
            self.send(&socket, "unsubscribe", &self.topic)?;
        }
        if socket.is_connected() && secondary_empty {
            if self.spot {
                socket.cleanup_message_handler_stats(&self.topic)?;
            }
            socket.cleanup_message_handler_stats(&self.secondary)?;
            self.send(&socket, "unsubscribe", &self.secondary)?;
        }
        Ok(())
    }
}

pub(crate) fn listen_ohlcv(
    socket: Rc<BaseWebSocket>,
    retained: Topics,
    options: OHLCVOptions,
    parent: Rc<BaseExchange>,
    recovery: Option<Rc<Recovery>>,
    callback: Rc<dyn Fn(Candle)>,
) -> Result<ListenerCleanup, ExchangeError> {
    let primary = matches!(
        parent.name,
        ExchangeName::Bybit | ExchangeName::Bybitdemo | ExchangeName::Hyrotrader
    );
    let spot = parent.name == ExchangeName::Bybitspot;
    let runtime = parent.runtime.clone();
    let interval =
        serde_json::to_value(options.interval).map_err(|e| ExchangeError::new(e.to_string()))?;
    let interval = interval.as_str().unwrap();
    let topic = format!(
        "kline.{}.{}",
        crate::types::INTERVAL[interval].as_str().unwrap(),
        options.symbol
    );
    let secondary = format!("publicTrade.{}", options.symbol);
    let last = Rc::new(RefCell::new(None::<Candle>));
    let state = last.clone();
    let arm = recovery.clone();
    let primary_topic = topic.clone();
    let trade_topic = secondary.clone();
    let emit = callback.clone();
    let parser: MessageHandler = Rc::new(move |frame| {
        let data = frame.get("data")?;
        let raw = super::candles::first_candle(&data)?;
        let Some(candle) = crate::candle_mapping::apply_kline(&mut state.borrow_mut(), &raw)?
        else {
            return Ok(());
        };
        if let Some(recovery) = &arm {
            recovery.arm(&primary_topic, Some(&trade_topic))?;
        }
        emit(candle);
        Ok(())
    });
    let start_of = Rc::new(RefCell::new(
        safe_cex_candle_cache::calendar::create_live_candle_start(interval),
    ));
    let state = last;
    let arm = recovery.clone();
    let primary_topic = topic.clone();
    let trade_topic = secondary.clone();
    let trades: MessageHandler = Rc::new(move |frame| {
        let trade = frame.get("data")?.get("0")?;
        let time = trade.get("T")?;
        let price = trade.get("p")?;
        let volume = trade.get("v")?;
        let side = if spot { Js::Undefined } else { trade.get("S")? };
        let start = start_of.borrow_mut()(time.to_number()? / 1000.);
        let early_price = if spot {
            None
        } else {
            Some(price.parse_float()?)
        };
        let Some(mut candle) = state.borrow().clone() else {
            return Ok(());
        };
        let price = early_price.map(Ok).unwrap_or_else(|| price.parse_float())?;
        if candle.timestamp > start {
            if primary || spot {
                runtime.console(
                    "warn",
                    &format!(
                        "Received trade with timestamp in the past, ignoring: {}",
                        time.to_display()
                    ),
                );
            }
            return Ok(());
        }
        if candle.timestamp < start {
            candle = Candle {
                timestamp: start,
                open: candle.close,
                high: candle.close,
                low: candle.close,
                close: candle.close,
                volume: 0.,
                last_side: None,
            };
        }
        candle.high = if candle.high > price {
            candle.high
        } else {
            price
        };
        candle.low = if candle.low < price {
            candle.low
        } else {
            price
        };
        candle.close = price;
        candle.volume += volume.to_number()? / price;
        if !spot {
            candle.last_side = Some(if side.strict_equals(&Js::str("Sell")) {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            });
        }
        *state.borrow_mut() = Some(candle.clone());
        if let Some(recovery) = &arm {
            recovery.arm(&primary_topic, Some(&trade_topic))?;
        }
        callback(candle);
        Ok(())
    });
    let subscription = Rc::new(Subscription {
        socket: Rc::downgrade(&socket),
        parent: Rc::downgrade(&parent),
        retained,
        topic,
        secondary,
        parser,
        trades,
        primary,
        trade_derived: parent.name != ExchangeName::Hyrotrader,
        spot,
        active: Cell::new(true),
        retry: RefCell::new(None),
        recovery,
    });
    subscription.connect()?;
    Ok(Box::new(move || subscription.cleanup()))
}
