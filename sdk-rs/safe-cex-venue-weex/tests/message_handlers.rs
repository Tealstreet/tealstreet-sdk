use safe_cex_venue_weex::message_handlers::{
    WeexMessageHandlerRuntime, add_handler, handle_routed_message, remove_handler,
};
use serde_json::Value;
use std::{cell::RefCell, path::Path, process::Command};

#[derive(Debug, Clone, PartialEq)]
enum NativeValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(i64),
    String(Vec<u16>),
    Function(usize),
    Array(usize),
    Registry(usize),
    Prototype,
    InheritedFunction(usize),
}

struct Array {
    items: Vec<Option<NativeValue>>,
    custom_iterator: bool,
}

struct IteratorState {
    array: usize,
    index: usize,
    custom: bool,
}

struct MessageData {
    number: i64,
    text: Vec<u16>,
}

struct Message(RefCell<MessageData>);

struct Call {
    handler: usize,
    number: i64,
    text: Vec<u16>,
    same_message: bool,
}

struct Host {
    registries: Vec<Vec<(Vec<u16>, NativeValue)>>,
    arrays: Vec<Array>,
    registry_reads: Vec<usize>,
    reads: usize,
    key: Vec<u16>,
    actions: Value,
    failures: Value,
    events: Vec<String>,
    calls: Vec<Call>,
    cleaned: Vec<Vec<u16>>,
    message_pointer: usize,
}

type Runtime = WeexMessageHandlerRuntime<Host, NativeValue, IteratorState, Message, String>;

fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}

fn decode(value: &Value) -> NativeValue {
    match value["kind"].as_str().unwrap() {
        "undefined" | "hole" => NativeValue::Undefined,
        "null" => NativeValue::Null,
        "boolean" => NativeValue::Boolean(value["value"].as_bool().unwrap()),
        "number" => NativeValue::Number(value["value"].as_i64().unwrap()),
        "string" => NativeValue::String(units(&value["units"])),
        "function" => NativeValue::Function(value["id"].as_u64().unwrap() as usize),
        "array" => NativeValue::Array(value["id"].as_u64().unwrap() as usize),
        other => panic!("unsupported input token {other}"),
    }
}

fn registry(host: &mut Host) -> Result<NativeValue, String> {
    host.events.push("registry".into());
    host.reads += 1;
    if host.failures["registry"].as_u64() == Some(host.reads as u64) {
        return Err("TypeError".into());
    }
    Ok(NativeValue::Registry(
        host.registry_reads[(host.reads - 1).min(host.registry_reads.len() - 1)],
    ))
}

fn get_property(host: &mut Host, value: &NativeValue, key: &[u16]) -> Result<NativeValue, String> {
    match value {
        NativeValue::Registry(id) => {
            host.events
                .push(format!("get:{id}:{}", serde_json::to_string(key).unwrap()));
            if host.failures["get"] == true {
                return Err("TypeError".into());
            }
            if let Some((_, value)) = host.registries[*id].iter().find(|(name, _)| name == key) {
                return Ok(value.clone());
            }
            Ok(match String::from_utf16(key).ok().as_deref() {
                Some("__proto__") => NativeValue::Prototype,
                Some("constructor") => NativeValue::InheritedFunction(1),
                Some("toString") => NativeValue::InheritedFunction(0),
                _ => NativeValue::Undefined,
            })
        }
        NativeValue::Undefined | NativeValue::Null => Err("TypeError".into()),
        _ => {
            if key == "length".encode_utf16().collect::<Vec<_>>() {
                Ok(match value {
                    NativeValue::Array(id) => {
                        NativeValue::Number(host.arrays[*id].items.len() as i64)
                    }
                    NativeValue::String(text) => NativeValue::Number(text.len() as i64),
                    NativeValue::InheritedFunction(length) => NativeValue::Number(*length as i64),
                    NativeValue::Function(_) => NativeValue::Number(1),
                    _ => NativeValue::Undefined,
                })
            } else {
                Ok(NativeValue::Undefined)
            }
        }
    }
}

fn set_property(
    host: &mut Host,
    object: &NativeValue,
    key: &[u16],
    value: NativeValue,
) -> Result<(), String> {
    let NativeValue::Registry(id) = object else {
        panic!("expected registry")
    };
    host.events
        .push(format!("set:{id}:{}", serde_json::to_string(key).unwrap()));
    if host.failures["set"] == true {
        return Err("TypeError".into());
    }
    if let Some((_, previous)) = host.registries[*id]
        .iter_mut()
        .find(|(name, _)| name == key)
    {
        *previous = value;
    } else {
        host.registries[*id].push((key.to_vec(), value));
    }
    Ok(())
}

fn truthy(value: &NativeValue) -> bool {
    match value {
        NativeValue::Undefined | NativeValue::Null => false,
        NativeValue::Boolean(value) => *value,
        NativeValue::Number(value) => *value != 0,
        NativeValue::String(value) => !value.is_empty(),
        _ => true,
    }
}

fn array(host: &mut Host, values: Vec<NativeValue>) -> Result<NativeValue, String> {
    let id = host.arrays.len();
    host.arrays.push(Array {
        items: values.into_iter().map(Some).collect(),
        custom_iterator: false,
    });
    Ok(NativeValue::Array(id))
}

fn iterator(host: &mut Host, value: &NativeValue) -> Result<IteratorState, String> {
    let NativeValue::Array(id) = value else {
        return Err("TypeError".into());
    };
    Ok(IteratorState {
        array: *id,
        index: 0,
        custom: host.arrays[*id].custom_iterator,
    })
}

fn next(host: &mut Host, iterator: &mut IteratorState) -> Result<Option<NativeValue>, String> {
    if iterator.custom && host.failures["next"].as_u64() == Some((iterator.index + 1) as u64) {
        return Err("TypeError".into());
    }
    if iterator.index >= host.arrays[iterator.array].items.len() {
        return Ok(None);
    }
    let value = host.arrays[iterator.array].items[iterator.index]
        .clone()
        .unwrap_or(NativeValue::Undefined);
    iterator.index += 1;
    Ok(Some(value))
}

fn close_on_throw(host: &mut Host, iterator: &mut IteratorState, error: String) -> String {
    if iterator.custom {
        host.events.push("close".into());
    }
    error
}

fn spread(host: &mut Host, value: &NativeValue) -> Result<Vec<NativeValue>, String> {
    let mut iterator = iterator(host, value)?;
    let mut values = Vec::new();
    while let Some(value) = next(host, &mut iterator)? {
        values.push(value);
    }
    Ok(values)
}

fn filter(
    host: &mut Host,
    value: &NativeValue,
    predicate: &mut dyn FnMut(&NativeValue) -> bool,
) -> Result<NativeValue, String> {
    let NativeValue::Array(id) = value else {
        return Err("TypeError".into());
    };
    let mut items = Vec::new();
    for index in 0..host.arrays[*id].items.len() {
        if let Some(item) = &host.arrays[*id].items[index] {
            if predicate(item) {
                items.push(item.clone());
            }
        }
    }
    array(host, items)
}

fn cleanup_stats(host: &mut Host, key: &[u16]) -> Result<(), String> {
    host.events.push("cleanup".into());
    host.cleaned.push(key.to_vec());
    if host.failures["cleanup"] == true {
        return Err("RangeError".into());
    }
    Ok(())
}

fn invoke(host: &mut Host, value: &NativeValue, message: &Message) -> Result<(), String> {
    let NativeValue::Function(handler) = value else {
        return Err("TypeError".into());
    };
    {
        let data = message.0.borrow();
        host.calls.push(Call {
            handler: *handler,
            number: data.number,
            text: data.text.clone(),
            same_message: message as *const Message as usize == host.message_pointer,
        });
    }
    host.events.push(format!("invoke:{handler}"));
    match host.actions[handler.to_string()].as_str() {
        Some("append") => host.arrays[0].items.push(Some(NativeValue::Function(3))),
        Some("replace") => {
            let empty = array(host, Vec::new())?;
            set_property(host, &NativeValue::Registry(0), &host.key.clone(), empty)?;
        }
        Some("remove_second") => remove_handler(
            host,
            &host.key.clone(),
            &NativeValue::Function(2),
            &runtime(),
        )?,
        Some("message_mutation") => {
            let mut data = message.0.borrow_mut();
            data.number = 99;
            data.text = "changed".encode_utf16().collect();
        }
        Some("throw") => return Err("RangeError".into()),
        _ => {}
    }
    Ok(())
}

fn runtime() -> Runtime {
    Runtime {
        registry,
        get_property,
        set_property,
        truthy,
        nullish: |value| matches!(value, NativeValue::Undefined | NativeValue::Null),
        array,
        spread,
        filter,
        strict_equal: |a, b| a == b,
        iterator,
        next,
        close_on_throw,
        invoke,
        cleanup_stats,
    }
}

fn assert_value(host: &Host, native: &NativeValue, expected: &Value, label: &str) {
    match native {
        NativeValue::Undefined => assert_eq!(expected["kind"], "undefined", "{label}"),
        NativeValue::Null => assert_eq!(expected["kind"], "null", "{label}"),
        NativeValue::Boolean(value) => {
            assert_eq!(expected["kind"], "boolean", "{label}");
            assert_eq!(*value, expected["value"].as_bool().unwrap(), "{label}");
        }
        NativeValue::Number(value) => {
            assert_eq!(expected["kind"], "number", "{label}");
            assert_eq!(*value, expected["value"].as_i64().unwrap(), "{label}");
        }
        NativeValue::String(value) => {
            assert_eq!(expected["kind"], "string", "{label}");
            assert_eq!(*value, units(&expected["units"]), "{label}");
        }
        NativeValue::Function(id) => {
            assert_eq!(expected["kind"], "function", "{label}");
            assert_eq!(*id as u64, expected["id"].as_u64().unwrap(), "{label}");
        }
        NativeValue::Array(id) => {
            assert_eq!(expected["kind"], "array", "{label}");
            let array = &host.arrays[*id];
            let values = expected["values"].as_array().unwrap();
            assert_eq!(array.items.len(), values.len(), "{label}");
            for (item, expected) in array.items.iter().zip(values) {
                assert_value(
                    host,
                    item.as_ref().unwrap_or(&NativeValue::Undefined),
                    expected,
                    label,
                );
            }
            let present: Vec<bool> = expected["present"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_bool().unwrap())
                .collect();
            assert_eq!(
                array.items.iter().map(Option::is_some).collect::<Vec<_>>(),
                present,
                "{label}"
            );
        }
        other => panic!("unexpected own native value {other:?}"),
    }
}

#[test]
fn message_handlers_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/message-handlers-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let ids = [
        "add/missing",
        "add/append",
        "add/duplicate",
        "add/null",
        "add/false",
        "add/holes",
        "add/utf16_key",
        "add/proto",
        "add/constructor",
        "add/changed_registry",
        "add/get_error",
        "add/set_error",
        "remove/all_duplicates",
        "remove/last",
        "remove/missing",
        "remove/absent_handler",
        "remove/holes",
        "remove/cleanup_error",
        "remove/proto",
        "remove/changed_registry",
        "route/missing",
        "route/null",
        "route/empty",
        "route/ordered",
        "route/live_append",
        "route/replace",
        "route/reenter_remove",
        "route/message_mutation",
        "route/throw_first",
        "route/custom_close",
        "route/close_throws",
        "route/next_throws",
        "route/constructor",
        "route/toString",
        "route/registry_error",
    ];
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 35, "handlers::case_count");
    let (mut call_count, mut cleanup_count) = (0, 0);
    let mut observed_events = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let id = ids[index];
        assert_eq!(case["id"], id, "handlers::case_identity");
        let input = &case["input"];
        let mut host = Host {
            registries: input["registries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|registry| {
                    registry
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|entry| (units(&entry["key"]), decode(&entry["value"])))
                        .collect()
                })
                .collect(),
            arrays: input["arrays"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(id, array)| Array {
                    items: array
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| (value["kind"] != "hole").then(|| decode(value)))
                        .collect(),
                    custom_iterator: id == 0 && input["customIterator"] == true,
                })
                .collect(),
            registry_reads: serde_json::from_value(input["registryReads"].clone()).unwrap(),
            reads: 0,
            key: units(&input["key"]),
            actions: input["actions"].clone(),
            failures: input["failures"].clone(),
            events: Vec::new(),
            calls: Vec::new(),
            cleaned: Vec::new(),
            message_pointer: 0,
        };
        let message = Message(RefCell::new(MessageData {
            number: input["message"]["number"].as_i64().unwrap(),
            text: units(&input["message"]["text"]),
        }));
        assert_eq!(
            message.0.borrow().number,
            37,
            "handlers::decoded_message_number"
        );
        assert_eq!(
            message.0.borrow().text,
            [0xd800, 0xd83d, 0xde00],
            "handlers::decoded_message_utf16"
        );
        host.message_pointer = &message as *const Message as usize;
        let key = host.key.clone();
        let handler = decode(&input["handler"]);
        let result = match case["name"].as_str().unwrap() {
            "addHandler" => add_handler(&mut host, &key, handler, &runtime()),
            "removeHandler" => remove_handler(&mut host, &key, &handler, &runtime()),
            "handleRoutedMessage" => handle_routed_message(&mut host, &key, &message, &runtime()),
            other => panic!("unexpected entry {other}"),
        };
        match result {
            Ok(()) => assert_eq!(
                case["result"]["unit"].as_bool(),
                Some(true),
                "handlers::{id}::result"
            ),
            Err(error) => assert_eq!(
                Some(error.as_str()),
                case["result"]["error"].as_str(),
                "handlers::{id}::result"
            ),
        }
        let states = case["states"].as_array().unwrap();
        assert_eq!(
            host.registries.len(),
            states.len(),
            "handlers::{id}::registry_count"
        );
        for (native, expected) in host.registries.iter().zip(states) {
            let entries = expected.as_array().unwrap();
            assert_eq!(native.len(), entries.len(), "handlers::{id}::own_key_count");
            for ((key, value), expected) in native.iter().zip(entries) {
                assert_eq!(*key, units(&expected["key"]), "handlers::{id}::state_key");
                assert_value(
                    &host,
                    value,
                    &expected["value"],
                    &format!("handlers::{id}::state_value"),
                );
            }
        }
        let originals = case["arrays"].as_array().unwrap();
        assert_eq!(
            originals.len(),
            input["arrays"].as_array().unwrap().len(),
            "handlers::{id}::original_inventory"
        );
        for (index, original) in originals.iter().enumerate() {
            assert_value(
                &host,
                &NativeValue::Array(index),
                original,
                &format!("handlers::{id}::original_value"),
            );
        }
        if let Some(expected) = case["retainsOriginal"].as_bool() {
            let native = host.registries[0]
                .iter()
                .find(|(name, _)| name == &key)
                .map(|(_, value)| value);
            assert_eq!(
                native == Some(&NativeValue::Array(0)),
                expected,
                "handlers::{id}::array_identity"
            );
        }
        let calls = case["calls"].as_array().unwrap();
        assert_eq!(host.calls.len(), calls.len(), "handlers::{id}::call_count");
        for (native, expected) in host.calls.iter().zip(calls) {
            assert_eq!(
                native.handler as u64,
                expected["handler"].as_u64().unwrap(),
                "handlers::{id}::callback_identity"
            );
            assert_eq!(
                native.number,
                expected["number"].as_i64().unwrap(),
                "handlers::{id}::callback_number"
            );
            assert_eq!(
                native.text,
                units(&expected["text"]),
                "handlers::{id}::callback_text"
            );
            assert_eq!(
                native.same_message,
                expected["sameMessage"].as_bool().unwrap(),
                "handlers::{id}::message_identity"
            );
        }
        let data = message.0.borrow();
        assert_eq!(
            data.number,
            case["messageAfter"]["number"].as_i64().unwrap(),
            "handlers::{id}::message_after_number"
        );
        assert_eq!(
            data.text,
            units(&case["messageAfter"]["text"]),
            "handlers::{id}::message_after_text"
        );
        let cleaned: Vec<Vec<u16>> = serde_json::from_value(case["cleaned"].clone()).unwrap();
        assert_eq!(host.cleaned, cleaned, "handlers::{id}::cleanup_keys");
        observed_events.push(host.events.clone());
        if id == "remove/missing" || id == "remove/last" {
            assert_eq!(host.registries[0].len(), 1, "handlers::empty_key_retained");
            let NativeValue::Array(array) = host.registries[0][0].1 else {
                panic!("expected retained array")
            };
            assert_eq!(
                host.arrays[array].items.len(),
                0,
                "handlers::retained_array_empty"
            );
            assert_eq!(host.cleaned.len(), 1, "handlers::empty_key_cleanup");
        }
        call_count += host.calls.len();
        cleanup_count += host.cleaned.len();
    }
    assert_eq!(call_count, 16, "handlers::independent_callback_count");
    assert_eq!(cleanup_count, 4, "handlers::independent_cleanup_count");
    for (index, case) in cases.iter().enumerate() {
        let id = ids[index];
        let expected: Vec<String> = serde_json::from_value(case["events"].clone()).unwrap();
        assert_eq!(observed_events[index], expected, "handlers::{id}::effects");
    }
}
