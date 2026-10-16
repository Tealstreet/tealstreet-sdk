#[path = "ws.rs"]
mod support;
use super::on_message;
use safe_cex_exchange::{
    runtime::ExchangeError,
    websocket::{BaseWebSocket, HandlerList, MessageHandler},
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use support::*;

#[test]
fn public_dispatch_matches_live_source_with_explicit_inherited_stats_write_finding() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/public-dispatch.json")).unwrap();
    assert_eq!(cases.len(), 126);
    let mut inherited_findings = 0;
    for case in cases {
        let input = &case["input"];
        let runtime = Rc::new(Runtime::default());
        let parent = Parent::new();
        parent.disposed.set(input["disposed"] == true);
        let socket = BaseWebSocket::new(1, parent, runtime.clone(), Rc::new(Protocol));
        let calls = Rc::new(RefCell::new(Vec::<String>::new()));
        for key in ["tickers", "ping", "pong"]
            .into_iter()
            .chain((input["shadow"] == true).then_some("constructor"))
        {
            let list: HandlerList = Rc::default();
            if input["empty"] != true {
                for index in 0..2 {
                    let calls = calls.clone();
                    let weak = Rc::downgrade(&socket);
                    let weak_list = Rc::downgrade(&list);
                    let remove = input["removeKey"] == true;
                    let append = input["append"] == true;
                    let throwing = input["throwFirst"] == true;
                    let callback: MessageHandler = Rc::new(move |_| {
                        calls.borrow_mut().push(format!("{key}:{index}"));
                        if index == 0 {
                            if remove {
                                weak.upgrade()
                                    .unwrap()
                                    .message_handlers
                                    .borrow_mut()
                                    .shift_remove(key);
                            }
                            if append {
                                let calls = calls.clone();
                                weak_list.upgrade().unwrap().borrow_mut().push(Rc::new(
                                    move |_| {
                                        calls.borrow_mut().push(format!("{key}:2"));
                                        Ok(())
                                    },
                                ));
                            }
                            if throwing {
                                let mut e = ExchangeError::new("callback failed");
                                e.name = "TypeError".into();
                                return Err(e);
                            }
                        }
                        Ok(())
                    });
                    list.borrow_mut().push(callback);
                }
            }
            socket
                .message_handlers
                .borrow_mut()
                .insert(key.into(), list);
        }
        let result = on_message(&socket, input["frame"].as_str().unwrap());
        let error = result
            .err()
            .map(|e| json!({"name":e.name,"message":e.message}));
        assert_eq!(json!(error), case["error"], "dispatch error {input}");
        assert_eq!(
            json!(*calls.borrow()),
            case["calls"],
            "dispatch calls {input}"
        );
        assert_eq!(
            usize::from(socket.diagnostics().last_message_timestamp.is_some()),
            case["received"].as_u64().unwrap() as usize,
            "receive tracking {input}"
        );
        let stats = socket.message_handlers_stats.borrow();
        assert_eq!(
            stats.len(),
            case["stats"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|entry| entry["own"] != false)
                .count(),
            "own stats entries {input}"
        );
        for expected in case["stats"].as_array().unwrap() {
            let key = expected["key"].as_str().unwrap();
            if expected["own"] == false {
                inherited_findings += 1;
                assert_eq!(expected["had"], true);
                assert!(
                    !stats.contains_key(key),
                    "FINDING core refuses inherited global stats writes"
                );
            } else {
                let actual = &stats[key];
                assert_eq!(json!(actual.had_handlers), expected["had"]);
                assert_eq!(json!(actual.last_callbacks_count), expected["count"]);
            }
        }
        drop(stats);
        runtime.pump();
        runtime.discard_tasks();
    }
    assert!(inherited_findings > 0);
}
