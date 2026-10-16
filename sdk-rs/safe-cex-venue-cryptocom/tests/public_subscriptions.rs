use safe_cex_venue_cryptocom::public_subscriptions::{
    self as bodies, ArraySlot, ChannelValue, OrderedStringSet, SubscriptionArray,
    SubscriptionFrame, SubscriptionState, SubscriptionTransport,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};

fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn text(v: &Value) -> Vec<u16> {
    if let Some(s) = v.as_str() {
        s.encode_utf16().collect()
    } else {
        units(&v["units"])
    }
}
fn input_slot(v: &Value) -> ArraySlot {
    if v["hole"] == true {
        ArraySlot::Hole
    } else if v["undefined"] == true {
        ArraySlot::Value(ChannelValue::Undefined)
    } else {
        ArraySlot::Value(ChannelValue::String(text(v)))
    }
}
fn observed_value(v: &Value) -> ChannelValue {
    match v["kind"].as_str().unwrap() {
        "undefined" => ChannelValue::Undefined,
        "string" => ChannelValue::String(units(&v["units"])),
        kind => panic!("unexpected native value {kind}"),
    }
}
fn observed_slots(v: &Value) -> Vec<ArraySlot> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|slot| match slot["kind"].as_str().unwrap() {
            "hole" => ArraySlot::Hole,
            "value" => ArraySlot::Value(observed_value(&slot["value"])),
            kind => panic!("unexpected native slot {kind}"),
        })
        .collect()
}
fn input(v: &Value) -> Vec<ArraySlot> {
    v.as_array()
        .map(|a| a.iter().map(input_slot).collect())
        .unwrap_or_default()
}
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("neg0") => -0.0,
        None => v.as_f64().unwrap(),
        other => panic!("unexpected number {other:?}"),
    }
}
fn observed_number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn native_number(actual: f64, expected: f64, label: &str) {
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-NUMBER {label}: actual {actual:?}, expected {expected:?}"
    )
}
fn string_value(s: &str) -> ChannelValue {
    ChannelValue::String(s.encode_utf16().collect())
}

struct ObservedArray<'a, 'b> {
    slots: &'a RefCell<Vec<ArraySlot>>,
    host: &'a Host<'b>,
}
impl ObservedArray<'_, '_> {
    fn main(&self) -> bool {
        std::ptr::eq(self.slots, self.host.input)
    }
    fn get(&self, index: usize) -> Result<ArraySlot, Rc<()>> {
        let mut slot = self
            .slots
            .borrow()
            .get(index)
            .cloned()
            .unwrap_or(ArraySlot::Hole);
        if self.main() {
            let effect = self.host.spec["arrayEffect"].as_str().unwrap_or("");
            let getter = self.host.spec["getterIndex"].as_u64().unwrap_or(0);
            if effect == "inherited" && index == 0 {
                self.host.event("array-get-0");
                slot = ArraySlot::Value(string_value("Inherited"));
            } else if effect.starts_with("index-") && u64::try_from(index).unwrap() == getter {
                self.host.event(&format!("array-get-{index}"));
                match effect {
                    "index-throw" => return Err(self.host.error.clone()),
                    "index-grow" => self
                        .slots
                        .borrow_mut()
                        .push(ArraySlot::Value(string_value("ETH"))),
                    "index-shrink" => self.slots.borrow_mut().truncate(1),
                    _ => unreachable!(),
                }
            }
        }
        Ok(slot)
    }
}
struct ObservedIterator<'s, 'a, 'b> {
    array: &'s ObservedArray<'a, 'b>,
    index: usize,
    custom: bool,
}
impl Iterator for ObservedIterator<'_, '_, '_> {
    type Item = Result<ChannelValue, Rc<()>>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.custom {
            self.array.host.event(&format!("next{}", self.index));
            self.index += 1;
            return Some(if self.index == 1 {
                Ok(string_value("CUSTOM"))
            } else {
                Err(self.array.host.error.clone())
            });
        }
        if self.index >= self.array.slots.borrow().len() {
            return None;
        }
        let slot = self.array.get(self.index);
        self.index += 1;
        Some(slot.map(|slot| match slot {
            ArraySlot::Hole => ChannelValue::Undefined,
            ArraySlot::Value(value) => value,
        }))
    }
}
impl<'a, 'b> SubscriptionArray<Rc<()>> for ObservedArray<'a, 'b> {
    type Slice = Vec<ArraySlot>;
    type Mapped = Vec<ArraySlot>;
    type Iter<'s>
        = ObservedIterator<'s, 'a, 'b>
    where
        Self: 's;
    fn length(&self) -> Result<f64, Rc<()>> {
        Ok(self.slots.borrow().len() as f64)
    }
    fn slice(&self, start: f64, end: f64) -> Result<Vec<ArraySlot>, Rc<()>> {
        let length = self.slots.borrow().len();
        ((start as usize).min(length)..(end as usize).min(length))
            .map(|i| self.get(i))
            .collect()
    }
    fn iterate(&self) -> Result<Self::Iter<'_>, Rc<()>> {
        let custom = self.main() && self.host.spec["arrayEffect"] == "iterator-throw";
        if custom {
            self.host.event("resolve-iterator");
            self.host.event("iterator");
        }
        Ok(ObservedIterator {
            array: self,
            index: 0,
            custom,
        })
    }
    fn map<F: FnMut(ChannelValue) -> ChannelValue>(
        &self,
        mut transform: F,
    ) -> Result<Vec<ArraySlot>, Rc<()>> {
        let length = self.slots.borrow().len();
        let mut result = Vec::with_capacity(length);
        for i in 0..length {
            result.push(match self.get(i)? {
                ArraySlot::Hole => ArraySlot::Hole,
                ArraySlot::Value(value) => ArraySlot::Value(transform(value)),
            });
        }
        Ok(result)
    }
}

struct Host<'a> {
    state: &'a SubscriptionState,
    input: &'a RefCell<Vec<ArraySlot>>,
    spec: &'a Value,
    error: Rc<()>,
    events: RefCell<Vec<String>>,
    frames: RefCell<Vec<SubscriptionFrame>>,
    helpers: RefCell<Vec<(String, Vec<ArraySlot>)>>,
    clock_ids: RefCell<Vec<f64>>,
    prepares: Cell<usize>,
    clocks: Cell<usize>,
    sends: Cell<usize>,
    prepared: RefCell<Vec<Rc<()>>>,
}
impl Host<'_> {
    fn event(&self, event: &str) {
        self.events.borrow_mut().push(event.into());
    }
    fn fail(&self, phase: &str, ordinal: usize) -> Result<(), Rc<()>> {
        if self.spec["errorPhase"].as_str() == Some(phase)
            && self.spec["failOn"].as_u64().unwrap_or(1) == u64::try_from(ordinal).unwrap()
        {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn resolve_frames(&self) -> Result<(), Rc<()>> {
        self.event("resolve-frames");
        self.fail("frames-get", 1)?;
        if let Some(channel) = self.spec["framesAdd"].as_str() {
            self.state
                .subscriptions
                .borrow()
                .insert(string_value(channel));
        }
        if self.spec.get("framesReplace").is_some() {
            let values = input(&self.spec["framesReplace"])
                .into_iter()
                .map(|slot| match slot {
                    ArraySlot::Hole => ChannelValue::Undefined,
                    ArraySlot::Value(value) => value,
                });
            self.state
                .subscriptions
                .replace(Rc::new(OrderedStringSet::new(values)));
        }
        Ok(())
    }
    fn send_frames(&self, _prepared: (), channels: Vec<ArraySlot>) -> Result<(), Rc<()>> {
        self.event("frames");
        self.helpers
            .borrow_mut()
            .push(("frames".into(), channels.clone()));
        self.fail("frames-call", 1)?;
        bodies::send_subscription_frames(self.state, &RefCell::new(channels), self)
    }
    fn resolve_register(&self) -> Result<(), Rc<()>> {
        self.event("resolve-register");
        self.fail("register-get", 1)?;
        if self.spec.get("registerGrow").is_some() {
            self.input
                .borrow_mut()
                .extend(input(&self.spec["registerGrow"]));
        }
        Ok(())
    }
    fn register(&self, _prepared: (), channels: Vec<ArraySlot>) -> Result<(), Rc<()>> {
        self.event("register");
        self.helpers
            .borrow_mut()
            .push(("register".into(), channels.clone()));
        self.fail("register-call", 1)?;
        bodies::register_and_subscribe(
            self.state,
            &RefCell::new(channels),
            || self.resolve_frames(),
            |p, c| self.send_frames(p, c),
        )
    }
}
impl SubscriptionTransport for Host<'_> {
    type Prepared = Rc<()>;
    type Error = Rc<()>;
    fn prepare(&self) -> Result<Rc<()>, Rc<()>> {
        let ordinal = self.prepares.get() + 1;
        self.prepares.set(ordinal);
        self.event("resolve-send");
        self.fail("send-get", ordinal)?;
        if self.spec["prepareShrink"] == true && ordinal == 1 {
            self.input.borrow_mut().clear();
        }
        self.event("resolve-json");
        self.fail("json-get", ordinal)?;
        let prepared = Rc::new(());
        self.prepared.borrow_mut().push(prepared.clone());
        Ok(prepared)
    }
    fn clock(&self) -> Result<f64, Rc<()>> {
        self.event("resolve-clock");
        self.fail("clock-get", self.clocks.get() + 1)?;
        self.event("clock");
        let ordinal = self.clocks.get() + 1;
        self.clocks.set(ordinal);
        self.clock_ids
            .borrow_mut()
            .push(self.state.message_id.get());
        self.fail("clock", ordinal)?;
        if ordinal == 1 {
            if self.spec.get("clockGrow").is_some() {
                self.input
                    .borrow_mut()
                    .extend(input(&self.spec["clockGrow"]));
            }
            if self.spec["clockShrink"] == true {
                self.input.borrow_mut().clear();
            }
            if let Some(value) = self.spec.get("clockSetId") {
                self.state.message_id.set(number(value));
            }
            if let Some(channel) = self.spec["clockAddSubscription"].as_str() {
                self.state
                    .subscriptions
                    .borrow()
                    .insert(string_value(channel));
            }
        }
        Ok(self
            .spec
            .get("clockValue")
            .map(number)
            .unwrap_or(1700000000000.0 + 13.0 * f64::from(u32::try_from(ordinal).unwrap())))
    }
    fn send(&self, prepared: Rc<()>, frame: SubscriptionFrame) -> Result<(), Rc<()>> {
        let index = self
            .prepared
            .borrow()
            .iter()
            .position(|p| Rc::ptr_eq(p, &prepared))
            .expect("SUBSCRIPTIONS native prepared callee identity");
        self.prepared.borrow_mut().remove(index);
        self.event("stringify");
        self.frames.borrow_mut().push(frame);
        self.fail("stringify", self.prepares.get())?;
        self.event("send");
        let ordinal = self.sends.get() + 1;
        self.sends.set(ordinal);
        self.fail("send", ordinal)?;
        if self.spec["sendReenter"] == true && ordinal == 1 {
            bodies::subscribe_to_ticker(
                self.state,
                &"REENTER".encode_utf16().collect::<Vec<_>>(),
                self,
            )?;
        }
        Ok(())
    }
}

#[test]
fn public_subscriptions_match_actual_typescript() {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("public_subscriptions_fixtures.json")).unwrap();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_subscriptions_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SUBSCRIPTIONS source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut compared = 0;
    for (index, spec) in fixtures.iter().enumerate() {
        let row = &rows[index];
        let name = spec["name"].as_str().unwrap();
        let channels = RefCell::new(if spec.get("input").is_some() {
            input(&spec["input"])
        } else {
            (0..spec["count"].as_u64().unwrap_or(0))
                .map(|i| ArraySlot::Value(string_value(&format!("channel-{i}"))))
                .collect()
        });
        let initial = input(&spec["initial"]).into_iter().map(|slot| match slot {
            ArraySlot::Hole => ChannelValue::Undefined,
            ArraySlot::Value(v) => v,
        });
        let state = SubscriptionState {
            subscriptions: RefCell::new(Rc::new(OrderedStringSet::new(initial))),
            message_id: Cell::new(spec.get("initialId").map(number).unwrap_or(1.0)),
        };
        let retained_set = state.subscriptions.borrow().clone();
        let host = Host {
            state: &state,
            input: &channels,
            spec,
            error: Rc::new(()),
            events: RefCell::new(Vec::new()),
            frames: RefCell::new(Vec::new()),
            helpers: RefCell::new(Vec::new()),
            clock_ids: RefCell::new(Vec::new()),
            prepares: Cell::new(0),
            clocks: Cell::new(0),
            sends: Cell::new(0),
            prepared: RefCell::new(Vec::new()),
        };
        let symbol = spec
            .get("symbol")
            .map(text)
            .unwrap_or_else(|| "BTC".encode_utf16().collect());
        let outcome = match spec["op"].as_str().unwrap() {
            "frames" => bodies::send_subscription_frames(
                &state,
                &ObservedArray {
                    slots: &channels,
                    host: &host,
                },
                &host,
            ),
            "register" => bodies::register_and_subscribe(
                &state,
                &ObservedArray {
                    slots: &channels,
                    host: &host,
                },
                || host.resolve_frames(),
                |p, c| host.send_frames(p, c),
            ),
            "replay" => bodies::resubscribe(
                &state,
                || host.resolve_frames(),
                |p, c| host.send_frames(p, c),
            ),
            "book" => bodies::subscribe_to_order_book(
                &state,
                &symbol,
                spec.get("depth").map(number),
                &host,
            ),
            "ticker" => bodies::subscribe_to_ticker(&state, &symbol, &host),
            "tickers" => bodies::subscribe_to_tickers(
                &ObservedArray {
                    slots: &channels,
                    host: &host,
                },
                || host.resolve_register(),
                |p, c| host.register(p, c),
            ),
            "trades" => bodies::subscribe_to_trades(&state, &symbol, &host),
            "candle" => {
                bodies::subscribe_to_candlestick(&state, &symbol, &text(&spec["interval"]), &host)
            }
            op => panic!("unexpected fixture {op}"),
        };
        match (row["result"].as_str().unwrap(), outcome) {
            ("void", Ok(())) => {}
            ("original", Err(error)) => assert!(
                Rc::ptr_eq(&error, &host.error),
                "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-ERROR {name}"
            ),
            (expected, actual) => panic!(
                "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-RETURN {name}: expected {expected}, actual {actual:?}"
            ),
        }
        let expected_state: Vec<_> = row["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(observed_value)
            .collect();
        assert_eq!(
            state.subscriptions.borrow().values(),
            expected_state,
            "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-STATE {name}"
        );
        let expected_retained: Vec<_> = row["retainedSubscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(observed_value)
            .collect();
        assert_eq!(
            retained_set.values(),
            expected_retained,
            "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-RETAINED-SET {name}"
        );
        native_number(
            state.message_id.get(),
            observed_number(&row["messageId"]),
            &format!("{name} messageId"),
        );
        let frames = host.frames.borrow();
        assert_eq!(
            u64::try_from(frames.len()).unwrap(),
            spec["expectedFrames"].as_u64().unwrap(),
            "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-FRAME-FLOOR {name}"
        );
        for (i, frame) in frames.iter().enumerate() {
            let expected = &row["frames"][i];
            native_number(
                frame.id,
                observed_number(&expected["id"]),
                &format!("{name} frame{i} id"),
            );
            assert_eq!(
                frame.method.encode_utf16().collect::<Vec<_>>(),
                units(&expected["method"]),
                "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-METHOD {name} frame{i}"
            );
            assert_eq!(
                frame.params.channels,
                observed_slots(&expected["channels"]),
                "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-CHANNELS {name} frame{i}"
            );
            native_number(
                frame.nonce,
                observed_number(&expected["nonce"]),
                &format!("{name} frame{i} nonce"),
            );
        }
        let expected_helpers: Vec<_> = row["helpers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| {
                (
                    h["kind"].as_str().unwrap().to_owned(),
                    observed_slots(&h["channels"]),
                )
            })
            .collect();
        assert_eq!(
            *host.helpers.borrow(),
            expected_helpers,
            "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-HELPERS {name}"
        );
        let clock_ids = host.clock_ids.borrow();
        let expected_ids = row["clockIds"].as_array().unwrap();
        assert_eq!(
            clock_ids.len(),
            expected_ids.len(),
            "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-CLOCK-CALLS {name}"
        );
        for (i, id) in clock_ids.iter().enumerate() {
            native_number(
                *id,
                observed_number(&expected_ids[i]),
                &format!("{name} clock{i} increment-before-clock"),
            );
        }
        let expected_events: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
        assert_eq!(
            *host.events.borrow(),
            expected_events,
            "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-EFFECTS {name}"
        );
        compared += 1;
    }
    // All native values/effects precede the independent inventories. No Rust wire encoder is used.
    assert_eq!(compared, 68, "SUBSCRIPTIONS independent native case floor");
    assert_eq!(
        fixtures.len(),
        68,
        "SUBSCRIPTIONS independent fixture floor"
    );
    assert_eq!(rows.len(), 68, "SUBSCRIPTIONS independent source floor");
    for (spec, row) in fixtures.iter().zip(rows.iter()) {
        assert_eq!(
            spec["name"], row["name"],
            "SUBSCRIPTIONS independent case id"
        );
    }
    assert_eq!(
        fixtures[67]["name"], "replay-replace-set",
        "SUBSCRIPTIONS final floor index67"
    );
    println!("SUBSCRIPTIONS floor: 68 native cases; last case67 replay-replace-set");
}

#[test]
fn opaque_array_map_result_retains_identity() {
    let output = Command::new("node")
        .args([
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_subscriptions_oracle.cjs"
            ),
            "--opaque-map",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SUBSCRIPTIONS opaque source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    struct OpaqueArray<'a> {
        payload: Rc<()>,
        events: &'a RefCell<Vec<String>>,
    }
    impl SubscriptionArray<Rc<()>> for OpaqueArray<'_> {
        type Slice = Rc<()>;
        type Mapped = Rc<()>;
        type Iter<'a>
            = std::iter::Empty<Result<ChannelValue, Rc<()>>>
        where
            Self: 'a;
        fn length(&self) -> Result<f64, Rc<()>> {
            panic!("opaque map touched length")
        }
        fn slice(&self, _start: f64, _end: f64) -> Result<Rc<()>, Rc<()>> {
            panic!("opaque map touched slice")
        }
        fn iterate(&self) -> Result<Self::Iter<'_>, Rc<()>> {
            panic!("opaque map touched iterator")
        }
        fn map<F: FnMut(ChannelValue) -> ChannelValue>(
            &self,
            _transform: F,
        ) -> Result<Rc<()>, Rc<()>> {
            self.events
                .borrow_mut()
                .extend(["resolve-map".into(), "map".into()]);
            Ok(self.payload.clone())
        }
    }
    let events = RefCell::new(Vec::new());
    let payload = Rc::new(());
    let registered = Cell::new(false);
    let array = OpaqueArray {
        payload: payload.clone(),
        events: &events,
    };
    let result = bodies::subscribe_to_tickers(
        &array,
        || {
            events.borrow_mut().push("resolve-register".into());
            Ok(())
        },
        |(), argument| {
            assert!(
                Rc::ptr_eq(&argument, &payload),
                "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-OPAQUE-MAP-IDENTITY"
            );
            events.borrow_mut().push("register".into());
            registered.set(true);
            Ok(Rc::new(()))
        },
    );
    assert!(
        result.is_ok(),
        "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-OPAQUE-RETURN"
    );
    assert_eq!(
        registered.get(),
        source["registered"].as_bool().unwrap(),
        "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-OPAQUE-REGISTERED"
    );
    assert_eq!(
        *events.borrow(),
        serde_json::from_value::<Vec<String>>(source["events"].clone()).unwrap(),
        "CRYPTOCOM-SUBSCRIPTIONS-NATIVE-OPAQUE-EFFECTS"
    );
    assert!(
        registered.get(),
        "SUBSCRIPTIONS independent opaque identity case floor1"
    );
    println!("SUBSCRIPTIONS opaque identity floor: one case, stopped at index0 map-result");
}
