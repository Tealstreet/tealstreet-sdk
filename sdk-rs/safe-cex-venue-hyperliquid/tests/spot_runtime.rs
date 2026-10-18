#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[allow(dead_code)]
#[path = "support/spot_runtime_host.rs"]
mod support;
use cache::{N, arr, eq, object, s};
use native::{assert_value, decode};
use safe_cex_venue_hyperliquid::{
    spot_client::*, spot_public_subscriptions::*, spot_runtime::*, spot_trading::*,
    ws_ping_lifecycle::WsPingHost,
};
use std::{process::Command, sync::OnceLock};
use support::{Host, get};
fn oracle() -> &'static serde_json::Value {
    static V: OnceLock<serde_json::Value> = OnceLock::new();
    V.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/spot_runtime_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            23,
            "RUNTIME independent case floor through22"
        );
        assert_eq!(
            v["sourceHashes"].as_object().unwrap().len(),
            8,
            "RUNTIME source file floor through7 (includes actual omitUndefined)"
        );
        v
    })
}
fn host() -> Host {
    Host::new(
        object(vec![]),
        decode(&oracle()["enums"]),
        decode(&oracle()["endpoints"]),
    )
}

fn row(family: &str, name: &str) -> &'static serde_json::Value {
    oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["family"] == family && r["name"] == name)
        .unwrap()
}
fn keys(v: &N, hidden: bool) -> N {
    let N::O(o) = v else { panic!("object") };
    arr(o
        .borrow()
        .fields
        .iter()
        .filter(|p| hidden || p.2)
        .map(|p| s(&p.0))
        .collect())
}
fn fields(v: &N) -> Vec<String> {
    let N::O(o) = v else { panic!("object") };
    o.borrow()
        .fields
        .iter()
        .filter(|p| p.2)
        .map(|p| p.0.clone())
        .collect()
}
fn run(h: &mut Host, v: &N, key: &str, args: &[N]) -> Result<N, N> {
    let f = h.get(v, key)?;
    h.call(&f, v, args)
}
fn resolved(h: &Host, p: &N) -> Result<N, N> {
    h.drain();
    h.native_result(p)
}
fn trading_host() -> Host {
    let mut h = host();
    let market = decode(&oracle()["market"]);
    let e = h.exchange.clone();
    let store = get(&e, "store");
    h.set(&store, "markets", &arr(vec![market.clone()]))
        .unwrap();
    h.set(
        &store,
        "marketsBySymbol",
        &object(vec![("BTC/USDC", market)]),
    )
    .unwrap();
    h.set(
        &store,
        "tickersBySymbol",
        &object(vec![(
            "BTC/USDC",
            object(vec![("symbol", s("BTC/USDC")), ("last", N::Num(20.))]),
        )]),
    )
    .unwrap();
    h.set(
        &e,
        "resolveCallerClientOrderId",
        &N::Builtin("resolveCallerClientOrderId".into()),
    )
    .unwrap();
    h.set(
        &e,
        "requestSpotBalanceRefresh",
        &N::Builtin("refresh-record".into()),
    )
    .unwrap();
    h.set(
        &e,
        "priceToPrecision",
        &N::Builtin("price-precision".into()),
    )
    .unwrap();
    h.response = object(vec![(
        "data",
        object(vec![(
            "response",
            object(vec![(
                "data",
                object(vec![(
                    "statuses",
                    arr(vec![
                        object(vec![("resting", object(vec![("oid", N::Num(42.))]))]),
                        object(vec![("filled", object(vec![("oid", N::Num(43.))]))]),
                        object(vec![("resting", object(vec![("oid", N::Num(0.))]))]),
                    ]),
                )]),
            )]),
        )]),
    )]);
    h
}
#[test]
fn assembly_native_metadata_and_dispatch() {
    let mut h = host();
    let class = spot_exchange_class(&mut h).unwrap();
    let opts = object(vec![("secret", s("secret"))]);
    let store = object(vec![("markets", arr(vec![decode(&oracle()["market"])]))]);
    let owner = new_spot_exchange(
        &mut h,
        &class,
        &[s("HyperliquidSpot"), opts.clone(), store.clone()],
        None,
    )
    .unwrap();
    let descriptors = arr(fields(&owner)
        .iter()
        .filter_map(|k| {
            let v = get(&owner, k);
            if let N::Builtin(_) = v {
                let (name, len) = h.metadata(&v);
                Some(arr(vec![s(k), s(&name), N::Num(len as f64), N::B(true)]))
            } else {
                None
            }
        })
        .collect());
    assert_eq!(
        EXCHANGE_ARROWS.len(),
        25,
        "ASSEMBLY independent arrow floor through24"
    );
    let summary = object(vec![
        ("keys", keys(&owner, false)),
        ("descriptors", descriptors),
        ("prototypeKeys", keys(&class.prototype, true)),
        ("className", get(&class.constructor, "name")),
        ("classLength", get(&class.constructor, "length")),
        ("staticKeys", keys(&class.constructor, false)),
        ("config", get(&class.constructor, "SPOT_SYMBOL_CONFIG")),
        ("publicKeys", keys(&get(&owner, "publicWebsocket"), false)),
        ("privateKeys", keys(&get(&owner, "privateWebsocket"), false)),
    ]);
    h.define_data(
        &summary,
        "initial",
        &object(vec![
            ("isSpot", get(&owner, "isSpot")),
            ("inFlight", get(&owner, "spotBalanceRefreshInFlight")),
            ("last", get(&owner, "lastSpotBalanceRefreshAt")),
            ("lastForced", get(&owner, "lastForcedSpotBalanceRefreshAt")),
        ]),
    )
    .unwrap();
    let public_socket = get(&owner, "publicWebsocket");
    let public_arrows = arr(fields(&public_socket)
        .iter()
        .skip(4)
        .filter_map(|k| {
            let v = get(&public_socket, k);
            if let N::Builtin(_) = v {
                let (name, len) = h.metadata(&v);
                Some(arr(vec![s(k), s(&name), N::Num(len as f64)]))
            } else {
                None
            }
        })
        .collect());
    h.define_data(
        &summary,
        "publicMetadata",
        &object(vec![
            ("name", get(&class.public.constructor, "name")),
            ("length", get(&class.public.constructor, "length")),
            ("prototypeKeys", keys(&class.public.prototype, true)),
            ("arrows", public_arrows),
        ]),
    )
    .unwrap();

    assert_value(
        &summary,
        &row("assembly", "class")["result"],
        "ASSEMBLY native keys metadata",
    );
    assert!(
        eq(&get(&owner, "options"), &opts) && eq(&get(&owner, "store"), &store),
        "ASSEMBLY original constructor handles"
    );
    let calls = h.originals();
    let api: Vec<_> = calls
        .iter()
        .filter(|(k, _)| k == "perp.createAPI")
        .collect();
    assert_eq!(api.len(), 2, "ASSEMBLY two imported perp factories");
    for (_, args) in api {
        assert!(
            eq(&args[0], &opts) && eq(&args[1], &owner),
            "ASSEMBLY API handles"
        )
    }
    let limit = calls.iter().find(|(k, _)| k == "rateLimit").unwrap();
    assert!(
        eq(&limit.1[0], &get(&owner, "xhr")),
        "ASSEMBLY limited xhr alias"
    );
    assert_value(
        &get(&limit.1[1], "maxRPS"),
        &serde_json::json!(["number", "4008000000000000"]),
        "ASSEMBLY maxRPS",
    );
    let orders = run(&mut h, &owner, "fetchOrders", &[]).unwrap();
    let result = resolved(&h, &orders).unwrap();
    assert!(
        matches!(result,N::A(a) if a.borrow().is_empty()),
        "ASSEMBLY owned async dispatcher"
    );
    let custom = object(vec![("prototype", object(vec![("custom", N::B(true))]))]);
    let other = new_spot_exchange(&mut h, &class, &[s("X"), opts, store], Some(&custom)).unwrap();
    let N::O(o) = other else { panic!("object") };
    assert!(
        eq(
            o.borrow().prototype.as_ref().unwrap(),
            &get(&custom, "prototype")
        ),
        "ASSEMBLY custom newTarget prototype"
    );
}
#[test]
fn limiter_native_state_and_wait() {
    let mut h = host();
    let class = rate_limiter_class(&mut h).unwrap();
    let limiter = new_rate_limiter(&mut h, &class, None).unwrap();
    let summary = object(vec![
        ("keys", keys(&limiter, false)),
        ("tokens", get(&limiter, "tokens")),
        ("lastRefill", get(&limiter, "lastRefill")),
        ("capacity", get(&limiter, "capacity")),
        ("prototypeKeys", keys(&class.prototype, true)),
        ("className", get(&class.constructor, "name")),
        ("classLength", get(&class.constructor, "length")),
    ]);
    assert_value(
        &summary,
        &row("rate", "constructor")["result"],
        "RATE native ctor state",
    );
    for name in ["immediate", "boundary", "before", "nan"] {
        let r = row("rate", name);
        let input = decode(&r["input"]);
        let mut h = host();
        h.clock(cache::num(&get(&input, "now")));
        let class = rate_limiter_class(&mut h).unwrap();
        let limiter = new_rate_limiter(&mut h, &class, None).unwrap();
        h.set(&limiter, "lastRefill", &N::Num(100000.)).unwrap();
        h.set(&limiter, "tokens", &get(&input, "tokens")).unwrap();
        let p = run(&mut h, &limiter, "waitForToken", &[get(&input, "weight")]).unwrap();
        if name != "nan" {
            resolved(&h, &p).unwrap();
        }
        let summary = object(vec![
            ("tokens", get(&limiter, "tokens")),
            ("lastRefill", get(&limiter, "lastRefill")),
            ("timers", arr(h.timer_delays())),
        ]);
        assert_value(&summary, &r["result"], &format!("RATE native {name}"));
    }
    let mut h = host();
    let class = rate_limiter_class(&mut h).unwrap();
    let limiter = new_rate_limiter(&mut h, &class, None).unwrap();
    h.set(&limiter, "tokens", &N::Num(0.)).unwrap();
    let p = run(&mut h, &limiter, "waitForToken", &[N::Num(3.)]).unwrap();
    assert_eq!(h.timer_count(), 1, "RATE wait pending timer");
    let delay = h.timer_delays()[0].clone();
    h.clock(160000.);
    h.fire_timer();
    resolved(&h, &p).unwrap();
    assert_value(
        &object(vec![
            ("tokens", get(&limiter, "tokens")),
            ("lastRefill", get(&limiter, "lastRefill")),
            ("delay", delay),
        ]),
        &row("rate", "wake")["result"],
        "RATE native wake recursion",
    );
    let executor = ClientCallback::RateExecutor { delay: N::Num(9.) };
    let resolve = N::Builtin("caller".into());
    let handle = run_client_callback(&mut h, &N::U, &executor, &[resolve]).unwrap();
    assert!(
        matches!(handle, N::Num(_)),
        "RATE executor expression return"
    );
}
fn calls(h: &Host, names: &[&str]) -> N {
    arr(h
        .originals()
        .iter()
        .filter(|(k, _)| names.contains(&k.as_str()))
        .map(|(k, a)| {
            let mut v = vec![s(k)];
            v.extend(a.clone());
            arr(v)
        })
        .collect())
}
#[test]
fn markets_native_fields_and_aliases() {
    let r = row("markets", "two");
    let input = decode(&r["input"]);
    let mut h = trading_host();
    h.response = object(vec![("data", input.clone())]);
    let e = h.exchange.clone();
    let p = h.promise(fetch_markets_and_tickers(h.clone(), e));
    let result = resolved(&h, &p).unwrap();
    assert_value(&result, &r["result"], "MARKETS native fields");
    let assets = get(&input, "1");
    let tickers = get(&result, "1");
    let N::A(t) = tickers else { panic!("tickers") };
    let aliases = arr(t
        .borrow()
        .iter()
        .map(|v| {
            let info = get(v, "info");
            let N::A(a) = &assets else { panic!("assets") };
            N::Num(
                a.borrow()
                    .iter()
                    .position(|p| eq(p, &info))
                    .map(|i| i as f64)
                    .unwrap_or(-1.),
            )
        })
        .collect());
    assert_value(
        &aliases,
        &r["tickerAliases"],
        "MARKETS native ordered info aliases",
    );
}
#[test]
fn place_native_signing_inputs() {
    for name in ["market", "limit-tpsl", "unsupported", "trailing", "missing"] {
        let r = row("place", name);
        let h = trading_host();
        let e = h.exchange.clone();
        let opts = decode(&r["input"]);
        let p = h.promise(place_order(h.clone(), e, opts));
        let result = resolved(&h, &p);
        assert_eq!(
            result.is_err(),
            r["rejected"].as_bool().unwrap(),
            "PLACE rejection {name}"
        );
        match result {
            Ok(v) => assert_value(&v, &r["result"], &format!("PLACE native ids {name}")),
            Err(v) => assert_value(&v, &r["error"], &format!("PLACE native error {name}")),
        };
        assert_value(
            &calls(
                &h,
                &[
                    "orderRequestToOrderWire",
                    "signL1Action",
                    "post",
                    "refresh-record",
                ],
            ),
            &r["calls"],
            &format!("PLACE native sign/post inputs {name}"),
        );
    }
}
#[test]
fn cancel_and_update_native_ordering() {
    let r = row("cancel", "two");
    let h = trading_host();
    let p = h.promise(cancel_orders(
        h.clone(),
        h.exchange.clone(),
        decode(&r["input"]),
    ));
    resolved(&h, &p).unwrap();
    assert_value(
        &calls(&h, &["signL1Action", "post", "refresh-record"]),
        &r["calls"],
        "CANCEL native signing numeric inputs",
    );
    let r = row("update", "inherited-price");
    let mut h = trading_host();
    let e = h.exchange.clone();
    let input = decode(&r["input"]);
    let order = get(&input, "order");
    let update = object(vec![("amount", N::Num(3.))]);
    if let N::O(o) = &update {
        o.borrow_mut().prototype = Some(object(vec![("price", N::Num(0.))]))
    }
    let opts = object(vec![("order", order.clone()), ("update", update)]);
    let dead = N::Set(std::rc::Rc::new(std::cell::RefCell::new(vec![])));
    h.set(&e, "_deadOrders", &dead).unwrap();
    h.set(&e, "cancelOrders", &N::Builtin("cancel-record".into()))
        .unwrap();
    h.set(&e, "placeOrder", &N::Builtin("place-record".into()))
        .unwrap();
    let p = h.promise(update_order(h.clone(), e, opts));
    let result = resolved(&h, &p).unwrap();
    assert_value(&result, &r["result"], "UPDATE native return");
    assert_value(
        &calls(&h, &["cancel-record", "place-record"]),
        &r["calls"],
        "UPDATE native inherited changes and call order",
    );
    let original = h.originals();
    let cancel = original.iter().find(|(k, _)| k == "cancel-record").unwrap();
    assert!(
        eq(&get(&cancel.1[0], "0"), &order),
        "UPDATE original cancel row handle"
    );
    let N::Set(s) = dead else { panic!("set") };
    assert_value(
        &arr(s.borrow().clone()),
        &r["dead"],
        "UPDATE native dead order set",
    );
}
#[test]
fn public_native_subscription_state_and_packets() {
    for name in ["connected", "poll", "shared"] {
        let r = row("candle", name);
        let mut h = trading_host();
        let parent = h.exchange.clone();
        let class = public_socket_class(&mut h).unwrap();
        let socket = new_public_socket(&mut h, &class, &parent, None).unwrap();
        if name == "poll" {
            h.set(&socket, "isConnected", &N::B(false)).unwrap()
        }
        let opts = object(vec![("symbol", s("BTC/USDC")), ("interval", s("1m"))]);
        let callback = N::Builtin("caller".into());
        let clean = listen_ohlcv(&mut h, &socket, &opts, &callback).unwrap();
        let second = if name == "shared" {
            Some(listen_ohlcv(&mut h, &socket, &opts, &callback).unwrap())
        } else {
            None
        };
        if name == "poll" {
            h.set(&socket, "isConnected", &N::B(true)).unwrap();
            h.fire_timer()
        }
        let topic = "candle.@1.1m";
        let count = get(&get(&socket, "topics"), topic);
        let event = arr(vec![
            object(vec![
                ("s", s("other")),
                ("i", s("1m")),
                ("t", N::Num(1000.)),
                ("o", s("1")),
                ("h", s("2")),
                ("l", s("0")),
                ("c", s("1.5")),
                ("v", s("4")),
            ]),
            object(vec![
                ("s", s("@1")),
                ("i", s("1m")),
                ("t", N::Num(123000.)),
                ("o", s("1")),
                ("h", s("2")),
                ("l", s("0")),
                ("c", s("1.5")),
                ("v", s("4")),
            ]),
        ]);
        let N::A(a) = get(&get(&socket, "messageHandlers"), topic) else {
            panic!("handlers")
        };
        for parser in a.borrow().clone() {
            h.call(&parser, &N::U, &[event.clone()]).unwrap();
        }
        h.call(&clean, &N::U, &[]).unwrap();
        if let Some(clean) = second {
            h.call(&clean, &N::U, &[]).unwrap();
        }
        let summary = object(vec![
            ("count", count),
            ("topicCount", get(&get(&socket, "topics"), topic)),
            (
                "handlerCount",
                get(&get(&get(&socket, "messageHandlers"), topic), "length"),
            ),
            ("timers", N::Num(h.timer_count() as f64)),
        ]);
        assert_value(
            &summary,
            &r["result"],
            &format!("CANDLE native state {name}"),
        );
        packets_and_callbacks(&h, r, &format!("CANDLE {name}"));
    }
}
fn packets_and_callbacks(h: &Host, r: &serde_json::Value, label: &str) {
    let calls = h.originals();
    let native_packets = calls
        .iter()
        .filter(|(k, _)| k == "serialize")
        .collect::<Vec<_>>();
    let (packet_count, callback_count) = match label {
        "BOOK" => (14, 1),
        "CANDLE shared" => (2, 2),
        _ => (1, 1),
    };
    assert_eq!(
        r["packets"].as_array().unwrap().len(),
        packet_count,
        "{label} independent source packet floor"
    );
    assert_eq!(
        r["callbacks"].as_array().unwrap().len(),
        callback_count,
        "{label} independent source callback floor"
    );
    assert_eq!(native_packets.len(), packet_count, "{label} packet floor");
    for ((_, a), expected) in native_packets.iter().zip(r["packets"].as_array().unwrap()) {
        assert_value(&a[0], expected, &format!("{label} native packet"));
    }
    let callbacks = calls
        .iter()
        .filter(|(k, _)| k == "caller")
        .collect::<Vec<_>>();
    assert_eq!(callbacks.len(), callback_count, "{label} callback floor");
    for ((_, a), expected) in callbacks.iter().zip(r["callbacks"].as_array().unwrap()) {
        assert_value(&a[0], expected, &format!("{label} native callback"));
    }
    let wire = calls
        .iter()
        .filter(|(k, _)| k == "send")
        .map(|(_, a)| support::string(&a[0]))
        .collect::<Vec<_>>();
    let expected = r["wire"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().encode_utf16().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(
        wire.into_iter().map(|s| s.0).collect::<Vec<_>>(),
        expected,
        "{label} wire after native"
    );
}
#[test]
fn public_native_book_merge_and_cleanup() {
    let r = row("book", "levels");
    let mut h = trading_host();
    let parent = h.exchange.clone();
    let class = public_socket_class(&mut h).unwrap();
    let socket = new_public_socket(&mut h, &class, &parent, None).unwrap();
    let cleanup = listen_order_book(
        &mut h,
        &socket,
        &s("BTC/USDC"),
        &N::Builtin("caller".into()),
    )
    .unwrap();
    let parser = get(&get(&get(&socket, "messageHandlers"), "l2Book.@1"), "0");
    h.call(&parser, &N::U, &[decode(&r["input"])]).unwrap();
    let snapshot = get(
        &get(&get(&socket, "orderBookInnerState"), "inner"),
        "isSnapshotLoaded",
    );
    h.call(&cleanup, &N::U, &[]).unwrap();
    let summary = object(vec![
        ("topicCount", get(&get(&socket, "topics"), "l2Book.@1")),
        (
            "handlerCount",
            get(
                &get(&get(&socket, "messageHandlers"), "l2Book.@1"),
                "length",
            ),
        ),
        (
            "stateRemoved",
            N::B(matches!(
                get(&get(&socket, "orderBookInnerState"), "inner"),
                N::U
            )),
        ),
        ("snapshotLoaded", snapshot),
    ]);
    assert_value(&summary, &r["result"], "BOOK native subscription cleanup");
    packets_and_callbacks(&h, r, "BOOK");
}
#[test]
fn event_native_union_and_attached_user() {
    use safe_cex_venue_hyperliquid::{spot_models::*, spot_user_event::WsUserEvent};
    for kind in ["fills", "funding", "liquidation", "cancels"] {
        let r = row("type", kind);
        let input = decode(&r["input"]);
        let N::S(raw) = get(&input, "raw") else {
            panic!("raw")
        };
        let raw = String::from_utf16_lossy(&raw.0);
        let N::S(user) = get(&input, "user") else {
            panic!("user")
        };
        let event = match kind {
            "fills" => WsUserEvent::Fills {
                entries: serde_json::from_str::<Vec<WsFill>>(&raw).unwrap(),
                user,
            },
            "funding" => WsUserEvent::Funding {
                entry: serde_json::from_str::<WsUserFunding>(&raw).unwrap(),
                user,
            },
            "liquidation" => WsUserEvent::Liquidation {
                entry: serde_json::from_str::<WsLiquidation>(&raw).unwrap(),
                user,
            },
            _ => WsUserEvent::NonUserCancels {
                entries: serde_json::from_str::<Vec<WsNonUserCancel>>(&raw).unwrap(),
                user,
            },
        };
        let expected = &r["result"][1];
        let expected_field = |key: &str| -> &serde_json::Value {
            &expected
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v[0] == key)
                .unwrap()[1]
        };
        assert_value(
            &N::S(event.user().clone()),
            expected_field("user"),
            &format!("TYPE native attached UTF16 user {kind}"),
        );
        assert_eq!(
            event.user().0.first(),
            Some(&0xd800),
            "TYPE lone surrogate retained"
        );
        assert_eq!(
            expected_field("ownUser"),
            &serde_json::json!(["bool", true]),
            "TYPE source own user property"
        );
        assert!(
            r["rejectedMissingUser"].as_bool().unwrap(),
            "TYPE compiler missing user rejected"
        );
        let parts = &expected_field("parts")[1];
        let check = |index: usize, key: &str, value: N| {
            let p = &parts[index][1];
            let expected = &p.as_array().unwrap().iter().find(|v| v[0] == key).unwrap()[1];
            assert_value(
                &value,
                expected,
                &format!("TYPE native {kind} entry{index}.{key}"),
            );
        };
        macro_rules! strings {($entry:expr,$index:expr,$($rust:ident=>$key:literal),*)=>{$(check($index,$key,N::S($entry.$rust.clone()));)*}}
        match event {
            WsUserEvent::Fills { entries, .. } => {
                assert_eq!(entries.len(), 2, "TYPE fills floor through1");
                for (i, f) in entries.iter().enumerate() {
                    strings!(f,i,coin=>"coin",px=>"px",sz=>"sz",side=>"side",start_position=>"startPosition",dir=>"dir",closed_pnl=>"closedPnl",hash=>"hash",fee=>"fee");
                    for (k, v) in [("time", f.time), ("oid", f.oid), ("tid", f.tid)] {
                        check(i, k, N::Num(v))
                    }
                    check(i, "crossed", N::B(f.crossed));
                }
            }
            WsUserEvent::Funding { entry: f, .. } => {
                strings!(f,0,coin=>"coin",usdc=>"usdc",szi=>"szi",funding_rate=>"fundingRate");
                check(0, "time", N::Num(f.time));
            }
            WsUserEvent::Liquidation { entry: f, .. } => {
                strings!(f,0,liquidator=>"liquidator",liquidated_user=>"liquidated_user",liquidated_ntl_pos=>"liquidated_ntl_pos",liquidated_account_value=>"liquidated_account_value");
                check(0, "lid", N::Num(f.lid));
            }
            WsUserEvent::NonUserCancels { entries, .. } => {
                assert_eq!(entries.len(), 2, "TYPE cancel floor through1");
                for (i, f) in entries.iter().enumerate() {
                    strings!(f,i,coin=>"coin");
                    check(i, "oid", N::Num(f.oid));
                }
            }
        }
    }
}

#[test]
fn source_mutation_is_rejected() {
    let noop = std::env::var_os("SAFE_CEX_SPOT_SOURCE_NOOP_CONTROL").is_some();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "assembly_native_metadata_and_dispatch",
            "--exact",
            "--nocapture",
        ])
        .env("SAFE_CEX_SPOT_SOURCE_CONTROL", if noop { "0" } else { "1" })
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(101),
        "ORACLE live-source mutation must fail native assembly"
    );
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(
        text.contains("ASSEMBLY native keys metadata"),
        "ORACLE source mutation fails named assertion: {text}"
    );
}
