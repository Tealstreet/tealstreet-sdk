#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/public_class_host.rs"]
mod host;
#[path = "support/private_class_host.rs"]
mod private_host;
use cache::{N, object, s};
use host::{Host, same};
use safe_cex_venue_hyperliquid::{public_class::*, public_constructor::PublicSocketArrow};
use std::process::Command;
const ARROWS: [(&str, PublicSocketArrow, u8); 17] = [
    (
        "connectAndSubscribe",
        PublicSocketArrow::ConnectAndSubscribe,
        0,
    ),
    ("onOpen", PublicSocketArrow::OnOpen, 0),
    ("startPingLoop", PublicSocketArrow::StartPingLoop, 0),
    ("schedulePing", PublicSocketArrow::SchedulePing, 0),
    ("stopPingLoop", PublicSocketArrow::StopPingLoop, 0),
    (
        "subscribePublicTopics",
        PublicSocketArrow::SubscribePublicTopics,
        0,
    ),
    ("sendSubscription", PublicSocketArrow::SendSubscription, 1),
    (
        "sendUnsubscription",
        PublicSocketArrow::SendUnsubscription,
        1,
    ),
    (
        "subscribeUserTopics",
        PublicSocketArrow::SubscribeUserTopics,
        0,
    ),
    ("ping", PublicSocketArrow::Ping, 0),
    ("onMessage", PublicSocketArrow::OnMessage, 1),
    (
        "sendWithDiagnostics",
        PublicSocketArrow::SendWithDiagnostics,
        1,
    ),
    (
        "emitReceiveDiagnostics",
        PublicSocketArrow::EmitReceiveDiagnostics,
        2,
    ),
    ("handleAllMids", PublicSocketArrow::HandleAllMids, 1),
    (
        "handleFastAssetCtxs",
        PublicSocketArrow::HandleFastAssetCtxs,
        1,
    ),
    ("listenOHLCV", PublicSocketArrow::ListenOHLCV, 2),
    ("listenOrderBook", PublicSocketArrow::ListenOrderBook, 2),
];
fn num(v: N, n: f64) {
    let N::Num(v) = v else {
        panic!("PUBLIC native number")
    };
    assert_eq!(v.to_bits(), n.to_bits(), "PUBLIC native bits")
}
fn size(h: &mut Host, socket: &N) -> usize {
    let N::Set(s) = h.get(socket, "activeSubscriptions").unwrap() else {
        panic!("PUBLIC native set")
    };
    let n = s.borrow().len();
    n
}
fn payload(h: &mut Host, kind: &str, coin: &str) -> N {
    let o = object(vec![("type", s(kind)), ("coin", s(coin))]);
    h.inner.labels.push((o.clone(), "subscription".into()));
    o
}
#[test]
fn source_class_uses_installed_native_arrows_and_live_subscription_state() {
    native_installed_public_composition();
    let mut h = Host::default();
    let base_proto = object(vec![("dispose", N::Builtin("super.dispose".into()))]);
    let base = object(vec![("prototype", base_proto.clone())]);
    let c = create_core_public_class(&mut h, &base).unwrap();
    let proto = h.get(&c, "prototype").unwrap();
    num(h.get(&c, "PING_INTERVAL_MS").unwrap(), 15000.);
    let parent = object(vec![]);
    let socket = construct_core_public_class(&mut h, &c, &parent, None).unwrap();
    let second = construct_core_public_class(&mut h, &c, &parent, None).unwrap();
    let N::O(o) = &socket else {
        panic!("PUBLIC native object")
    };
    assert_eq!(
        o.borrow().fields.len(),
        26,
        "PUBLIC independent native field floor including base parent"
    );
    assert!(
        same(&h.get(&socket, "parent").unwrap(), &parent),
        "PUBLIC original parent handle"
    );
    assert!(same(&h.inner.supers[0].0, &c));
    assert!(same(&h.inner.supers[0].2, &c));
    let N::O(p) = &proto else { panic!("prototype") };
    let dispose_descriptor = p
        .borrow()
        .fields
        .iter()
        .find(|p| p.0 == "dispose")
        .unwrap()
        .clone();
    assert!(
        !dispose_descriptor.2,
        "PUBLIC prototype method nonenumerable"
    );
    let mut callables = 0;
    for (name, body, len) in ARROWS {
        let f = h.get(&socket, name).unwrap();
        let f2 = h.get(&second, name).unwrap();
        assert!(!same(&f, &f2), "PUBLIC fresh native function {name}");
        let N::Builtin(id) = &f else {
            panic!("PUBLIC native callable")
        };
        let host::Function::Arrow(cap) = h.functions.get(id).unwrap() else {
            panic!("PUBLIC arrow capture")
        };
        assert_eq!(cap.body, body, "PUBLIC actual dispatcher body {name}");
        assert!(
            same(&cap.receiver, &socket),
            "PUBLIC lexical receiver {name}"
        );
        assert!(same(&cap.class, &c), "PUBLIC defining class {name}");
        let meta = h.metadata.iter().find(|m| same(&m.0, &f)).unwrap();
        assert_eq!((&meta.1, meta.2), (&name.to_owned(), len));
        callables += 1;
    }
    assert_eq!(callables, 17, "PUBLIC independent native callable floor");
    assert!(
        !same(
            &h.get(&socket, "activeSubscriptions").unwrap(),
            &h.get(&second, "activeSubscriptions").unwrap()
        ),
        "PUBLIC fresh native Set"
    );
    h.put(&socket, "send", &N::Builtin("send".into()));
    h.current_socket = Some(socket.clone());
    let fixed = h.get(&socket, "subscribePublicTopics").unwrap();
    h.invoke(&fixed, &second, &[]).unwrap();
    h.invoke(&fixed, &second, &[]).unwrap();
    assert_eq!(h.sent.len(), 4, "PUBLIC repeated fixed feeds bypass dedup");
    assert_eq!(
        size(&mut h, &socket),
        0,
        "PUBLIC fixed feeds leave active Set empty"
    );
    let topics = object(vec![
        ("candle.BTC.1m", N::Num(1.)),
        ("l2Book.ETH", N::Num(1.)),
        ("ignored.X", N::Num(1.)),
    ]);
    h.put(&socket, "topics", &topics);
    let user = h.get(&socket, "subscribeUserTopics").unwrap();
    h.invoke(&user, &second, &[]).unwrap();
    assert_eq!(
        h.sent.len(),
        7,
        "PUBLIC candle plus depth plus BBO native send floor"
    );
    assert_eq!(
        size(&mut h, &socket),
        3,
        "PUBLIC installed arrows share constructed native Set"
    );
    assert_eq!(
        h.before_send_sizes,
        [0, 0, 0, 0, 1, 2, 3],
        "PUBLIC native Set writes precede each sender"
    );
    let sub = payload(&mut h, "l2Book", "ETH");
    let send = h.get(&socket, "sendSubscription").unwrap();
    assert!(
        matches!(
            h.invoke(&send, &second, &[sub.clone()]).unwrap(),
            N::B(false)
        ),
        "PUBLIC native duplicate false"
    );
    assert_eq!(h.sent.len(), 7);
    let unsubscribe = h.get(&socket, "sendUnsubscription").unwrap();
    assert!(matches!(
        h.invoke(&unsubscribe, &second, &[sub]).unwrap(),
        N::U
    ));
    assert_eq!(
        size(&mut h, &socket),
        2,
        "PUBLIC native delete before unsubscribe"
    );
    assert_eq!(h.before_send_sizes.last(), Some(&2));
    h.send_fail = true;
    let failed = payload(&mut h, "bbo", "BTC");
    let err = h.invoke(&send, &second, &[failed.clone()]).unwrap_err();
    assert!(
        same(&err, &h.inner.marker),
        "PUBLIC original send failure handle"
    );
    assert_eq!(
        size(&mut h, &socket),
        3,
        "PUBLIC failed send retains active native key"
    );
    assert!(matches!(
        h.invoke(&send, &second, &[failed]).unwrap(),
        N::B(false)
    ));
    h.send_fail = false;
    for (index, kind) in ["allMids", "fastAssetCtxs", "allMids", "fastAssetCtxs"]
        .iter()
        .enumerate()
    {
        let expected =
            format!("{{\"method\":\"subscribe\",\"subscription\":{{\"type\":\"{kind}\"}}}}");
        assert!(
            same(&h.sent[index], &s(&expected)),
            "PUBLIC native fixed feed order {index}"
        );
    }
    for (index, expected) in [
        (
            4,
            r#"{"method":"subscribe","subscription":{"type":"candle","coin":"BTC","interval":"1m"}}"#,
        ),
        (
            5,
            r#"{"method":"subscribe","subscription":{"type":"l2Book","coin":"ETH"}}"#,
        ),
        (
            6,
            r#"{"method":"subscribe","subscription":{"type":"bbo","coin":"ETH"}}"#,
        ),
        (
            7,
            r#"{"method":"unsubscribe","subscription":{"type":"l2Book","coin":"ETH"}}"#,
        ),
    ] {
        assert!(
            same(&h.sent[index], &s(expected)),
            "PUBLIC native user-topic sender text {index}"
        );
    }
    // Every native assertion above precedes JSON/wire assertions.
    let payloads: Vec<serde_json::Value> = h
        .sent
        .iter()
        .map(|v| {
            let N::S(v) = v else {
                panic!("PUBLIC wire text")
            };
            serde_json::from_str(&String::from_utf16(&v.0).unwrap()).unwrap()
        })
        .collect();
    assert_eq!(
        payloads[4]["subscription"],
        serde_json::json!({"type":"candle","coin":"BTC","interval":"1m"})
    );
    assert_eq!(
        payloads[5]["subscription"],
        serde_json::json!({"type":"l2Book","coin":"ETH"})
    );
    assert_eq!(
        payloads[6]["subscription"],
        serde_json::json!({"type":"bbo","coin":"ETH"})
    );
    assert_eq!(payloads[7]["method"], "unsubscribe");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-hyperliquid/tests/finding-public-class-source.cjs"))
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "PUBLIC actual maintained source witness: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["source_cases"], 7, "PUBLIC source completed floor");
    assert_eq!(receipt["callable_floor"], 17);
    assert_eq!(receipt["stop"], "dispose stop-before-super");
    let supplement =
        Command::new("node")
            .arg(root.join(
                "sdk-rs/safe-cex-venue-hyperliquid/tests/finding-public-composition-source.cjs",
            ))
            .current_dir(&root)
            .output()
            .unwrap();
    assert!(
        supplement.status.success(),
        "PUBLIC composition actual source: {}",
        String::from_utf8_lossy(&supplement.stderr)
    );
    let receipt: serde_json::Value = serde_json::from_slice(&supplement.stdout).unwrap();
    assert_eq!(
        receipt["cases"], 2,
        "PUBLIC composition independent source floor"
    );
    assert_eq!(receipt["stop"], "installed-dispose-dynamic-receiver");
}

fn native_installed_public_composition() {
    let mut h = Host::default();
    let base = object(vec![(
        "prototype",
        object(vec![("dispose", N::Builtin("super.dispose".into()))]),
    )]);
    let class = create_core_public_class(&mut h, &base).unwrap();
    let owner = construct_core_public_class(&mut h, &class, &object(vec![]), None).unwrap();
    let other = construct_core_public_class(&mut h, &class, &object(vec![]), None).unwrap();
    h.put(&owner, "send", &N::Builtin("send".into()));
    h.current_socket = Some(owner.clone());
    h.put(
        &owner,
        "topics",
        &object(vec![
            ("candle.BTC.1m", N::Num(1.)),
            ("l2Book.ETH", N::Num(1.)),
            ("ignored.X", N::Num(1.)),
        ]),
    );
    let fixed = h.get(&owner, "subscribePublicTopics").unwrap();
    assert!(matches!(h.invoke(&fixed, &other, &[]).unwrap(), N::U));
    assert_eq!(
        h.topic_subscriptions.len(),
        3,
        "PUBLIC candle plus depth plus BBO native send floor: installed fixed-to-user edge"
    );
    for (i, (kind, coin, interval)) in [
        ("candle", "BTC", Some("1m")),
        ("l2Book", "ETH", None),
        ("bbo", "ETH", None),
    ]
    .into_iter()
    .enumerate()
    {
        let input = h.topic_subscriptions[i].clone();
        {
            let N::O(fields) = &input else {
                panic!("PUBLIC composed native subscription object {i}")
            };
            let fields = fields.borrow();
            let keys: Vec<_> = fields.fields.iter().map(|field| field.0.as_str()).collect();
            let expected = if i == 0 {
                vec!["type", "coin", "interval"]
            } else {
                vec!["type", "coin"]
            };
            assert_eq!(
                keys, expected,
                "PUBLIC composed native own subscription keys {i}"
            );
            assert!(
                fields.fields.iter().all(|field| field.2),
                "PUBLIC composed native enumerable subscription keys {i}"
            );
        }
        assert!(
            same(&h.get(&input, "type").unwrap(), &s(kind)),
            "PUBLIC composed native subscription type {i}"
        );
        assert!(
            same(&h.get(&input, "coin").unwrap(), &s(coin)),
            "PUBLIC composed native subscription coin {i}"
        );
        let actual = h.get(&input, "interval").unwrap();
        if let Some(value) = interval {
            assert!(
                same(&actual, &s(value)),
                "PUBLIC composed native interval {i}"
            );
        } else {
            assert!(
                matches!(actual, N::U),
                "PUBLIC composed native interval absent {i}"
            );
        }
    }
    assert_eq!(h.sent.len(), 5, "PUBLIC composed independent sender floor");
    assert_eq!(
        h.before_send_sizes,
        [0, 0, 1, 2, 3],
        "PUBLIC composed native mutation-before-send order"
    );
    assert_eq!(
        size(&mut h, &owner),
        3,
        "PUBLIC composed lexical owner state"
    );
    assert_eq!(
        size(&mut h, &other),
        0,
        "PUBLIC composed call-time receiver untouched"
    );

    let proto = h.get(&class, "prototype").unwrap();
    let dispose = h.get(&proto, "dispose").unwrap();
    assert!(
        matches!(h.invoke(&dispose, &other, &[]).unwrap(), N::U),
        "PUBLIC installed dispose native undefined"
    );
    assert_eq!(
        h.lifecycle_calls.len(),
        2,
        "PUBLIC installed dispose reaches stop then super"
    );
    for (index, expected) in ["stopPingLoop", "super.dispose"].into_iter().enumerate() {
        assert_eq!(
            h.lifecycle_calls[index].0, expected,
            "PUBLIC installed dispose native effect order {index}"
        );
        assert!(
            same(&h.lifecycle_calls[index].1, &other),
            "PUBLIC installed dispose original dynamic receiver {index}"
        );
        assert!(
            !same(&h.lifecycle_calls[index].1, &owner),
            "PUBLIC installed dispose does not capture first owner {index}"
        );
    }
}
