use safe_cex_venue_blofin::public_stream_core::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
    sync::OnceLock,
};

fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_stream_core_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "actual maintained source oracle: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["frames"].as_array().unwrap().len(),
            19,
            "independent frame-case floor"
        );
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            46,
            "independent host-case floor"
        );
        assert_eq!(
            v["stoppedAfter"],
            json!({"frames":19,"hostCases":46,"alias":1})
        );
        v
    })
}
fn topics(v: &Value) -> Vec<SubscriptionTopic> {
    serde_json::from_value(v.clone()).unwrap()
}
fn topic_assert(actual: &SubscriptionTopic, expected: &Value) {
    let entries = expected.as_object().unwrap();
    assert_eq!(
        actual.fields.len(),
        entries.len(),
        "native retained own-property count"
    );
    for ((key, value), (expected_key, expected_value)) in actual.fields.iter().zip(entries) {
        assert_eq!(key, expected_key, "native retained own-key order");
        assert_eq!(value, expected_value, "native decoded own-property value");
    }
}
fn decoded_list_assert(actual: &[Value], expected: &Value) {
    let entries = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        entries.len(),
        "independent decoded frame topic count"
    );
    for (actual, expected) in actual.iter().zip(entries) {
        let fields = actual
            .as_object()
            .expect("independent decoder: object topic");
        let expected = expected.as_object().unwrap();
        assert_eq!(
            fields.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>(),
            "independent decoded field order"
        );
        for (key, value) in fields {
            assert_eq!(
                value, &expected[key],
                "independent decoded literal field {key}"
            );
        }
    }
}
fn topic_list_assert(actual: &[SubscriptionTopic], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "native topic count");
    for (a, e) in actual.iter().zip(expected) {
        topic_assert(a, e)
    }
}
#[derive(Deserialize)]
struct DecodedFrame {
    op: String,
    args: Vec<Value>,
}
fn decode(frame: &str) -> DecodedFrame {
    serde_json::from_str(frame).unwrap()
}

#[test]
fn native_frame_limit() {
    assert_eq!(BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES, 4096.0);
    assert_eq!(corpus()["maximum"], 4096);
}
#[test]
fn native_build_frames() {
    let cases = corpus()["frames"].as_array().unwrap();
    let mut outputs = Vec::new();
    for c in cases {
        let maximum = match c["input"]["max"].as_str() {
            Some("nan") => Some(f64::NAN),
            Some("infinity") => Some(f64::INFINITY),
            _ => c["input"]["max"].as_f64(),
        };
        let input = topics(&c["input"]["topics"]);
        topic_list_assert(&input, &c["input"]["topics"]);
        let result = build_blofin_subscribe_frames(&input, maximum);
        if let Some(expected) = c["result"]["ok"].as_array() {
            let frames = result.as_ref().expect("native frames");
            assert_eq!(frames.len(), expected.len(), "native frame count");
            let mut decoded = Vec::new();
            for frame in frames {
                let d = decode(frame);
                assert_eq!(d.op, "subscribe", "native operation");
                decoded.extend(d.args);
            }
            decoded_list_assert(&decoded, &c["input"]["topics"]);
        } else {
            assert_eq!(
                result,
                Err(PublicStreamError::FrameTooLarge),
                "native oversized-topic error variant"
            );
        }
        outputs.push(result);
    }
    let large = outputs[14].as_ref().unwrap();
    assert_eq!(large.len(), 2, "independent 200-topic frame floor");
    assert_eq!(
        decode(&large[0]).args.len(),
        101,
        "independent literal-byte partition"
    );
    assert_eq!(
        decode(&large[1]).args.len(),
        99,
        "independent second partition"
    );
    assert_eq!(
        (large[0].len(), large[1].len()),
        (4058, 4086),
        "independent literal-byte frame sizes"
    );
    // Only after ALL decoded native assertions, compare exact serialized frames.
    for (result, c) in outputs.iter().zip(cases) {
        match result {
            Ok(frames) => assert_eq!(
                frames,
                &serde_json::from_value::<Vec<String>>(c["result"]["ok"].clone()).unwrap(),
                "wire frames"
            ),
            Err(e) => assert_eq!(
                e.to_string(),
                c["result"]["error"].as_str().unwrap(),
                "source error text"
            ),
        }
    }
}

type TimerCallback = Rc<dyn Fn() -> Result<(), String>>;
struct Host {
    initial: Value,
    created_url: RefCell<Option<String>>,
    trace: RefCell<Vec<Value>>,
    sent: RefCell<Vec<Option<DecodedFrame>>>,
    timers: RefCell<BTreeMap<u32, TimerCallback>>,
    next_timer: Cell<u32>,
    disposed: Cell<bool>,
    socket: Cell<bool>,
    market_reads: Cell<u32>,
    send_count: Cell<u32>,
}
impl Host {
    fn new(i: &Value) -> Self {
        Self {
            initial: i.clone(),
            created_url: RefCell::new(None),
            trace: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            timers: RefCell::new(BTreeMap::new()),
            next_timer: Cell::new(i["firstTimer"].as_u64().unwrap_or(1) as u32),
            disposed: Cell::new(i["disposed"].as_bool().unwrap_or(false)),
            socket: Cell::new(i["socket"].as_bool().unwrap_or(false)),
            market_reads: Cell::new(0),
            send_count: Cell::new(0),
        }
    }
    fn call(&self, label: &str) -> Result<(), String> {
        if self.initial["fail"].as_str() == Some(label) {
            Err(format!("{label} failed"))
        } else {
            Ok(())
        }
    }
    fn trace(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
}
impl PublicStreamHost for Host {
    type Error = String;
    type Timer = u32;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_testnet(&self) -> bool {
        self.initial["testnet"].as_bool().unwrap_or(false)
    }
    fn market_ids(&self) -> Result<Vec<std::string::String>, std::string::String> {
        let read = self.market_reads.get() + 1;
        self.market_reads.set(read);
        self.trace(json!(["markets", read]));
        Ok(
            if read > 1 && self.initial["marketsAfterFirst"].is_array() {
                serde_json::from_value(self.initial["marketsAfterFirst"].clone()).unwrap()
            } else {
                vec!["BTC-USDT".into(), "ETH-USDT".into()]
            },
        )
    }
    fn has_socket(&self) -> bool {
        self.socket.get()
    }
    fn create_and_assign_socket(&self, url: &str) -> Result<(), String> {
        self.call("create")?;
        *self.created_url.borrow_mut() = Some(url.into());
        self.trace(json!(["create", url]));
        self.socket.set(true);
        Ok(())
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), String> {
        self.call(&format!("listener:{event}"))?;
        self.trace(json!(["listener", event]));
        Ok(())
    }
    fn send(&self, frame: &str) -> Result<(), String> {
        let count = self.send_count.get() + 1;
        self.send_count.set(count);
        self.call("send")?;
        self.call(&format!("send:{count}"))?;
        self.sent.borrow_mut().push(if frame == "ping" {
            None
        } else {
            Some(decode(frame))
        });
        self.trace(json!(["send", frame]));
        if self.initial["disposeOnSend"].as_bool() == Some(true) {
            self.disposed.set(true)
        }
        Ok(())
    }
    fn performance_now(&self) -> f64 {
        self.trace(json!(["now"]));
        if self.initial["now"].as_str() == Some("negative-zero") {
            -0.0
        } else {
            self.initial["now"].as_f64().unwrap_or(123.5)
        }
    }
    fn markets_changed_off(&self) -> Result<(), String> {
        self.call("off")?;
        self.trace(json!(["off"]));
        Ok(())
    }
    fn markets_changed_on(&self) -> Result<(), String> {
        self.call("on")?;
        self.trace(json!(["on"]));
        Ok(())
    }
    fn reconnecting_enabled(&self) -> bool {
        self.initial["reconnecting"].as_bool() != Some(false)
    }
    fn emit_ws_reconnected(&self) -> Result<(), String> {
        self.call("reconnected")?;
        self.trace(json!(["reconnected"]));
        Ok(())
    }
    fn set_timeout(&self, callback: TimerCallback, delay_ms: f64) -> Result<u32, String> {
        self.call("timer")?;
        assert_eq!(
            delay_ms.to_bits(),
            20_000.0f64.to_bits(),
            "native watchdog delay"
        );
        let id = self.next_timer.get();
        self.next_timer.set(id + 1);
        self.timers.borrow_mut().insert(id, callback);
        self.trace(json!(["timer", id, delay_ms as u64]));
        Ok(id)
    }
    fn timer_is_truthy(&self, timer: &u32) -> bool {
        *timer != 0
    }
    fn clear_timeout(&self, timer: &u32) -> Result<(), String> {
        self.call("clear")?;
        self.timers.borrow_mut().remove(timer);
        self.trace(json!(["clear", timer]));
        Ok(())
    }
    fn log_warning(&self, message: &str) -> Result<(), String> {
        self.call("log")?;
        self.trace(json!(["log", message, "warning"]));
        Ok(())
    }
    fn emit_warning(&self, message: &str, force_toast: bool) -> Result<(), String> {
        self.call("warning")?;
        self.trace(json!(["warning", message, force_toast]));
        Ok(())
    }
}
struct Snapshot {
    topics: Vec<(String, Vec<SubscriptionTopic>)>,
    ping_bits: u64,
    timeout: Option<u32>,
    notified: bool,
    socket: bool,
    disposed: bool,
}
fn snapshot(state: &PublicStreamState<u32>, host: &Host) -> Snapshot {
    Snapshot {
        topics: state
            .topics
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        ping_bits: state.ping_at.get().to_bits(),
        timeout: *state.connection_failure_notice_timeout.borrow(),
        notified: state.connection_failure_notified.get(),
        socket: host.socket.get(),
        disposed: host.disposed.get(),
    }
}
fn snapshot_assert(actual: &Snapshot, expected: &Value) {
    let entries = expected["topics"].as_array().unwrap();
    assert_eq!(
        actual.topics.len(),
        entries.len(),
        "native dictionary count"
    );
    for ((key, list), entry) in actual.topics.iter().zip(entries) {
        assert_eq!(key, entry[0].as_str().unwrap(), "native own-key order");
        topic_list_assert(list, &entry[1]);
    }
    assert_eq!(
        actual.ping_bits,
        u64::from_str_radix(expected["pingBits"].as_str().unwrap(), 16).unwrap(),
        "native ping timestamp bits"
    );
    assert_eq!(
        actual.timeout,
        expected["timeout"].as_u64().map(|n| n as u32),
        "native timeout handle"
    );
    assert_eq!(
        actual.notified,
        expected["notified"].as_bool().unwrap(),
        "native notified flag"
    );
    assert_eq!(
        actual.socket,
        expected["socket"].as_bool().unwrap(),
        "native socket assignment"
    );
    assert_eq!(
        actual.disposed,
        expected["disposed"].as_bool().unwrap(),
        "native disposed flag"
    );
}
fn group(group: &str, floor: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["group"].as_str() == Some(group))
        .collect();
    assert_eq!(cases.len(), floor, "independent group floor");
    let mut runs = Vec::new();
    for c in &cases {
        let i = &c["initial"];
        let host = Rc::new(Host::new(i));
        let state = Rc::new(PublicStreamState::<u32>::default());
        if let Some(entries) = i["topics"].as_array() {
            for entry in entries {
                state
                    .topics
                    .borrow_mut()
                    .insert(entry[0].as_str().unwrap().into(), topics(&entry[1]));
            }
        }
        *state.connection_failure_notice_timeout.borrow_mut() =
            i["timeout"].as_u64().map(|n| n as u32);
        state
            .connection_failure_notified
            .set(i["notified"].as_bool().unwrap_or(false));
        let mut snapshots = Vec::new();
        let mut results = Vec::new();
        for action in c["actions"].as_array().unwrap() {
            let name = action[0].as_str().unwrap();
            let result = match name {
                "connect" => connect_and_subscribe(&state, &host),
                "open" => on_open(&state, host.as_ref()),
                "add" => on_markets_changed(
                    &state,
                    host.as_ref(),
                    &serde_json::from_value::<Vec<String>>(action[1].clone()).unwrap(),
                ),
                "ping" => ping(&state, host.as_ref()),
                "subscribe" => subscribe(&state, host.as_ref()),
                "send" => send_subscription_topics(host.as_ref(), &topics(&action[1])),
                "arm" => arm_connection_failure_notice(&state, &host),
                "mark" => mark_connection_established(&state, host.as_ref()),
                "disposed" => {
                    host.disposed.set(action[1].as_bool().unwrap());
                    Ok(())
                }
                "notified" => {
                    state
                        .connection_failure_notified
                        .set(action[1].as_bool().unwrap());
                    Ok(())
                }
                "fire" => {
                    let callback = host
                        .timers
                        .borrow_mut()
                        .remove(&(action[1].as_u64().unwrap() as u32));
                    callback.map_or(Ok(()), |cb| cb().map_err(PublicStreamError::Host))
                }
                _ => panic!("unknown action"),
            };
            results.push(result.err().map(|e| e.to_string()));
            snapshots.push(snapshot(&state, &host));
        }
        let expected = c["snapshots"].as_array().unwrap();
        assert_eq!(snapshots.len(), expected.len());
        for (a, e) in snapshots.iter().zip(expected) {
            snapshot_assert(a, e)
        }
        assert_eq!(
            results,
            serde_json::from_value::<Vec<Option<String>>>(c["results"].clone()).unwrap(),
            "native result sequence"
        );
        let source_url = c["trace"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event[0] == "create")
            .map(|event| event[1].as_str().unwrap());
        assert_eq!(
            host.created_url.borrow().as_deref(),
            source_url,
            "native socket endpoint"
        );
        let sent = host.sent.borrow();
        let expected = c["sent"].as_array().unwrap();
        assert_eq!(sent.len(), expected.len(), "native sent frame count");
        for (a, e) in sent.iter().zip(expected) {
            match a {
                None => assert_eq!(e["ping"], true, "native ping message"),
                Some(d) => {
                    assert_eq!(d.op, "subscribe", "native operation");
                    decoded_list_assert(&d.args, &e["args"]);
                }
            }
        }
        drop(sent);
        host.timers.borrow_mut().clear();
        runs.push(host);
    }
    // All native state and decoded frame checks above precede trace/wire checks.
    for (host, c) in runs.iter().zip(cases) {
        assert_eq!(
            *host.trace.borrow(),
            *c["trace"].as_array().unwrap(),
            "source call order and wire trace"
        );
    }
}
#[test]
fn native_connect() {
    group("connect", 7)
}
#[test]
fn native_open() {
    group("open", 6)
}
#[test]
fn native_markets_changed() {
    let state = PublicStreamState::<u32>::default();
    let host = Host::new(&json!({"socket":true}));
    let original: SubscriptionTopic =
        serde_json::from_value(json!({"channel":"tickers","instId":"ORIGINAL"})).unwrap();
    state
        .topics
        .borrow_mut()
        .insert("tickers".into(), vec![original]);
    let mut old = state.topics.borrow()["tickers"].clone();
    on_markets_changed(&state, &host, &["NEW".into()]).unwrap();
    old[0].fields.insert("instId".into(), json!("MUTATED"));
    assert_eq!(
        state.topics.borrow()["tickers"][0].fields["instId"],
        "ORIGINAL",
        "native owns topic record after concatenation"
    );
    assert_eq!(state.topics.borrow()["tickers"][1].fields["instId"], "NEW");
    assert_eq!(corpus()["alias"]["sourceFirst"], "MUTATED");
    assert_eq!(old[0].fields["instId"], "MUTATED");
    group("add", 5);
}
#[test]
fn native_ping() {
    group("ping", 4)
}
#[test]
fn native_subscribe() {
    group("subscribe", 3)
}
#[test]
fn native_send_topics() {
    group("send", 5)
}
#[test]
fn native_arm_notice() {
    group("arm", 11)
}
#[test]
fn native_mark_established() {
    group("mark", 5)
}
