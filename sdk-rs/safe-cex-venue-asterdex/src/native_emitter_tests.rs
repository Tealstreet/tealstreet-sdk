//! Actual venue assembly, native scheduler/HTTP and private-frame dispatcher.
use super::*;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{js_runtime::client, native::native_runtime};
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
            sink.borrow_mut()
                .push(safe_cex_exchange::js_runtime::value::to_json(value)?);
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

#[cfg(test)]
fn native_input_source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/accepted-native-input-source.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[cfg(test)]
fn native_input_tag(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Null => json!({"kind":"null"}),
        Js::Bool(value) => json!({"kind":"boolean","value":value}),
        Js::Number(value) => json!({"kind":"number","bits":format!("{:016x}",value.to_bits())}),
        Js::String(units) => json!({"kind":"string","units":units.as_ref()}),
        _ => {
            json!({"kind":"json","units":safe_cex_exchange::js_runtime::globals::stringify_json(value).unwrap().unwrap()})
        }
    }
}
#[test]
fn native_input_fill_keeps_the_source_symbol_value() {
    let source = native_input_source();
    run(|venue, _| {
        let fills = Rc::new(RefCell::new(Vec::new()));
        let saved = fills.clone();
        venue.base().on(
            "fill",
            Rc::new(move |_, fill| {
                saved.borrow_mut().push(fill.clone());
                Ok(())
            }),
        );
        let socket = venue
            .base()
            .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private)
            .unwrap();
        for (index, case) in source["fillCases"].as_array().unwrap().iter().enumerate() {
            socket
                .on_message(&safe_cex_transport::socket::SocketFrame::Text(
                    case["frame"].as_str().unwrap().into(),
                ))
                .unwrap();
            assert_eq!(
                fills.borrow().len(),
                index + 1,
                "actual private socket emits each execution"
            );
            let fill = fills.borrow()[index].clone();
            assert!(fill.has_own("symbol"));
            assert_eq!(
                native_input_tag(&fill.get("symbol").unwrap()),
                case["symbol"],
                "ASTER-NATIVE-INPUT source fill copies its symbol before any typed projection: {index}"
            );
        }
    });
}
#[test]
fn native_input_installed_signing_globals_match_source_byte_domains() {
    let source = native_input_source();
    run(|venue, _| {
        for case in source["cryptoCases"].as_array().unwrap() {
            let bytes = safe_cex_exchange::js_runtime::value::from_json(&case["bytes"]);
            for (global, field) in [
                ("ethers.keccak256", "keccak"),
                ("ethers.hashMessage", "hashMessage"),
            ] {
                let result = venue
                    .rt
                    .global(global)
                    .unwrap()
                    .call(Js::Undefined, vec![bytes.clone()])
                    .unwrap();
                assert_eq!(
                    result
                        .array_values()
                        .unwrap()
                        .unwrap()
                        .iter()
                        .map(|value| value.to_number().unwrap())
                        .collect::<Vec<_>>(),
                    case[field]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_f64().unwrap())
                        .collect::<Vec<_>>()
                );
            }
            let result = venue
                .rt
                .global("ethers.getBytes")
                .unwrap()
                .call(Js::Undefined, vec![Js::str(case["hex"].as_str().unwrap())])
                .unwrap();
            assert_eq!(
                result
                    .array_values()
                    .unwrap()
                    .unwrap()
                    .iter()
                    .map(|value| value.to_number().unwrap())
                    .collect::<Vec<_>>(),
                case["bytes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_f64().unwrap())
                    .collect::<Vec<_>>()
            );
        }
        for case in source["signatures"].as_array().unwrap() {
            let result = venue
                .rt
                .global("generateV1Signature")
                .unwrap()
                .call(
                    Js::Undefined,
                    vec![
                        Js::str(case["query"].as_str().unwrap()),
                        Js::str(case["secret"].as_str().unwrap()),
                    ],
                )
                .unwrap();
            assert!(result.strict_equals(&Js::str(case["signature"].as_str().unwrap())));
        }
        let original = Js::object(vec![("marker", Js::Number(21.0))]);
        let saved = original.clone();
        let query = Js::object(vec![(
            "toString",
            Js::function("toString", move |_, _| {
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                    saved.clone(),
                ))
            }),
        )]);
        let error = venue
            .rt
            .global("generateV1Signature")
            .unwrap()
            .call(Js::Undefined, vec![query, Js::str("secret")])
            .unwrap_err();
        assert!(
            error.value.strict_equals(&original),
            "text coercion retains the original getter failure"
        );
    });
}
#[test]
fn native_input_scope_keeps_the_constructed_receiver_and_weak_owner() {
    run(|venue, _| {
        let receiver = Js::object(vec![("marker", Js::Number(21.0))]);
        let weak = Rc::downgrade(&venue.rt.shared);
        let context = binding::scope(&weak, receiver.clone()).unwrap();
        assert!(Rc::ptr_eq(&context.shared, &venue.rt.shared));
        assert!(
            context.this.strict_equals(&receiver),
            "ASTER-NATIVE-INPUT scope must retain its actual receiver"
        );
        drop(context);
        let error = match binding::scope(&std::rc::Weak::new(), receiver) {
            Ok(_) => panic!("expired owner must visibly reject"),
            Err(error) => error,
        };
        assert!(
            error
                .value
                .get("message")
                .unwrap()
                .strict_equals(&Js::str("AsterDEX exchange was dropped"))
        );
    });
}
#[test]
fn native_input_klines_preserves_custom_map_receiver_and_row_getter_failure() {
    run(|_, _| {
        let original = Js::object(vec![("failure", Js::Bool(true))]);
        let saved = original.clone();
        let row = Js::object(vec![]);
        row.define_accessor(
            "1",
            Some(Rc::new(move |_| {
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                    saved.clone(),
                ))
            })),
            None,
        )
        .unwrap();
        let error = crate::exchange_helpers::transform_klines(&Js::array(vec![row])).unwrap_err();
        assert!(error.value.strict_equals(&original));
        let rows = Js::object(vec![]);
        let expected_receiver = rows.clone();
        rows.define(
            "map",
            Js::function("map", move |this, args| {
                assert!(this.strict_equals(&expected_receiver));
                let row = safe_cex_exchange::js_runtime::value::from_json(&json!([
                    1000,
                    "-0",
                    "Infinity",
                    "-Infinity",
                    "bad",
                    true
                ]));
                args[0].call(Js::Undefined, vec![row])
            }),
        )
        .unwrap();
        let result = crate::exchange_helpers::transform_klines(&rows).unwrap();
        assert_eq!(
            result.get("open").unwrap().to_number().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            result.get("high").unwrap().to_number().unwrap(),
            f64::INFINITY
        );
        assert!(result.get("close").unwrap().to_number().unwrap().is_nan());
        rows.define("map", Js::Undefined).unwrap();
    });
}

#[test]
fn constructed_private_handlers_keep_surrogate_frames_and_their_neighbors() {
    run(|venue, _| {
        let socket = venue
            .base()
            .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Private)
            .unwrap();
        let calls = Rc::new(RefCell::new(0));
        let seen = calls.clone();
        socket.message_handlers.borrow_mut().insert(
            "retainedProbe".into(),
            Rc::new(RefCell::new(vec![Rc::new(move |_| {
                *seen.borrow_mut() += 1;
                Ok(())
            })])),
        );
        for marker in [r"\ud800", r"\udfff", r"\ud83e\udd80"] {
            let frame = format!(r#"{{"e":"retainedProbe","unused":{{"{marker}":["{marker}"]}}}}"#);
            socket
                .on_message(&safe_cex_transport::socket::SocketFrame::Text(frame))
                .unwrap();
        }
        frame(&venue, json!({"e":"retainedProbe","neighbor":true}));
        assert_eq!(
            *calls.borrow(),
            4,
            "all source-valid surrogate frames and the ordinary neighbor reach the constructed handler"
        );
    });
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
                    sink.borrow_mut().push(to_json(&Js::array(args.to_vec()))?);
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
        venue
            .base()
            .emit("positions", from_json(&json!(["a", "b"])))
            .unwrap();
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
fn private_fill_object_id_calls_the_inherited_object_intrinsic() {
    let source = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(["--import", "tsx", "-e", r#"
const assert = require('node:assert/strict');
const {AsterDEXExchange} = require('./packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const {DefaultStore} = require('./packages/safe-cex/src/store/store.base.ts');
const exchange = new AsterDEXExchange('asterdex', {publicDataOnly:true,tuning:{consoleLogging:false}}, new DefaultStore());
const fills=[]; exchange.on('fill', value=>fills.push(value));
for (const id of [{}, {toString:null}]) {
  exchange.privateWebsocket.onMessage({data:JSON.stringify({e:'ORDER_TRADE_UPDATE',o:{i:id,s:'BTCUSDT',X:'PARTIALLY_FILLED',S:'BUY',l:'2',L:'3',ap:'3'}})});
}
assert.equal(fills.length,1); assert.equal(fills[0].orderId,'[object Object]');
const bits = value => {const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.toString('hex');};
exchange.dispose(); process.stdout.write(JSON.stringify(fills.map(fill=>({...fill,price:bits(fill.price),amount:bits(fill.amount)}))),()=>process.exit(0));
"#])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Value = serde_json::from_slice(&source.stdout).unwrap();
    run(|venue, _| {
        let fills = watch(&venue, "fill");
        for id in [json!({}), json!({"toString":null})] {
            frame(
                &venue,
                json!({"e":"ORDER_TRADE_UPDATE","o":{"i":id,"s":"BTCUSDT","X":"PARTIALLY_FILLED","S":"BUY","l":"2","L":"3","ap":"3"}}),
            );
        }
        assert_eq!(
            fills.borrow().len(),
            1,
            "inherited Object.toString must emit the first fill; own noncallable shadow still refuses"
        );
        let mut observed = serde_json::to_value(&*fills.borrow()).unwrap();
        for fill in observed.as_array_mut().unwrap() {
            for field in ["price", "amount"] {
                fill[field] = json!(format!("{:016x}", fill[field].as_f64().unwrap().to_bits()));
            }
        }
        assert_eq!(observed, expected);
    });
}
#[test]
fn native_private_balance_frame_commits_store_and_emits_account() {
    run(|venue, store| {
        let account = watch(&venue, "account");
        let data = json!({"e":"outboundAccountPosition","B":[{"a":"USDT","f":"70","l":"30"}]});
        frame(&venue, data.clone());
        assert_eq!(store.balance().unwrap().free, 70.0);
        assert_eq!(store.balance().unwrap().total, 100.0);
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
        assert_eq!(store.positions().unwrap()[0].contracts, 2.0);
        assert_eq!(store.positions().unwrap()[0].entry_price, 102.0);
        assert_eq!(
            *positions.borrow(),
            vec![serde_json::to_value(store.positions().unwrap().to_vec()).unwrap()]
        );
    });
}

#[cfg(test)]
mod retained_handler_support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-exchange/tests/support/retained_handlers.rs"
    ));
}
#[test]
fn shared_core_handlers_retain_the_source_graph_and_native_fields() {
    run(|venue, _| {
        for (slot, source_slot) in [
            (
                safe_cex_exchange::socket_lifecycle::SocketSlot::Public,
                "publicWebsocket",
            ),
            (
                safe_cex_exchange::socket_lifecycle::SocketSlot::Private,
                "privateWebsocket",
            ),
        ] {
            let socket = venue.base().get_socket(slot).unwrap();
            let cases = retained_handler_support::source_frames("asterdex", source_slot);
            let (handlers, calls) = retained_handler_support::pair(&cases, false);
            socket
                .message_handlers
                .borrow_mut()
                .insert("retainedProbe".into(), handlers);
            for case in cases {
                socket
                    .on_message(&safe_cex_transport::socket::SocketFrame::Text(
                        case["frame"].as_str().unwrap().into(),
                    ))
                    .unwrap();
            }
            assert_eq!(calls.get(), 8, "{source_slot} native callback observations");
        }
    });
}
