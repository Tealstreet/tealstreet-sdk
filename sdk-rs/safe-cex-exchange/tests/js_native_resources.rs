#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, to_json},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{cell::Cell, process::Command, rc::Rc, sync::Arc};

struct Resource {
    value: Cell<u64>,
    dropped: Rc<Cell<usize>>,
}
impl Drop for Resource {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
    }
}
fn scope() -> Rc<ScopeShared<()>> {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    shared
}

#[test]
fn native_resource_retains_exact_owner_until_the_last_alias_and_lookup_release() {
    let dropped = Rc::new(Cell::new(0));
    let resource = Rc::new(Resource {
        value: Cell::new(0x7ff8_0000_0000_0042),
        dropped: dropped.clone(),
    });
    let weak = Rc::downgrade(&resource);
    let object = Js::host_object(resource.clone());
    let alias = object.clone();
    let handle = object
        .resource::<Resource>()
        .expect("NATIVE_RESOURCE exact owner");
    assert!(
        Rc::ptr_eq(&handle, &resource),
        "NATIVE_RESOURCE original Rc"
    );
    handle.value.set((-0.0_f64).to_bits());
    assert_eq!(resource.value.get(), (-0.0_f64).to_bits());
    handle.value.set(0x7ff8_0000_0000_0042);
    assert_eq!(
        alias.resource::<Resource>().unwrap().value.get(),
        0x7ff8_0000_0000_0042
    );
    assert!(
        object.resource::<Cell<u64>>().is_none(),
        "NATIVE_RESOURCE exact native type"
    );
    drop(handle);
    drop(resource);
    drop(object);
    assert!(
        weak.upgrade().is_some(),
        "NATIVE_RESOURCE alias retains payload"
    );
    let lookup = alias.resource::<Resource>().unwrap();
    drop(alias);
    assert_eq!(dropped.get(), 0, "NATIVE_RESOURCE lookup retains payload");
    drop(lookup);
    assert!(
        weak.upgrade().is_none(),
        "NATIVE_RESOURCE releases final native owner"
    );
    assert_eq!(dropped.get(), 1);
}

#[test]
fn native_resource_is_private_and_ordinary_object_operations_match_node() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_native_resources_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let shared = scope();
    let object_ctor = shared.globals.get("Object").unwrap();
    let resource = Rc::new(Cell::new(7));
    let object = Js::host_object(resource.clone());
    object.set("label", Js::str("visible")).unwrap();
    let prototype = Js::object(vec![]);
    let getter = Js::function("get inherited", |this, _| this.get("label"));
    object_ctor
        .call_method(
            "defineProperty",
            vec![
                prototype.clone(),
                Js::str("inherited"),
                Js::object(vec![("get", getter)]),
            ],
        )
        .unwrap();
    let Js::Object(data) = &object else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(prototype);
    let child = Js::object(vec![("label", Js::str("child"))]);
    let Js::Object(data) = &child else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(object.clone());
    assert!(
        child.resource::<Cell<i32>>().is_none(),
        "NATIVE_RESOURCE not inherited"
    );
    let spread = object.spread_object().unwrap();
    assert!(
        spread.resource::<Cell<i32>>().is_none(),
        "NATIVE_RESOURCE not copied as a property"
    );
    object_ctor
        .call_method("freeze", vec![object.clone()])
        .unwrap();
    let write_error = object.set("label", Js::str("changed")).unwrap_err();
    object.resource::<Cell<i32>>().unwrap().set(8);
    let keys = object_ctor
        .call_method("keys", vec![object.clone()])
        .unwrap();
    let names = object_ctor
        .call_method("getOwnPropertyNames", vec![object.clone()])
        .unwrap();
    let json = shared
        .globals
        .get("JSON")
        .unwrap()
        .call_method("stringify", vec![object.clone()])
        .unwrap();
    let actual = json!({
        "keys":to_json(&keys).unwrap(), "names":to_json(&names).unwrap(),
        "inherited":to_json(&object.get("inherited").unwrap()).unwrap(),
        "child":to_json(&child.get("inherited").unwrap()).unwrap(),
        "json":json.to_display(), "privateValue":resource.get(),
        "frozen":object_ctor.call_method("isFrozen",vec![object.clone()]).unwrap().truthy(),
        "writeError":write_error.value.get("name").unwrap().to_display(),
    });
    assert_eq!(
        actual, expected,
        "NATIVE_RESOURCE ordinary reference semantics"
    );
    assert_eq!(object.own_property_names().unwrap().len(), 1);
    assert!(Js::Null.resource::<Cell<i32>>().is_none());
    assert!(Js::object(vec![]).resource::<Cell<i32>>().is_none());
    assert!(Js::array(vec![]).resource::<Cell<i32>>().is_none());
}
