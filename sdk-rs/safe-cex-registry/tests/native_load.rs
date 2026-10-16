use safe_cex_registry::registry::load;
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("REGISTRY native load unexpectedly pending"),
    }
}
#[test]
fn native_load_is_lazy_reusable_and_invokes_before_returning_future() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/native_load.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "REGISTRY native source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let events = RefCell::new(Vec::new());
    let constructor = Rc::new("native constructor identity");
    let wrapped = load(|| {
        events.borrow_mut().push("loader");
        constructor.clone()
    });
    assert!(
        events.borrow().is_empty(),
        "REGISTRY native lazy construction"
    );
    events.borrow_mut().push("created");
    let future = wrapped();
    assert_eq!(
        *events.borrow(),
        ["created", "loader"],
        "REGISTRY native callback before future return"
    );
    events.borrow_mut().push("returned");
    let first = ready(future);
    assert!(
        Rc::ptr_eq(&first, &constructor),
        "REGISTRY native first constructor identity"
    );
    events.borrow_mut().push("settled");
    let second = ready(wrapped());
    assert!(
        Rc::ptr_eq(&second, &constructor),
        "REGISTRY native second constructor identity"
    );
    assert_eq!(
        *events.borrow(),
        ["created", "loader", "returned", "settled", "loader"],
        "REGISTRY native repeated invocation order"
    );
    assert_eq!(
        source["events"],
        serde_json::json!(["created", "loader", "returned", "settled", "loader"]),
        "REGISTRY actual source invocation order"
    );
    assert_eq!(
        source["firstIdentity"], true,
        "REGISTRY source first constructor identity"
    );
    assert_eq!(
        source["secondIdentity"], true,
        "REGISTRY source second constructor identity"
    );
    assert_eq!(
        source["settledBeforeAwait"], false,
        "REGISTRY chosen JS microtask scheduling"
    );
    assert_eq!(
        source["caughtSame"], true,
        "REGISTRY source synchronous exception identity"
    );
    assert_eq!(
        source["returned"], false,
        "REGISTRY source exception before Promise return"
    );
    let throwing = load(|| panic!("native loader refusal"));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(throwing)).is_err(),
        "REGISTRY native panic before future return"
    );
}

#[test]
fn heterogeneous_loader_closures_share_one_registry_map_type() {
    let constructor = Rc::new("constructor");
    let first = constructor.clone();
    let second = constructor.clone();
    let loaders = std::collections::BTreeMap::from([
        ("first", load(move || first.clone())),
        (
            "second",
            load(move || {
                let copied = second.clone();
                copied
            }),
        ),
    ]);
    assert_eq!(loaders.len(), 2, "REGISTRY uniform boxed loader count");
    for name in ["first", "second"] {
        assert!(
            Rc::ptr_eq(&ready(loaders[name]()), &constructor),
            "REGISTRY uniform constructor identity"
        );
    }
}
