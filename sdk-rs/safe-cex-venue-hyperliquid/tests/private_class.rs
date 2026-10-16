#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[path = "support/private_class_host.rs"]
mod native;
use cache::{N, object, s};
use native::{Function, Host, same};
use safe_cex_venue_hyperliquid::{
    json_boundaries::JsPayloadHost, private_class::*, private_constructor::*,
};
use serde_json::{Value, json};
use std::process::Command;
const METHODS: [(&str, CorePrivateMethod, u8); 20] = [
    (
        "getStaleCheckChannelKey",
        CorePrivateMethod::GetStaleCheckChannelKey,
        2,
    ),
    (
        "getMessageTimestamp",
        CorePrivateMethod::GetMessageTimestamp,
        1,
    ),
    (
        "extractCoreBalanceSummary",
        CorePrivateMethod::ExtractCoreBalanceSummary,
        1,
    ),
    (
        "collectHip3DexesFromFills",
        CorePrivateMethod::CollectHip3DexesFromFills,
        1,
    ),
    (
        "resolvePositionMode",
        CorePrivateMethod::ResolvePositionMode,
        1,
    ),
    ("isStaleMessage", CorePrivateMethod::IsStaleMessage, 1),
    ("processMessage", CorePrivateMethod::ProcessMessage, 1),
    ("handleHip3AllMids", CorePrivateMethod::HandleHip3AllMids, 1),
    ("handleServerError", CorePrivateMethod::HandleServerError, 1),
    (
        "applyCoreClearinghouseState",
        CorePrivateMethod::ApplyCoreClearinghouseState,
        1,
    ),
    (
        "applyOptimisticCorePositionFromFill",
        CorePrivateMethod::ApplyOptimisticCorePositionFromFill,
        2,
    ),
    (
        "getFillPositionSide",
        CorePrivateMethod::GetFillPositionSide,
        1,
    ),
    ("safeParseNumber", CorePrivateMethod::SafeParseNumber, 1),
    (
        "trackProcessedRealizedPnlFill",
        CorePrivateMethod::TrackProcessedRealizedPnlFill,
        1,
    ),
    ("getRealizedPnlKey", CorePrivateMethod::GetRealizedPnlKey, 2),
    (
        "resolveSymbolFromCoin",
        CorePrivateMethod::ResolveSymbolFromCoin,
        2,
    ),
    (
        "setRealizedPnlForPosition",
        CorePrivateMethod::SetRealizedPnlForPosition,
        4,
    ),
    (
        "incrementRealizedPnlForPosition",
        CorePrivateMethod::IncrementRealizedPnlForPosition,
        4,
    ),
    (
        "applyRealizedPnlToCurrentPosition",
        CorePrivateMethod::ApplyRealizedPnlToCurrentPosition,
        3,
    ),
    (
        "getRealizedPnlForPosition",
        CorePrivateMethod::GetRealizedPnlForPosition,
        2,
    ),
];
const ARROWS: [(&str, CorePrivateArrow); 19] = [
    ("connectAndSubscribe", CorePrivateArrow::ConnectAndSubscribe),
    ("onOpen", CorePrivateArrow::OnOpen),
    ("startPingLoop", CorePrivateArrow::StartPingLoop),
    ("schedulePing", CorePrivateArrow::SchedulePing),
    ("stopPingLoop", CorePrivateArrow::StopPingLoop),
    ("subscribe", CorePrivateArrow::Subscribe),
    ("ping", CorePrivateArrow::Ping),
    (
        "subscribeSystemTopics",
        CorePrivateArrow::SubscribeSystemTopics,
    ),
    (
        "subscribeAllActiveAssetData",
        CorePrivateArrow::SubscribeAllActiveAssetData,
    ),
    ("onMessage", CorePrivateArrow::OnMessage),
    ("sendWithDiagnostics", CorePrivateArrow::SendWithDiagnostics),
    (
        "emitReceiveDiagnostics",
        CorePrivateArrow::EmitReceiveDiagnostics,
    ),
    ("handleUserEvents", CorePrivateArrow::HandleUserEvents),
    ("dispose", CorePrivateArrow::Dispose),
    (
        "handleHip3ClearinghouseState",
        CorePrivateArrow::HandleHip3ClearinghouseState,
    ),
    ("handleFills", CorePrivateArrow::HandleFills),
    ("handleFundings", CorePrivateArrow::HandleFundings),
    (
        "handleUserNonFundingLedgerUpdates",
        CorePrivateArrow::HandleUserNonFundingLedgerUpdates,
    ),
    ("handleOrderUpdates", CorePrivateArrow::HandleOrderUpdates),
];
const METHOD_EFFECTS: [Option<&str>; 20] = [
    Some("get:arg1:dex"),
    Some("get:arg0:serverTime"),
    Some("get:arg0:marginSummary"),
    Some("global:Set"),
    Some("global:String"),
    Some("get:arg0:channel"),
    Some("get:arg0:channel"),
    Some("get:arg0:mids"),
    Some("get:receiver:parent"),
    Some("get:receiver:getMessageTimestamp"),
    Some("get:arg0:coin"),
    Some("get:arg0:includes"),
    Some("global:Number"),
    Some("get:receiver:processedRealizedPnlFillIds"),
    Some("get:arg0:@@toPrimitive"),
    Some("get:arg0:@@toPrimitive"),
    Some("get:receiver:resolveSymbolFromCoin"),
    Some("get:receiver:resolveSymbolFromCoin"),
    Some("get:receiver:resolveSymbolFromCoin"),
    Some("get:receiver:realizedPnlByPositionKey"),
];
const ARROW_EFFECTS: [Option<&str>; 19] = [
    Some("get:receiver:isDisposed"),
    Some("get:receiver:isDisposed"),
    Some("get:receiver:stopPingLoop"),
    Some("global:setTimeout"),
    Some("get:receiver:hlPingTimeoutId"),
    Some("get:receiver:subscribeSystemTopics"),
    Some("get:receiver:isDisposed"),
    Some("get:receiver:parent"),
    Some("get:receiver:parent"),
    Some("get:arg0:data"),
    Some("get:receiver:send"),
    Some("global:shouldEmitHyperliquidWsReceiveDiagnostic"),
    Some("get:arg0:user"),
    Some("get:receiver:stopPingLoop"),
    Some("global:String"),
    Some("get:arg0:fills"),
    None,
    Some("get:receiver:parent"),
    Some("global:Array"),
];
fn get(h: &mut Host, o: &N, k: &str) -> N {
    JsPayloadHost::get(&mut h.cache, o, k).unwrap()
}
fn own(o: &N) -> Vec<String> {
    let N::O(o) = o else {
        panic!("CLASS decoded object")
    };
    let keys = o.borrow().fields.iter().map(|p| p.0.clone()).collect();
    keys
}
fn num(v: &N) -> f64 {
    let N::Num(n) = v else {
        panic!("CLASS decoded number")
    };
    *n
}
fn fixture() -> (Host, N, N) {
    let mut h = Host::default();
    let base = object(vec![("prototype", object(vec![]))]);
    let class = create_core_private_class(&mut h, &base).unwrap();
    (h, base, class)
}
fn method(h: &mut Host, c: &N, body: CorePrivateMethod, receiver: &N, args: &[N]) -> Result<N, N> {
    let name = METHODS
        .iter()
        .find(|(_, candidate, _)| *candidate == body)
        .unwrap()
        .0;
    let prototype = h
        .classes
        .iter()
        .find(|(class, _)| same(class, c))
        .unwrap()
        .1
        .clone();
    let function = get(h, &prototype, name);
    h.invoke(&function, receiver, args)
}
#[test]
fn private_class_native_then_actual_source() {
    let mut completed = 0;
    let mut probes = Vec::new();
    for (i, (name, body, _)) in METHODS.iter().enumerate() {
        let (mut h, _, class) = fixture();
        let receiver = object(vec![]);
        let mut args = [
            object(vec![]),
            object(vec![]),
            object(vec![]),
            object(vec![]),
        ];
        if *body == CorePrivateMethod::GetStaleCheckChannelKey {
            args[0] = s("clearinghouseState");
        }
        h.labels.push((receiver.clone(), "receiver".into()));
        for (i, a) in args.iter().enumerate() {
            h.labels.push((a.clone(), format!("arg{i}")))
        }
        h.events.clear();
        h.probe = true;
        let result = method(&mut h, &class, *body, &receiver, &args);
        let expected = METHOD_EFFECTS[i];
        if let Some(effect) = expected {
            assert!(
                same(result.as_ref().unwrap_err(), &h.marker),
                "CLASS original method error {name}"
            );
            assert_eq!(
                h.events,
                vec![effect],
                "CLASS method first native effect {name}"
            )
        } else {
            assert!(matches!(result.unwrap(), N::U));
            assert!(h.events.is_empty())
        }
        probes.push(json!([name, expected]));
        completed += 1;
    }
    for (i, (name, _)) in ARROWS.iter().enumerate() {
        let (mut h, _, class) = fixture();
        let receiver = construct_core_private_class(&mut h, &class, &N::U, None).unwrap();
        let function = get(&mut h, &receiver, name);
        let args = [object(vec![]), object(vec![])];
        h.labels.push((receiver.clone(), "receiver".into()));
        for (i, a) in args.iter().enumerate() {
            h.labels.push((a.clone(), format!("arg{i}")))
        }
        h.events.clear();
        h.probe = true;
        let result = h.invoke(&function, &N::U, &args);
        let expected = ARROW_EFFECTS[i];
        if let Some(effect) = expected {
            assert!(
                same(result.as_ref().unwrap_err(), &h.marker),
                "CLASS original arrow error {name}"
            );
            assert_eq!(
                h.events,
                vec![effect],
                "CLASS arrow first native effect {name}"
            )
        } else {
            assert!(matches!(result.unwrap(), N::U));
            assert!(h.events.is_empty())
        }
        probes.push(json!([name, expected]));
        completed += 1;
    }
    assert_eq!(completed, 39, "CLASS independently completed route floor");
    let (mut h, base, class) = fixture();
    let proto = h.classes[0].1.clone();
    assert_eq!(
        own(&class),
        [
            "length",
            "name",
            "prototype",
            "MAX_TRACKED_REALIZED_PNL_FILL_IDS",
            "CORE_FILL_POSITION_EPSILON",
            "PING_INTERVAL_MS",
            "SKIP_TIMESTAMP_CHECK_CHANNELS"
        ],
        "CLASS exact static own key floor"
    );
    assert_eq!(
        own(&proto),
        std::iter::once("constructor")
            .chain(METHODS.iter().map(|m| m.0))
            .collect::<Vec<_>>(),
        "CLASS exact prototype key floor"
    );
    for (name, body, length) in METHODS {
        let f = get(&mut h, &proto, name);
        let N::Builtin(id) = &f else {
            panic!("CLASS native method")
        };
        let Function::Method(c) = h.functions.get(id).unwrap() else {
            panic!("CLASS method capture")
        };
        assert_eq!(c.body, body);
        assert!(same(&c.class, &class), "CLASS defining class capture");
        assert!(
            h.descriptors.iter().any(|(o, k, e, w, c)| same(o, &f)
                && k == &format!("{name}/{length}")
                && !*e
                && !*w
                && *c),
            "CLASS native function metadata"
        );
        assert!(
            h.descriptors.iter().any(|(o, k, e, w, c)| same(o, &proto)
                && k == name
                && !*e
                && *w
                && *c),
            "CLASS native method descriptor"
        );
    }
    assert_eq!(
        num(&get(&mut h, &class, "MAX_TRACKED_REALIZED_PNL_FILL_IDS")).to_bits(),
        20000f64.to_bits()
    );
    assert_eq!(
        num(&get(&mut h, &class, "CORE_FILL_POSITION_EPSILON")).to_bits(),
        1e-10f64.to_bits()
    );
    assert_eq!(
        num(&get(&mut h, &class, "PING_INTERVAL_MS")).to_bits(),
        15000f64.to_bits()
    );
    let skip = get(&mut h, &class, "SKIP_TIMESTAMP_CHECK_CHANNELS");
    let N::Set(skip) = skip else {
        panic!("CLASS decoded static Set")
    };
    let expected = [
        "userFills",
        "userFundings",
        "userNonFundingLedgerUpdates",
        "notification",
        "orderUpdates",
        "userEvents",
    ];
    assert_eq!(
        skip.borrow().len(),
        6,
        "CLASS independent skip collection floor"
    );
    for (a, b) in skip.borrow().iter().zip(expected) {
        assert!(same(a, &s(b)), "CLASS exact skip native value")
    }
    let N::O(c) = &class else { unreachable!() };
    assert!(same(c.borrow().prototype.as_ref().unwrap(), &base));
    completed += 1;
    let second = create_core_private_class(&mut h, &base).unwrap();
    assert!(!same(&class, &second));
    assert!(!same(&proto, &h.classes[1].1));
    for (name, _, _) in METHODS {
        let a = get(&mut h, &proto, name);
        let p = h.classes[1].1.clone();
        assert!(
            !same(&a, &get(&mut h, &p, name)),
            "CLASS fresh methods across class evaluations"
        )
    }
    let secondskip = get(&mut h, &second, "SKIP_TIMESTAMP_CHECK_CHANNELS");
    assert!(!same(&N::Set(skip.clone()), &secondskip));
    completed += 1;
    let parent = object(vec![]);
    let instance = construct_core_private_class(&mut h, &class, &parent, None).unwrap();
    let N::O(i) = &instance else { unreachable!() };
    assert!(same(i.borrow().prototype.as_ref().unwrap(), &proto));
    assert!(same(&get(&mut h, &instance, "parent"), &parent));
    assert_eq!(
        own(&instance).len(),
        28,
        "CLASS base parent plus independent27 instance field floor"
    );
    assert_eq!(h.supers.len(), 1);
    assert!(
        same(&h.supers[0].0, &class)
            && same(&h.supers[0].1, &parent)
            && same(&h.supers[0].2, &class)
    );
    completed += 1;
    let borrowed = object(vec![]);
    let template = method(
        &mut h,
        &class,
        CorePrivateMethod::GetRealizedPnlKey,
        &N::U,
        &[s("A"), s("long")],
    )
    .unwrap();
    assert!(
        same(&template, &s("A:long")),
        "CLASS pure helper undefined borrowed receiver"
    );
    completed += 1;
    let onopen = get(&mut h, &instance, "onOpen");
    h.put(&instance, "isDisposed", &N::B(false), true);
    for (k, v) in [
        ("trackLastReceived", "trackLastReceived"),
        ("subscribe", "subscribed"),
        ("startPingLoop", "start"),
    ] {
        h.put(&instance, k, &N::Builtin(v.into()), true)
    }
    h.events.clear();
    assert!(
        matches!(h.invoke(&onopen, &borrowed, &[]).unwrap(), N::U),
        "CLASS detached arrow discards foreign return"
    );
    assert_eq!(
        h.events,
        vec![
            "get:other:isDisposed",
            "get:other:trackLastReceived",
            "call:trackLastReceived",
            "get:other:subscribe",
            "call:subscribed",
            "get:other:startPingLoop",
            "call:start"
        ]
    );
    completed += 1;
    // Same class scope serves all statics; global class and this.constructor are deliberately foreign.
    h.globals
        .insert("HyperliquidPrivateWebSocket".into(), second.clone());
    h.put(&instance, "constructor", &second, true);
    h.put(&class, "PING_INTERVAL_MS", &N::Num(17.25), true);
    h.put(&second, "PING_INTERVAL_MS", &N::Num(999.), true);
    let schedule_function = get(&mut h, &instance, "schedulePing");
    h.invoke(&schedule_function, &borrowed, &[]).unwrap();
    assert_eq!(h.timers.len(), 1);
    assert_eq!(
        num(&h.timers[0].1).to_bits(),
        17.25f64.to_bits(),
        "CLASS lexical ping value"
    );
    completed += 1;
    h.put(
        &class,
        "MAX_TRACKED_REALIZED_PNL_FILL_IDS",
        &N::Num(0.),
        true,
    );
    h.put(
        &second,
        "MAX_TRACKED_REALIZED_PNL_FILL_IDS",
        &N::Num(99.),
        true,
    );
    method(
        &mut h,
        &class,
        CorePrivateMethod::TrackProcessedRealizedPnlFill,
        &instance,
        &[s("f")],
    )
    .unwrap();
    let queue = get(&mut h, &instance, "processedRealizedPnlFillQueue");
    let N::A(q) = queue else { unreachable!() };
    assert_eq!(q.borrow().len(), 0, "CLASS lexical mutable max");
    completed += 1;
    // A retained native callback outlives the original borrowed scope; actual socket arrows reenter with their class.
    let timer = h.timers[0].0.clone();
    h.put(&instance, "isConnected", &N::B(true), true);
    h.put(&instance, "ping", &N::Builtin("start".into()), true);
    h.invoke(&timer, &N::U, &[]).unwrap();
    assert_eq!(h.timers.len(), 2);
    assert_eq!(
        num(&h.timers[1].1),
        17.25,
        "CLASS retained callback uses lexical arrow"
    );
    completed += 1;
    for (scene, body, args, expected_summary) in [
        (
            "number-identity",
            CorePrivateMethod::SafeParseNumber,
            vec![N::Num(9.)],
            false,
        ),
        (
            "timestamp-callee",
            CorePrivateMethod::GetMessageTimestamp,
            vec![object(vec![("serverTime", N::Num(9.))])],
            false,
        ),
        (
            "summary-callee",
            CorePrivateMethod::ExtractCoreBalanceSummary,
            vec![object(vec![(
                "marginSummary",
                object(vec![
                    ("accountValue", N::Num(9.)),
                    ("totalMarginUsed", N::Num(3.)),
                ]),
            )])],
            true,
        ),
        (
            "mode-callee",
            CorePrivateMethod::ResolvePositionMode,
            vec![object(vec![("type", s("hedged"))])],
            false,
        ),
    ] {
        let (mut x, _, c) = fixture();
        x.scene = scene.into();
        x.events.clear();
        let out = method(&mut x, &c, body, &N::U, &args).unwrap();
        if expected_summary {
            assert_eq!(
                own(&out),
                ["accountValue", "totalMarginUsed", "sourceField"],
                "CLASS native summary exact keys"
            );
            assert!(
                same(&get(&mut x, &out, "accountValue"), &x.a),
                "CLASS native captured Number account identity"
            );
            assert!(
                same(&get(&mut x, &out, "totalMarginUsed"), &x.b),
                "CLASS native fresh Number used identity"
            );
            assert!(
                same(&get(&mut x, &out, "sourceField"), &s("marginSummary")),
                "CLASS decoded summary sourceField value"
            );
            assert_eq!(x.finite_count, 2, "CLASS finite twice")
        } else {
            assert!(
                same(
                    &out,
                    if scene == "timestamp-callee" {
                        &x.b
                    } else {
                        &x.a
                    }
                ),
                "CLASS native helper original result identity {scene}"
            )
        };
        completed += 1;
    }
    for (name, body, arg, fail) in [
        (
            "finite-error",
            CorePrivateMethod::SafeParseNumber,
            N::Num(9.),
            "get:other:isFinite",
        ),
        (
            "enum-error",
            CorePrivateMethod::ResolvePositionMode,
            object(vec![("type", s("hedged"))]),
            "get:other:Hedged",
        ),
    ] {
        let (mut x, _, c) = fixture();
        x.fail = Some(fail.into());
        let err = method(&mut x, &c, body, &N::U, &[arg]).unwrap_err();
        assert!(same(&err, &x.marker), "CLASS original helper error {name}");
        completed += 1;
    }
    let (mut x, _, c) = fixture();
    x.scene = "finite-false".into();
    let payload = object(vec![(
        "marginSummary",
        object(vec![
            ("accountValue", N::Num(9.)),
            ("totalMarginUsed", N::Num(3.)),
        ]),
    )]);
    let out = method(
        &mut x,
        &c,
        CorePrivateMethod::ExtractCoreBalanceSummary,
        &N::U,
        &[payload],
    )
    .unwrap();
    assert!(matches!(out, N::Null), "CLASS decoded summary null");
    assert_eq!(x.finite_count, 1, "CLASS finite short circuit");
    completed += 1;

    // Reentry into a second class must not replace the first invocation's class.
    let second_instance = construct_core_private_class(&mut h, &second, &N::U, None).unwrap();
    let second_schedule = get(&mut h, &second_instance, "schedulePing");
    h.reentry_target = class.clone();
    h.reentry = Some((second_schedule, N::U, vec![]));
    h.invoke(&schedule_function, &N::U, &[]).unwrap();
    assert_eq!(h.timers.len(), 4, "CLASS independent nested timer floor");
    assert_eq!(num(&h.timers[2].1), 999.0, "CLASS nested class B static");
    assert_eq!(num(&h.timers[3].1), 17.25, "CLASS resumed class A static");
    assert!(
        same(
            h.globals.get("HyperliquidPrivateWebSocket").unwrap(),
            &second
        ),
        "CLASS no global class mutation"
    );
    completed += 1;
    let (mut x, _, c) = fixture();
    let out = method(
        &mut x,
        &c,
        CorePrivateMethod::ExtractCoreBalanceSummary,
        &N::U,
        &[object(vec![(
            "crossMarginSummary",
            object(vec![
                ("accountValue", N::Num(9.)),
                ("totalMarginUsed", N::Num(3.)),
            ]),
        )])],
    )
    .unwrap();
    assert_eq!(
        own(&out),
        ["accountValue", "totalMarginUsed", "sourceField"]
    );
    assert_eq!(num(&get(&mut x, &out, "accountValue")), 9.0);
    assert_eq!(num(&get(&mut x, &out, "totalMarginUsed")), 3.0);
    assert!(
        same(&get(&mut x, &out, "sourceField"), &s("crossMarginSummary")),
        "CLASS decoded cross summary sourceField"
    );
    completed += 1;
    let (mut x, _, c) = fixture();
    x.scene = "finite-false".into();
    let out = method(
        &mut x,
        &c,
        CorePrivateMethod::SafeParseNumber,
        &N::U,
        &[N::Num(9.)],
    )
    .unwrap();
    assert_eq!(
        num(&out).to_bits(),
        0.0f64.to_bits(),
        "CLASS decoded invalid-number positive zero"
    );
    completed += 1;
    let (mut x, _, c) = fixture();
    let out = method(
        &mut x,
        &c,
        CorePrivateMethod::GetMessageTimestamp,
        &N::U,
        &[object(vec![("serverTime", N::Num(0.))])],
    )
    .unwrap();
    assert!(
        matches!(out, N::Null),
        "CLASS decoded nonpositive timestamp null"
    );
    completed += 1;
    let (mut x, _, c) = fixture();
    let out = method(
        &mut x,
        &c,
        CorePrivateMethod::ResolvePositionMode,
        &N::U,
        &[object(vec![("type", s("other"))])],
    )
    .unwrap();
    assert!(
        same(&out, &x.b),
        "CLASS decoded OneWay import result identity"
    );
    completed += 1;
    let (mut x, _, c) = fixture();
    let a = N::S(safe_cex_venue_hyperliquid::spot_json::JsString(vec![
        0xd800, 65,
    ]));
    let b = N::S(safe_cex_venue_hyperliquid::spot_json::JsString(vec![
        0xdcff,
    ]));
    let out = method(
        &mut x,
        &c,
        CorePrivateMethod::GetRealizedPnlKey,
        &N::U,
        &[a, b],
    )
    .unwrap();
    let N::S(units) = out else {
        panic!("CLASS decoded UTF16 key variant")
    };
    assert_eq!(
        units.0,
        vec![0xd800, 65, 58, 0xdcff],
        "CLASS decoded native UTF16 template units"
    );
    completed += 1;
    assert_eq!(completed, 60, "CLASS independently completed native floor");
    assert_eq!(completed - 1, 59, "CLASS independent native stop");
    // Wire assertions begin only after every native scene has completed.
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private-class-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source["probes"], Value::Array(probes));
    assert_eq!(source["completed"], json!(60));
    assert_eq!(source["last"], json!(59));
}
