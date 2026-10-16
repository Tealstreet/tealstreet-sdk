use safe_cex_venue_cryptocom::{
    private_lifecycle::UnsubscribeChannels,
    public_lifecycle as bodies,
    public_subscriptions::{
        ArraySlot, ChannelValue, OrderedStringSet, SubscriptionFrame, SubscriptionState,
        SubscriptionTransport,
    },
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
fn observed_number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn native_number(actual: f64, expected: f64, label: &str) {
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-NUMBER {label}: actual {actual:?}, expected {expected:?}"
    )
}
fn string_value(s: &str) -> ChannelValue {
    ChannelValue::String(s.encode_utf16().collect())
}

type Error = Rc<()>;
type State = bodies::PublicSocketState<&'static str, &'static str>;
type Callback = Box<dyn Fn() -> Result<(), Error>>;
#[derive(Clone, Debug, PartialEq)]
enum CallbackResult {
    Bool(bool),
    Text(Vec<u16>),
}
#[derive(Clone)]
struct NativeFrame {
    id: f64,
    method: &'static str,
    channels: Vec<ArraySlot>,
    nonce: f64,
}
#[derive(Clone)]
struct Ticker {
    id: &'static str,
}
type IndexEntries = Vec<(Vec<u16>, Rc<OrderedStringSet>)>;
type IndexValues = Vec<(Vec<u16>, Vec<ChannelValue>)>;
type IdCursor = Rc<RefCell<Vec<Option<ChannelValue>>>>;
struct Snapshot {
    subscriptions: Vec<ChannelValue>,
    retained: Vec<ChannelValue>,
    index_map: IndexValues,
    retained_map: IndexValues,
    retained_sets: IndexValues,
    id: f64,
    disposed: bool,
    timer: Option<&'static str>,
    retained_timer: Option<&'static str>,
    socket: Option<&'static str>,
    events: Vec<String>,
    frames: Vec<NativeFrame>,
    registered: Vec<Vec<ArraySlot>>,
    warnings: Vec<(Vec<u16>, &'static str)>,
    urls_used: Vec<Vec<u16>>,
    listeners: Vec<(&'static str, &'static str)>,
    closed: Vec<&'static str>,
    patches: Vec<(&'static str, bodies::ValuationPatch)>,
    calls: Vec<usize>,
    callback_results: Vec<CallbackResult>,
    input: Vec<ArraySlot>,
}
struct Host {
    spec: Value,
    state: Rc<State>,
    input: Rc<RefCell<Vec<ArraySlot>>>,
    retained: Rc<OrderedStringSet>,
    retained_map: Rc<bodies::IndexMarketMap>,
    retained_sets: IndexEntries,
    error: Error,
    prepared: Rc<()>,
    urls: Rc<()>,
    environment: Cell<&'static str>,
    old_timer: Option<Rc<&'static str>>,
    new_timer: Rc<&'static str>,
    old_socket: Rc<&'static str>,
    new_socket: Rc<&'static str>,
    replacement_socket: Rc<&'static str>,
    events: RefCell<Vec<String>>,
    frames: RefCell<Vec<NativeFrame>>,
    registered: RefCell<Vec<Vec<ArraySlot>>>,
    warnings: RefCell<Vec<(Vec<u16>, &'static str)>>,
    urls_used: RefCell<Vec<Vec<u16>>>,
    listeners: RefCell<Vec<(&'static str, &'static str)>>,
    closed: RefCell<Vec<&'static str>>,
    callback_results: RefCell<Vec<CallbackResult>>,
    patches: RefCell<Vec<(&'static str, bodies::ValuationPatch)>>,
    calls: RefCell<Vec<usize>>,
    callback: RefCell<Option<Callback>>,
    clock_count: Cell<u32>,
    models: Vec<Rc<bodies::CryptocomMarketDataSubscription>>,
    iteration: Cell<usize>,
    filtered: RefCell<Option<Rc<Vec<usize>>>>,
    channel_count: Cell<usize>,
    valuation_ids: Rc<OrderedStringSet>,
    cursor: RefCell<Option<IdCursor>>,
    tickers: Vec<Rc<Ticker>>,
    callbacks: RefCell<Vec<Option<usize>>>,
    update: Rc<()>,
}
impl Host {
    fn event(&self, phase: &str) -> Result<(), Error> {
        self.events.borrow_mut().push(phase.into());
        if self.spec["fail"] == phase {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn current_set(&self) -> Rc<OrderedStringSet> {
        self.state.channels.subscriptions.borrow().clone()
    }
    fn current_map(&self) -> Rc<bodies::IndexMarketMap> {
        self.state.index_markets.borrow().clone()
    }
    fn clear(&self, timer: Option<Rc<&'static str>>) -> Result<(), Error> {
        assert!(
            match (&timer, &self.old_timer) {
                (None, None) => true,
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                _ => false,
            },
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-OLD-TIMER-IDENTITY"
        );
        self.event("clear")?;
        if self.spec["replaceSocketAtClear"] == true {
            self.state.ws.replace(Some(self.replacement_socket.clone()));
        }
        if self.spec["clearSocketAtClear"] == true {
            self.state.ws.replace(None);
        }
        if self.spec["clearSetAtClear"] == true {
            self.current_set().clear();
        }
        if let Some(v) = self.spec["addSubscriptionAtClear"].as_str() {
            self.current_set().insert(string_value(v));
        }
        Ok(())
    }
    fn prepare(&self) -> Result<Rc<()>, Error> {
        self.event("resolve-send")?;
        self.event("resolve-json")?;
        Ok(self.prepared.clone())
    }
    fn clock(&self) -> Result<f64, Error> {
        self.event("clock")?;
        let n = self.clock_count.get() + 1;
        self.clock_count.set(n);
        self.input
            .borrow_mut()
            .extend(input(&self.spec["growAtClock"]));
        Ok(1700000000000.0 + 13.0 * f64::from(n))
    }
    fn send_effects(&self) -> Result<(), Error> {
        self.event("send")?;
        self.input
            .borrow_mut()
            .extend(input(&self.spec["growAtSend"]));
        if self.spec["replaceSetAtSend"].is_array() {
            self.state
                .channels
                .subscriptions
                .replace(Rc::new(OrderedStringSet::new(
                    input(&self.spec["replaceSetAtSend"])
                        .into_iter()
                        .map(slot_value),
                )));
        }
        Ok(())
    }
    fn expected_socket(&self, tag: &str) -> &Rc<&'static str> {
        match tag {
            "old-socket" => &self.old_socket,
            "new-socket" => &self.new_socket,
            "replacement-socket" => &self.replacement_socket,
            _ => panic!("unknown native socket"),
        }
    }
    fn snapshot(&self) -> Snapshot {
        let timer = self.state.reconnect_subscription_timer.borrow().clone();
        if let Some(timer) = &timer {
            let expected = if **timer == "old-timer" {
                self.old_timer.as_ref().unwrap()
            } else {
                &self.new_timer
            };
            assert!(
                Rc::ptr_eq(timer, expected),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-TIMER-IDENTITY"
            );
        }
        let socket = self.state.ws.borrow().clone();
        if let Some(socket) = &socket {
            assert!(
                Rc::ptr_eq(socket, self.expected_socket(socket)),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-SOCKET-IDENTITY"
            );
        }
        Snapshot {
            subscriptions: self.current_set().values(),
            retained: self.retained.values(),
            index_map: map_values(self.current_map().entries()),
            retained_map: map_values(self.retained_map.entries()),
            retained_sets: map_values(self.retained_sets.clone()),
            id: self.state.channels.message_id.get(),
            disposed: self.state.is_disposed.get(),
            timer: timer.as_ref().map(|v| **v),
            retained_timer: self.old_timer.as_ref().map(|v| **v),
            socket: socket.as_ref().map(|v| **v),
            events: self.events.borrow().clone(),
            frames: self.frames.borrow().clone(),
            registered: self.registered.borrow().clone(),
            warnings: self.warnings.borrow().clone(),
            urls_used: self.urls_used.borrow().clone(),
            listeners: self.listeners.borrow().clone(),
            closed: self.closed.borrow().clone(),
            patches: self.patches.borrow().clone(),
            calls: self.calls.borrow().clone(),
            callback_results: self.callback_results.borrow().clone(),
            input: self.input.borrow().clone(),
        }
    }
}
fn slot_value(slot: ArraySlot) -> ChannelValue {
    match slot {
        ArraySlot::Hole => ChannelValue::Undefined,
        ArraySlot::Value(v) => v,
    }
}
fn map_values(entries: IndexEntries) -> IndexValues {
    entries.into_iter().map(|(k, v)| (k, v.values())).collect()
}
impl bodies::PublicConnectionHost<&'static str> for Host {
    type Error = Error;
    type Urls = Rc<()>;
    fn testnet(&self) -> Result<bool, Error> {
        self.event("testnet")?;
        Ok(match &self.spec["testnet"] {
            Value::Null => false,
            Value::Bool(v) => *v,
            Value::Number(n) => n.as_f64().unwrap() != 0.0,
            Value::String(v) => !v.is_empty(),
            _ => true,
        })
    }
    fn urls(&self, environment: &'static str) -> Result<Rc<()>, Error> {
        self.event("urls")?;
        self.environment.set(environment);
        Ok(self.urls.clone())
    }
    fn create(&self, urls: Rc<()>) -> Result<Rc<&'static str>, Error> {
        assert!(
            Rc::ptr_eq(&urls, &self.urls),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-URL-IDENTITY"
        );
        self.event("resolve-create")?;
        self.event("market-url")?;
        self.event("create")?;
        let url = if self.environment.get() == "TESTNET" {
            "wss://uat-stream.3ona.co/exchange/v1/market"
        } else {
            "wss://stream.crypto.com/exchange/v1/market"
        };
        self.urls_used
            .borrow_mut()
            .push(url.encode_utf16().collect());
        Ok(self.new_socket.clone())
    }
    fn add_listener(
        &self,
        socket: Option<Rc<&'static str>>,
        event: &'static str,
    ) -> Result<(), Error> {
        let socket = socket.unwrap();
        assert!(
            Rc::ptr_eq(&socket, self.expected_socket(&socket)),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-LISTENER-RECEIVER"
        );
        self.event(&format!("resolve-add-{event}"))?;
        self.event(&format!("listener-{event}"))?;
        self.event(&format!("add-{event}"))?;
        self.listeners.borrow_mut().push((*socket, event));
        if self.spec["replaceOnListener"] == true && event == "open" {
            self.state.ws.replace(Some(self.replacement_socket.clone()));
        }
        Ok(())
    }
}
impl bodies::PublicTimerHost<&'static str> for Host {
    type Error = Error;
    type ResubscribeReturn = Rc<()>;
    fn track_last_received(&self) -> Result<(), Error> {
        self.event("track")?;
        if self.spec["disposeDuringTrack"] == true {
            self.state.is_disposed.set(true);
        }
        Ok(())
    }
    fn clear_timeout(&self, timer: Option<Rc<&'static str>>) -> Result<(), Error> {
        self.clear(timer)
    }
    fn set_timeout<F: Fn() -> Result<(), Error> + 'static>(
        &self,
        callback: F,
        delay: f64,
    ) -> Result<Rc<&'static str>, Error> {
        self.event("schedule")?;
        assert_eq!(delay, 1000.0, "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-DELAY");
        self.callback.replace(Some(Box::new(callback)));
        if self.spec["synchronousTimer"] == true {
            self.callback.borrow().as_ref().unwrap()()?;
        }
        Ok(self.new_timer.clone())
    }
    fn resubscribe(&self) -> Result<Rc<()>, Error> {
        self.event("timer-resubscribe-get")?;
        self.event("timer-resubscribe-call")?;
        Ok(self.prepared.clone())
    }
}
impl bodies::PublicDisposeHost<&'static str, &'static str> for Host {
    type Error = Error;
    fn clear_timeout(&self, timer: Option<Rc<&'static str>>) -> Result<(), Error> {
        self.clear(timer)
    }
    fn close(&self, socket: Option<Rc<&'static str>>) -> Result<(), Error> {
        let socket = socket.unwrap();
        assert!(
            Rc::ptr_eq(&socket, self.expected_socket(&socket)),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CLOSE-RECEIVER"
        );
        self.event("resolve-close")?;
        self.event("close")?;
        self.closed.borrow_mut().push(*socket);
        Ok(())
    }
}
impl SubscriptionTransport<Rc<RefCell<Vec<ArraySlot>>>> for Host {
    type Prepared = Rc<()>;
    type Error = Error;
    fn prepare(&self) -> Result<Rc<()>, Error> {
        Host::prepare(self)
    }
    fn clock(&self) -> Result<f64, Error> {
        Host::clock(self)
    }
    fn send(
        &self,
        prepared: Rc<()>,
        frame: SubscriptionFrame<Rc<RefCell<Vec<ArraySlot>>>>,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&prepared, &self.prepared),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-PREPARED-IDENTITY"
        );
        assert!(
            Rc::ptr_eq(&frame.params.channels, &self.input),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CHANNEL-ARRAY-IDENTITY"
        );
        self.events.borrow_mut().push("stringify".into());
        let channels = frame.params.channels.borrow().clone();
        self.frames.borrow_mut().push(NativeFrame {
            id: frame.id,
            method: frame.method,
            channels,
            nonce: frame.nonce,
        });
        if self.spec["fail"] == "stringify" {
            return Err(self.error.clone());
        }
        self.send_effects()
    }
}
struct Channels<'a>(&'a Host);
impl UnsubscribeChannels<Error> for Channels<'_> {
    type Native = Rc<RefCell<Vec<ArraySlot>>>;
    fn length(&self) -> Result<f64, Error> {
        Ok(self.0.input.borrow().len() as f64)
    }
    fn native(&self) -> Self::Native {
        self.0.input.clone()
    }
    fn for_each<F: FnMut(ChannelValue) -> Result<bool, Error>>(
        &self,
        mut callback: F,
    ) -> Result<(), Error> {
        let h = self.0;
        if h.spec["customForeach"] == true {
            h.event("resolve-foreach")?;
            h.event("foreach")?;
        }
        let len = h.input.borrow().len();
        for i in 0..len {
            if h.spec["foreachGetterThrow"] == true && i == 1 {
                h.event("foreach-get-1")?;
                return Err(h.error.clone());
            }
            let slot = h.input.borrow().get(i).cloned().unwrap_or(ArraySlot::Hole);
            if let ArraySlot::Value(v) = slot {
                let out = callback(v)?;
                if h.spec["customForeach"] == true {
                    h.callback_results
                        .borrow_mut()
                        .push(CallbackResult::Bool(out));
                }
            }
        }
        Ok(())
    }
}
struct MarketIter<'a> {
    host: &'a Host,
    index: usize,
    iteration: usize,
    done: bool,
}
impl Iterator for MarketIter<'_> {
    type Item = Result<usize, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        self.host
            .events
            .borrow_mut()
            .push(format!("next-{}-{}", self.iteration, self.index));
        if self.index < self.host.models.len() {
            let i = self.index;
            self.index += 1;
            Some(Ok(i))
        } else {
            self.done = true;
            None
        }
    }
}
impl Drop for MarketIter<'_> {
    fn drop(&mut self) {
        if !self.done && self.host.spec["iteratorClose"] == true {
            self.host
                .events
                .borrow_mut()
                .push(format!("iterator-return-{}", self.iteration)); // Source body error wins over a throwing custom return().
        }
    }
}
impl bodies::MarketSubscriptions<Error> for Host {
    type Item = usize;
    type Iter<'a> = MarketIter<'a>;
    fn iterate(&self) -> Result<MarketIter<'_>, Error> {
        let iteration = self.iteration.get() + 1;
        self.iteration.set(iteration);
        self.event(&format!("iterator-get-{iteration}"))?;
        self.event(&format!("iterator-{iteration}"))?;
        Ok(MarketIter {
            host: self,
            index: 0,
            iteration,
            done: false,
        })
    }
    fn instrument_name(&self, item: &usize) -> Result<Vec<u16>, Error> {
        self.event("instrument-get")?;
        Ok(
            if self.iteration.get() == 2 && !self.spec["secondPassInstrument"].is_null() {
                text(&self.spec["secondPassInstrument"])
            } else {
                self.models[*item].instrument_name.clone()
            },
        )
    }
    fn index_instrument_name(&self, item: &usize) -> Result<Option<Vec<u16>>, Error> {
        self.event("index-get")?;
        Ok(self.models[*item].index_instrument_name.clone())
    }
}
impl bodies::MarketSubscriptionHost for Host {
    type Error = Error;
    type Registered = Rc<()>;
    fn log_warning(&self, message: Vec<u16>) -> Result<(), Error> {
        self.event("resolve-log")?;
        self.event("log")?;
        self.warnings.borrow_mut().push((message, "warning"));
        Ok(())
    }
    fn register(&self, channels: Vec<ArraySlot>) -> Result<Rc<()>, Error> {
        self.event("resolve-register")?;
        self.registered.borrow_mut().push(channels);
        self.event("register")?;
        if let Some(v) = self.spec["registerAdd"].as_str() {
            self.current_set().insert(string_value(v));
        }
        Ok(self.prepared.clone())
    }
}
impl bodies::MarketCatalog<Error> for Host {
    type Item = usize;
    type Filtered = Rc<Vec<usize>>;
    type Mapped = Vec<ChannelValue>;
    type Iter = std::vec::IntoIter<Result<ChannelValue, Error>>;
    fn filter<F: FnMut(&usize) -> Result<bool, Error>>(
        &self,
        mut predicate: F,
    ) -> Result<Rc<Vec<usize>>, Error> {
        self.event("resolve-filter")?;
        self.event("filter")?;
        let mut selected = vec![];
        if let Some(catalog) = self.spec["catalog"].as_array() {
            for (i, m) in catalog.iter().enumerate() {
                if m["hole"] != true && predicate(&i)? {
                    selected.push(i);
                }
            }
        }
        let filtered = Rc::new(selected);
        self.filtered.replace(Some(filtered.clone()));
        Ok(filtered)
    }
    fn underlying_matches(&self, item: &usize, index: &[u16]) -> Result<bool, Error> {
        let m = &self.spec["catalog"][*item];
        if m["noInfo"] == true {
            return Ok(false);
        }
        self.event("underlying-get")?;
        if self.spec["replaceMapDuringFilter"] == true {
            self.state
                .index_markets
                .replace(Rc::new(bodies::IndexMarketMap::default()));
        }
        Ok(!m["underlying"].is_null() && text(&m["underlying"]) == index)
    }
    fn map<F: FnMut(&usize) -> Result<ChannelValue, Error>>(
        &self,
        filtered: Rc<Vec<usize>>,
        mut transform: F,
    ) -> Result<Vec<ChannelValue>, Error> {
        assert!(
            Rc::ptr_eq(&filtered, self.filtered.borrow().as_ref().unwrap()),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-FILTERED-IDENTITY"
        );
        self.event("resolve-map")?;
        self.event("map")?;
        let mut mapped = vec![];
        for i in &*filtered {
            mapped.push(transform(i)?);
        }
        if self.spec["customMapped"].is_array() {
            mapped = input(&self.spec["customMapped"])
                .into_iter()
                .map(slot_value)
                .collect();
        }
        Ok(mapped)
    }
    fn market_id(&self, item: &usize) -> Result<ChannelValue, Error> {
        self.event("id-get")?;
        Ok(slot_value(input_slot(&self.spec["catalog"][*item]["id"])))
    }
    fn iterate(&self, mapped: Vec<ChannelValue>) -> Result<Self::Iter, Error> {
        Ok(mapped.into_iter().map(Ok).collect::<Vec<_>>().into_iter())
    }
}
struct IndexIter<'a> {
    host: &'a Host,
    slots: IdCursor,
    index: usize,
    done: bool,
}
impl Iterator for IndexIter<'_> {
    type Item = Result<ChannelValue, Error>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.host.spec["iteratorClose"] == true {
            self.host.events.borrow_mut().push("ids-next".into());
        }
        loop {
            let slot = self.slots.borrow().get(self.index).cloned();
            self.index += 1;
            match slot {
                Some(Some(v)) => return Some(Ok(v)),
                Some(None) => {}
                None => {
                    self.done = true;
                    return None;
                }
            }
        }
    }
}
impl Drop for IndexIter<'_> {
    fn drop(&mut self) {
        if !self.done && self.host.spec["iteratorClose"] == true {
            self.host.events.borrow_mut().push("ids-return".into());
        }
    }
}
impl bodies::ValuationHost for Host {
    type Error = Error;
    type Prepared = Rc<()>;
    type Ids = Rc<OrderedStringSet>;
    type Iter<'a> = IndexIter<'a>;
    type Market = &'static str;
    type Ticker = Rc<Ticker>;
    fn raw_value(&self) -> Result<bodies::ValuationValue, Error> {
        if self.spec["emptyData"] == true {
            return Ok(bodies::ValuationValue::Other);
        }
        self.event("raw-get")?;
        Ok(
            if self.spec["raw"].is_string() || self.spec["raw"]["units"].is_array() {
                bodies::ValuationValue::String(text(&self.spec["raw"]))
            } else {
                bodies::ValuationValue::Other
            },
        )
    }
    fn channel(&self) -> Result<Vec<u16>, Error> {
        self.event("channel-get")?;
        let i = self.channel_count.get();
        let v = self.spec["channelSequence"]
            .get(i)
            .filter(|v| !v.is_null())
            .unwrap_or(&self.spec["valuationChannel"]);
        if self.spec["channelSequence"].is_array() {
            self.channel_count.set(i + 1);
        }
        Ok(text(v))
    }
    fn prepare_index_ids(&self) -> Result<Rc<()>, Error> {
        self.event("resolve-index")?;
        Ok(self.prepared.clone())
    }
    fn instrument_name(&self) -> Result<Vec<u16>, Error> {
        self.event("instrument-name-get")?;
        Ok(if self.spec["valuationChannel"] == "index" {
            "IDX"
        } else {
            "BTC"
        }
        .encode_utf16()
        .collect())
    }
    fn index_ids(&self, prepared: Rc<()>, index: Vec<u16>) -> Result<Rc<OrderedStringSet>, Error> {
        assert!(
            Rc::ptr_eq(&prepared, &self.prepared),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-INDEX-CALLEE-IDENTITY"
        );
        assert_eq!(
            index,
            if self.spec["valuationChannel"] == "index" {
                "IDX"
            } else {
                "BTC"
            }
            .encode_utf16()
            .collect::<Vec<_>>(),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-INDEX-ARGUMENT"
        );
        self.event("index-call")?;
        Ok(self.valuation_ids.clone())
    }
    fn iterate_ids(&self, ids: Rc<OrderedStringSet>) -> Result<IndexIter<'_>, Error> {
        assert!(
            Rc::ptr_eq(&ids, &self.valuation_ids),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-ID-SET-IDENTITY"
        );
        if self.spec["iteratorClose"] == true {
            self.event("ids-iterator-get")?;
            self.event("ids-iterator")?;
        }
        let slots = Rc::new(RefCell::new(
            ids.values().into_iter().map(Some).collect::<Vec<_>>(),
        ));
        self.cursor.replace(Some(slots.clone()));
        Ok(IndexIter {
            host: self,
            slots,
            index: 0,
            done: false,
        })
    }
    fn market(&self, id: ChannelValue) -> Result<Option<&'static str>, Error> {
        self.event("market-get")?;
        let id = match id {
            ChannelValue::String(v) if v == "BTC".encode_utf16().collect::<Vec<_>>() => "BTC",
            ChannelValue::String(v) if v == "ETH".encode_utf16().collect::<Vec<_>>() => "ETH",
            ChannelValue::String(v) if v == "LATE".encode_utf16().collect::<Vec<_>>() => "LATE",
            _ => return Ok(None),
        };
        Ok(
            if self.spec["missingMarket"] == true && id == "BTC"
                || self.spec["missingIndexMarket"] == true && id == "ETH"
            {
                None
            } else {
                Some(id)
            },
        )
    }
    fn market_from_update(&self) -> Result<Option<&'static str>, Error> {
        self.event("instrument-name-get")?;
        bodies::ValuationHost::market(
            self,
            string_value(if self.spec["valuationChannel"] == "index" {
                "IDX"
            } else {
                "BTC"
            }),
        )
    }
    fn ticker(&self, market: &'static str) -> Result<Option<Rc<Ticker>>, Error> {
        self.event("symbol-get")?;
        self.event("ticker-get")?;
        Ok(
            if self.spec["missingTicker"] == true && market == "BTC"
                || self.spec["missingIndexTicker"] == true && market == "ETH"
            {
                None
            } else {
                self.tickers.iter().find(|t| t.id == market).cloned()
            },
        )
    }
    fn update(&self, ticker: Rc<Ticker>, patch: bodies::ValuationPatch) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(
                &ticker,
                self.tickers.iter().find(|t| t.id == ticker.id).unwrap()
            ),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-TICKER-IDENTITY"
        );
        self.event("resolve-update")?;
        self.event("update")?;
        self.patches.borrow_mut().push((ticker.id, patch));
        if self.patches.borrow().len() == 1 {
            if self.spec["growIndexOnUpdate"] == true {
                self.valuation_ids.insert(string_value("LATE"));
                if let Some(cursor) = self.cursor.borrow().as_ref() {
                    cursor.borrow_mut().push(Some(string_value("LATE")));
                }
            }
            if self.spec["deleteIndexOnUpdate"] == true {
                self.valuation_ids.remove(&string_value("ETH"));
                if let Some(cursor) = self.cursor.borrow().as_ref() {
                    for slot in &mut *cursor.borrow_mut() {
                        if slot.as_ref() == Some(&string_value("ETH")) {
                            *slot = None;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
impl bodies::CandlestickHost<Rc<()>> for Host {
    type Error = Error;
    type Callbacks = Rc<()>;
    type Callback = usize;
    type Return = Vec<u16>;
    fn subscription(&self, update: &Rc<()>) -> Result<Vec<u16>, Error> {
        assert!(Rc::ptr_eq(update, &self.update));
        self.event("subscription-get")?;
        Ok("candlestick.1m.BTC".encode_utf16().collect())
    }
    fn callbacks(&self, handler: Vec<u16>) -> Result<Rc<()>, Error> {
        assert_eq!(
            handler,
            "candlestick.1m.BTC".encode_utf16().collect::<Vec<_>>()
        );
        self.event("handlers-get")?;
        Ok(self.prepared.clone())
    }
    fn for_each<F: FnMut(usize) -> Result<Vec<u16>, Error>>(
        &self,
        callbacks: Rc<()>,
        mut callback: F,
    ) -> Result<(), Error> {
        assert!(
            Rc::ptr_eq(&callbacks, &self.prepared),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CALLBACKS-IDENTITY"
        );
        if self.spec["customForeach"] == true {
            self.event("resolve-foreach")?;
            self.event("foreach")?;
        }
        let len = self.callbacks.borrow().len();
        for i in 0..len {
            let item = self.callbacks.borrow().get(i).copied().flatten();
            if let Some(item) = item {
                let result = callback(item)?;
                if self.spec["customForeach"] == true {
                    self.callback_results
                        .borrow_mut()
                        .push(CallbackResult::Text(result));
                }
            }
        }
        Ok(())
    }
    fn call(&self, callback: usize, update: &Rc<()>) -> Result<Vec<u16>, Error> {
        assert!(
            Rc::ptr_eq(update, &self.update),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-UPDATE-IDENTITY"
        );
        self.event(&format!("callback-{callback}"))?;
        self.calls.borrow_mut().push(callback);
        if self.spec["growCallbacks"] == true && callback == 0 {
            self.callbacks.borrow_mut().push(Some(99));
        }
        Ok(format!("return-{callback}").encode_utf16().collect())
    }
}
fn observed_map(v: &Value) -> IndexValues {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                units(&r["key"]),
                r["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(observed_value)
                    .collect(),
            )
        })
        .collect()
}
fn observe_snapshot(actual: &Snapshot, expected: &Value, name: &str, phase: &str) {
    let label = format!("{name} {phase}");
    assert_eq!(
        actual.subscriptions,
        expected["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(observed_value)
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-STATE {label}"
    );
    assert_eq!(
        actual.retained,
        expected["retainedSubscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .map(observed_value)
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-RETAINED-SET {label}"
    );
    assert_eq!(
        actual.index_map,
        observed_map(&expected["indexMap"]),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-INDEX-MAP {label}"
    );
    assert_eq!(
        actual.retained_map,
        observed_map(&expected["retainedMap"]),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-RETAINED-MAP {label}"
    );
    assert_eq!(
        actual.retained_sets,
        observed_map(&expected["retainedSets"]),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-RETAINED-INDEX-SETS {label}"
    );
    native_number(actual.id, observed_number(&expected["messageId"]), &label);
    assert_eq!(
        actual.disposed,
        expected["disposed"].as_bool().unwrap(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-DISPOSED {label}"
    );
    assert_eq!(
        actual.timer,
        expected["timer"].as_str(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-TIMER {label}"
    );
    assert_eq!(
        actual.retained_timer,
        expected["retainedTimer"].as_str(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-RETAINED-TIMER {label}"
    );
    assert_eq!(
        actual.socket,
        expected["socket"].as_str(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-SOCKET {label}"
    );
    for (i, (f, s)) in actual
        .frames
        .iter()
        .zip(expected["frames"].as_array().unwrap())
        .enumerate()
    {
        native_number(
            f.id,
            observed_number(&s["id"]),
            &format!("{label} frame{i} id"),
        );
        assert_eq!(
            f.method.encode_utf16().collect::<Vec<_>>(),
            units(&s["method"]),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-METHOD {label}"
        );
        assert_eq!(
            f.channels,
            observed_slots(&s["channels"]),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CHANNELS {label}"
        );
        native_number(
            f.nonce,
            observed_number(&s["nonce"]),
            &format!("{label} frame{i} nonce"),
        );
    }
    for (i, (channels, s)) in actual
        .registered
        .iter()
        .zip(expected["registered"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            *channels,
            observed_slots(s),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-REGISTERED {label} registration{i}"
        );
    }
    for ((message, severity), source) in actual
        .warnings
        .iter()
        .zip(expected["warnings"].as_array().unwrap())
    {
        assert_eq!(
            *message,
            units(&source["message"]),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-WARNING {label}"
        );
        assert_eq!(
            *severity,
            source["severity"].as_str().unwrap(),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-WARNING-SEVERITY {label}"
        );
    }
    assert_eq!(
        actual.warnings.len(),
        expected["warnings"].as_array().unwrap().len(),
        "PUBLIC-LIFECYCLE warning inventory {label}"
    );
    assert_eq!(
        actual.urls_used,
        expected["urlsUsed"]
            .as_array()
            .unwrap()
            .iter()
            .map(units)
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-URL {label}"
    );
    for (i, ((ticker, patch), s)) in actual
        .patches
        .iter()
        .zip(expected["patches"].as_array().unwrap())
        .enumerate()
    {
        assert_eq!(
            *ticker,
            s["ticker"].as_str().unwrap(),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-PATCH-TICKER {label} patch{i}"
        );
        let (key, value) = match patch {
            bodies::ValuationPatch::Mark(v) => ("mark", *v),
            bodies::ValuationPatch::Index(v) => ("index", *v),
            bodies::ValuationPatch::FundingRate(v) => ("fundingRate", *v),
        };
        assert_eq!(
            key,
            s["fields"][0][0].as_str().unwrap(),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-PATCH-FIELD {label} patch{i}"
        );
        native_number(
            value,
            observed_number(&s["fields"][0][1]),
            &format!("{label} patch{i}"),
        );
        assert_eq!(
            s["fields"].as_array().unwrap().len(),
            1,
            "independent valuation patch field floor"
        );
    }
    assert_eq!(
        actual.calls,
        expected["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CALLS {label}"
    );
    assert_eq!(
        actual.callback_results,
        expected["callbackResults"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| if let Some(b) = v.as_bool() {
                CallbackResult::Bool(b)
            } else {
                CallbackResult::Text(v.as_str().unwrap().encode_utf16().collect())
            })
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CALLBACK-RETURN {label}"
    );
    assert_eq!(
        actual.input,
        observed_slots(&expected["input"]),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-INPUT {label}"
    );
    assert_eq!(
        actual.listeners,
        expected["listeners"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v[0].as_str().unwrap(), v[1].as_str().unwrap()))
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-LISTENERS {label}"
    );
    assert_eq!(
        actual.closed,
        expected["closed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-CLOSED {label}"
    );
    assert_eq!(
        actual.events,
        expected["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<Vec<_>>(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-EFFECTS {label}"
    );
    assert_eq!(
        actual.frames.len(),
        expected["frames"].as_array().unwrap().len(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-FRAME-INVENTORY {label}"
    );
    assert_eq!(
        actual.registered.len(),
        expected["registered"].as_array().unwrap().len(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-REGISTRATION-INVENTORY {label}"
    );
    assert_eq!(
        actual.patches.len(),
        expected["patches"].as_array().unwrap().len(),
        "CRYPTOCOM-PUBLIC-LIFECYCLE-PATCH-INVENTORY {label}"
    );
}
fn result_name(result: Result<(), Error>, host: &Host) -> &'static str {
    match result {
        Ok(()) => "void",
        Err(e) => {
            assert!(
                Rc::ptr_eq(&e, &host.error),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-ERROR-IDENTITY"
            );
            "original"
        }
    }
}
#[test]
fn public_lifecycle_matches_actual_typescript_native() {
    let fixtures: Value =
        serde_json::from_str(include_str!("public_lifecycle_fixtures.json")).unwrap();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_lifecycle_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").expect("SAFE_CEX_TYPESCRIPT_PATH required"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = &corpus["rows"];
    for (index, spec) in fixtures.as_array().unwrap().iter().enumerate() {
        let source = &rows[index];
        let name = spec["name"].as_str().unwrap();
        let mut initial = if let Some(n) = spec["initialCount"].as_u64() {
            (0..n)
                .map(|i| string_value(&format!("already-{i}")))
                .collect::<Vec<_>>()
        } else {
            input(&spec["initial"])
                .into_iter()
                .map(slot_value)
                .collect()
        };
        initial.extend(input(&spec["initialExtra"]).into_iter().map(slot_value));
        let retained = Rc::new(OrderedStringSet::new(initial));
        let retained_map = Rc::new(bodies::IndexMarketMap::default());
        if let Some(cache) = spec["cache"].as_array() {
            for row in cache {
                retained_map.set(
                    text(&row["key"]),
                    Rc::new(OrderedStringSet::new(
                        input(&row["values"]).into_iter().map(slot_value),
                    )),
                );
            }
        }
        let retained_sets = retained_map.entries();
        let old_timer = (spec["noTimer"] != true).then(|| Rc::new("old-timer"));
        let new_timer = Rc::new("new-timer");
        let old_socket = Rc::new("old-socket");
        let new_socket = Rc::new("new-socket");
        let replacement_socket = Rc::new("replacement-socket");
        let state = Rc::new(State {
            channels: SubscriptionState {
                subscriptions: RefCell::new(retained.clone()),
                message_id: Cell::new(1.0),
            },
            index_markets: RefCell::new(retained_map.clone()),
            is_disposed: Cell::new(spec["disposed"] == true),
            reconnect_subscription_timer: RefCell::new(old_timer.clone()),
            ws: RefCell::new((spec["noSocket"] != true).then(|| old_socket.clone())),
        });
        let models = if let Some(n) = spec["marketCount"].as_u64() {
            (0..n)
                .map(|i| {
                    Rc::new(bodies::CryptocomMarketDataSubscription {
                        instrument_name: format!("M{i}").encode_utf16().collect(),
                        index_instrument_name: None,
                    })
                })
                .collect::<Vec<_>>()
        } else {
            spec["markets"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            Rc::new(bodies::CryptocomMarketDataSubscription {
                                instrument_name: text(&r["instrument"]),
                                index_instrument_name: (!r["index"].is_null())
                                    .then(|| text(&r["index"])),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        for (i, (m, s)) in models
            .iter()
            .zip(source["models"].as_array().unwrap())
            .enumerate()
        {
            assert_eq!(
                ChannelValue::String(m.instrument_name.clone()),
                observed_value(&s["instrument"]),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-MODEL {name} model{i} instrument"
            );
            let native = m
                .index_instrument_name
                .as_ref()
                .map(|v| ChannelValue::String(v.clone()))
                .unwrap_or(ChannelValue::Undefined);
            assert_eq!(
                native,
                observed_value(&s["index"]),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-MODEL {name} model{i} index"
            );
        }
        let callbacks = (0..spec["callbacks"].as_u64().unwrap_or(0))
            .map(|i| {
                if spec["sparseCallbacks"] == true && i == 0 {
                    None
                } else {
                    Some(usize::try_from(i).unwrap())
                }
            })
            .collect();
        let host = Rc::new(Host {
            spec: spec.clone(),
            state: state.clone(),
            input: Rc::new(RefCell::new(input(&spec["input"]))),
            retained,
            retained_map,
            retained_sets,
            error: Rc::new(()),
            prepared: Rc::new(()),
            urls: Rc::new(()),
            environment: Cell::new(""),
            old_timer,
            new_timer,
            old_socket,
            new_socket,
            replacement_socket,
            events: RefCell::new(vec![]),
            frames: RefCell::new(vec![]),
            registered: RefCell::new(vec![]),
            warnings: RefCell::new(vec![]),
            urls_used: RefCell::new(vec![]),
            listeners: RefCell::new(vec![]),
            closed: RefCell::new(vec![]),
            callback_results: RefCell::new(vec![]),
            patches: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            callback: RefCell::new(None),
            clock_count: Cell::new(0),
            models,
            iteration: Cell::new(0),
            filtered: RefCell::new(None),
            channel_count: Cell::new(0),
            valuation_ids: Rc::new(OrderedStringSet::new([
                string_value("BTC"),
                string_value("ETH"),
            ])),
            cursor: RefCell::new(None),
            tickers: ["BTC", "ETH", "LATE"]
                .into_iter()
                .map(|id| Rc::new(Ticker { id }))
                .collect(),
            callbacks: RefCell::new(callbacks),
            update: Rc::new(()),
        });
        let op = spec["op"].as_str().unwrap();
        let mut returned = None;
        let result = match op {
            "connect" => bodies::connect_and_subscribe(&state, &*host),
            "open" => bodies::on_open(state.clone(), host.clone()),
            "market" => bodies::subscribe_to_market_data(&state, &*host, &*host),
            "index" => {
                bodies::get_market_ids_by_index_instrument(&state, &text(&spec["index"]), &*host)
                    .map(|ids| {
                        returned = Some(ids);
                    })
            }
            "valuation" => bodies::handle_valuation_update(&*host),
            "candle" => bodies::handle_candlestick_update(&host.update, &*host),
            "unsubscribe" => bodies::unsubscribe(&state, &Channels(&host), &*host),
            "dispose" => bodies::dispose(&state, &*host),
            _ => panic!("unknown op"),
        };
        if let Some(ids) = returned {
            assert_eq!(
                ids.values(),
                source["returned"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(observed_value)
                    .collect::<Vec<_>>(),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-INDEX-RESULT {name}"
            );
            let cached = host.current_map().get(&text(&spec["index"]));
            let identity = cached
                .as_ref()
                .is_some_and(|cached| Rc::ptr_eq(cached, &ids));
            assert_eq!(
                identity,
                source["returnedIsCache"].as_bool().unwrap(),
                "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-INDEX-CACHE-IDENTITY {name}"
            );
        } else {
            assert!(source["returned"].is_null());
        }
        observe_snapshot(&host.snapshot(), &source["immediate"], name, "immediate");
        assert_eq!(
            result_name(result, &host),
            source["result"].as_str().unwrap(),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-RETURN {name}"
        );
        let mut timer_result = "not-fired";
        if op == "open" && spec["fireTimer"] == true && host.callback.borrow().is_some() {
            if spec["disposeBeforeFire"] == true {
                state.is_disposed.set(true);
            }
            timer_result = result_name(host.callback.borrow().as_ref().unwrap()(), &host);
        }
        observe_snapshot(&host.snapshot(), &source["final"], name, "final");
        assert_eq!(
            timer_result,
            source["timerResult"].as_str().unwrap(),
            "CRYPTOCOM-PUBLIC-LIFECYCLE-NATIVE-TIMER-RETURN {name}"
        );
        assert_eq!(
            host.frames.borrow().len() as u64,
            spec["expectedFrames"].as_u64().unwrap(),
            "PUBLIC-LIFECYCLE independent frame floor {name}"
        );
        assert_eq!(
            host.registered.borrow().len() as u64,
            spec["expectedRegister"].as_u64().unwrap(),
            "PUBLIC-LIFECYCLE independent registration floor {name}"
        );
        assert_eq!(
            host.models.len() as u64,
            spec["expectedModels"].as_u64().unwrap(),
            "PUBLIC-LIFECYCLE independent model floor {name}"
        );
        assert_eq!(
            source["models"].as_array().unwrap().len() as u64,
            spec["expectedModels"].as_u64().unwrap(),
            "PUBLIC-LIFECYCLE source model floor {name}"
        );
        assert_eq!(source["name"], spec["name"]);
    }
    assert_eq!(corpus["schema"][0]["name"], "instrumentName");
    assert_eq!(corpus["schema"][0]["kind"], "StringKeyword");
    assert_eq!(corpus["schema"][0]["optional"], false);
    assert_eq!(corpus["schema"][1]["name"], "indexInstrumentName");
    assert_eq!(corpus["schema"][1]["kind"], "StringKeyword");
    assert_eq!(corpus["schema"][1]["optional"], true);
    assert_eq!(
        corpus["schema"].as_array().unwrap().len(),
        2,
        "independent source model field floor"
    );
    assert_eq!(
        corpus["cap"]["MAX_PUBLIC_SUBSCRIPTIONS_PER_CONNECTION"],
        350
    );
    assert_eq!(corpus["cap"]["CRYPTOCOM_PUBLIC_SESSION_LIMIT"], 400);
    assert_eq!(
        fixtures.as_array().unwrap().len(),
        162,
        "PUBLIC-LIFECYCLE independent input floor"
    );
    assert_eq!(
        rows.as_array().unwrap().len(),
        162,
        "PUBLIC-LIFECYCLE independent result floor"
    );
    assert_eq!(fixtures[161]["name"], "dispose-index-map-retained");
}
