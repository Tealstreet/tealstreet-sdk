use crate::payload::{StoreWorkerOp, StoreWorkerPayload};
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_transport::how::HOWWebSocket;
use std::rc::Rc;

pub struct DevHandler {
    exchange: Rc<dyn Exchange>,
    how_ja: Rc<HOWWebSocket>,
    how_sg: Rc<HOWWebSocket>,
}
impl DevHandler {
    pub fn new(
        exchange: Rc<dyn Exchange>,
        how_ja: Rc<HOWWebSocket>,
        how_sg: Rc<HOWWebSocket>,
    ) -> Self {
        Self {
            exchange,
            how_ja,
            how_sg,
        }
    }
    pub fn handle(&self, payload: StoreWorkerPayload) -> Result<(), ExchangeError> {
        match payload.op {
            StoreWorkerOp::DevSimulateWebSocketsReconnect => {
                for socket in self.exchange.base().get_all_sockets() {
                    socket.reconnect()?;
                }
            }
            StoreWorkerOp::DevSimulateHOWClose => {
                self.how_ja.simulate_close();
                self.how_sg.simulate_close();
            }
            StoreWorkerOp::DevForceOpen => {
                for socket in self.exchange.base().get_all_sockets() {
                    socket.force_open()?;
                }
            }
            StoreWorkerOp::DevForceClose => {
                for socket in self.exchange.base().get_all_sockets() {
                    socket.force_close();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
