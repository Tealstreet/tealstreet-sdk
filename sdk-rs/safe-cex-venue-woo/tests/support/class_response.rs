use super::*;

fn string_property(value: &V, key: &str) -> Option<V> {
    match value.as_ref() {
        Js::S(value) => Some(match key {
            "length" => n(value.encode_utf16().count() as f64),
            "toUpperCase" => function(|r, _| Ok(s(&text(&r).to_uppercase()))),
            "split" => function(|r, a| Ok(arr(text(&r).split(&text(&a[0])).map(s).collect()))),
            "replace" => function(|r, a| {
                let pattern = text(&get(&a[0], "pattern")?);
                let flags = text(&get(&a[0], "flags")?);
                let raw = text(&r);
                if pattern == "/" {
                    assert_eq!(flags, "g", "WOO-CLASS-RESPONSE native slash flags");
                    Ok(s(&raw.replace('/', &text(&a[1]))))
                } else {
                    assert_eq!(flags, "");
                    let suffix = match pattern.as_str() {
                        "\\.P$" => ".P",
                        "_PERP$" => "_PERP",
                        "-PERP$" => "-PERP",
                        "PERP$" => "PERP",
                        _ => panic!("unexpected fixture pattern {pattern}"),
                    };
                    Ok(s(raw.strip_suffix(suffix).unwrap_or(&raw)))
                }
            }),
            _ => u(),
        }),
        Js::A(_) if key == "pop" => Some(function(|r, _| {
            let Js::A(rows) = r.as_ref() else {
                panic!("pop receiver")
            };
            Ok(rows.borrow_mut().pop().unwrap_or_else(u))
        })),
        _ => None,
    }
}

#[test]
fn successful_class_construction_binds_live_requested_symbol_method() {
    let runtime = Rc::new(Runtime::default());
    let mut life = runtime.operations();
    let getter_error = Rc::new(RefCell::new(None::<E>));
    let getter_state = getter_error.clone();
    let getter_calls = Rc::new(Cell::new(0));
    let count_getters = getter_calls.clone();
    let owners = Rc::new(RefCell::new(Vec::<V>::new()));
    let owners_for_get = owners.clone();
    let l = Rc::get_mut(&mut life).unwrap();
    let value = Rc::get_mut(&mut l.value).unwrap();
    value.get = Rc::new(move |v, key| {
        if key == "normalizeTransactionSymbol"
            && owners_for_get
                .borrow()
                .iter()
                .any(|owner| Rc::ptr_eq(owner, v))
        {
            if let Some(error) = getter_state.borrow().clone() {
                count_getters.set(count_getters.get() + 1);
                return Err(error);
            }
        }
        string_property(v, key).map_or_else(|| get(v, key), Ok)
    });
    value.global = Rc::new(|name| Ok(s(name)));
    l.binding = Rc::new(|value, pattern| {
        assert_eq!(pattern, &[true, true, true]);
        let Js::A(rows) = value.as_ref() else {
            panic!("client iterator")
        };
        Ok(rows.borrow().clone())
    });
    let clients = [
        obj(vec![("id", n(31.))]),
        obj(vec![("id", n(37.))]),
        obj(vec![("id", n(41.))]),
    ];
    let proxy = obj(vec![("id", n(43.))]);
    for (name, value) in [
        ("BaseExchange", s("BaseExchange")),
        ("GRAFANA_ENDPOINTS_EVERY", s("every")),
        ("GRAFANA_ENDPOINTS_PERIODIC", s("periodic")),
        (
            "createApiWithSecondaryKey",
            runtime.bind("clients", arr(clients.to_vec())),
        ),
        ("createAPI", runtime.bind("proxy", proxy.clone())),
        (
            "getTransport",
            function(|_, _| {
                Ok(obj(vec![(
                    "requireProxy",
                    function(|_, a| Ok(a[0].clone())),
                )]))
            }),
        ),
        ("WooPublicWebsocket", s("public")),
        ("WooPrivateWebscoket", s("private")),
        (
            "normalizeSymbol",
            function(|_, a| {
                Ok(s(&safe_cex_venue_woo::utils::normalize_symbol(&text(
                    &a[0],
                ))))
            }),
        ),
    ] {
        runtime.imports.borrow_mut().insert(name.into(), value);
    }
    let init = Rc::new(RefCell::new(None::<Initialize<V, E>>));
    let ctor = Rc::new(RefCell::new(None::<Constructor<V, E>>));
    let init_for_super = init.clone();
    let owners_for_super = owners.clone();
    let subscribed = Rc::new(Cell::new(0));
    let subscription_count = subscribed.clone();
    let construction = Rc::new(ConstructionOps {
        value: life,
        super_construct: Rc::new(move |_, _, args| {
            assert_eq!(args.len(), 3);
            let subscribed = subscription_count.clone();
            let owner = obj(vec![
                ("options", args[1].clone()),
                (
                    "on",
                    function(move |_, args| {
                        assert_eq!(text(&args[0]), "fill");
                        assert!(matches!(args[1].as_ref(), Js::F(_)));
                        subscribed.set(subscribed.get() + 1);
                        Ok(u())
                    }),
                ),
            ]);
            owners_for_super.borrow_mut().push(owner.clone());
            init_for_super.borrow().as_ref().unwrap()(owner.clone())?;
            Ok(owner)
        }),
        construct: Rc::new(|kind, args| {
            Ok(obj(vec![
                ("kind", kind),
                ("owner", args.first().cloned().unwrap_or_else(u)),
            ]))
        }),
        set: Rc::new(|r, k, v| {
            set(r, k, v);
            Ok(())
        }),
    });
    let init_slot = init.clone();
    let ctor_slot = ctor.clone();
    let ops = Rc::new(ExchangeClassOps {
        construction,
        derive: Rc::new(move |name, _, arity, construct, initialize| {
            assert_eq!((name, arity), ("WOOXExchange", 3));
            *init_slot.borrow_mut() = Some(initialize);
            *ctor_slot.borrow_mut() = Some(construct);
            Ok(obj(vec![]))
        }),
        define: Rc::new(|r, k, v| {
            set(r, k, v);
            Ok(())
        }),
        unbound_member: Rc::new(|_, name, _, _| {
            assert!(
                !matches!(
                    name,
                    "normalizeTransactionSymbol" | "normalizeRequestedSymbol"
                ),
                "WOO-CLASS-RESPONSE native bound slot must bypass unbound host"
            );
            let name = name.to_owned();
            Ok(function(move |_, _| {
                Err(Rc::new(format!("unbound {name}")))
            }))
        }),
        unbound_prototype: Rc::new(|_, _, _, _| Ok(())),
        regexp: Rc::new(|pattern, flags| {
            Ok(obj(vec![("pattern", s(pattern)), ("flags", s(flags))]))
        }),
        multiply: Rc::new(|a, b| Ok(n(number(&a) * number(&b)))),
        null: Rc::new(|| Rc::new(Js::Null)),
        is_string: Rc::new(|v| matches!(v.as_ref(), Js::S(_))),
    });
    let class = define_exchange_class(ops).unwrap();
    for _ in 0..2 {
        ctor.borrow().as_ref().unwrap()(
            class.clone(),
            class.clone(),
            vec![s("woo"), obj(vec![]), obj(vec![])],
        )
        .unwrap();
    }
    assert_eq!(
        subscribed.get(),
        2,
        "WOO-CLASS-RESPONSE native successful construction subscriptions"
    );
    assert_eq!(
        &*runtime.trace.borrow(),
        &["clients", "proxy", "clients", "proxy"]
    );
    let owner = owners.borrow()[0].clone();
    let other = owners.borrow()[1].clone();
    for (key, expected) in [
        ("xhr", &clients[0]),
        ("_primaryXhr", &clients[1]),
        ("_secondaryXhr", &clients[2]),
        ("_proxyEnforcedXhr", &proxy),
    ] {
        assert!(
            Rc::ptr_eq(&get(&owner, key).unwrap(), expected),
            "WOO-CLASS-RESPONSE native client identity"
        );
    }
    for key in ["publicWebsocket", "privateWebsocket"] {
        assert!(Rc::ptr_eq(
            &get(&get(&owner, key).unwrap(), "owner").unwrap(),
            &owner
        ));
    }
    let arrow = get(&owner, "normalizeRequestedSymbol").unwrap();
    assert!(!Rc::ptr_eq(
        &arrow,
        &get(&other, "normalizeRequestedSymbol").unwrap()
    ));
    for (raw, expected) in [
        ("x:btcusdt.P", "BTCUSDT"),
        ("x:", "X"),
        ("PERP/BTC/USDT", "BTCUSDT"),
        ("x:btc_PERP-PERP.P", "BTC"),
        ("ß", "SS"),
    ] {
        assert_eq!(
            text(&call(arrow.clone(), other.clone(), vec![s(raw)]).unwrap()),
            expected,
            "WOO-CLASS-RESPONSE native default {raw}"
        );
    }
    for raw in [u(), s(""), s("PERP")] {
        assert!(matches!(
            call(arrow.clone(), other.clone(), vec![raw])
                .unwrap()
                .as_ref(),
            Js::U
        ));
    }
    let original = obj(vec![("result", n(97.))]);
    let result = original.clone();
    let captured = owner.clone();
    let args_seen = Rc::new(RefCell::new(Vec::<String>::new()));
    let seen = args_seen.clone();
    set(
        &owner,
        "normalizeTransactionSymbol",
        function(move |receiver, args| {
            assert!(
                Rc::ptr_eq(&receiver, &captured),
                "WOO-CLASS-RESPONSE native lexical owner"
            );
            seen.borrow_mut().push(text(&args[0]));
            Ok(result.clone())
        }),
    );
    assert!(
        Rc::ptr_eq(
            &call(arrow.clone(), other.clone(), vec![s("btc.p")]).unwrap(),
            &original
        ),
        "WOO-CLASS-RESPONSE native dynamic return identity"
    );
    call(arrow.clone(), other.clone(), vec![s("PERP")]).unwrap();
    assert_eq!(
        &*args_seen.borrow(),
        &["BTC", ""],
        "WOO-CLASS-RESPONSE native live arguments"
    );
    let error: E = Rc::new("original method error".into());
    let thrown = error.clone();
    set(
        &owner,
        "normalizeTransactionSymbol",
        function(move |_, _| Err(thrown.clone())),
    );
    assert!(Rc::ptr_eq(
        &call(arrow.clone(), other.clone(), vec![s("btc")])
            .err()
            .unwrap(),
        &error
    ));
    let getter_sentinel: E = Rc::new("distinct getter error".into());
    let method_calls = Rc::new(Cell::new(0));
    let count_methods = method_calls.clone();
    set(
        &owner,
        "normalizeTransactionSymbol",
        function(move |_, _| {
            count_methods.set(count_methods.get() + 1);
            Ok(u())
        }),
    );
    *getter_error.borrow_mut() = Some(getter_sentinel.clone());
    assert!(
        Rc::ptr_eq(
            &call(arrow, other, vec![s("btc")])
                .err()
                .expect("WOO-CLASS-RESPONSE native getter must throw"),
            &getter_sentinel
        ),
        "WOO-CLASS-RESPONSE native getter error identity"
    );
    assert_eq!(
        getter_calls.get(),
        1,
        "WOO-CLASS-RESPONSE native getter count"
    );
    assert_eq!(
        method_calls.get(),
        0,
        "WOO-CLASS-RESPONSE native getter suppresses method"
    );
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-class-oracle.cjs"
        ))
        .arg("--bound-response")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["constructed"], 2);
    assert_eq!(source["subscriptions"], 2);
    assert_eq!(source["defaultCases"], 8);
}
