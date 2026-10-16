#[path = "support/cache_host.rs"]
mod support;
use safe_cex_venue_hyperliquid::{
    cache_runtime::*, cross_cache::CrossCacheHost, json_boundaries::JsPayloadHost,
};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};
use support::*;
fn oracle() -> &'static Vec<Value> {
    static DATA: OnceLock<Vec<Value>> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/cache_runtime_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(rows.len(), 27, "independent source case floor");
        rows
    })
}
fn setup(row: &Value) -> (Host, CacheModule<N>, N) {
    let input = &row["input"];
    let mut h = Host {
        broadcast: input["absent"] != true,
        fail_broadcast: input["failBroadcast"] == true,
        fail_handler: input["failHandler"] == true,
        ..Host::default()
    };
    if input["storage"] == true {
        h.storage = object(vec![
            ("removeItem", N::Builtin("removeItem".into())),
            ("getItem", N::Builtin("getItem".into())),
            ("setItem", N::Builtin("setItem".into())),
        ]);
    }
    if let Some(t) = input["ticks"].as_array() {
        h.ticks = t.iter().map(|t| t.as_f64().unwrap()).collect();
    }
    let module = new_module(&mut h).unwrap();
    let c = new_cache(&mut h, &module).unwrap().value;
    if let Some(pre) = input["pre"].as_array() {
        let map = h.get(&c, "workerPrePlacementCache").unwrap();
        h.invoke(
            &map,
            "set",
            &[s("X|buy"), arr(pre.iter().map(Host::decode).collect())],
        )
        .unwrap();
    }
    (h, module, c)
}
fn compare(h: &mut Host, c: &N, result: &N, row: &Value) {
    let name = row["name"].as_str().unwrap();
    assert_native(result, &row["result"], &format!("CACHE result {name}"));
    for (field, key) in [
        ("workerPendingOrdersCache", "pending"),
        ("workerPrePlacementCache", "pre"),
        ("workerSeenFillsSet", "seen"),
    ] {
        let value = h.get(c, field).unwrap();
        assert_native(&value, &row[key], &format!("CACHE {key} {name}"));
    }
    assert_eq!(
        h.messages.len(),
        row["messages"].as_array().unwrap().len(),
        "CACHE broadcast count {name}"
    );
    for (a, e) in h.messages.iter().zip(row["messages"].as_array().unwrap()) {
        assert_native(a, e, &format!("CACHE broadcast {name}"));
    }
    assert_eq!(
        h.reads,
        row["reads"].as_u64().unwrap() as usize,
        "CACHE global reads {name}"
    );
}
#[test]
fn class_descriptors() {
    let row = oracle().iter().find(|r| r["family"] == "class").unwrap();
    let mut h = Host::default();
    let m = new_module(&mut h).unwrap();
    let other = new_module(&mut h).unwrap();
    assert!(
        !eq(&m.constructor, &other.constructor),
        "CACHE distinct class constructors"
    );
    let N::O(p) = &m.prototype else {
        panic!("CACHE class prototype")
    };
    let p = p.borrow();
    assert_eq!(p.fields.len(), 19, "CACHE class independent method floor");
    assert!(p.fields.iter().all(|p| !p.2), "CACHE class nonenumerable");
    assert!(
        eq(&h.get(&m.prototype, "constructor").unwrap(), &m.constructor),
        "CACHE class constructor identity"
    );
    assert_eq!(
        p.fields.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        row["names"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "CACHE class names"
    );
    let a = new_cache(&mut h, &m).unwrap().value;
    let b = new_cache(&mut h, &m).unwrap().value;
    assert!(!eq(&a, &b));
    assert!(
        eq(
            &h.get(&a, "addPrePlacement").unwrap(),
            &h.get(&b, "addPrePlacement").unwrap()
        ),
        "CACHE shared methods"
    );
}
#[test]
fn constructor_order() {
    let rows = oracle()
        .iter()
        .filter(|r| r["family"] == "constructor")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 4);
    for row in rows {
        let (mut h, _, c) = setup(row);
        compare(&mut h, &c, &N::U, row);
        assert_eq!(
            &h.events[..3],
            ["map", "map", "set"],
            "CACHE constructor fields before init"
        );
        assert_eq!(
            &h.events[3..],
            row["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "CACHE constructor initialization before cleanup"
        );
    }
}
#[test]
fn broadcast_receives() {
    let rows = oracle()
        .iter()
        .filter(|r| r["family"] == "receive")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 9);
    for row in rows {
        let (mut h, _, c) = setup(row);
        let channel = h.get(&c, "orderCacheBroadcast").unwrap();
        let handler = h.get(&channel, "onmessage").unwrap();
        let input = &row["input"];
        let n = if input["evict"] == true { 501 } else { 1 };
        for i in 0..n {
            let (action, key, data) = if n == 501 {
                (s("seenFill"), s(&format!("f{i}")), N::U)
            } else {
                (
                    Host::decode(&input["action"]),
                    Host::decode(&input["key"]),
                    Host::decode(&input["data"]),
                )
            };
            let event = object(vec![(
                "data",
                object(vec![
                    ("action", action),
                    ("key", key),
                    ("data", data.clone()),
                ]),
            )]);
            h.call(&handler, &channel, &[event]).unwrap();
            if input["action"] == "addPrePlacement" && matches!(data, N::A(_)) {
                let map = h.get(&c, "workerPrePlacementCache").unwrap();
                let got = h.invoke(&map, "get", &[s("X|buy")]).unwrap();
                assert!(eq(&got, &data), "CACHE receive array identity");
            }
        }
        compare(&mut h, &c, &N::U, row);
        assert!(
            h.call(&handler, &channel, &[object(vec![("data", N::Null)])])
                .is_err(),
            "CACHE receive exceptions outside catch"
        );
    }
}
#[test]
fn add_boundaries() {
    let rows = oracle()
        .iter()
        .filter(|r| r["family"] == "add")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    for row in rows {
        let (mut h, _, c) = setup(row);
        let args = row["input"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(Host::decode)
            .collect::<Vec<_>>();
        let result = h.invoke(&c, "addPrePlacement", &args).unwrap();
        compare(&mut h, &c, &result, row);
    }
}
#[test]
fn match_priorities() {
    let rows = oracle()
        .iter()
        .filter(|r| r["family"] == "match")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 9);
    for row in rows {
        let (mut h, _, c) = setup(row);
        let args = row["input"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(Host::decode)
            .collect::<Vec<_>>();
        let result = h.invoke(&c, "matchPrePlacement", &args).unwrap();
        compare(&mut h, &c, &result, row);
    }
}
fn module() -> (Host, CacheModule<N>, N) {
    let mut h = Host::default();
    let m = new_module(&mut h).unwrap();
    let c = get_cross_cache(&mut h, &m).unwrap();
    (h, m, c)
}
#[test]
fn singleton_identity_and_retry() {
    let (mut h, m, c) = module();
    assert!(
        eq(&c, &get_cross_cache(&mut h, &m).unwrap()),
        "CACHE singleton reused"
    );
    h.invoke(&c, "dispose", &[]).unwrap();
    assert!(
        eq(&c, &get_cross_cache(&mut h, &m).unwrap()),
        "CACHE singleton retained after dispose"
    );
    let mut h = Host {
        fail_map: true,
        ..Host::default()
    };
    let m = new_module(&mut h).unwrap();
    assert!(get_cross_cache(&mut h, &m).is_err());
    h.fail_map = false;
    let c = get_cross_cache(&mut h, &m).unwrap();
    assert!(
        matches!(h.get(&c, "workerPendingOrdersCache").unwrap(), N::Map(_)),
        "CACHE retry pending map"
    );
    assert!(
        matches!(h.get(&c, "workerPrePlacementCache").unwrap(), N::Map(_)),
        "CACHE retry pre map"
    );
    assert!(
        matches!(h.get(&c, "workerSeenFillsSet").unwrap(), N::Set(_)),
        "CACHE retry seen set"
    );
    assert_eq!(
        h.events.iter().filter(|e| *e == "map").count(),
        3,
        "CACHE retry reconstructed maps"
    );
    assert!(
        eq(&c, &get_cross_cache(&mut h, &m).unwrap()),
        "CACHE failed construction retry"
    );
}
#[test]
fn module_has_native_value() {
    let row = oracle().iter().find(|r| r["family"] == "module").unwrap();
    let (mut h, m, c) = module();
    let id = object(vec![]);
    let before = has_seen_fill(&mut h, &m, &id).unwrap();
    assert_native(&before, &row["before"], "CACHE module has unseen");
    h.invoke(&c, "markFillSeen", &[id.clone()]).unwrap();
    let after = has_seen_fill(&mut h, &m, &id).unwrap();
    assert_native(
        &after,
        &row["after"],
        "CACHE module has seen opaque identity",
    );
    assert!(
        matches!(
            has_seen_fill(&mut h, &m, &object(vec![])).unwrap(),
            N::B(false)
        ),
        "CACHE module no string coercion"
    );
}
#[test]
fn module_mark_native_effect() {
    let (mut h, m, c) = module();
    let id = object(vec![]);
    mark_fill_seen(&mut h, &m, &id).unwrap();
    let seen = h.get(&c, "workerSeenFillsSet").unwrap();
    let N::Set(seen) = seen else {
        panic!("CACHE mark set")
    };
    assert_eq!(seen.borrow().len(), 1, "CACHE module mark inserts");
    assert!(
        eq(&seen.borrow()[0], &id),
        "CACHE module mark retains argument"
    );
    assert_eq!(h.messages.len(), 1, "CACHE module mark broadcasts");
    let payload = h.messages[0].clone();
    assert!(
        eq(&h.get(&payload, "action").unwrap(), &s("seenFill")),
        "CACHE mark action"
    );
    assert!(
        eq(&h.get(&payload, "key").unwrap(), &id),
        "CACHE mark broadcast key identity"
    );
    assert!(
        matches!(h.get(&payload, "data").unwrap(), N::U),
        "CACHE mark broadcast undefined data"
    );
    let N::O(payload) = payload else {
        panic!("CACHE mark payload")
    };
    assert_eq!(
        payload
            .borrow()
            .fields
            .iter()
            .map(|p| p.0.clone())
            .collect::<Vec<_>>(),
        ["action", "key", "data"],
        "CACHE mark own payload keys"
    );
}
