use super::host::{Host, R, V, v};
use crate::classes::Return;
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;
impl crate::socket_classes::SocketHost for Host {
    fn super_socket(&self, _: bool, args: Vec<V>) -> R<V> {
        Ok(v(safe_cex_exchange::js_runtime::sockets::base_websocket(
            &self.rt()?.shared.sockets,
            args.first().map_or(Js::Undefined, |x| (**x).clone()),
        )))
    }
    fn define(&self, receiver: V, key: &str, value: V) -> R<()> {
        receiver.define(key, (*value).clone())
    }
    fn function(&self, name: &str, body: crate::classes::BoundMember<Self>) -> V {
        Host::function(self, name, body)
    }
    fn member(
        &self,
        private: bool,
        name: &'static str,
        _: &'static str,
    ) -> crate::classes::NativeMember<Self> {
        let host = self.clone();
        Rc::new(move |receiver, args| {
            let arg = |i| args.get(i).cloned().unwrap_or_else(|| v(Js::Undefined));
            if private {
                if name == "onOpen" {
                    let base = socket_base(&host, &receiver)?;
                    let host = host.clone();
                    return Ok(Return::Future(Box::pin(async move {
                        base.authenticate_and_subscribe(
                            Box::pin(async move {
                                crate::ws_controls::private_open(&host, receiver)
                                    .map_err(|e| e.to_exchange_error())?;
                                Ok(())
                            }),
                            |_| Ok(()),
                        )
                        .await
                        .map_err(Thrown::from)?;
                        Ok(v(Js::Undefined))
                    })));
                }
                let result = match name {
                    "connectAndSubscribe" => crate::ws_controls::private_connect(&host, receiver),
                    "onOpen" => crate::ws_controls::private_open(&host, receiver),
                    "subscribe" => crate::ws_controls::private_subscribe(&host, receiver),
                    "ping" => crate::ws_controls::private_ping(&host, receiver),
                    "onMessage" => crate::stream_reports::private_message(&host, receiver, arg(0)),
                    "handleExecutionReport" => {
                        crate::stream_reports::execution(&host, receiver, arg(0))
                    }
                    "handleAlgoExecutionReport" => {
                        crate::stream_reports::algo_execution(&host, receiver, arg(0))
                    }
                    "handlePositionReport" => {
                        crate::stream_reports::positions(&host, receiver, arg(0))
                    }
                    _ => {
                        return Err(Thrown::error(
                            "Error",
                            &format!("Unknown Orderly private socket member {name}"),
                        ));
                    }
                };
                result?;
                return Ok(Return::Value(v(Js::Undefined)));
            }
            let value = match name {
                "connectAndSubscribe" => {
                    crate::ws_controls::public_connect(&host, receiver)?;
                    v(Js::Undefined)
                }
                "onOpen" => {
                    crate::ws_controls::public_open(&host, receiver)?;
                    v(Js::Undefined)
                }
                "subscribe" => {
                    crate::ws_controls::public_subscribe(&host, receiver)?;
                    v(Js::Undefined)
                }
                "ping" => {
                    crate::ws_controls::public_ping(&host, receiver)?;
                    v(Js::Undefined)
                }
                "handlePingEvent" => {
                    crate::ws_controls::public_handle_ping(&host, receiver)?;
                    v(Js::Undefined)
                }
                "onMessage" => {
                    crate::ws_controls::public_message(&host, receiver, arg(0))?;
                    v(Js::Undefined)
                }
                "handleTickersStreamEvents" => {
                    crate::stream_reports::tickers(&host, receiver, arg(0))?;
                    v(Js::Undefined)
                }
                "handleBBOStreamEvents" => {
                    crate::stream_reports::bbo(&host, receiver, arg(0))?;
                    v(Js::Undefined)
                }
                "handleMarkPricesStreamEvents" => {
                    crate::stream_reports::mark_prices(&host, receiver, arg(0))?;
                    v(Js::Undefined)
                }
                "handleFundingRateStreamEvents" => {
                    crate::stream_reports::funding_rates(&host, receiver, arg(0))?;
                    v(Js::Undefined)
                }
                "listenOHLCV" => {
                    crate::public_feeds::listen_candles(&host, receiver, arg(0), arg(1))?
                }
                "listenOrderBook" => {
                    crate::public_feeds::listen_book(&host, receiver, arg(0), arg(1))?
                }
                "processOrderBookUpdate" => {
                    crate::public_feeds::process_book(&host, arg(0), arg(1))?
                }
                _ => {
                    return Err(Thrown::error(
                        "Error",
                        &format!("Unknown Orderly public socket member {name}"),
                    ));
                }
            };
            Ok(Return::Value(value))
        })
    }
}

fn socket_base(host: &Host, receiver: &Js) -> R<Rc<safe_cex_exchange::websocket::BaseWebSocket>> {
    let rt = host.rt()?;
    let cores = rt.shared.sockets.cores.borrow();
    cores
        .iter()
        .find(|(object, _)| object.same_reference(receiver))
        .and_then(|(_, core)| core.base.get().cloned())
        .ok_or_else(|| Thrown::error("Error", "Orderly private socket is not connected"))
}
