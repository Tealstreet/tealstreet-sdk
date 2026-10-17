//! Actual venue assembly, native scheduler/HTTP and private-frame dispatcher.
use super::*;
use safe_cex_exchange::{js_runtime::client, native::native_runtime};
use safe_cex_store::store::DefaultStore;
use serde_json::{Value, json};
use std::{cell::RefCell, sync::Arc};

fn run(test: impl FnOnce(Rc<AsterDEXExchange>, Rc<DefaultStore>)) {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let store = Rc::new(DefaultStore::new());
        let venue = AsterDEXExchange::new(
            ExchangeName::Asterdex,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store.clone(),
            AsterDEXDependencies {
                runtime,
                http: Rc::new(move |_| Ok(http.clone())),
                external_public_data: None,
            },
        )
        .unwrap();
        test(venue.clone(), store);
        venue.dispose().unwrap();
    });
}
fn watch(venue: &AsterDEXExchange, event: &str) -> Rc<RefCell<Vec<Value>>> {
    let values = Rc::new(RefCell::new(vec![]));
    let sink = values.clone();
    venue.base().on(
        event,
        Rc::new(move |_, value| {
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    values
}
fn frame(venue: &AsterDEXExchange, value: Value) {
    venue
        .base()
        .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private)
        .unwrap()
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            value.to_string(),
        ))
        .unwrap();
}
#[test]
fn native_facade_preserves_zero_single_array_and_multiple_arguments() {
    run(|venue, _| {
        use safe_cex_exchange::js_runtime::value::{from_json, to_json};
        let posts = Rc::new(RefCell::new(vec![]));
        let sink = posts.clone();
        for event in ["warning", "log"] {
            let sink = sink.clone();
            venue.base().on_args(
                event,
                Rc::new(move |_, args| {
                    sink.borrow_mut().push(Value::Array(args.to_vec()));
                    Ok(())
                }),
            );
        }
        let emitter = venue.rt.this.get("emitter").unwrap();
        for expected in [
            json!([]),
            json!(["message"]),
            json!([["a", "b"]]),
            json!(["message",{"forceToast":true}]),
        ] {
            let mut args = vec![Js::str("warning")];
            args.extend(expected.as_array().unwrap().iter().map(from_json));
            emitter.call_method("emit", args).unwrap();
            assert_eq!(
                posts.borrow().last(),
                Some(&expected),
                "facade preserves the actual emitted argument list"
            );
        }
        venue.base().log("message", "info").unwrap();
        assert_eq!(
            posts.borrow().last().cloned(),
            Some(json!(["[asterdex] message", "info"])),
            "native base log keeps two arguments"
        );
        // An array emitted natively remains one argument for JS subscribers.
        let calls = Rc::new(RefCell::new(vec![]));
        let sink = calls.clone();
        emitter
            .call_method(
                "on",
                vec![
                    Js::str("positions"),
                    Js::function("observe", move |_, args| {
                        sink.borrow_mut().push(to_json(&Js::array(args))?);
                        Ok(Js::Undefined)
                    }),
                ],
            )
            .unwrap();
        venue.base().emit("positions", json!(["a", "b"])).unwrap();
        assert_eq!(*calls.borrow(), vec![json!([["a", "b"]])]);
    });
}
#[test]
fn native_shared_facade_fill_is_the_first_event_argument() {
    run(|venue, _| {
        let fills = watch(&venue, "fill");
        venue
            .rt
            .this
            .get("emitter")
            .unwrap()
            .call_method(
                "emit",
                vec![
                    Js::str("fill"),
                    safe_cex_exchange::js_runtime::value::from_json(
                        &json!({"orderId":"7","amount":2}),
                    ),
                ],
            )
            .unwrap();
        assert_eq!(*fills.borrow(), vec![json!({"orderId":"7","amount":2.0})]);
    });
}
#[test]
fn native_private_balance_frame_commits_store_and_emits_account() {
    run(|venue, store| {
        let account = watch(&venue, "account");
        let data = json!({"e":"outboundAccountPosition","B":[{"a":"USDT","f":"70","l":"30"}]});
        frame(&venue, data.clone());
        assert_eq!(store.balance().free, 70.0);
        assert_eq!(store.balance().total, 100.0);
        assert_eq!(*account.borrow(), vec![data]);
    });
}
#[test]
fn native_private_position_frame_commits_store_and_emits_one_positions_array() {
    run(|venue, store| {
        safe_cex_store::Store::update(store.as_ref(), safe_cex_store::StoreUpdate { positions: Some(vec![serde_json::from_value(json!({
            "id":"p1","symbol":"BTCUSDT","side":"long","entryPrice":100,"notional":100,"leverage":5,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0
        })).unwrap()]), ..Default::default() }, false).unwrap();
        let positions = watch(&venue, "positions");
        frame(
            &venue,
            json!({"e":"ACCOUNT_UPDATE","E":1700000000000u64,"a":{"P":[{"s":"BTCUSDT","ps":"BOTH","pa":"2","ep":"102","up":"0"}],"B":[]}}),
        );
        assert_eq!(store.positions()[0].contracts, 2.0);
        assert_eq!(store.positions()[0].entry_price, 102.0);
        assert_eq!(
            *positions.borrow(),
            vec![serde_json::to_value(store.positions().to_vec()).unwrap()]
        );
    });
}
