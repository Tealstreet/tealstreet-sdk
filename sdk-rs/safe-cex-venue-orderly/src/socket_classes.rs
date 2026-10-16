//! Websocket initializer factories with mandatory exact-source member bindings.
use crate::{
    account_order_reads::AccountReadHost,
    classes::{BoundMember, Definition, Initializer, NativeMember, Return},
    ws_controls::{Result, Value},
};
use std::rc::Rc;
pub trait SocketHost: AccountReadHost + Clone + 'static {
    fn super_socket(&self, private: bool, args: Vec<Value<Self>>) -> Result<Self, Value<Self>>;
    fn define(&self, receiver: Value<Self>, key: &str, value: Value<Self>) -> Result<Self, ()>;
    // Provision every original member ABI without observable effects.
    fn member(&self, private: bool, name: &'static str, target: &'static str)
    -> NativeMember<Self>;
    fn function(&self, name: &str, body: BoundMember<Self>) -> Value<Self>;
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
pub fn construct<H: SocketHost>(h: &H, private: bool, args: Vec<Value<H>>) -> Result<H, Value<H>> {
    let r = h.super_socket(private, args)?;
    if !private {
        h.define(r.clone(), "topics", h.object(vec![])?)?;
        let handlers = h.object(vec![])?;
        for (topic, method, data) in [
            ("ping", "handlePingEvent", false),
            ("pong", "handlePongEvent", false),
            ("tickers", "handleTickersStreamEvents", true),
            ("bbos", "handleBBOStreamEvents", true),
            ("markprices", "handleMarkPricesStreamEvents", true),
            ("estfundingrate", "handleFundingRateStreamEvents", true),
        ] {
            let captured = r.clone();
            let host = h.clone();
            let cb = h.function(
                "",
                Rc::new(move |args| {
                    let parent_args = if data {
                        let json = args.first().cloned().unwrap_or_else(|| host.undefined());
                        vec![host.get(&json, &u("data"))?]
                    } else {
                        vec![]
                    };
                    let f = host.get(&captured, &u(method))?;
                    Ok(Return::Value(host.call(
                        f,
                        Some(captured.clone()),
                        parent_args,
                    )?))
                }),
            );
            h.set(&handlers, &u(topic), h.array(vec![cb]))?;
        }
        h.define(r.clone(), "messageHandlers", handlers)?;
    }
    for d in definitions(private) {
        if let Initializer::Arrow(target) = d.initializer {
            let f = h.member(private, d.name, target);
            let captured = r.clone();
            let cb = h.function(d.name, Rc::new(move |args| f(captured.clone(), args)));
            h.define(r.clone(), d.name, cb)?;
        }
    }
    Ok(r)
}

pub fn public<H: SocketHost>(h: &H, args: Vec<Value<H>>) -> Result<H, Value<H>> {
    construct(h, false, args)
}
pub fn private<H: SocketHost>(h: &H, args: Vec<Value<H>>) -> Result<H, Value<H>> {
    construct(h, true, args)
}
pub fn definitions(private: bool) -> &'static [Definition] {
    if private {
        &[
            Definition {
                name: "connectAndSubscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::private_connect",
                ),
            },
            Definition {
                name: "onOpen",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::private_open",
                ),
            },
            Definition {
                name: "subscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::private_subscribe",
                ),
            },
            Definition {
                name: "ping",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::private_ping",
                ),
            },
            Definition {
                name: "onMessage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::private_message",
                ),
            },
            Definition {
                name: "handlePositionReport",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::positions",
                ),
            },
            Definition {
                name: "handleExecutionReport",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::execution",
                ),
            },
            Definition {
                name: "handleAlgoExecutionReport",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::algo_execution",
                ),
            },
        ]
    } else {
        &[
            Definition {
                name: "connectAndSubscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::public_connect",
                ),
            },
            Definition {
                name: "onOpen",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::public_open",
                ),
            },
            Definition {
                name: "subscribe",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::public_subscribe",
                ),
            },
            Definition {
                name: "onMessage",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::public_message",
                ),
            },
            Definition {
                name: "ping",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::public_ping",
                ),
            },
            Definition {
                name: "handlePingEvent",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/ws_controls.rs::public_handle_ping",
                ),
            },
            Definition {
                name: "handleTickersStreamEvents",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::tickers",
                ),
            },
            Definition {
                name: "handleBBOStreamEvents",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::bbo",
                ),
            },
            Definition {
                name: "handleMarkPricesStreamEvents",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::mark_prices",
                ),
            },
            Definition {
                name: "handleFundingRateStreamEvents",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/stream_reports.rs::funding_rates",
                ),
            },
            Definition {
                name: "listenOHLCV",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/public_feeds.rs::listen_candles",
                ),
            },
            Definition {
                name: "listenOrderBook",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/public_feeds.rs::listen_book",
                ),
            },
            Definition {
                name: "processOrderBookUpdate",
                initializer: Initializer::Arrow(
                    "safe-cex-venue-orderly/src/public_feeds.rs::process_book",
                ),
            },
        ]
    }
}
