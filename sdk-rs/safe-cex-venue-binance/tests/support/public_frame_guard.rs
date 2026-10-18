use super::*;
use safe_cex_transport::socket::SocketFrame;
use safe_cex_venue_binance::public_listeners::ListenerHost;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn production_native_public_frame_guard_releases_success_error_and_unwind() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let socket = owner.public_socket();
        let original = Rc::new(ExchangeError::new("original public consumer failure"));
        for outcome in 0..3 {
            let packet = Rc::new(RefCell::new(None));
            let captured = packet.clone();
            let error = original.clone();
            socket.replace_handlers(
                "guard".into(),
                vec![Rc::new(move |events| {
                    let EventValue::Array(rows) = events else {
                        panic!("production dispatch must batch its parsed packet");
                    };
                    *captured.borrow_mut() = Some(Rc::downgrade(&rows));
                    match outcome {
                        0 => Ok(()),
                        1 => Err(error.clone()),
                        _ => panic!("consumer panic exercises native frame unwind"),
                    }
                })],
            );
            let frame = r#"{"e":"guard","s":"BTCUSDT"}"#;
            match outcome {
                0 => socket
                    .base()
                    .on_message(&SocketFrame::Text(frame.into()))
                    .unwrap(),
                1 => assert!(
                    Rc::ptr_eq(&socket.on_message(frame).unwrap_err(), &original),
                    "native bridge retains the consumer's original failure {venue:?}"
                ),
                _ => assert!(
                    catch_unwind(AssertUnwindSafe(|| {
                        socket.base().on_message(&SocketFrame::Text(frame.into()))
                    }))
                    .is_err(),
                    "consumer panic reaches the host {venue:?}"
                ),
            }
            assert!(
                packet
                    .borrow()
                    .as_ref()
                    .expect("actual registered consumer received a packet")
                    .upgrade()
                    .is_none(),
                "native frame guard releases parsed packet after outcome {outcome} {venue:?}"
            );
            let bid = 97.0 - f64::from(outcome);
            let ask = 103.0 + f64::from(outcome);
            socket
                .base()
                .on_message(&SocketFrame::Text(
                    json!({"e":"bookTicker","s":"BTCUSDT","b":bid.to_string(),"a":ask.to_string()})
                        .to_string(),
                ))
                .unwrap();
            let tickers = owner.base.store.tickers().unwrap();
            let ticker = &tickers[0];
            assert_eq!(ticker.bid, bid, "next native bid after {outcome} {venue:?}");
            assert_eq!(ticker.ask, ask, "next native ask after {outcome} {venue:?}");
        }
        owner.base.dispose().unwrap();
    }
}
