use super::{CoinbaseDependencies, CoinbaseExchange};
use crate::{
    cdp_signing::CdpSigningCache,
    exchange_tick::{TickHandlers, TickHost, TickRegistry},
    public_market_listeners::{
        ListenerFailure, PublicBook, PublicBookLevel, PublicBookLevels, PublicListenerBook,
    },
    public_stream_dispatch::{
        PublicSocket, PublicStreamFailure, PublicStreamHandler, PublicStreamHandlers,
        PublicStreamMessage,
    },
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{js_runtime::client, native::native_runtime};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn owner(name: ExchangeName) -> Rc<CoinbaseExchange> {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    CoinbaseExchange::new(
        name,
        serde_json::from_value(json!({"key":"synthetic","secret":"synthetic"})).unwrap(),
        Rc::new(DefaultStore::new()),
        CoinbaseDependencies {
            runtime,
            http,
            external_public_data: None,
            signing_cache: Rc::new(CdpSigningCache::default()),
            local_wall_time_to_utc: Rc::new(|value| value),
        },
    )
    .unwrap()
}

fn native<T>(run: impl FnOnce() -> T) -> T {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async { run() })
}

#[test]
fn stream_and_tick_aliases_capture_actual_class_handlers_and_native_frames() {
    native(|| {
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let exchange = owner(name);
            let public = exchange.class.public_websocket.borrow().clone().unwrap();
            let handlers: PublicStreamHandlers = Rc::default();
            let captured = handlers.clone();
            assert!(Rc::ptr_eq(&handlers, &captured));
            let seen = Rc::new(RefCell::new(Vec::new()));
            let seen_first = seen.clone();
            let appended = Rc::new(RefCell::new(false));
            let append_once = appended.clone();
            let append_target = Rc::downgrade(&handlers);
            handlers
                .borrow_mut()
                .push(PublicStreamHandler::Callback(Rc::new(move |message| {
                    let PublicStreamMessage::Frame(frame) = message else {
                        panic!("callback lost frame variant")
                    };
                    seen_first.borrow_mut().push(frame.clone());
                    if !append_once.replace(true) {
                        let seen_second = seen_first.clone();
                        append_target.upgrade().unwrap().borrow_mut().push(
                            PublicStreamHandler::Callback(Rc::new(move |message| {
                                let PublicStreamMessage::Frame(frame) = message else {
                                    panic!("appended callback lost frame variant")
                                };
                                seen_second.borrow_mut().push(frame);
                                Ok(())
                            })),
                        );
                    }
                    Ok(())
                })));
            public
                .receiver
                .message_handlers
                .borrow_mut()
                .insert("alias_probe".into(), handlers.clone());
            public
                .on_message(r#"{"channel":"alias_probe","x":-0.0,"text":"\ud800"}"#)
                .unwrap();
            assert_eq!(
                seen.borrow().len(),
                2,
                "actual live for-of sees appended callback"
            );
            assert!(Rc::ptr_eq(
                &handlers,
                public
                    .receiver
                    .message_handlers
                    .borrow()
                    .get("alias_probe")
                    .unwrap()
            ));
            let first = seen.borrow()[0].clone();
            let second = seen.borrow()[1].clone();
            let (Frame::Object(first_root), Frame::Object(second_root)) = (&first, &second) else {
                panic!("native object frame required")
            };
            assert!(Rc::ptr_eq(first_root, second_root));
            assert!(
                matches!(first.get("x"), Some(Frame::Number(value)) if value.to_bits() == (-0.0_f64).to_bits())
            );
            assert!(matches!(second.get("text"), Some(Frame::String(units)) if units == [0xd800]));
            let registry: TickRegistry = exchange.host.public_message_handlers().unwrap();
            let callbacks: TickHandlers = registry
                .borrow()
                .get(&"alias_probe".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .clone();
            assert_eq!(callbacks.borrow().len(), 2);
            let socket: PublicSocket =
                Rc::new(RefCell::new(Frame::Array(Rc::new(RefCell::new(vec![
                    Frame::Number(f64::NAN),
                    Frame::Number(f64::INFINITY),
                    Frame::Undefined,
                ])))));
            *public.receiver.ws.borrow_mut() = Some(socket.clone());
            let socket_view = public.receiver.ws.borrow().clone().unwrap();
            assert!(Rc::ptr_eq(&socket, &socket_view));
            let raw = socket_view.borrow().clone();
            callbacks.borrow()[0](raw.clone()).unwrap();
            callbacks.borrow()[1](raw).unwrap();
            let seen = seen.borrow();
            assert_eq!(seen.len(), 4);
            let Frame::Array(delivered) = &seen[2] else {
                panic!("native frame array required")
            };
            let Frame::Array(original) = &*socket.borrow() else {
                panic!("socket root required")
            };
            assert!(Rc::ptr_eq(delivered, original));
            let values = delivered.borrow();
            assert!(matches!(values[0], Frame::Number(value) if value.is_nan()));
            assert!(matches!(values[1], Frame::Number(value) if value == f64::INFINITY));
            assert!(matches!(values[2], Frame::Undefined));
        }
    });
}

#[test]
fn listener_failure_alias_is_the_exact_actual_dispatch_failure() {
    native(|| {
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let exchange = owner(name);
            let public = exchange.class.public_websocket.borrow().clone().unwrap();
            let failure: ListenerFailure = Rc::new(PublicStreamFailure {
                name: "TypeError".into(),
                message: "alias listener failure".into(),
            });
            let expected = failure.clone();
            let handlers: PublicStreamHandlers =
                Rc::new(RefCell::new(vec![PublicStreamHandler::Callback(Rc::new(
                    move |_| Err(failure.clone()),
                ))]));
            public
                .receiver
                .message_handlers
                .borrow_mut()
                .insert("alias_error".into(), handlers);
            let actual: ListenerFailure = public
                .on_message(r#"{"channel":"alias_error"}"#)
                .unwrap_err();
            assert!(Rc::ptr_eq(&actual, &expected));
            assert_eq!(actual.name, "TypeError");
            assert_eq!(actual.message, "alias listener failure");
        }
    });
}

#[test]
fn book_aliases_retain_native_level_roots_at_the_production_store_adapter() {
    let first = Rc::new(RefCell::new(PublicBookLevel {
        price: -0.0,
        amount: f64::NAN,
        total: f64::INFINITY,
    }));
    let second = Rc::new(RefCell::new(PublicBookLevel {
        price: 7.0,
        amount: 3.0,
        total: f64::NEG_INFINITY,
    }));
    let bids: PublicBookLevels = Rc::new(RefCell::new(vec![first.clone(), second.clone()]));
    let asks: PublicBookLevels = Rc::new(RefCell::new(vec![second.clone()]));
    let book: PublicListenerBook = Rc::new(RefCell::new(PublicBook {
        bids: bids.clone(),
        asks: asks.clone(),
    }));
    let captured = book.clone();
    assert!(Rc::ptr_eq(&book, &captured));
    assert!(Rc::ptr_eq(&captured.borrow().bids, &bids));
    assert!(Rc::ptr_eq(&captured.borrow().asks, &asks));
    assert!(Rc::ptr_eq(&bids.borrow()[0], &first));
    assert!(Rc::ptr_eq(&bids.borrow()[1], &second));
    let observed = crate::binding::sockets::native_book(&captured);
    assert_eq!(observed.bids.len(), 2);
    assert_eq!(observed.asks.len(), 1);
    assert_eq!(observed.bids[0].price.to_bits(), (-0.0_f64).to_bits());
    assert!(observed.bids[0].amount.is_nan());
    assert_eq!(observed.bids[0].total, f64::INFINITY);
    assert_eq!(observed.bids[1].price, 7.0);
    assert_eq!(observed.bids[1].amount, 3.0);
    assert_eq!(observed.bids[1].total, f64::NEG_INFINITY);
    assert_eq!(observed.asks[0].price, 7.0);
    assert_eq!(observed.asks[0].amount, 3.0);
    assert_eq!(observed.asks[0].total, f64::NEG_INFINITY);
    first.borrow_mut().price = 9.0;
    assert_eq!(
        crate::binding::sockets::native_book(&captured).bids[0].price,
        9.0
    );
}
