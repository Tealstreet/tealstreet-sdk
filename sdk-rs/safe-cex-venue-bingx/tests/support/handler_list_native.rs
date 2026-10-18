use super::binding_test_harness as harness;
use crate::public_stream::{HandlerList, MessageCallback, PublicFrame};
use safe_cex_exchange::{
    exchange::Exchange, js_runtime::value::Js, runtime::ExchangeError, websocket::WsEvent,
};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, rc::Rc};

fn observe(rows: &[serde_json::Value]) -> usize {
    assert_eq!(rows.len(), 1, "actual callback receives one parsed object");
    assert_eq!(
        rows[0]["data"]["n"].as_f64().unwrap().to_bits(),
        (-0f64).to_bits()
    );
    assert_eq!(
        rows[0]["data"]["size"].as_f64().unwrap().to_bits(),
        17f64.to_bits()
    );
    assert_eq!(
        rows[0]["data"]["text"]
            .as_str()
            .unwrap()
            .encode_utf16()
            .collect::<Vec<_>>(),
        "paired-🙂".encode_utf16().collect::<Vec<_>>()
    );
    rows.as_ptr() as usize
}

#[test]
fn native_callback_registry_keeps_live_lists_and_original_errors() {
    let h = harness::harness(ExchangeName::Bingx, harness::options(), |_| {});
    let stream = h.exchange.inner.public_websocket.stream.clone();
    let handlers: HandlerList = Rc::default();
    let replacement: HandlerList = Rc::default();
    let seen = Rc::new(RefCell::new(Vec::<(&'static str, usize)>::new()));
    let next_seen = seen.clone();
    let second: MessageCallback = Rc::new(move |rows| {
        let pointer = observe(rows);
        next_seen.borrow_mut().push(("second", pointer));
        Ok(())
    });
    let replace_seen = seen.clone();
    replacement.borrow_mut().push(Rc::new(move |rows| {
        let pointer = observe(rows);
        replace_seen.borrow_mut().push(("replacement", pointer));
        Ok(())
    }));
    let list_weak = Rc::downgrade(&handlers);
    let stream_weak = Rc::downgrade(&stream);
    let first_seen = seen.clone();
    let next_list = replacement.clone();
    let first: MessageCallback = Rc::new(move |rows| {
        let pointer = observe(rows);
        first_seen.borrow_mut().push(("first", pointer));
        list_weak
            .upgrade()
            .unwrap()
            .borrow_mut()
            .push(second.clone());
        let owner = stream_weak.upgrade().unwrap();
        owner
            .message_handlers
            .borrow_mut()
            .iter_mut()
            .find(|(key, _)| key == "native-alias")
            .unwrap()
            .1 = next_list.clone();
        Ok(())
    });
    handlers.borrow_mut().push(first.clone());
    stream
        .message_handlers
        .borrow_mut()
        .push(("native-alias".into(), handlers.clone()));
    assert!(Rc::ptr_eq(
        &handlers,
        &stream
            .message_handlers
            .borrow()
            .iter()
            .find(|(topic, _)| topic == "native-alias")
            .unwrap()
            .1
    ));
    assert!(Rc::ptr_eq(&first, &handlers.borrow()[0]));
    stream.connect_and_subscribe().unwrap();
    for _ in 0..12 {
        h.runtime.pump();
    }
    assert_eq!(
        h.runtime.sockets.borrow().len(),
        1,
        "actual public socket constructed"
    );
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    let raw = "{\"dataType\":\"native-alias\",\"data\":{\"n\":-0,\"size\":17,\"text\":\"paired-\\ud83d\\ude42\"}}";
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(raw.into())));
    for _ in 0..12 {
        h.runtime.pump();
    }
    let first_seen = seen.borrow();
    assert_eq!(
        first_seen.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        vec!["first", "second"],
        "actual dispatch retains the captured list and observes its appended callback"
    );
    assert_eq!(
        first_seen[0].1, first_seen[1].1,
        "both callbacks borrow the original parsed payload"
    );
    drop(first_seen);
    assert_eq!(handlers.borrow().len(), 2);
    assert!(Rc::ptr_eq(&first, &handlers.borrow()[0]));
    assert!(Rc::ptr_eq(
        &replacement,
        &stream
            .message_handlers
            .borrow()
            .iter()
            .find(|(topic, _)| topic == "native-alias")
            .unwrap()
            .1
    ));
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(raw.into())));
    for _ in 0..12 {
        h.runtime.pump();
    }
    assert_eq!(
        seen.borrow()
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        vec!["first", "second", "replacement"],
        "the next actual frame uses the replacement registry entry"
    );
    assert!(h.runtime.reports.borrow().is_empty());
    let code = Js::object(vec![
        ("native", Js::utf16(&[0xd800, 65, 0xdc00])),
        ("ownUndefined", Js::Undefined),
    ]);
    let mut original = ExchangeError::new("native callback failed");
    original.name = "RangeError".into();
    original.metadata.code = Some(code.clone());
    let throw = original.clone();
    let failures: HandlerList = Rc::new(RefCell::new(vec![Rc::new(move |_| Err(throw.clone()))]));
    stream
        .message_handlers
        .borrow_mut()
        .iter_mut()
        .find(|(topic, _)| topic == "native-alias")
        .unwrap()
        .1 = failures;
    let error = h
        .drive(stream.on_message(PublicFrame::Text(raw.into())))
        .unwrap_err();
    assert_eq!(
        error, original,
        "callback failure propagates with all metadata"
    );
    assert!(error.metadata.code.as_ref().unwrap().same_reference(&code));
    assert!(code.has_own("ownUndefined") && code.get("ownUndefined").unwrap().is_undefined());
    assert_eq!(
        code.get("native").unwrap().as_str().unwrap().as_ref(),
        &[0xd800, 65, 0xdc00]
    );
    h.exchange.dispose().unwrap();
}
