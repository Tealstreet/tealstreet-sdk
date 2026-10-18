use crate::binding::Host;
use crate::binding_test_support::{drive, fixture_for};
use crate::ws_controls::{
    Entries as HostEntries, Result as HostResult, Value as HostValue, WsHost,
};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::value::{Js, Thrown},
    websocket::WsEvent,
};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[test]
fn native_host_aliases_preserve_values_entries_and_original_callback_failures() {
    for name in [
        ExchangeName::Kodiakfi,
        ExchangeName::Mode,
        ExchangeName::Salsadex,
        ExchangeName::TealstreetDex,
        ExchangeName::What,
        ExchangeName::Woofi,
    ] {
        let (venue, runtime, _, _) = fixture_for(
            name.clone(),
            json!({"key":"fixture-key","secret":"11111111111111111111111111111111","applicationId":"fixture-account"}),
        );
        let host = Host::new(&venue.rt);
        let child = Js::object(vec![
            ("n", Js::Number(-0.0)),
            ("infinity", Js::Number(f64::INFINITY)),
            ("nan", Js::Number(f64::from_bits(0x7ff8000000000017))),
            ("ownUndefined", Js::Undefined),
            ("text", Js::utf16(&[0xd800, 65, 0xdc00])),
        ]);
        let root: HostValue<Host> = Rc::new(Js::object(vec![("child", child.clone())]));
        let captured: HostValue<Host> = host.get(&root, &units("child")).unwrap();
        assert!(
            captured.same_reference(&child),
            "{name:?}: native Value keeps the child referent"
        );
        let number: HostValue<Host> = host.get(&captured, &units("n")).unwrap();
        assert_eq!(
            number.as_f64().unwrap().to_bits(),
            (-0.0f64).to_bits(),
            "native Value keeps negative-zero bits"
        );
        assert_eq!(
            host.get(&captured, &units("infinity"))
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            f64::INFINITY.to_bits()
        );
        assert_eq!(
            host.get(&captured, &units("nan"))
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            0x7ff8000000000017
        );
        assert!(
            captured.has_own("ownUndefined")
                && captured.get("ownUndefined").unwrap().is_undefined()
        );
        assert_eq!(
            captured.get("text").unwrap().as_str().unwrap().as_ref(),
            &[0xd800, 65, 0xdc00]
        );
        let input = Js::object(vec![
            ("tail", child.clone()),
            ("2", Js::Number(-0.0)),
            ("1", Js::Number(17.0)),
        ]);
        input
            .define_key(&[0xd800, 65, 0xdc00], Js::Undefined)
            .unwrap();
        let entries: HostEntries<Host> = host.entries(Rc::new(input.clone())).unwrap();
        assert_eq!(entries.len(), 4, "native Entries keeps all own members");
        assert_eq!(
            entries
                .iter()
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>(),
            vec![
                units("1"),
                units("2"),
                units("tail"),
                vec![0xd800, 65, 0xdc00]
            ]
        );
        assert_eq!(entries[0].1.as_f64().unwrap().to_bits(), 17f64.to_bits());
        assert_eq!(entries[1].1.as_f64().unwrap().to_bits(), (-0f64).to_bits());
        assert!(entries[2].1.same_reference(&child));
        assert!(entries[3].1.is_undefined());
        input.define("later", Js::Bool(true)).unwrap();
        child.set("afterCapture", Js::Number(23.0)).unwrap();
        assert_eq!(
            entries.len(),
            4,
            "entries snapshot owns the original cardinality"
        );
        assert_eq!(
            entries[2].1.get("afterCapture").unwrap().as_f64(),
            Some(23.0)
        );
        let receiver = Js::object(vec![]);
        let expected_receiver = receiver.clone();
        let expected_child = child.clone();
        let callback = Js::function("nativeAliasSuccess", move |this, args| {
            assert!(this.same_reference(&expected_receiver));
            assert!(args[0].same_reference(&expected_child));
            Ok(args[0].clone())
        });
        let success: HostResult<Host, HostValue<Host>> =
            host.call(Rc::new(callback), Some(Rc::new(receiver)), vec![captured]);
        assert!(success.unwrap().same_reference(&child));
        let failure = Thrown::error("RangeError", "original native callback");
        failure
            .value
            .set("message", Js::utf16(&[0xd800, 65, 0xdc00]))
            .unwrap();
        failure.value.set("code", Js::Undefined).unwrap();
        let original = failure.value.clone();
        let throw = failure.clone();
        let rejected = Js::function("nativeAliasFailure", move |_, _| Err(throw.clone()));
        let result: HostResult<Host, HostValue<Host>> =
            host.call(Rc::new(rejected.clone()), None, vec![]);
        assert!(
            result.unwrap_err().value.same_reference(&original),
            "native Result keeps original failure identity"
        );
        let socket = venue.rt.this.get("publicWebsocket").unwrap();
        let seen = Rc::new(RefCell::new(None::<Js>));
        let count = Rc::new(Cell::new(0));
        let seen_first = seen.clone();
        let count_first = count.clone();
        let first = Js::function("firstAliasHandler", move |_, args| {
            assert_eq!(args.len(), 1);
            let value = args[0].clone();
            value.set("nativeUndefined", Js::Undefined)?;
            *seen_first.borrow_mut() = Some(value);
            count_first.set(count_first.get() + 1);
            Ok(Js::Undefined)
        });
        let seen_next = seen.clone();
        let count_next = count.clone();
        let second = Js::function("secondAliasHandler", move |_, args| {
            assert!(args[0].same_reference(seen_next.borrow().as_ref().unwrap()));
            assert!(
                args[0].has_own("nativeUndefined")
                    && args[0].get("nativeUndefined")?.is_undefined()
            );
            let data = args[0].get("data")?;
            assert_eq!(
                data.get("n")?.as_f64().unwrap().to_bits(),
                (-0f64).to_bits()
            );
            assert_eq!(data.get("text")?.as_str().unwrap().as_ref(), &[0xd800]);
            count_next.set(count_next.get() + 1);
            Ok(Js::Undefined)
        });
        socket
            .set(
                "messageHandlers",
                Js::object(vec![("native-alias", Js::array(vec![first, second]))]),
            )
            .unwrap();
        socket.call_method("connectAndSubscribe", vec![]).unwrap();
        for _ in 0..12 {
            runtime.pump();
        }
        assert_eq!(
            runtime.sockets.borrow().len(),
            1,
            "actual public socket constructed"
        );
        let wire = runtime.sockets.borrow()[0].1.clone();
        let raw = "{\"topic\":\"native-alias\",\"data\":{\"n\":-0,\"text\":\"\\ud800\"}}";
        wire.events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(raw.into())));
        for _ in 0..12 {
            runtime.pump();
        }
        assert_eq!(
            count.get(),
            2,
            "actual websocket dispatch runs both retained callbacks"
        );
        socket
            .set(
                "messageHandlers",
                Js::object(vec![("native-alias", Js::array(vec![rejected]))]),
            )
            .unwrap();
        let error = socket
            .call_method("onMessage", vec![Js::object(vec![("data", Js::str(raw))])])
            .unwrap_err();
        assert!(
            error.value.same_reference(&original),
            "actual websocket Result retains the original callback failure"
        );
        assert_eq!(
            error
                .value
                .get("message")
                .unwrap()
                .as_str()
                .unwrap()
                .as_ref(),
            &[0xd800, 65, 0xdc00]
        );
        assert!(error.value.has_own("code") && error.value.get("code").unwrap().is_undefined());
        drive(&runtime, async { venue.dispose().unwrap() });
    }
}
