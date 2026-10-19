use safe_cex_exchange::{
    fill_event_units::{FILL_EVENT_UNIT_VENUES, FillEventUnit, fill_event_units},
    websocket::{MessageHandler, MessageHandlerStat, MessageHandlers, MessageHandlersStats},
};
use safe_cex_num::{ProtoMember, TableHit};
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, process::Command, rc::Rc};
fn oracle(mode: &str) -> Value {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/root_rows_oracle.cjs"))
        .arg(mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ROOT actual TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn root_fill_table_native_values_keys_and_prototype_kind() {
    const KEYS: [&str; 22] = [
        "bybit",
        "bybitdemo",
        "bybitinverse",
        "bybitspot",
        "hyrotrader",
        "bitrue",
        "binance",
        "binancepm",
        "binancespot",
        "hyperliquid",
        "hyperliquidspot",
        "grvt",
        "asterdex",
        "weex",
        "backpack",
        "okx",
        "okxeu",
        "okxdemo",
        "okxeudemo",
        "kucoin",
        "blofin",
        "mexc",
    ];
    assert_eq!(
        FILL_EVENT_UNIT_VENUES, &KEYS,
        "ROOT fill native ordered 22 keys"
    );
    for (i, key) in KEYS.iter().enumerate() {
        assert_eq!(
            fill_event_units(key),
            TableHit::Mapped(if i < 15 {
                FillEventUnit::Native
            } else {
                FillEventUnit::Base
            }),
            "ROOT fill native unit {key}"
        );
    }
    assert_eq!(fill_event_units("missing"), TableHit::Missing);
    assert_eq!(
        fill_event_units("toString"),
        TableHit::Inherited(ProtoMember::ToString)
    );
    assert_eq!(
        fill_event_units("__proto__"),
        TableHit::Inherited(ProtoMember::Prototype)
    );
    assert_eq!(ProtoMember::ToString.length(), Some(0));
    let source = oracle("fill");
    assert_eq!(source["keys"], json!(KEYS));
    assert_eq!(source["native"], 15);
    assert_eq!(source["base"], 7);
    assert_eq!(source["prototypeArity"], 0);
    let venues = source["venues"].as_array().unwrap();
    assert_eq!(venues.len(), 49);
    let prototype = source["prototype"].as_array().unwrap();
    assert_eq!(prototype.len(), 12);
    for row in prototype {
        let name = row["name"].as_str().unwrap();
        let TableHit::Inherited(member) = fill_event_units(name) else {
            panic!("ROOT source builtin missing {name}");
        };
        assert_eq!(json!(member.length()), row["length"], "ROOT arity {name}");
        assert_eq!(member.js_string(), row["text"], "ROOT builtin text {name}");
    }
    for name in source["absent"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        assert_eq!(fill_event_units(name), TableHit::Missing, "ROOT miss {name}");
    }
    for venue in venues {
        let name = venue["name"].as_str().unwrap();
        let expected = match venue["unit"].as_str() {
            Some("native") => TableHit::Mapped(FillEventUnit::Native),
            Some("base") => TableHit::Mapped(FillEventUnit::Base),
            None => TableHit::Missing,
            other => panic!("unexpected source fill unit {other:?}"),
        };
        assert_eq!(
            fill_event_units(name),
            expected,
            "ROOT full venue fill unit {name}"
        );
    }
}
#[test]
fn root_handler_aliases_native_presence_bits_and_callback_identity() {
    let mut stats = MessageHandlersStats::new();
    stats.insert(
        "other".into(),
        MessageHandlerStat {
            last_message_timestamp: -0.0,
            last_callbacks_count: safe_cex_num::PrototypeScalar::Undefined,
            had_handlers: None,
            had_handlers_undefined: false,
        },
    );
    stats.insert(
        "42".into(),
        MessageHandlerStat {
            last_message_timestamp: 17.25,
            last_callbacks_count: safe_cex_num::PrototypeScalar::from(Some(2)),
            had_handlers: Some(false),
            had_handlers_undefined: false,
        },
    );
    assert_eq!(
        stats["other"].last_message_timestamp.to_bits(),
        (-0.0f64).to_bits(),
        "ROOT stats native negative zero"
    );
    assert_eq!(
        stats["other"].last_callbacks_count,
        safe_cex_num::PrototypeScalar::from(None)
    );
    assert_eq!(stats["other"].had_handlers, None);
    assert_eq!(
        stats["42"].last_callbacks_count,
        safe_cex_num::PrototypeScalar::from(Some(2))
    );
    assert_eq!(stats["42"].had_handlers, Some(false));
    let seen = Rc::new(RefCell::new(vec![]));
    let out = seen.clone();
    let cb: MessageHandler = Rc::new(move |v| {
        out.borrow_mut().push(v.clone());
        Ok(())
    });
    let handlers = Rc::new(RefCell::new(vec![cb.clone(), cb.clone()]));
    let mut map = MessageHandlers::new();
    map.insert("other".into(), handlers.clone());
    map.insert("42".into(), Rc::new(RefCell::new(vec![])));
    assert!(Rc::ptr_eq(&map["other"], &handlers));
    assert!(
        Rc::ptr_eq(&map["other"].borrow()[0], &cb),
        "ROOT handlers native callback identity"
    );
    assert_eq!(
        map.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["other", "42"],
        "ROOT handlers native insertion order"
    );
    for cb in map["other"].borrow().iter() {
        cb(&json!({"price":17})).unwrap();
    }
    assert_eq!(
        *seen.borrow(),
        vec![json!({"price":17}), json!({"price":17})]
    );
    let source = oracle("types");
    assert_eq!(source["statFields"], 3);
    assert_eq!(source["optionalFields"], 2);
    assert_eq!(source["aliases"], 2);
}
