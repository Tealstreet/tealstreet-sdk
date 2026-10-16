use crate::public_message::{Failure, MessageData, ReadData};
use safe_cex_exchange::websocket::BaseWebSocket;
use std::rc::Rc;

// ts: BinancePublicWebsocket.onMessage
pub fn on_spot_public_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    dispatch: impl FnOnce(Option<MessageData>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let data = read_data()?;
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received().map_err(Rc::new)?;
    dispatch(data)
}
