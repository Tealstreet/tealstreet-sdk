#[path = "support/inverse_socket_realm.rs"]
mod realm;
use realm::*;
use safe_cex_store::{StoreMutationError, store::DefaultStore};
use safe_cex_types::{Order, StoreSubscriberError};
use safe_cex_venue_bybit::{hyro_private_session::SessionHost, inverse_socket_bodies as api};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::Arc,
};
fn base(h: &Realm) -> V {
    let owner = obj(vec![
        ("isDisposed", b(false)),
        ("authRetries", n(0.0)),
        ("parent", obj(vec![])),
        ("messageHandlers", obj(vec![])),
    ]);
    for (name, tag) in [
        ("trackLastReceived", "track"),
        ("emitWsDiagnosticReceive", "diagnostic"),
        ("recordMessageHandlerStats", "stats"),
        ("handlePongEvent", "pong"),
        ("handleOrderTopic", "order"),
        ("handlePositionTopic", "position"),
        ("emitAndLogError", "error"),
        ("sendReconnectSequence", "reconnect"),
    ] {
        put(
            &owner,
            name,
            fun(move |h, _, a| {
                if tag == "stats" || tag == "error" {
                    h.log(&format!("{tag}:{}", label(&a[0])));
                } else {
                    h.log(tag);
                }
                Ok(undefined())
            }),
        );
    }
    put(
        &owner,
        "directFallback",
        obj(vec![(
            "onMessage",
            fun(|h, _, _| {
                h.log("fallback");
                Ok(undefined())
            }),
        )]),
    );
    *h.parsed.borrow_mut() = null();
    owner
}
fn mapped(id: &str) -> V {
    obj(vec![
        ("id", t(id)),
        ("side", t("buy")),
        ("symbol", t("BTCUSD")),
        ("status", t("open")),
    ])
}
fn order(status: &str) -> V {
    obj(vec![
        ("orderStatus", t(status)),
        ("price", t("12")),
        ("qty", t("10")),
        ("cumExecValue", t("0.25")),
        (
            "execution",
            obj(vec![("amount", n(5.0)), ("price", n(19.0))]),
        ),
    ])
}
fn order_host(h: &Realm, owner: &V, orders: V, fill: V) {
    let parent = h.field(owner, "parent");
    put(
        &parent,
        "mapOrder",
        fun(move |h, _, _| {
            h.log("map-order");
            Ok(orders.clone())
        }),
    );
    put(
        &parent,
        "markOrderSeen",
        fun(|h, _, a| {
            h.log(&format!("seen:{}", label(&a[0])));
            Ok(undefined())
        }),
    );
    put(
        owner,
        "incrementalFills",
        fun(move |h, _, a| {
            h.log("incremental");
            put(&h.global_value(), "fillArgs", arr(a));
            Ok(fill.clone())
        }),
    );
    put(
        owner,
        "emitter",
        obj(vec![(
            "emit",
            fun(|h, _, a| {
                h.log("fill");
                put(&h.global_value(), "fill", a[1].clone());
                Ok(undefined())
            }),
        )]),
    );
    put(
        owner,
        "store",
        obj(vec![(
            "addOrUpdateOrders",
            fun(|h, _, a| {
                h.log("store-orders");
                put(&h.global_value(), "stored", a[0].clone());
                Ok(undefined())
            }),
        )]),
    );
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-socket-bodies.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source inverse socket bodies: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn factory_retains_fresh_callable_state() {
    let h = Realm::new();
    let a = api::incremental_fills(&h).unwrap();
    let b = api::incremental_fills(&h).unwrap();
    assert!(
        !Rc::ptr_eq(&a, &b),
        "factory returns distinct retained functions"
    );
    let created = rows(&h.field(&h.global_value(), "createdFunctions"));
    assert_eq!(
        created.len(),
        2,
        "initializer invokes factory exactly once per field"
    );
    assert!(
        Rc::ptr_eq(&a, &created[0]) && Rc::ptr_eq(&b, &created[1]),
        "initializer returns exact imported callable roots"
    );
    assert_eq!(num(&h.field(&a, "tag")), 1.0);
    assert_eq!(num(&h.field(&b, "tag")), 2.0);
    let first = h
        .receiver_call(undefined(), a.clone(), vec![t("a"), n(4.0), undefined()])
        .unwrap();
    let second = h
        .receiver_call(undefined(), a, vec![t("a"), n(7.0), undefined()])
        .unwrap();
    let separate = h
        .receiver_call(undefined(), b, vec![t("a"), n(7.0), undefined()])
        .unwrap();
    assert_eq!(
        num(&h.field(&first, "qty")),
        4.0,
        "factory native first delta"
    );
    assert_eq!(
        num(&h.field(&second, "qty")),
        3.0,
        "factory native retained delta"
    );
    assert_eq!(
        num(&h.field(&separate, "qty")),
        7.0,
        "factory native independent delta"
    );
}
#[test]
fn private_dispatch_rereads_frame_and_starts_retry_eagerly() {
    let h = Realm::new();
    let owner = base(&h);
    let frame = obj(vec![
        ("topic", t("order")),
        ("data", arr(vec![])),
        ("op", t("pong")),
        ("success", b(false)),
        ("ret_msg", t("denied")),
    ]);
    *h.parsed.borrow_mut() = frame.clone();
    let f = frame.clone();
    put(
        &owner,
        "handleOrderTopic",
        fun(move |h, _, _| {
            h.log("order");
            put(&f, "topic", t("position"));
            Ok(undefined())
        }),
    );
    api::private_on_message(&h, &owner, obj(vec![("data", t("wire"))])).unwrap();
    assert_eq!(
        *h.trace.borrow(),
        [
            "track",
            "fallback",
            "diagnostic",
            "parse",
            "stats:order",
            "order",
            "stats:position",
            "position",
            "pong",
            "error:[WS]pong:denied"
        ],
        "private native dispatch rereads"
    );
    h.trace.borrow_mut().clear();
    put(&frame, "op", t("auth"));
    put(&frame, "topic", t("other"));
    put(&owner, "authRetries", n(3.0));
    let keep = owner.clone();
    put(
        &owner,
        "sendReconnectSequence",
        fun(move |h, this, a| {
            assert!(Rc::ptr_eq(&this, &keep));
            assert!(h.equal(&a[0], &b(true)));
            assert_eq!(
                num(&h.field(&keep, "authRetries")),
                4.0,
                "retry increment before eager start"
            );
            h.log("reconnect");
            Ok(obj(vec![]))
        }),
    );
    api::private_on_message(&h, &owner, obj(vec![("data", t("wire"))])).unwrap();
    assert_eq!(
        *h.trace.borrow(),
        [
            "track",
            "fallback",
            "diagnostic",
            "parse",
            "error:[WS]auth:denied",
            "error:[WS] Syncing time with BybitInverse...",
            "reconnect"
        ],
        "private native retry threshold"
    );
    h.trace.borrow_mut().clear();
    api::private_on_message(&h, &owner, obj(vec![("data", t("wire"))])).unwrap();
    assert!(
        !h.trace.borrow().contains(&"reconnect".to_owned()),
        "private retry stops after four"
    );
}
#[test]
fn destructuring_precedes_disposal_and_errors_stop_dispatch() {
    let h = Realm::new();
    let owner = base(&h);
    put(&owner, "isDisposed", b(true));
    assert!(
        api::private_on_message(&h, &owner, null()).is_err(),
        "private destructure before guard"
    );
    assert!(
        api::public_on_message(&h, &owner, null()).is_err(),
        "public destructure before guard"
    );
    let event = obj(vec![("data", t("wire"))]);
    api::private_on_message(&h, &owner, event.clone()).unwrap();
    api::public_on_message(&h, &owner, event).unwrap();
    assert!(h.trace.borrow().is_empty());
    put(&owner, "isDisposed", b(false));
    let original = obj(vec![]);
    let e = original.clone();
    put(
        &owner,
        "emitWsDiagnosticReceive",
        fun(move |_, _, _| Err(e.clone())),
    );
    let actual = api::private_on_message(&h, &owner, obj(vec![("data", t("wire"))])).unwrap_err();
    assert!(
        Rc::ptr_eq(&actual, &original),
        "private original error root"
    );
    assert_eq!(
        *h.trace.borrow(),
        ["track", "fallback"],
        "diagnostic failure prevents parse"
    );
}
#[test]
fn order_guard_fills_statuses_and_outer_iteration_have_native_values() {
    let h = Realm::new();
    let owner = base(&h);
    let first = mapped("one");
    let second = mapped("two");
    let mapped = arr(vec![first.clone(), second.clone()]);
    // qty / value is 2.9999999999999996 in binary; the emitted price is cleaned to 3.
    order_host(
        &h,
        &owner,
        mapped.clone(),
        obj(vec![("qty", n(0.3)), ("value", n(0.1))]),
    );
    let raw = order("PartiallyFilled");
    put(&raw, "price", t("0"));
    put(&raw, "qty", t("-0"));
    api::handle_order_topic(&h, &owner, arr(vec![raw])).unwrap();
    assert_eq!(
        *h.trace.borrow(),
        ["map-order", "seen:one", "seen:two"],
        "order seen before replace guard"
    );
    h.trace.borrow_mut().clear();
    let raw = order("Filled");
    let batch = arr(vec![raw.clone()]);
    let batch_capture = batch.clone();
    let emitter = h.field(&owner, "emitter");
    let emitter_old = h.field(&emitter, "emit");
    put(
        &emitter,
        "emit",
        fun(move |h, this, a| {
            push(&batch_capture, order("New"));
            h.receiver_call(this, emitter_old.clone(), a)
        }),
    );
    api::handle_order_topic(&h, &owner, batch).unwrap();
    assert_eq!(
        *h.trace.borrow(),
        [
            "map-order",
            "seen:one",
            "seen:two",
            "execution",
            "incremental",
            "fill",
            "store-orders"
        ],
        "order fill precedes store and forEach captures length"
    );
    let fill = h.field(&h.global_value(), "fill");
    assert_eq!(
        num(&h.field(&fill, "price")).to_bits(),
        3.0_f64.to_bits(),
        "order native inverse execution price is cleaned"
    );
    assert_eq!(
        num(&h.field(&fill, "amount")),
        0.3,
        "order native execution delta"
    );
    assert_eq!(label(&h.field(&fill, "id")), "one");
    assert_eq!(
        label(&h.field(&first, "status")),
        "closed",
        "order native in-place terminal status"
    );
    assert_eq!(label(&h.field(&second, "status")), "closed");
    assert!(
        Rc::ptr_eq(&h.field(&h.global_value(), "stored"), &mapped),
        "store receives exact mapped array"
    );
    let args = rows(&h.field(&h.global_value(), "fillArgs"));
    assert_eq!(
        num(&args[1]),
        5.0,
        "execution amount not raw order quantity"
    );
    assert_eq!(num(&args[2]), 0.25);
}
#[test]
fn order_live_aliases_status_rereads_and_nonfinite_cumulative_value() {
    let h = Realm::new();
    let owner = base(&h);
    let one = mapped("one");
    let two = mapped("two");
    let mapped = arr(vec![one.clone()]);
    order_host(
        &h,
        &owner,
        mapped.clone(),
        obj(vec![("qty", n(2.0)), ("value", n(-0.0))]),
    );
    let append = mapped.clone();
    let parent = h.field(&owner, "parent");
    put(
        &parent,
        "markOrderSeen",
        fun(move |h, _, a| {
            h.log(&format!("seen:{}", label(&a[0])));
            if label(&a[0]) == "one" {
                push(&append, two.clone());
            }
            Ok(undefined())
        }),
    );
    let raw = order("PartiallyFilled");
    put(&raw, "cumExecValue", t("Infinity"));
    let raw_capture = raw.clone();
    let emitter = h.field(&owner, "emitter");
    let old = h.field(&emitter, "emit");
    let one_capture = one.clone();
    put(
        &emitter,
        "emit",
        fun(move |h, this, a| {
            put(&raw_capture, "orderStatus", t("Rejected"));
            put(&raw_capture, "rejectReason", t("EC_NoError"));
            put(&one_capture, "side", t("sell"));
            h.receiver_call(this, old.clone(), a)
        }),
    );
    api::handle_order_topic(&h, &owner, arr(vec![raw])).unwrap();
    assert_eq!(
        *h.trace.borrow(),
        [
            "map-order",
            "seen:one",
            "seen:two",
            "execution",
            "incremental",
            "fill",
            "store-orders"
        ],
        "for-of sees append and status rereads see emitter mutation"
    );
    assert_eq!(
        label(&h.field(&one, "status")),
        "canceled",
        "terminal status reread after fill"
    );
    let args = rows(&h.field(&h.global_value(), "fillArgs"));
    assert!(
        is_undefined(&args[2]),
        "nonfinite cumulative value becomes undefined"
    );
    let fill = h.field(&h.global_value(), "fill");
    assert_eq!(
        num(&h.field(&fill, "price")),
        19.0,
        "falsey inverse value uses execution price"
    );
    assert_eq!(
        label(&h.field(&fill, "side")),
        "buy",
        "fill captured side before emitter mutation"
    );
}
#[derive(Debug)]
struct Cause(V);
impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "subscriber origin")
    }
}
impl std::error::Error for Cause {}
fn typed_order(h: &Realm, row: &V) -> Order {
    serde_json::from_value(json!({"id":label(&h.field(row,"id")),"status":label(&h.field(row,"status")),"symbol":"BTCUSD","type":"limit","side":"buy","price":20,"amount":5,"filled":5,"remaining":0,"reduceOnly":false,"lastUpdate":1})).unwrap()
}
#[test]
fn real_store_failure_commits_orders_retains_cause_and_stops_batch() {
    let h = Realm::new();
    let owner = base(&h);
    let mapped = arr(vec![mapped("one")]);
    order_host(
        &h,
        &owner,
        mapped.clone(),
        obj(vec![("qty", n(5.0)), ("value", n(0.25))]),
    );
    let store = Rc::new(DefaultStore::new());
    let original = obj(vec![("name", t("SubscriberError"))]);
    let seen = Rc::new(Cell::new(0));
    let late = Rc::new(Cell::new(0));
    let seen_capture = seen.clone();
    let cause = Rc::new(Cause(original.clone()));
    let _first = store.subscribe(Arc::from(Box::new(
        move |_: &safe_cex_types::StoreData, _: bool| {
            seen_capture.set(seen_capture.get() + 1);
            Err(StoreSubscriberError::new("subscriber origin").with_cause(cause.clone()))
        },
    ) as safe_cex_types::StoreSubscriber));
    let late_capture = late.clone();
    let _later = store.subscribe(Arc::from(Box::new(
        move |_: &safe_cex_types::StoreData, _: bool| {
            late_capture.set(late_capture.get() + 1);
            Ok(())
        },
    ) as safe_cex_types::StoreSubscriber));
    let held = store.clone();
    let raw_original = original.clone();
    let bridge = h.field(&owner, "store");
    put(
        &bridge,
        "addOrUpdateOrders",
        fun(move |h, _, a| {
            h.log("store-orders");
            let incoming = rows(&a[0]).iter().map(|row| typed_order(h, row)).collect();
            let failure = held.add_or_update_orders(incoming).unwrap_err();
            let StoreMutationError::Notification(failure) = failure else {
                panic!("native Store notification failure")
            };
            assert_eq!(failure.subscriber_id(), 0);
            let origin = std::error::Error::source(failure.cause())
                .unwrap()
                .downcast_ref::<Cause>()
                .unwrap();
            assert!(
                Rc::ptr_eq(&origin.0, &raw_original),
                "Store retains original subscriber cause"
            );
            Err(origin.0.clone())
        }),
    );
    let result = api::handle_order_topic(&h, &owner, arr(vec![order("Filled"), order("Filled")]))
        .unwrap_err();
    assert!(
        Rc::ptr_eq(&result, &original),
        "order original Store throw identity"
    );
    assert_eq!(
        store.orders().len(),
        1,
        "order Store commit survives subscriber throw"
    );
    assert_eq!(
        store.orders()[0].status,
        safe_cex_types::OrderStatus::Closed,
        "native committed Store status"
    );
    assert_eq!(seen.get(), 1);
    assert_eq!(late.get(), 0, "later subscriber skipped");
    assert_eq!(label(&h.field(&rows(&mapped)[0], "status")), "closed");
    assert_eq!(
        *h.trace.borrow(),
        [
            "map-order",
            "seen:one",
            "execution",
            "incremental",
            "fill",
            "store-orders"
        ],
        "Store throw preserves earlier fill and prevents later row"
    );
}
fn position_host(h: &Realm, owner: &V) -> V {
    let parent = h.field(owner, "parent");
    put(&parent, "lastWebsocketPositionsUpdate", n(7.0));
    put(
        &parent,
        "mapPosition",
        fun(|h, _, a| {
            h.log("map-position");
            Ok(a[0].clone())
        }),
    );
    put(
        owner,
        "store",
        obj(vec![(
            "updatePositions",
            fun(|h, _, a| {
                h.log("store-positions");
                put(&h.global_value(), "updates", a[0].clone());
                Ok(undefined())
            }),
        )]),
    );
    parent
}
#[test]
fn positions_retain_pair_identity_and_timestamp_assignment_receiver() {
    let h = Realm::new();
    let owner = base(&h);
    let parent = position_host(&h, &owner);
    let first = obj(vec![("id", t("p1"))]);
    let second = obj(vec![("id", t("p2"))]);
    let replacement = obj(vec![("lastWebsocketPositionsUpdate", n(9.0))]);
    let owner_capture = owner.clone();
    let replacement_capture = replacement.clone();
    put(
        &h.global_value(),
        "virtualClock",
        obj(vec![(
            "getCurrentTimeMs",
            fun(move |h, _, _| {
                h.log("clock");
                put(&owner_capture, "parent", replacement_capture.clone());
                Ok(n(-0.0))
            }),
        )]),
    );
    api::handle_position_topic(&h, &owner, arr(vec![first.clone(), second.clone()])).unwrap();
    let updates = rows(&h.field(&h.global_value(), "updates"));
    assert_eq!(updates.len(), 2, "independent position pair floor");
    for (i, p) in [first, second].iter().enumerate() {
        let pair = rows(&updates[i]);
        assert_eq!(pair.len(), 2);
        assert!(
            Rc::ptr_eq(&pair[0], p) && Rc::ptr_eq(&pair[1], p),
            "position native pair aliases"
        );
    }
    assert_eq!(
        num(&h.field(&parent, "lastWebsocketPositionsUpdate")).to_bits(),
        (-0.0_f64).to_bits(),
        "position timestamp writes captured parent and retains negative zero"
    );
    assert_eq!(
        num(&h.field(&replacement, "lastWebsocketPositionsUpdate")),
        9.0
    );
    assert_eq!(
        *h.trace.borrow(),
        ["map-position", "map-position", "store-positions", "clock"]
    );
}
#[test]
fn position_store_throw_prevents_clock_and_timestamp() {
    let h = Realm::new();
    let owner = base(&h);
    let parent = position_host(&h, &owner);
    let original = obj(vec![]);
    let e = original.clone();
    put(
        &h.field(&owner, "store"),
        "updatePositions",
        fun(move |h, _, _| {
            h.log("store-positions");
            Err(e.clone())
        }),
    );
    let actual = api::handle_position_topic(&h, &owner, arr(vec![obj(vec![])])).unwrap_err();
    assert!(
        Rc::ptr_eq(&actual, &original),
        "position original Store error"
    );
    assert_eq!(
        num(&h.field(&parent, "lastWebsocketPositionsUpdate")),
        7.0,
        "position failed Store prevents timestamp"
    );
    assert_eq!(
        *h.trace.borrow(),
        ["map-position", "store-positions"],
        "position failed Store prevents clock"
    );
}
#[test]
fn public_dispatch_retains_arrays_utf16_and_live_iteration() {
    let h = Realm::new();
    let owner = base(&h);
    let frame = obj(vec![]);
    *h.parsed.borrow_mut() = frame.clone();
    let handlers = h.field(&owner, "messageHandlers");
    let exact = arr(vec![]);
    let prefix = arr(vec![]);
    let expected_frame = frame.clone();
    let prefix_capture = prefix.clone();
    let third = fun(move |h, this, a| {
        assert!(Rc::ptr_eq(&this, &prefix_capture));
        assert!(Rc::ptr_eq(&a[0], &expected_frame));
        h.log("prefix-appended");
        Ok(undefined())
    });
    let exact_capture = exact.clone();
    let prefix_capture = prefix.clone();
    let first = fun(move |h, this, a| {
        assert!(
            Rc::ptr_eq(&this, &exact_capture),
            "indexed handler array receiver"
        );
        put(&a[0], "touched", b(true));
        h.log("exact");
        push(&prefix_capture, third.clone());
        Ok(undefined())
    });
    push(&exact, first);
    let expected = prefix.clone();
    push(
        &prefix,
        fun(move |h, this, a| {
            assert!(Rc::ptr_eq(&this, &expected));
            assert!(h.equal(&h.field(&a[0], "touched"), &b(true)));
            h.log("prefix");
            Ok(undefined())
        }),
    );
    let mut key: Vec<u16> = "tickers.BTC".encode_utf16().collect();
    key.push(0xd800);
    put_key(&handlers, key.clone(), exact.clone());
    let proto = obj(vec![("tickers", prefix.clone())]);
    prototype(&handlers, proto);
    let replacement = handlers.clone();
    put(
        &owner,
        "recordMessageHandlerStats",
        fun(move |h, _, a| {
            h.log("stats-utf16");
            assert_eq!(
                units(&a[0]).last(),
                Some(&0xd800),
                "public native lone surrogate topic"
            );
            put(&replacement, "tickers", arr(vec![]));
            Ok(undefined())
        }),
    );
    let mut wire: Vec<u16> = "{\"topic\":\"".encode_utf16().collect();
    wire.extend(key);
    wire.extend("\"}".encode_utf16());
    api::public_on_message(&h, &owner, obj(vec![("data", text(wire))])).unwrap();
    assert_eq!(
        *h.trace.borrow(),
        [
            "track",
            "fallback",
            "stats-utf16",
            "parse",
            "exact",
            "prefix",
            "prefix-appended"
        ],
        "public captured prefix root and live append"
    );
    assert_eq!(rows(&prefix).len(), 2);
    assert_eq!(rows(&exact).len(), 1);
}
#[test]
fn public_pong_allocates_per_callback_and_failures_stop_iteration() {
    let h = Realm::new();
    let owner = base(&h);
    let received = Rc::new(RefCell::new(vec![]));
    let a = received.clone();
    let handler = fun(move |h, _, args| {
        h.log("pong-callback");
        a.borrow_mut().push(args[0].clone());
        Ok(undefined())
    });
    put(
        &h.field(&owner, "messageHandlers"),
        "pong",
        arr(vec![handler.clone(), handler]),
    );
    api::public_on_message(&h, &owner, obj(vec![("data", t("pong"))])).unwrap();
    let values = received.borrow();
    assert_eq!(values.len(), 2, "independent pong callback floor");
    assert!(
        !Rc::ptr_eq(&values[0], &values[1]),
        "public pong fresh object per callback"
    );
    assert_eq!(
        *h.trace.borrow(),
        [
            "track",
            "fallback",
            "stats:pong",
            "pong-callback",
            "pong-callback"
        ]
    );
    h.trace.borrow_mut().clear();
    let original = obj(vec![]);
    let e = original.clone();
    put(
        &h.field(&owner, "messageHandlers"),
        "pong",
        arr(vec![
            fun(move |_, _, _| Err(e.clone())),
            fun(|_, _, _| panic!("later callback after throw")),
        ]),
    );
    let actual = api::public_on_message(&h, &owner, obj(vec![("data", t("pong"))])).unwrap_err();
    assert!(
        Rc::ptr_eq(&actual, &original),
        "public original callback error"
    );
}
#[test]
fn custom_foreach_return_is_ignored_and_iterator_close_preserves_throw() {
    let h = Realm::new();
    let owner = base(&h);
    let data = obj(vec![("forEach", fun(|_, _, _| Ok(n(42.0))))]);
    assert!(
        is_undefined(&api::handle_order_topic(&h, &owner, data).unwrap()),
        "order body ignores overridden forEach return"
    );
    let mapped = obj(vec![]);
    let original = obj(vec![]);
    let e = original.clone();
    let closed = Rc::new(Cell::new(0));
    let closed_capture = closed.clone();
    let iterator = obj(vec![
        (
            "next",
            fun(|_, _, _| {
                Ok(obj(vec![
                    ("done", b(false)),
                    ("value", obj(vec![("id", t("x"))])),
                ]))
            }),
        ),
        (
            "return",
            fun(move |_, _, _| {
                closed_capture.set(closed_capture.get() + 1);
                Err(t("secondary close throw"))
            }),
        ),
    ]);
    put(
        &mapped,
        "$iterator",
        fun(move |_, _, _| Ok(iterator.clone())),
    );
    order_host(&h, &owner, mapped, obj(vec![]));
    put(
        &h.field(&owner, "parent"),
        "markOrderSeen",
        fun(move |_, _, _| Err(e.clone())),
    );
    let actual = api::handle_order_topic(&h, &owner, arr(vec![order("New")])).unwrap_err();
    assert!(
        Rc::ptr_eq(&actual, &original),
        "iterator original body throw wins over close throw"
    );
    assert_eq!(closed.get(), 1, "order abrupt body closes iterator");
}
#[test]
fn private_status_matrix_uses_independent_expectations() {
    let cases = [
        ("New", 1, "open", false),
        ("Untriggered", 1, "open", false),
        ("Cancelled", 1, "canceled", false),
        ("Deactivated", 1, "canceled", false),
        ("Rejected", 1, "canceled", false),
        ("Filled", 1, "closed", true),
        ("PartiallyFilled", 1, "open", true),
        ("Unknown", 0, "open", false),
    ];
    let mut seen = vec![];
    for (status, writes, next, fill) in cases {
        let h = Realm::new();
        let owner = base(&h);
        let mapped = mapped("one");
        order_host(
            &h,
            &owner,
            arr(vec![mapped.clone()]),
            obj(vec![("qty", n(5.0)), ("value", n(0.25))]),
        );
        api::handle_order_topic(&h, &owner, arr(vec![order(status)])).unwrap();
        assert_eq!(
            h.trace
                .borrow()
                .iter()
                .filter(|v| v.as_str() == "store-orders")
                .count(),
            writes,
            "native status {status} store call count"
        );
        assert_eq!(
            label(&h.field(&mapped, "status")),
            next,
            "native status {status} mapped value"
        );
        assert_eq!(
            h.trace.borrow().iter().any(|v| v == "fill"),
            fill,
            "native status {status} fill presence"
        );
        seen.push(status);
    }
    assert_eq!(
        seen,
        [
            "New",
            "Untriggered",
            "Cancelled",
            "Deactivated",
            "Rejected",
            "Filled",
            "PartiallyFilled",
            "Unknown"
        ],
        "status enumeration floor stopped at Unknown"
    );
}
#[test]
fn real_position_store_failure_retains_commit_and_skips_timestamp() {
    use safe_cex_store::model_patch::ModelPatch;
    let h = Realm::new();
    let owner = base(&h);
    let parent = position_host(&h, &owner);
    let store = Rc::new(DefaultStore::new());
    let position:safe_cex_types::Position=serde_json::from_value(json!({"id":"p","symbol":"BTCUSD","side":"long","entryPrice":10,"notional":20,"leverage":2,"unrealizedPnl":0,"contracts":2,"liquidationPrice":1})).unwrap();
    store.add_or_update_positions(vec![position]).unwrap();
    let original = obj(vec![]);
    let cause = Rc::new(Cause(original.clone()));
    let count = Rc::new(Cell::new(0));
    let late = Rc::new(Cell::new(0));
    let first_count = count.clone();
    let _first = store.subscribe(Arc::from(Box::new(
        move |_: &safe_cex_types::StoreData, _: bool| {
            first_count.set(first_count.get() + 1);
            Err(StoreSubscriberError::new("subscriber origin").with_cause(cause.clone()))
        },
    ) as safe_cex_types::StoreSubscriber));
    let late_count = late.clone();
    let _late = store.subscribe(Arc::from(
        Box::new(move |_: &safe_cex_types::StoreData, _: bool| {
            late_count.set(late_count.get() + 1);
            Ok(())
        }) as safe_cex_types::StoreSubscriber,
    ));
    let held = store.clone();
    put(
        &h.field(&owner, "store"),
        "updatePositions",
        fun(move |h, _, a| {
            h.log("store-positions");
            let updates = rows(&a[0])
                .iter()
                .map(|v| {
                    let pair = rows(v);
                    assert!(
                        Rc::ptr_eq(&pair[0], &pair[1]),
                        "real Store receives aliased position tuple"
                    );
                    let mut patch = ModelPatch::default();
                    patch.set_number("contracts", num(&h.field(&pair[1], "contracts")));
                    (label(&h.field(&pair[0], "id")), patch)
                })
                .collect::<Vec<_>>();
            let failure = held.update_positions(&updates).unwrap_err();
            let StoreMutationError::Notification(failure) = failure else {
                panic!("position native notification failure")
            };
            assert_eq!(failure.subscriber_id(), 0);
            let original = std::error::Error::source(failure.cause())
                .unwrap()
                .downcast_ref::<Cause>()
                .unwrap();
            Err(original.0.clone())
        }),
    );
    let actual = api::handle_position_topic(
        &h,
        &owner,
        arr(vec![obj(vec![("id", t("p")), ("contracts", n(7.0))])]),
    )
    .unwrap_err();
    assert!(
        Rc::ptr_eq(&actual, &original),
        "position real Store original subscriber throw"
    );
    assert_eq!(
        store.positions()[0].contracts,
        7.0,
        "position real Store commit retained"
    );
    assert_eq!(count.get(), 1);
    assert_eq!(late.get(), 0);
    assert_eq!(
        num(&h.field(&parent, "lastWebsocketPositionsUpdate")),
        7.0,
        "position real Store failure skips timestamp"
    );
    assert_eq!(
        *h.trace.borrow(),
        ["map-position", "store-positions"],
        "position real Store failure skips clock"
    );
}

#[test]
fn source_comparison_runs_after_all_native_assertions() {
    factory_retains_fresh_callable_state();
    private_dispatch_rereads_frame_and_starts_retry_eagerly();
    destructuring_precedes_disposal_and_errors_stop_dispatch();
    order_guard_fills_statuses_and_outer_iteration_have_native_values();
    order_live_aliases_status_rereads_and_nonfinite_cumulative_value();
    real_store_failure_commits_orders_retains_cause_and_stops_batch();
    positions_retain_pair_identity_and_timestamp_assignment_receiver();
    position_store_throw_prevents_clock_and_timestamp();
    public_dispatch_retains_arrays_utf16_and_live_iteration();
    public_pong_allocates_per_callback_and_failures_stop_iteration();
    custom_foreach_return_is_ignored_and_iterator_close_preserves_throw();
    private_status_matrix_uses_independent_expectations();
    real_position_store_failure_retains_commit_and_skips_timestamp();
    iterator_rejects_primitive_result_before_property_gets();
    let s = source();
    assert_eq!(
        s.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "factory",
            "privateRetry",
            "filled",
            "storeFailure",
            "positions",
            "publicLive",
            "primitiveIterators",
            "abruptIterator",
            "pong"
        ],
        "source case content floor stopped at pong"
    );
    assert_eq!(
        s["primitiveIterators"],
        json!([{"mode":"primitive-result","typeError":true,"readDone":0,"closed":0},{"mode":"primitive-iterator","typeError":true,"readDone":0,"closed":0}])
    );
    assert_eq!(s["abruptIterator"], json!({"same":true,"closed":1}));
    assert_eq!(s["pong"], json!({"fresh":true,"same":true,"later":0}));
    assert_eq!(s["factory"], json!([4, 3, 7]));
    assert_eq!(
        s["privateRetry"],
        json!([
            "track",
            "fallback",
            "diagnostic",
            "parse",
            "error:[WS]auth:denied",
            "error:[WS] Syncing time with BybitInverse...",
            "reconnect"
        ])
    );
    assert_eq!(
        s["filled"],
        json!({"price":3,"amount":0.3,"status":["closed","closed"],"trace":["map-order","seen:one","seen:two","execution","incremental","fill","store-orders"]})
    );
    assert_eq!(
        s["storeFailure"],
        json!({"same":true,"committed":1,"later":0,"status":"closed","trace":["map-order","seen:one","execution","incremental","fill","store-orders"]})
    );
    assert_eq!(
        s["positions"],
        json!({"aliases":true,"oldNegativeZero":true,"replacement":9,"trace":["map-position","map-position","store-positions","clock"]})
    );
    assert_eq!(
        s["publicLive"],
        json!([
            "track",
            "fallback",
            "stats-utf16",
            "parse",
            "exact",
            "prefix",
            "prefix-appended"
        ])
    );
}
#[test]
fn iterator_rejects_primitive_result_before_property_gets() {
    let h = Realm::new();
    let owner = base(&h);
    let mapped = obj(vec![]);
    let primitive = n(1.0);
    getter(&primitive, "done", |h| {
        h.log("bad-done-read");
        Ok(b(true))
    });
    let iterator = obj(vec![
        ("next", fun(move |_, _, _| Ok(primitive.clone()))),
        (
            "return",
            fun(|h, _, _| {
                h.log("bad-close");
                Ok(obj(vec![]))
            }),
        ),
    ]);
    put(
        &mapped,
        "$iterator",
        fun(move |_, _, _| Ok(iterator.clone())),
    );
    order_host(&h, &owner, mapped, obj(vec![]));
    let result = match api::handle_order_topic(&h, &owner, arr(vec![order("New")])) {
        Err(error) => error,
        Ok(_) => panic!("iterator native primitive result rejected"),
    };
    assert_eq!(
        label(&result),
        "iterator result must be object",
        "iterator native primitive result rejected"
    );
    assert_eq!(
        *h.trace.borrow(),
        ["map-order"],
        "invalid iterator result has no property reads, body or close"
    );
    let primitive_iter = obj(vec![("$iterator", fun(|_, _, _| Ok(n(1.0))))]);
    order_host(&h, &owner, primitive_iter, obj(vec![]));
    let result = api::handle_order_topic(&h, &owner, arr(vec![order("New")])).unwrap_err();
    assert_eq!(
        label(&result),
        "iterator must be object",
        "iterator acquisition rejects primitive iterator"
    );
}
