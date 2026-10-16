use super::*;
use safe_cex_types::{OrderSide, OrderStatus, OrderTimeInForce, OrderType};
use safe_cex_venue_bybit::primary_local_order_writes as primary;

#[derive(Clone)]
enum LocalEvent {
    Clock,
    Fallback,
    Record(V),
    Add(V),
}
struct LocalHost {
    inner: H,
    events: RefCell<Vec<LocalEvent>>,
    recorded: RefCell<Vec<V>>,
    passed: RefCell<Option<V>>,
    recent: V,
    fallback: bool,
    clock_calls: Cell<usize>,
    map_gets: Cell<usize>,
    map_sets: Cell<usize>,
    parse_calls: RefCell<Vec<(bool, V)>>,
}
fn enum_value(value: impl serde::Serialize) -> V {
    let value = serde_json::to_value(value).unwrap();
    s(value
        .as_str()
        .expect("local-write enum must serialize as string"))
}
fn coercion_error() -> E {
    E(o(vec![
        ("name", s("TypeError")),
        ("message", s("Cannot convert object to primitive value")),
    ]))
}
fn string_units(value: &V) -> Result<Vec<u16>, E> {
    match value {
        V::S(units) => Ok(units.clone()),
        V::A(values) => {
            let mut output = vec![];
            for (index, value) in values.borrow().iter().enumerate() {
                if index != 0 {
                    output.push(u16::from(b','));
                }
                if !matches!(value, V::U | V::Null) {
                    output.extend(string_units(value)?);
                }
            }
            Ok(output)
        }
        V::O(values) if values.borrow().iter().any(|(key, _)| key == "toString") => {
            Err(coercion_error())
        }
        _ => Ok(text(value).encode_utf16().collect()),
    }
}
macro_rules! local_forward {
    ($($name:ident($($arg:ident:$ty:ty),*)->$result:ty;)*) => {$ (
        fn $name(&self,$($arg:$ty),*)->$result { self.inner.$name($($arg),*) }
    )*};
}
impl SessionHost for LocalHost {
    type Value = V;
    type Error = E;
    local_forward! {
        get(owner:&V,key:&str)->Result<V,E>;
        optional(owner:&V,key:&str)->Result<V,E>;
        set(owner:&V,key:&str,value:V)->Result<(),E>;
        define(owner:&V,key:&str,value:V)->Result<(),E>;
        truthy(value:&V)->bool;
        equal(a:&V,b:&V)->bool;
        undefined()->V;
        literal(value:&str)->V;
        boolean(value:bool)->V;
        number(value:f64)->V;
        numeric(value:&V)->Result<f64,E>;
        text(value:Vec<u16>)->V;
        object(fields:Vec<(&'static str,V)>)->V;
        array(values:Vec<V>)->V;
        global()->V;
        select(owner:V,key:&str)->Result<V,E>;
        wait(value:V)->SessionFuture<'_,V,E>;
        intrinsic(name:&str,args:Vec<V>)->Result<V,E>;
        allocate(parent:V)->Result<V,E>;
        callback(owner:V,code:&'static str,captures:Vec<V>)->Result<V,E>;
        promise()->Result<(V,V,V),E>;
        error(message:V)->Result<E,E>;
        error_value(error:E)->V;
        normalize_error(error:E)->Result<V,E>;
    }
    fn units(&self, value: &V) -> Result<Vec<u16>, E> {
        string_units(value)
    }
    fn receiver_call(&self, owner: V, callable: V, args: Vec<V>) -> Result<V, E> {
        let V::F(_, key) = callable else {
            panic!("local_write_native_receiver_callable")
        };
        self.apply(V::F(Box::new(owner), key), args)
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, E> {
        let V::F(receiver, key) = selected else {
            panic!("local_write_native_selected_callable");
        };
        match key.as_str() {
            "parseFloat" => {
                assert!(
                    matches!(receiver.as_ref(), V::U),
                    "local_write_native_parse_receiver"
                );
                assert_eq!(args.len(), 1, "local_write_native_parse_arguments");
                self.parse_calls
                    .borrow_mut()
                    .push((matches!(receiver.as_ref(), V::U), args[0].clone()));
                let units = string_units(&args[0])?;
                // Surrogates terminate parseFloat's numeric grammar without altering the input.
                let prefix: String = units
                    .iter()
                    .copied()
                    .take_while(|unit| !(0xd800..=0xdfff).contains(unit))
                    .map(|unit| char::from_u32(u32::from(unit)).unwrap())
                    .collect();
                Ok(V::N(safe_cex_num::parse_float(&prefix)))
            }
            "max" => {
                assert!(
                    eq(&receiver, &get(&self.inner.global, "Math")),
                    "local_write_native_math_receiver"
                );
                assert_eq!(args.len(), 2, "local_write_native_max_arguments");
                let left = num(&args[0]);
                let right = num(&args[1]);
                Ok(V::N(if left.is_nan() || right.is_nan() {
                    f64::NAN
                } else {
                    left.max(right)
                }))
            }
            "getCurrentTimeMs" => {
                assert!(
                    eq(&receiver, &get(&self.inner.global, "virtualClock")),
                    "local_write_native_clock_receiver"
                );
                assert!(args.is_empty(), "local_write_native_clock_arguments");
                self.clock_calls.set(self.clock_calls.get() + 1);
                self.events.borrow_mut().push(LocalEvent::Clock);
                Ok(V::N(1234.5))
            }
            "recordRecentlyPlacedOrder" => {
                assert!(
                    eq(&receiver, &self.inner.owner),
                    "local_write_native_record_receiver"
                );
                assert_eq!(args.len(), 1, "local_write_native_record_arguments");
                self.recorded.borrow_mut().push(args[0].clone());
                self.events
                    .borrow_mut()
                    .push(LocalEvent::Record(args[0].clone()));
                Ok(V::U)
            }
            "shouldUseLocalRestOrderLifecycleFallback" => {
                assert!(
                    eq(&receiver, &self.inner.owner),
                    "local_write_native_fallback_receiver"
                );
                assert!(args.is_empty(), "local_write_native_fallback_arguments");
                self.events.borrow_mut().push(LocalEvent::Fallback);
                Ok(V::B(self.fallback))
            }
            "get" => {
                assert!(
                    eq(&receiver, &self.recent),
                    "local_write_native_map_get_receiver"
                );
                assert_eq!(args.len(), 1, "local_write_native_map_get_arguments");
                native(&args[0], &s("order-1"), "map lookup original id");
                self.map_gets.set(self.map_gets.get() + 1);
                Ok(get(&self.recent, "order-1"))
            }
            "set" => {
                assert!(
                    eq(&receiver, &self.recent),
                    "local_write_native_map_set_receiver"
                );
                assert_eq!(args.len(), 2, "local_write_native_map_set_arguments");
                native(&args[0], &s("order-1"), "map set original id");
                self.map_sets.set(self.map_sets.get() + 1);
                put(&self.recent, "order-1", args[1].clone());
                Ok(self.recent.clone())
            }
            "addOrUpdateOrders" => {
                assert!(
                    eq(&receiver, &get(&self.inner.owner, "store")),
                    "local_write_native_store_receiver"
                );
                assert_eq!(args.len(), 1, "local_write_native_store_arguments");
                assert!(
                    self.passed.borrow().is_none(),
                    "local_write_native_single_publication"
                );
                *self.passed.borrow_mut() = Some(args[0].clone());
                self.events
                    .borrow_mut()
                    .push(LocalEvent::Add(args[0].clone()));
                Ok(V::U)
            }
            _ => panic!("local_write_native_unexpected_dependency {key}"),
        }
    }
}
impl SpotOrderHost for LocalHost {
    fn spread(&self, value: V) -> Result<V, E> {
        self.inner.spread(value)
    }
    fn map<F>(&self, method: V, body: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<V, E>,
    {
        self.inner.map(method, body)
    }
    fn filter<F>(&self, method: V, body: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        self.inner.filter(method, body)
    }
    fn some<F>(&self, method: V, body: F) -> Result<bool, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        self.inner.some(method, body)
    }
    fn find<F>(&self, method: V, body: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        self.inner.find(method, body)
    }
    fn reduce<F>(&self, method: V, initial: V, body: F) -> Result<V, E>
    where
        F: FnMut(V, V) -> Result<V, E>,
    {
        self.inner.reduce(method, initial, body)
    }
    fn for_of<F>(&self, values: V, body: F) -> Result<(), E>
    where
        F: FnMut(V) -> Result<(), E>,
    {
        self.inner.for_of(values, body)
    }
    fn series<'a, F>(&'a self, callee: V, values: V, body: F) -> SessionFuture<'a, V, E>
    where
        F: FnMut(V, usize) -> SessionFuture<'a, V, E> + 'a,
    {
        self.inner.series(callee, values, body)
    }
}
impl LocalHost {
    fn new(fallback: bool) -> Self {
        let inner = H::new("primary-local-write");
        put(
            &inner.global,
            "parseFloat",
            V::F(Box::new(V::U), "parseFloat".into()),
        );
        put(
            &inner.global,
            "OrderStatus",
            o(vec![("Open", enum_value(OrderStatus::Open))]),
        );
        put(
            &inner.global,
            "OrderType",
            o(vec![
                ("Limit", enum_value(OrderType::Limit)),
                ("Market", enum_value(OrderType::Market)),
                ("Conditional", enum_value(OrderType::Conditional)),
            ]),
        );
        put(
            &inner.global,
            "OrderSide",
            o(vec![("Buy", enum_value(OrderSide::Buy))]),
        );
        put(
            &inner.global,
            "OrderTimeInForce",
            o(vec![
                (
                    "GoodTillCancel",
                    enum_value(OrderTimeInForce::GoodTillCancel),
                ),
                (
                    "ImmediateOrCancel",
                    enum_value(OrderTimeInForce::ImmediateOrCancel),
                ),
            ]),
        );
        let recent = o(vec![]);
        put(&inner.owner, "recentlyPlacedOrders", recent.clone());
        Self {
            inner,
            recent,
            fallback,
            events: RefCell::new(vec![]),
            recorded: RefCell::new(vec![]),
            passed: RefCell::new(None),
            clock_calls: Cell::new(0),
            map_gets: Cell::new(0),
            map_sets: Cell::new(0),
            parse_calls: RefCell::new(vec![]),
        }
    }
    fn event_keys(&self) -> Vec<&'static str> {
        self.events
            .borrow()
            .iter()
            .map(|event| match event {
                LocalEvent::Clock => "clock",
                LocalEvent::Fallback => "fallback",
                LocalEvent::Record(_) => "record",
                LocalEvent::Add(_) => "add",
            })
            .collect()
    }
    fn observed_events(&self) -> Vec<Json> {
        self.events
            .borrow()
            .iter()
            .map(|event| match event {
                LocalEvent::Clock => json!("clock"),
                LocalEvent::Fallback => json!("fallback"),
                LocalEvent::Record(value) => json!(["record", observe_local(value)]),
                LocalEvent::Add(value) => json!(["add", observe_local(value)]),
            })
            .collect()
    }
}
fn observe_local(value: &V) -> Json {
    match value {
        V::U => json!({"undefined":true}),
        V::Null => Json::Null,
        V::B(value) => json!(value),
        V::N(value) => {
            json!({"number":if value.is_nan() {"nan".into()} else {format!("{:016x}",value.to_bits())}})
        }
        V::S(value) => json!({"stringUnits":value}),
        V::A(values) => Json::Array(values.borrow().iter().map(observe_local).collect()),
        V::O(values) => Json::Object(
            values
                .borrow()
                .iter()
                .map(|(key, value)| (key.clone(), observe_local(value)))
                .collect(),
        ),
        _ => panic!("local_write_native_unexpected_observer_value"),
    }
}
fn base_opts() -> V {
    o(vec![
        ("symbol", s("BTCUSD")),
        ("type", enum_value(OrderType::Limit)),
        ("side", enum_value(OrderSide::Buy)),
        ("amount", V::N(7.)),
        ("price", V::N(11.)),
        ("triggerPrice", V::N(13.)),
        ("reduceOnly", V::B(true)),
    ])
}
fn expected_record(amount: f64, price: f64, opts: &V, reduce: bool, tif: OrderTimeInForce) -> V {
    o(vec![
        ("id", s("order-1")),
        ("status", enum_value(OrderStatus::Open)),
        ("symbol", s("BTCUSD")),
        ("type", get(opts, "type")),
        ("side", enum_value(OrderSide::Buy)),
        ("price", V::N(price)),
        ("amount", V::N(amount)),
        ("filled", V::N(0.)),
        ("remaining", V::N(amount)),
        ("reduceOnly", V::B(reduce)),
        ("timeInForce", enum_value(tif)),
        ("lastUpdate", V::N(1234.5)),
    ])
}
fn check_record(
    host: &LocalHost,
    amount: f64,
    price: f64,
    opts: &V,
    reduce: bool,
    tif: OrderTimeInForce,
    label: &str,
) {
    assert_eq!(
        host.clock_calls.get(),
        1,
        "local_write_native_record_clock {label}"
    );
    assert_eq!(
        host.event_keys(),
        ["clock", "record"],
        "local_write_native_record_order {label}"
    );
    let records = host.recorded.borrow();
    assert_eq!(records.len(), 1, "local_write_native_record_floor {label}");
    assert!(
        matches!(get(&records[0], "reduceOnly"), V::B(value) if value == reduce),
        "local_write_native_reduce_only {label}"
    );
    native(
        &records[0],
        &expected_record(amount, price, opts, reduce, tif),
        label,
    );
    assert_eq!(
        host.map_gets.get(),
        0,
        "local_write_native_record_no_map_reads"
    );
    assert_eq!(
        host.map_sets.get(),
        0,
        "local_write_native_record_no_map_writes"
    );
    assert!(
        host.passed.borrow().is_none(),
        "local_write_native_record_no_publication"
    );
}
fn original_order(info: V) -> V {
    o(vec![
        ("id", s("order-1")),
        ("status", enum_value(OrderStatus::Open)),
        ("symbol", s("BTCUSD")),
        ("type", enum_value(OrderType::Limit)),
        ("side", enum_value(OrderSide::Buy)),
        ("price", V::N(11.)),
        ("amount", V::N(7.)),
        ("filled", V::N(2.)),
        ("remaining", V::N(5.)),
        ("reduceOnly", V::B(true)),
        ("timeInForce", enum_value(OrderTimeInForce::GoodTillCancel)),
        ("lastUpdate", V::N(42.)),
        ("info", info),
    ])
}
#[test]
#[ignore = "requires installed maintained TypeScript and source enum modules"]
fn primary_local_order_write_native_source_oracle() {
    let quantities = [
        V::U,
        s("0"),
        s("-0"),
        s("NaN"),
        s("Infinity"),
        s("-Infinity"),
        s("0x10"),
        s("1.75tail"),
        V::Null,
        a(vec![]),
        a(vec![s("2.5")]),
        o(vec![]),
        o(vec![("toString", V::Null)]),
        V::S(vec![0xd800]),
    ];
    let prices = [V::U, s("0"), s("4.25tail"), V::Null];
    let expected_amounts = [
        7.,
        7.,
        7.,
        7.,
        f64::INFINITY,
        f64::NEG_INFINITY,
        7.,
        1.75,
        7.,
        7.,
        2.5,
        7.,
        7.,
        7.,
    ];
    let expected_prices = [17.5, 11., 4.25, 17.5];
    assert_eq!(
        quantities.len(),
        14,
        "local_write_native_quantity_declaration"
    );
    assert_eq!(prices.len(), 4, "local_write_native_price_declaration");
    let mut cases = vec![];
    let mut coercion_failures = 0;
    for (qi, qty) in quantities.iter().enumerate() {
        for (pi, price) in prices.iter().enumerate() {
            let h = LocalHost::new(true);
            let opts = base_opts();
            let payload = o(vec![
                ("qty", qty.clone()),
                ("price", price.clone()),
                ("triggerPrice", s("17.5")),
            ]);
            let original_opts = h.spread(opts.clone()).unwrap();
            let original_payload = h.spread(payload.clone()).unwrap();
            let result = api::record_recently_placed_order_from_opts(
                &h,
                &h.inner.owner,
                s("order-1"),
                opts.clone(),
                payload.clone(),
            );
            let error = if qi == 12 {
                let failure = result.expect_err("local_write_native_own_null_to_string_refusal");
                native(&get(&failure.0, "name"), &s("TypeError"), "coercion name");
                native(
                    &get(&failure.0, "message"),
                    &s("Cannot convert object to primitive value"),
                    "coercion message",
                );
                assert!(
                    h.event_keys().is_empty(),
                    "local_write_native_coercion_before_effects"
                );
                assert_eq!(
                    h.clock_calls.get(),
                    0,
                    "local_write_native_failed_clock_floor"
                );
                assert!(
                    h.recorded.borrow().is_empty(),
                    "local_write_native_failed_record_floor"
                );
                coercion_failures += 1;
                Some(
                    json!({"name":"TypeError","message":"Cannot convert object to primitive value"}),
                )
            } else {
                result.unwrap();
                check_record(
                    &h,
                    expected_amounts[qi],
                    expected_prices[pi],
                    &original_opts,
                    true,
                    OrderTimeInForce::GoodTillCancel,
                    &format!("quantity{qi}/price{pi}"),
                );
                None
            };
            native(&opts, &original_opts, "record opts unchanged");
            native(&payload, &original_payload, "record payload unchanged");
            let mut row = json!({"input":observe_local(&payload),"events":h.observed_events()});
            if let Some(error) = error {
                row["error"] = error;
            }
            cases.push(row);
        }
    }
    assert_eq!(cases.len(), 56, "local_write_native_quantity_price_floor");
    assert_eq!(
        coercion_failures, 4,
        "local_write_native_coercion_error_floor"
    );
    let supplements_data = vec![
        (
            "conditional-clear",
            o(vec![("type", enum_value(OrderType::Conditional))]),
            o(vec![("qty", s("3")), ("price", s("2"))]),
            3.,
            2.,
        ),
        (
            "market-retained",
            o(vec![("type", enum_value(OrderType::Market))]),
            o(vec![("qty", s("3")), ("price", s("2"))]),
            3.,
            2.,
        ),
        (
            "explicit-reduce-false",
            o(vec![("reduceOnly", V::B(false))]),
            o(vec![("qty", s("3")), ("price", s("2"))]),
            3.,
            2.,
        ),
        (
            "explicit-tif",
            o(vec![(
                "timeInForce",
                enum_value(OrderTimeInForce::ImmediateOrCancel),
            )]),
            o(vec![("qty", s("3")), ("price", s("2"))]),
            3.,
            2.,
        ),
        (
            "zero-amount",
            o(vec![("amount", V::N(0.))]),
            o(vec![]),
            0.,
            11.,
        ),
        (
            "negative-zero-amount",
            o(vec![("amount", V::N(-0.))]),
            o(vec![]),
            0.,
            11.,
        ),
        (
            "nan-amount",
            o(vec![("amount", V::N(f64::NAN))]),
            o(vec![]),
            0.,
            11.,
        ),
        (
            "zero-price",
            o(vec![("price", V::N(0.))]),
            o(vec![("price", s("0"))]),
            7.,
            13.,
        ),
        (
            "nan-price",
            o(vec![("price", V::N(f64::NAN))]),
            o(vec![("price", s("NaN"))]),
            7.,
            13.,
        ),
        (
            "infinite-price",
            o(vec![("price", V::N(f64::INFINITY))]),
            o(vec![("price", s("0"))]),
            7.,
            f64::INFINITY,
        ),
    ];
    let mut supplements = vec![];
    for (kind, changes, payload, amount, price) in supplements_data {
        let h = LocalHost::new(true);
        let opts = base_opts();
        let V::O(changes) = changes else {
            panic!("local_write_native_option_patch")
        };
        for (key, value) in changes.borrow().iter() {
            put(&opts, key, value.clone());
        }
        let original_opts = h.spread(opts.clone()).unwrap();
        let original_payload = h.spread(payload.clone()).unwrap();
        api::record_recently_placed_order_from_opts(
            &h,
            &h.inner.owner,
            s("order-1"),
            opts.clone(),
            payload.clone(),
        )
        .unwrap();
        check_record(
            &h,
            amount,
            price,
            &original_opts,
            !matches!(kind, "conditional-clear" | "explicit-reduce-false"),
            if kind == "explicit-tif" {
                OrderTimeInForce::ImmediateOrCancel
            } else {
                OrderTimeInForce::GoodTillCancel
            },
            kind,
        );
        native(&opts, &original_opts, "supplement opts unchanged");
        native(&payload, &original_payload, "supplement payload unchanged");
        supplements.push(json!({"kind":kind,"opts":observe_local(&opts),"payload":observe_local(&payload),"events":h.observed_events()}));
    }
    assert_eq!(supplements.len(), 10, "local_write_native_options_floor");
    let update_data = vec![
        ("ordinary", o(vec![]), false, true, 7., 2., 5., "order-1"),
        ("recent", o(vec![]), true, true, 7., 2., 5., "order-1"),
        (
            "fallback-off",
            o(vec![]),
            true,
            false,
            7.,
            2.,
            5.,
            "order-1",
        ),
        (
            "amount",
            o(vec![("amount", V::N(10.))]),
            true,
            true,
            10.,
            2.,
            8.,
            "order-1",
        ),
        (
            "remaining-override",
            o(vec![("amount", V::N(10.)), ("remaining", V::N(27.))]),
            true,
            true,
            10.,
            2.,
            8.,
            "order-1",
        ),
        (
            "zero-amount",
            o(vec![("amount", V::N(0.))]),
            true,
            true,
            0.,
            2.,
            0.,
            "order-1",
        ),
        (
            "nan-amount",
            o(vec![("amount", V::N(f64::NAN))]),
            true,
            true,
            f64::NAN,
            2.,
            f64::NAN,
            "order-1",
        ),
        (
            "infinite-amount",
            o(vec![("amount", V::N(f64::INFINITY))]),
            true,
            true,
            f64::INFINITY,
            2.,
            f64::INFINITY,
            "order-1",
        ),
        (
            "id-change",
            o(vec![("id", s("replacement"))]),
            true,
            true,
            7.,
            2.,
            5.,
            "replacement",
        ),
        (
            "filled-change",
            o(vec![("amount", V::N(10.)), ("filled", V::N(8.))]),
            true,
            true,
            10.,
            8.,
            8.,
            "order-1",
        ),
    ];
    let mut updates = vec![];
    for (kind, patch, recent, fallback, amount, filled, remaining, id) in update_data {
        let h = LocalHost::new(fallback);
        let info = o(vec![("child", s("retained"))]);
        let original = original_order(info.clone());
        let original_patch = h.spread(patch.clone()).unwrap();
        let prior = o(vec![
            ("placedAt", V::N(12.)),
            ("order", original.clone()),
            ("extra", info.clone()),
        ]);
        if recent {
            put(&h.recent, "order-1", prior.clone());
        }
        primary::apply_local_order_update_after_rest_write(
            &h,
            &h.inner.owner,
            original.clone(),
            patch.clone(),
        )
        .unwrap();
        native(&patch, &original_patch, "update patch unchanged");
        native(
            &original,
            &original_order(info.clone()),
            "source order remains unchanged",
        );
        native(
            &get(&info, "child"),
            &s("retained"),
            "retained child remains unchanged",
        );
        assert!(
            eq(&get(&prior, "order"), &original),
            "local_write_native_prior_order_alias"
        );
        assert!(
            eq(&get(&prior, "extra"), &info),
            "local_write_native_prior_extra_alias"
        );
        native(
            &get(&prior, "placedAt"),
            &V::N(12.),
            "prior timestamp unchanged",
        );
        let passed = h.passed.borrow().clone();
        if !fallback {
            assert_eq!(
                h.event_keys(),
                ["fallback"],
                "local_write_native_disabled_event_order"
            );
            assert!(
                passed.is_none(),
                "local_write_native_disabled_no_publication"
            );
            assert!(
                eq(&get(&h.recent, "order-1"), &prior),
                "local_write_native_disabled_map_unchanged"
            );
        } else {
            assert_eq!(
                h.event_keys(),
                ["fallback", "clock", "add"],
                "local_write_native_update_event_order {kind}"
            );
            let rows = vals(passed.as_ref().expect("local_write_native_updated_array"));
            assert_eq!(rows.len(), 1, "local_write_native_updated_array_floor");
            let next = &rows[0];
            assert!(!eq(next, &original), "local_write_native_fresh_order");
            assert!(
                eq(&get(next, "info"), &info),
                "local_write_native_shallow_info_alias"
            );
            let expected = original_order(info.clone());
            for (key, value) in [
                ("id", s(id)),
                ("amount", V::N(amount)),
                ("filled", V::N(filled)),
                ("remaining", V::N(remaining)),
                ("lastUpdate", V::N(1234.5)),
            ] {
                put(&expected, key, value);
            }
            let V::N(actual_remaining) = get(next, "remaining") else {
                panic!("local_write_native_remaining_number");
            };
            assert_eq!(
                actual_remaining.to_bits(),
                remaining.to_bits(),
                "local_write_native_original_filled_remaining {kind}"
            );
            native(next, &expected, kind);
            if recent {
                let entry = get(&h.recent, "order-1");
                assert!(!eq(&entry, &prior), "local_write_native_fresh_recent_entry");
                assert!(
                    eq(&get(&entry, "order"), next),
                    "local_write_native_same_published_recent_order"
                );
                assert!(
                    eq(&get(&entry, "extra"), &info),
                    "local_write_native_recent_extra_alias"
                );
                native(
                    &get(&entry, "placedAt"),
                    &V::N(12.),
                    "recent original placed time",
                );
                native(
                    &entry,
                    &o(vec![
                        ("placedAt", V::N(12.)),
                        ("order", next.clone()),
                        ("extra", info.clone()),
                    ]),
                    "recent retained fields and order",
                );
                let V::O(fields) = entry else {
                    panic!("local_write_native_recent_entry")
                };
                assert_eq!(
                    fields.borrow().len(),
                    3,
                    "local_write_native_recent_entry_fields"
                );
            }
        }
        assert_eq!(
            h.clock_calls.get(),
            usize::from(fallback),
            "local_write_native_update_clock {kind}"
        );
        assert_eq!(
            h.map_gets.get(),
            usize::from(fallback),
            "local_write_native_map_get_floor {kind}"
        );
        assert_eq!(
            h.map_sets.get(),
            usize::from(fallback && recent),
            "local_write_native_map_set_floor {kind}"
        );
        assert!(
            h.recorded.borrow().is_empty(),
            "local_write_native_update_no_record"
        );
        let V::O(entries) = &h.recent else {
            panic!("local_write_native_map")
        };
        assert_eq!(
            entries.borrow().len(),
            usize::from(recent),
            "local_write_native_recent_map_key_count {kind}"
        );
        let observed_recent: Vec<Json> = entries
            .borrow()
            .iter()
            .map(|(key, value)| json!([key, observe_local(value)]))
            .collect();
        updates.push(json!({"kind":kind,"patch":observe_local(&patch),"events":h.observed_events(),"recent":observed_recent,"passed":passed.as_ref().map(observe_local).unwrap_or_else(||observe_local(&V::U))}));
    }
    assert_eq!(updates.len(), 10, "local_write_native_update_floor");
    assert_eq!(
        cases.len() + supplements.len() + updates.len(),
        76,
        "local_write_native_all_profiles_before_source"
    );
    let mut dependency_profiles = vec![];
    {
        let kind = "strict-global-parse-float";
        let h = LocalHost::new(true);
        let callable = get(&h.inner.global, "parseFloat");
        assert!(
            matches!(callable, V::F(receiver, ref key) if matches!(receiver.as_ref(), V::U) && key == "parseFloat"),
            "local_write_native_raw_parse_binding"
        );
        let opts = base_opts();
        let payload = o(vec![
            ("qty", s("3.5")),
            ("price", s("4.25")),
            ("triggerPrice", s("99")),
        ]);
        api::record_recently_placed_order_from_opts(
            &h,
            &h.inner.owner,
            s("order-1"),
            opts.clone(),
            payload.clone(),
        )
        .unwrap();
        check_record(
            &h,
            3.5,
            4.25,
            &opts,
            true,
            OrderTimeInForce::GoodTillCancel,
            kind,
        );
        let calls = h.parse_calls.borrow();
        assert_eq!(calls.len(), 2, "local_write_native_dependency_parse_calls");
        assert!(
            calls.iter().all(|(bare, _)| *bare),
            "local_write_native_dependency_parse_receiver"
        );
        assert!(
            eq(&calls[0].1, &s("3.5")),
            "local_write_native_dependency_parse_qty"
        );
        assert!(
            eq(&calls[1].1, &s("4.25")),
            "local_write_native_dependency_parse_price"
        );
        let observed_calls: Vec<Json> = calls
            .iter()
            .map(|(bare, input)| json!({"receiverUndefined":bare,"input":observe_local(input)}))
            .collect();
        dependency_profiles.push(json!({"kind":kind,"payload":observe_local(&payload),"calls":observed_calls,"events":h.observed_events()}));
    }
    assert_eq!(
        dependency_profiles.len(),
        1,
        "local_write_native_dependency_profile_floor"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-bybit/scripts/local-order-write-source.cjs"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "local order write source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Json = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["declaredCases"], 56);
    assert_eq!(source["declaredDependencyCases"], 1);
    for (key, actual, count) in [
        ("cases", cases, 56),
        ("supplements", supplements, 10),
        ("updates", updates, 10),
        ("dependencyProfiles", dependency_profiles, 1),
    ] {
        assert_eq!(
            source[key].as_array().unwrap().len(),
            count,
            "local_write_source_fixed_floor {key}"
        );
        assert_eq!(
            Json::Array(actual),
            source[key],
            "local write supplemental comparison {key}"
        );
    }
}
