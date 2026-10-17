use safe_cex_num::js_number_string;
use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::{
        AuthTokenEncoding, HostThrownValue, SigningResult, TransactionSigningError,
    },
    ws_private,
    ws_private_class::*,
    ws_private_feed_helpers::LighterPrivateFeedRuntime,
    ws_private_recovery::{PrivateRecoveryHost, RecoveryCall, RecoveryCallback, RecoveryTimerCall},
    ws_private_stream_handlers::LighterPrivateStreamRuntime,
    ws_private_warnings::{
        PrivateStreamWarningParams, PrivateWarningFlags, PrivateWarningHost, ReadonlyWarningTimes,
    },
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
};

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn t(s: &str) -> AccountValue {
    AccountValue::String(u(s))
}
fn n(v: f64) -> AccountValue {
    AccountValue::Number(v)
}
fn b(v: bool) -> AccountValue {
    AccountValue::Boolean(v)
}

struct Obj {
    entries: RefCell<Vec<(String, AccountValue)>>,
    array: bool,
}
impl AccountObject for Obj {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(self
            .entries
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}

#[derive(Default)]
struct Runtime {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, f: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(f)
    }
    fn now_millis(&self) -> f64 {
        panic!("unused clock")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("unused encoding")
    }
}

#[derive(Default)]
struct Host {
    state: RefCell<BTreeMap<String, AccountValue>>,
    returns: RefCell<BTreeMap<String, AccountValue>>,
    throws: RefCell<Vec<String>>,
    trace: RefCell<Vec<String>>,
    objects: RefCell<Vec<Rc<Obj>>>,
    listeners: RefCell<Vec<PrivateSocketListener>>,
    timers: RefCell<Vec<(String, f64, RecoveryCallback)>>,
    ping_advances: RefCell<bool>,
    runtime: Rc<Runtime>,
}
fn token() -> HostThrownValue {
    thread_local!(static TOKEN: HostThrownValue = HostThrownValue::new("thrown"));
    TOKEN.with(Clone::clone)
}
impl Host {
    fn new() -> Rc<Self> {
        let host = Rc::new(Self::default());
        for (k, v) in [
            ("isDisposed", b(false)),
            ("isConnected", b(true)),
            ("parent.isDisposed", b(false)),
            ("__now", n(1_000_000.0)),
            ("__date", n(2_000_000.0)),
        ] {
            host.set(k, v);
        }
        let parent = host.obj(vec![], false);
        host.set("parent", parent);
        host
    }
    fn set(&self, k: &str, v: AccountValue) {
        self.state.borrow_mut().insert(k.into(), v);
    }
    fn read(&self, k: &str) -> AccountValue {
        self.state
            .borrow()
            .get(k)
            .cloned()
            .unwrap_or(AccountValue::Undefined)
    }
    fn ret(&self, k: &str, v: AccountValue) {
        self.returns.borrow_mut().insert(k.into(), v);
    }
    fn obj(&self, entries: Vec<(&str, AccountValue)>, array: bool) -> AccountValue {
        let o = Rc::new(Obj {
            entries: RefCell::new(entries.into_iter().map(|(k, v)| (k.into(), v)).collect()),
            array,
        });
        self.objects.borrow_mut().push(o.clone());
        AccountValue::Object(o)
    }
    fn find(&self, v: &AccountValue) -> Option<Rc<Obj>> {
        let AccountValue::Object(o) = v else {
            return None;
        };
        self.objects
            .borrow()
            .iter()
            .find(|c| std::ptr::addr_eq(Rc::as_ptr(c), Rc::as_ptr(o)))
            .cloned()
    }
    fn render(&self, v: &AccountValue) -> String {
        match v {
            AccountValue::Undefined => "undefined".into(),
            AccountValue::Null => "null".into(),
            AccountValue::Boolean(v) => v.to_string(),
            AccountValue::Number(v) => js_number_string(*v),
            AccountValue::String(s) => String::from_utf16_lossy(s),
            AccountValue::Function(f) => format!(
                "fn:{}",
                f.value()
                    .downcast_ref::<String>()
                    .cloned()
                    .unwrap_or_default()
            ),
            AccountValue::Object(_) => match self.find(v) {
                Some(o) => {
                    let inner: Vec<String> = o
                        .entries
                        .borrow()
                        .iter()
                        .map(|(k, v)| format!("{k}={}", self.render(v)))
                        .collect();
                    format!("{{{}}}", inner.join(","))
                }
                None => "[foreign]".into(),
            },
            _ => "?".into(),
        }
    }
    fn record(&self, name: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
        let args: Vec<String> = args.iter().map(|a| self.render(a)).collect();
        self.trace
            .borrow_mut()
            .push(format!("{name}({})", args.join("|")));
        if self.throws.borrow().iter().any(|n| n == name) {
            return Err(token().into());
        }
        if name == "ping" && *self.ping_advances.borrow() {
            let next = match self.read("pingAt") {
                AccountValue::Number(v) => v + 1.0,
                _ => 1.0,
            };
            self.set("pingAt", n(next));
        }
        Ok(self
            .returns
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or(AccountValue::Undefined))
    }
    fn trace(&self) -> Vec<String> {
        self.trace.borrow().clone()
    }
    fn clear(&self) {
        self.trace.borrow_mut().clear();
    }
    fn has(&self, entry: &str) -> bool {
        self.trace.borrow().iter().any(|t| t == entry)
    }
    fn assert_no_auth_failed(&self) {
        assert!(
            !self
                .trace
                .borrow()
                .iter()
                .any(|t| t.to_lowercase().contains("auth failed")),
            "P4: never produce 'auth failed'"
        );
    }
}
struct Call(Rc<Host>, String);
impl PrivateSocketCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.0.record(&self.1, args)
    }
}
impl RecoveryCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<()> {
        self.0.record(&self.1, args).map(|_| ())
    }
}
struct Timer(Rc<Host>, &'static str);
impl RecoveryTimerCall for Timer {
    fn call(&self, callback: RecoveryCallback, delay: f64) -> SigningResult<AccountValue> {
        self.0
            .trace
            .borrow_mut()
            .push(format!("{}({})", self.1, js_number_string(delay)));
        self.0
            .timers
            .borrow_mut()
            .push((self.1.into(), delay, callback));
        Ok(t(&format!("{}-id", self.1)))
    }
}
#[derive(Clone)]
struct H(Rc<Host>);
impl PrivateRecoveryHost for H {
    fn boolean(&self, name: &str) -> SigningResult<bool> {
        Ok(self.0.read(name).truthy())
    }
    fn number(&self, name: &str) -> SigningResult<f64> {
        Ok(match self.0.read(name) {
            AccountValue::Number(v) => v,
            other => panic!("typed number {name}: {}", self.0.render(&other)),
        })
    }
    fn set_number(&self, name: &str, value: f64) -> SigningResult<()> {
        self.0.set(name, n(value));
        Ok(())
    }
    fn timer(&self, name: &str) -> SigningResult<AccountValue> {
        Ok(self.0.read(name))
    }
    fn set_timer(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        self.0.set(name, value);
        Ok(())
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn RecoveryCall>> {
        Ok(Rc::new(Call(self.0.clone(), name.into())))
    }
    fn capture_set_timeout(&self) -> SigningResult<Rc<dyn RecoveryTimerCall>> {
        Ok(Rc::new(Timer(self.0.clone(), "setTimeout")))
    }
    fn now(&self) -> SigningResult<f64> {
        self.number("__now")
    }
}
impl PrivateSocketHost for H {
    fn value(&self, name: &str) -> SigningResult<AccountValue> {
        Ok(self.0.read(name))
    }
    fn set_value(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        self.0.set(name, value);
        Ok(())
    }
    fn method(&self, name: &str) -> SigningResult<Rc<dyn PrivateSocketCall>> {
        Ok(Rc::new(Call(self.0.clone(), name.into())))
    }
    fn binding(&self, name: &str) -> SigningResult<AccountValue> {
        Ok(self.0.read(&format!("binding.{name}")))
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        match object {
            AccountValue::Object(o) => o.get(key),
            AccountValue::Undefined | AccountValue::Null => Err(token().into()),
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn call(
        &self,
        callee: AccountValue,
        receiver: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let name = format!("{}@{}", self.0.render(&callee), self.0.render(&receiver));
        self.0.record(&name, args)
    }
    fn function(&self, body: PrivateSocketListener) -> SigningResult<AccountValue> {
        let mut listeners = self.0.listeners.borrow_mut();
        listeners.push(body);
        Ok(AccountValue::Function(HostThrownValue::new(format!(
            "listener{}",
            listeners.len() - 1
        ))))
    }
    fn capture_set_interval(&self) -> SigningResult<Rc<dyn RecoveryTimerCall>> {
        Ok(Rc::new(Timer(self.0.clone(), "setInterval")))
    }
    fn date_now(&self) -> SigningResult<f64> {
        self.number("__date")
    }
    fn iso_string(&self, ms: f64) -> SigningResult<Vec<u16>> {
        Ok(u(&format!("ISO({})", js_number_string(ms))))
    }
    fn to_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        Ok(u(&self.0.render(&value)))
    }
    fn to_number(&self, value: AccountValue) -> SigningResult<f64> {
        Ok(match value {
            AccountValue::Number(v) => v,
            AccountValue::Boolean(v) => f64::from(u8::from(v)),
            AccountValue::Null => 0.0,
            AccountValue::String(s) => String::from_utf16_lossy(&s)
                .trim()
                .parse()
                .unwrap_or(f64::NAN),
            _ => f64::NAN,
        })
    }
    fn json_parse(&self, data: AccountValue) -> SigningResult<AccountValue> {
        self.0.record("jsonParse", &[data])?;
        Ok(self.0.read("__parsed"))
    }
    fn json_stringify(&self, value: AccountValue) -> SigningResult<AccountValue> {
        Ok(t(&format!("JSON{}", self.0.render(&value))))
    }
    fn object(&self, entries: Vec<(&'static str, AccountValue)>) -> SigningResult<AccountValue> {
        Ok(self.0.obj(entries, false))
    }
    fn spread_with(
        &self,
        source: AccountValue,
        key: &'static str,
        value: AccountValue,
    ) -> SigningResult<AccountValue> {
        let mut entries: Vec<(String, AccountValue)> =
            self.0.find(&source).unwrap().entries.borrow().clone();
        entries.retain(|(k, _)| k != key);
        entries.push((key.into(), value));
        let o = Rc::new(Obj {
            entries: RefCell::new(entries),
            array: false,
        });
        self.0.objects.borrow_mut().push(o.clone());
        Ok(AccountValue::Object(o))
    }
    fn own_key_count(&self, value: AccountValue) -> SigningResult<f64> {
        Ok(self.0.find(&value).map_or(0, |o| o.entries.borrow().len()) as f64)
    }
    fn is_array(&self, value: &AccountValue) -> SigningResult<bool> {
        Ok(self.0.find(value).is_some_and(|o| o.array))
    }
    fn strict_equals(&self, left: &AccountValue, right: &AccountValue) -> bool {
        match (left, right) {
            (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
            (AccountValue::String(a), AccountValue::String(b)) => a == b,
            (AccountValue::Boolean(a), AccountValue::Boolean(b)) => a == b,
            (AccountValue::Undefined, AccountValue::Undefined)
            | (AccountValue::Null, AccountValue::Null) => true,
            (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn promise_runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
}
fn socket(host: &Rc<Host>) -> Rc<dyn PrivateSocketHost> {
    Rc::new(H(host.clone()))
}

#[test]
fn check_last_received_reconnects_after_idle_threshold() {
    let host = Host::new();
    host.set("lastReceived", n(1_000.0));
    host.set("__date", n(200_000.0));
    host.set("lastReceivedTimer", t("recv-timer"));
    host.set("pingSent", b(true));
    host.set("lastIdlePingAt", n(5.0));
    host.ret("getIdlePingThresholdMs", n(20_000.0));
    host.ret("getIdleReconnectThresholdMs", n(180_000.0));
    check_last_received(socket(&host).as_ref()).unwrap();
    assert_eq!(
        host.trace(),
        vec![
            "getIdlePingThresholdMs()",
            "getIdleReconnectThresholdMs()",
            "parent.log([Lighter][PWS] No private stream messages for 199000ms (last at ISO(1000)); reconnecting.|warning)",
            "clearTimeout(recv-timer)",
            "reconnect()",
        ]
    );
    assert_eq!(host.render(&host.read("lastReceived")), "0");
    assert_eq!(host.render(&host.read("pingSent")), "false");
    assert_eq!(host.render(&host.read("lastIdlePingAt")), "0");
}

#[test]
fn check_last_received_pings_when_idle_and_tracks_sent_ping() {
    let host = Host::new();
    host.set("lastReceived", n(170_000.0));
    host.set("__date", n(200_000.0));
    host.set("pingSent", b(false));
    host.set("lastIdlePingAt", n(0.0));
    host.set("pingAt", n(7.0));
    host.ret("getIdlePingThresholdMs", n(20_000.0));
    host.ret("getIdleReconnectThresholdMs", n(180_000.0));
    host.ret("getPingIntervalMs", n(10_000.0));
    *host.ping_advances.borrow_mut() = true;
    check_last_received(socket(&host).as_ref()).unwrap();
    assert_eq!(
        host.trace()[2..],
        [
            "getPingIntervalMs()",
            "parent.log([Lighter][PWS] Private stream idle for 30000ms (last at ISO(170000)); sending ping.|info)",
            "ping()",
        ]
    );
    assert_eq!(host.render(&host.read("pingSent")), "true");
    assert_eq!(host.render(&host.read("lastIdlePingAt")), "200000");

    // Already pinged within the retry interval: no new ping.
    host.clear();
    host.set("__date", n(205_000.0));
    check_last_received(socket(&host).as_ref()).unwrap();
    assert!(!host.has("ping()"));

    // A deferred ping (pingAt unchanged) leaves pingSent false; a throwing ping is swallowed.
    host.set("pingSent", b(false));
    *host.ping_advances.borrow_mut() = false;
    host.throws.borrow_mut().push("ping".into());
    check_last_received(socket(&host).as_ref()).unwrap();
    assert_eq!(host.render(&host.read("pingSent")), "false");

    // Fresh traffic resets the idle ping state.
    host.set("lastReceived", n(204_000.0));
    host.set("pingSent", b(true));
    check_last_received(socket(&host).as_ref()).unwrap();
    assert_eq!(host.render(&host.read("pingSent")), "false");
    assert_eq!(host.render(&host.read("lastIdlePingAt")), "0");
}

#[test]
fn begin_diagnostics_loop_resets_and_warns_once() {
    let host = Host::new();
    host.set("diagnosticsInterval", t("old-interval"));
    host.set("hasWarnedNoAccountStream", b(true));
    host.set("accountStreamMessageCount", n(4.0));
    host.set("lastAccountStreamMessageAt", n(9.0));
    host.set("lastStaleAccountStreamWarningAt", n(9.0));
    begin_diagnostics_loop(socket(&host)).unwrap();
    assert_eq!(
        host.trace(),
        vec!["clearInterval(old-interval)", "setInterval(5000)"]
    );
    assert_eq!(
        host.render(&host.read("diagnosticsInterval")),
        "setInterval-id"
    );
    for f in [
        "hasWarnedNoAccountStream",
        "hasWarnedNoAccountStreamToast",
        "hasWarnedProxyStreamErrorToast",
    ] {
        assert_eq!(host.render(&host.read(f)), "false", "{f}");
    }
    for f in [
        "lastStaleAccountStreamWarningAt",
        "accountStreamMessageCount",
        "lastAccountStreamMessageAt",
    ] {
        assert_eq!(host.render(&host.read(f)), "0", "{f}");
    }
    host.set("lastConnectTime", n(1_000_000.0 - 95_400.0));
    host.set("useReadonlyFallback", b(false));
    host.ret("isWsRateLimited", b(false));
    host.ret("isWsProxyRouteEnabled", b(false));
    let tick = host.timers.borrow()[0].2.clone();
    host.clear();
    tick().unwrap();
    assert_eq!(
        host.trace(),
        vec![
            "isWsRateLimited()",
            "isWsProxyRouteEnabled()",
            "parent.log([Lighter][PWS] Connected but no private account stream messages after 95s. wsRoute=false readonlyFallback=false.|warning)",
            "isWsProxyRouteEnabled()",
            "parent.emitter.emit(warning|Lighter private websocket is connected but not receiving account updates. Private updates may rely on REST reconciliation in this environment.|{forceToast=true})",
            "maybeRecoverPrivateStream()",
        ]
    );
    host.clear();
    host.set("lastAccountStreamMessageAt", n(1_000_000.0 - 91_000.0));
    tick().unwrap();
    assert_eq!(
        host.trace(),
        vec![
            "parent.log([Lighter][PWS] Private account stream stale (91s since last message).|warning)",
            "maybeRecoverPrivateStream()",
        ]
    );
    host.clear();
    tick().unwrap();
    assert_eq!(host.trace(), vec!["maybeRecoverPrivateStream()"]);
    host.set("isConnected", b(false));
    host.clear();
    tick().unwrap();
    assert!(host.trace().is_empty());
}

fn settle(host: &Rc<Host>, promise: NativePromise<AccountValue>) -> bool {
    let ok = Rc::new(RefCell::new(None));
    let seen = ok.clone();
    promise.subscribe(move |r| *seen.borrow_mut() = Some(r.is_ok()));
    while let Some(f) = host.runtime.queue.borrow_mut().pop_front() {
        f();
    }
    ok.borrow().expect("settled")
}

#[test]
fn connect_and_subscribe_wires_listeners_for_each_mode() {
    let host = Host::new();
    host.set("useReadonlyFallback", b(true));
    host.set("failedHandshakes", n(2.0));
    host.set(
        "parent.lighterDeploymentProfile.wsUrl",
        t("wss://lighter/stream?x=1"),
    );
    host.set("openedSinceLastConnect", b(true));
    host.set("onOpen", t("onOpenFn"));
    host.set("onMessage", t("onMessageFn"));
    host.set("onClose", t("onCloseFn"));
    let ws = host.obj(
        vec![(
            "addEventListener",
            AccountValue::Function(HostThrownValue::new(String::from("add"))),
        )],
        false,
    );
    host.ret("createWebsocket", ws.clone());
    host.ret("isWsProxyRouteEnabled", b(false));
    assert!(settle(&host, connect_and_subscribe(socket(&host))));
    let ws_text = host.render(&ws);
    assert_eq!(
        host.trace(),
        vec![
            "isWsProxyRouteEnabled()".to_string(),
            "parent.log([Lighter][PWS] Connecting (readonly-fallback, attempt #3): wss://lighter/stream?x=1&readonly=true|info)".into(),
            "createWebsocket(wss://lighter/stream?x=1&readonly=true)".into(),
            format!("fn:add@{ws_text}(open|onOpenFn)"),
            format!("fn:add@{ws_text}(message|onMessageFn)"),
            format!("fn:add@{ws_text}(close|onCloseFn)"),
            format!("fn:add@{ws_text}(error|fn:listener0)"),
        ]
    );
    assert_eq!(host.render(&host.read("openedSinceLastConnect")), "false");
    host.clear();
    let error = host.obj(
        vec![("error", host.obj(vec![("message", t("boom"))], false))],
        false,
    );
    let listener = host.listeners.borrow()[0].clone();
    listener(&[error]).unwrap();
    assert_eq!(
        host.trace(),
        vec!["parent.log([Lighter][PWS] Connection error: boom (mode: readonly-fallback)|warning)"]
    );
    host.clear();
    listener(&[AccountValue::Undefined]).unwrap();
    assert_eq!(
        host.trace(),
        vec!["parent.log([Lighter][PWS] Connection error (mode: readonly-fallback)|warning)"]
    );

    // Proxy route ignores the readonly fallback and uses the routed host.
    host.clear();
    host.ret("isWsProxyRouteEnabled", b(true));
    let routing = host.obj(
        vec![(
            "wsPrivate",
            host.obj(vec![("host", t("wss://proxy"))], false),
        )],
        false,
    );
    host.ret("resolveTransportRouting", routing);
    host.set("parent.name", t("lighter"));
    host.set("parent.options", t("opts"));
    assert!(settle(&host, connect_and_subscribe(socket(&host))));
    assert_eq!(
        host.trace()[1..4],
        [
            "resolveTransportRouting(lighter|opts)",
            "parent.log([Lighter][PWS] Connecting (proxy-route, attempt #3): wss://proxy|info)",
            "createWebsocket(wss://proxy)",
        ]
    );

    // Disposed sockets do nothing; a thrown createWebsocket rejects the promise.
    host.clear();
    host.set("isDisposed", b(true));
    assert!(settle(&host, connect_and_subscribe(socket(&host))));
    assert!(host.trace().is_empty());
    host.set("isDisposed", b(false));
    host.throws.borrow_mut().push("createWebsocket".into());
    assert!(!settle(&host, connect_and_subscribe(socket(&host))));
    host.assert_no_auth_failed();
}

fn close_event(host: &Rc<Host>, code: f64, reason: &str) -> AccountValue {
    host.obj(vec![("code", n(code)), ("reason", t(reason))], false)
}

#[test]
fn on_close_counts_handshake_failures_and_schedules_backoff_reconnect() {
    let host = Host::new();
    let ws = host.obj(
        vec![(
            "removeEventListener",
            AccountValue::Function(HostThrownValue::new(String::from("remove"))),
        )],
        false,
    );
    for (k, v) in [
        ("ws", ws.clone()),
        ("onOpen", t("onOpenFn")),
        ("onMessage", t("onMessageFn")),
        ("onClose", t("onCloseFn")),
        ("openedSinceLastConnect", b(false)),
        ("lastConnectTime", n(0.0)),
        ("accountStreamMessageCount", n(0.0)),
        ("failedHandshakes", n(2.0)),
        ("readonlyFallbackFailures", n(2.0)),
        ("useReadonlyFallback", b(false)),
        ("lastReceived", n(55.0)),
        ("lastIdlePingAt", n(66.0)),
        ("lastReceivedTimer", t("recv")),
        ("pingTimeoutId", t("ping-timer")),
        ("pongMissTimeoutId", AccountValue::Undefined),
        ("reconnecting", b(false)),
        ("onCloseCount", n(0.0)),
        ("binding.WS_MIN_RECONNECTION_DELAY", n(1_000.0)),
    ] {
        host.set(k, v);
    }
    host.ret("isWsProxyRouteEnabled", b(false));
    host.ret("isWsRateLimited", b(false));
    on_close(socket(&host), close_event(&host, 1006.0, "")).unwrap();
    let ws_text = host.render(&ws);
    assert_eq!(
        host.trace(),
        vec![
            "recordCloseDiagnostics(1006|{syntheticIfNoSocket=true})".to_string(),
            "stopDiagnosticsLoop()".into(),
            "clearStreamRecoveryReconnectTimeout()".into(),
            "clearRateLimitResubscribeTimeout()".into(),
            "isWsProxyRouteEnabled()".into(),
            "emitReadonlyFallbackWarning(true)".into(),
            "clearTimeout(recv)".into(),
            "parent.log([Lighter][PWS] WebSocket closed (code 1006, handshakes failed: 3)|warning)"
                .into(),
            "clearTimeout(ping-timer)".into(),
            format!("fn:remove@{ws_text}(open|onOpenFn)"),
            format!("fn:remove@{ws_text}(message|onMessageFn)"),
            format!("fn:remove@{ws_text}(close|onCloseFn)"),
            "isWsRateLimited()".into(),
            "parent.log([Lighter][PWS] Reconnecting in 3375ms...|info)".into(),
            "setTimeout(3375)".into(),
        ]
    );
    for (k, v) in [
        ("failedHandshakes", "3"),
        ("readonlyFallbackFailures", "3"),
        ("useReadonlyFallback", "true"),
        ("hasWarnedReadonlyFallback", "true"),
        ("openedSinceLastConnect", "false"),
        ("lastReceived", "0"),
        ("lastIdlePingAt", "0"),
        ("lastReceivedTimer", "undefined"),
        ("pingTimeoutId", "undefined"),
        ("reconnecting", "true"),
        ("ws", "undefined"),
        ("onCloseCount", "1"),
    ] {
        assert_eq!(host.render(&host.read(k)), v, "{k}");
    }
    host.clear();
    let reconnect = host.timers.borrow()[0].2.clone();
    reconnect().unwrap();
    assert_eq!(host.trace(), vec!["connectAndSubscribe()"]);
    assert_eq!(host.render(&host.read("reconnecting")), "false");
    assert_eq!(host.render(&host.read("lastCloseTime")), "2000000");

    // Already reconnecting: logs the close and stops.
    host.clear();
    host.set("reconnecting", b(true));
    on_close(socket(&host), close_event(&host, 1000.0, "bye")).unwrap();
    assert!(!host.trace().iter().any(|t| t.contains("Reconnecting")));
    assert!(!host.trace().iter().any(|t| t.contains("WebSocket closed")));

    // Rate-limit close: cooldown and the 30s floor.
    host.clear();
    host.set("reconnecting", b(false));
    host.set("openedSinceLastConnect", b(true));
    host.set("lastConnectTime", n(1_000_000.0 - 50_000.0));
    on_close(socket(&host), close_event(&host, 4000.0, "Rate limit hit")).unwrap();
    assert_eq!(host.render(&host.read("wsRateLimitedUntil")), "1060000");
    assert!(host.has("parent.log([Lighter][PWS] WebSocket closed (code 4000, reason: Rate limit hit, handshakes failed: 3)|warning)"));
    assert!(host.has("setTimeout(30000)"));

    // The streak cap stops reconnection.
    host.clear();
    host.set("reconnecting", b(false));
    host.set("onCloseCount", n(100.0));
    on_close(socket(&host), close_event(&host, 1001.0, "")).unwrap();
    assert!(
        host.has("parent.log([Lighter][PWS] onClose count > 100, stopping reconnection|error)")
    );
    assert!(!host.trace().iter().any(|t| t.starts_with("setTimeout")));
    host.assert_no_auth_failed();
}

fn message(host: &Rc<Host>, json: Vec<(&str, AccountValue)>) {
    let parsed = host.obj(json, false);
    host.set("__parsed", parsed);
    host.clear();
    let event = host.obj(vec![("data", t("raw-frame"))], false);
    on_message(socket(&host).as_ref(), event).unwrap();
}

#[test]
fn on_message_dispatches_account_streams_acks_and_heartbeats() {
    let host = Host::new();
    host.ret("shouldUseRestOrderReconciliation", b(true));
    let by_market = host.obj(vec![("1", t("o"))], false);
    message(
        &host,
        vec![
            ("channel", t("account_all/7")),
            ("orders", by_market.clone()),
        ],
    );
    let json = host.read("__parsed");
    assert_eq!(
        host.trace(),
        vec![
            "trackLastReceived()".to_string(),
            "emitWsDiagnosticReceive(raw-frame)".into(),
            "jsonParse(raw-frame)".into(),
            "noteAccountStreamActivity(account_all|account_all/7)".into(),
            "parent.invalidateAccountSnapshotCache()".into(),
            "throttleFetchBalance(500)".into(),
            "shouldUseRestOrderReconciliation()".into(),
            "throttleFetchOrders(500|{skipIfRecent=true})".into(),
            format!("handleAccountAllTopic({})", host.render(&json)),
        ]
    );

    let orders = host.obj(vec![], true);
    let envelope = host.obj(vec![("orders", orders)], false);
    message(
        &host,
        vec![("channel", t("account_all")), ("data", envelope.clone())],
    );
    let spread = host.render(&host.obj(
        vec![("channel", t("account_all")), ("data", envelope)],
        false,
    ));
    assert!(host.has(&format!("handleAccountTopic({spread})")));

    let flattened = host.obj(vec![("length", n(2.0))], true);
    host.ret("flattenOrdersByMarket", flattened.clone());
    message(
        &host,
        vec![
            ("channel", t("account_all_orders:7")),
            ("orders", by_market.clone()),
            ("type", t("update/account_all_orders")),
        ],
    );
    assert_eq!(
        host.trace()[3..],
        [
            format!("flattenOrdersByMarket({})", host.render(&by_market)),
            "noteAccountStreamActivity(account_all_orders|account_all_orders:7)".into(),
            format!("handleOrderTopic({})", host.render(&flattened)),
            "parent.log([Lighter][PWS] Streaming update received (type=update/account_all_orders).|info)".into(),
        ]
    );

    host.set("hasPendingAccountAllOrdersSubscribe", b(true));
    message(
        &host,
        vec![
            ("type", t("subscribed/account_all_orders")),
            ("channel", t("account_all_orders/7")),
        ],
    );
    assert_eq!(
        host.render(&host.read("hasPendingAccountAllOrdersSubscribe")),
        "false"
    );
    assert_eq!(
        host.render(&host.read("hasReceivedAccountAllOrdersAck")),
        "true"
    );
    assert!(host.has("parent.log([Lighter][PWS] Subscription acknowledged for account_all_orders/7 (type=subscribed/account_all_orders).|info)"));

    host.ret("getPingIntervalMs", n(10_000.0));
    message(&host, vec![("type", t("ping"))]);
    assert_eq!(
        host.trace()[3..],
        ["send(JSON{type=pong,timestamp=1000000})"]
    );
    message(&host, vec![("type", t("pong"))]);
    assert_eq!(
        host.trace()[3..],
        ["getPingIntervalMs()", "handlePongEvent(10000)"]
    );

    host.set("isDisposed", b(true));
    message(&host, vec![("type", t("ping"))]);
    assert!(host.trace().is_empty());
    host.assert_no_auth_failed();
}

#[test]
fn on_message_handles_stream_errors_without_auth_failed_text() {
    let host = Host::new();
    host.set("hasPendingAccountAllOrdersSubscribe", b(true));
    message(
        &host,
        vec![
            ("type", t("error")),
            (
                "error",
                host.obj(vec![("code", n(30003.0)), ("message", t("dup"))], false),
            ),
        ],
    );
    assert_eq!(
        host.trace()[3..],
        [
            "parent.log([Lighter][PWS] Ignoring benign duplicate subscription error from private stream: [30003] dup|info)"
        ]
    );
    assert_eq!(
        host.render(&host.read("hasReceivedAccountAllOrdersAck")),
        "true"
    );

    host.ret("getPrivateStreamErrorWarningMessage", t("RATE GUIDANCE"));
    host.ret("getRateLimitWarningMessage", t("RATE GUIDANCE"));
    host.ret("shouldForceToastPrivateStreamRateLimit", b(false));
    message(&host, vec![("error", t("Too many websocket messages"))]);
    assert_eq!(
        host.trace()[3..],
        [
            "notePrivateStreamRateLimitError(1000000)",
            "parent.log([Lighter][PWS] Server rate-limited private WS messages. Applying 60s cooldown.|warning)",
            "scheduleRateLimitResubscribe()",
            "parent.log([Lighter][PWS] Received error from private stream: Too many websocket messages|warning)",
            "getPrivateStreamErrorWarningMessage({errMsg=Too many websocket messages,isRateLimitError=true,isPolicyRestrictionError=false})",
            "getRateLimitWarningMessage()",
            "shouldForceToastPrivateStreamRateLimit(1000000)",
        ]
    );
    assert_eq!(host.render(&host.read("wsRateLimitedUntil")), "1060000");
    assert_eq!(
        host.render(&host.read("hasPendingAccountAllOrdersSubscribe")),
        "false"
    );

    host.set("hasPendingAccountAllOrdersSubscribe", b(true));
    host.set("hasReceivedAccountAllOrdersAck", b(false));
    message(
        &host,
        vec![("type", t("error")), ("message", t("Invalid Channel"))],
    );
    assert_eq!(
        host.trace()[3..],
        [
            "parent.log([Lighter][PWS] account_all_orders channel appears unsupported in this environment. Continuing with REST order reconciliation.|warning)"
        ]
    );
    assert_eq!(
        host.render(&host.read("accountAllOrdersChannelEnabled")),
        "false"
    );

    host.ret("getPrivateStreamErrorWarningMessage", t("GEO"));
    message(
        &host,
        vec![("type", t("error")), ("message", t("restricted region"))],
    );
    assert_eq!(
        host.trace()[5..],
        [
            "getRateLimitWarningMessage()",
            "parent.emitter.emit(warning|GEO|{forceToast=true})",
        ]
    );
    assert!(host.has("getPrivateStreamErrorWarningMessage({errMsg=restricted region,isRateLimitError=false,isPolicyRestrictionError=true})"));
    assert_eq!(
        host.render(&host.read("hasWarnedProxyStreamErrorToast")),
        "true"
    );
    // P4: no stream error path emits a subscribe or the 'auth failed' text.
    assert!(!host.trace().iter().any(|t| t.contains("subscribe")));
    host.assert_no_auth_failed();
}

struct Realm {
    host: Rc<Host>,
    class: AccountValue,
    owner: AccountValue,
    log: RefCell<Vec<String>>,
}
impl PrivateWebsocketContexts<()> for Realm {
    fn socket(&self, _owner: AccountValue) -> Rc<dyn PrivateSocketHost> {
        socket(&self.host)
    }
    fn warnings(&self, _: AccountValue) -> Rc<dyn PrivateWarningHost> {
        unimplemented!("warnings family is covered by live_private_warnings")
    }
    fn warning_flags(&self, _: AccountValue) -> Rc<dyn PrivateWarningFlags> {
        unimplemented!()
    }
    fn warning_params(&self, _: AccountValue) -> Rc<dyn PrivateStreamWarningParams> {
        unimplemented!()
    }
    fn readonly_warning_times(&self) -> ReadonlyWarningTimes {
        Rc::default()
    }
    fn feed(&self) -> Rc<LighterPrivateFeedRuntime<(), AccountValue>> {
        unimplemented!("feed family is covered by live_private_feed_helpers")
    }
    fn stream(&self) -> Rc<LighterPrivateStreamRuntime<(), AccountValue>> {
        unimplemented!("stream family is covered by live_private_stream_handlers")
    }
    fn flatten_by_market_payload(
        &self,
        payload: &AccountValue,
        include_single_objects: bool,
    ) -> SigningResult<AccountValue> {
        use safe_cex_exchange::js_runtime::value::{Js, to_json};
        let input = match payload {
            AccountValue::Undefined => Js::Undefined,
            AccountValue::Null => Js::Null,
            AccountValue::Boolean(value) => Js::Bool(*value),
            AccountValue::Number(value) => Js::Number(*value),
            AccountValue::String(value) => Js::String(value.clone().into()),
            _ => return Err(self.type_error("flatten fixture requires a primitive input")),
        };
        let flattened = ws_private::flatten_by_market_payload(Some(&input), include_single_objects)
            .expect("native flatten helper");
        Ok(t(
            &serde_json::to_string(&to_json(&flattened).unwrap()).unwrap()
        ))
    }
    fn empty_array(&self) -> AccountValue {
        t("[]")
    }
    fn type_error(&self, message: &str) -> TransactionSigningError {
        TransactionSigningError::Thrown(HostThrownValue::new(message.to_string()))
    }
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue {
        assert!(settle(&self.host, value));
        t("promise")
    }
}
impl PrivateWebsocketRealm<()> for Realm {
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue> {
        self.log.borrow_mut().push(format!("binding:{name}"));
        Ok(t(name))
    }
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        _body: PrivateWebsocketConstructor<()>,
    ) -> SigningResult<AccountValue> {
        self.log
            .borrow_mut()
            .push(format!("class:{}:{name}:{length}", self.host.render(&base)));
        Ok(self.class.clone())
    }
    fn arrow(
        &self,
        _owner: &AccountValue,
        body: PrivateWebsocketMethod,
    ) -> SigningResult<AccountValue> {
        let m = body.metadata();
        Ok(t(&format!(
            "arrow:{}/{}{}",
            m.name,
            m.length,
            if m.asynchronous { "/async" } else { "" }
        )))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        let target = if H(self.host.clone()).strict_equals(object, &self.class) {
            "class"
        } else {
            "owner"
        };
        self.log
            .borrow_mut()
            .push(format!("{target}.{key}={}", self.host.render(&value)));
        if let Some(o) = self.host.find(object) {
            o.entries.borrow_mut().push((key.into(), value));
        }
        Ok(())
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        self.log.borrow_mut().push(format!("get:{key}"));
        match object {
            AccountValue::Object(o) => o.get(key),
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn readonly_warning_map(&self) -> SigningResult<AccountValue> {
        Ok(t("Map"))
    }
    fn finish_class_binding(&self, _class: &AccountValue) -> SigningResult<()> {
        self.log.borrow_mut().push("finish".into());
        Ok(())
    }
    fn construct_base(
        &self,
        _class: AccountValue,
        _new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let args: Vec<String> = args.iter().map(|a| self.host.render(a)).collect();
        self.log
            .borrow_mut()
            .push(format!("super({})", args.join("|")));
        Ok(self.owner.clone())
    }
}

#[test]
fn class_assembly_defines_statics_fields_and_dispatches_bodies() {
    let host = Host::new();
    let realm = Realm {
        class: host.obj(vec![], false),
        owner: host.obj(vec![], false),
        host: host.clone(),
        log: RefCell::default(),
    };
    let class = LighterPrivateWebsocket::define(&realm).unwrap();
    assert!(H(host.clone()).strict_equals(&class, &realm.class));
    let log = realm.log.take();
    assert_eq!(
        log,
        vec![
            "binding:BaseWebSocket",
            "class:BaseWebSocket:LighterPrivateWebsocket:0",
            "class.READONLY_FALLBACK_WARNING_COOLDOWN_MS=30000",
            "class.readonlyFallbackWarningAtByAccount=Map",
            "class.STANDARD_PING_INTERVAL_MS=10000",
            "class.DEGRADED_PING_INTERVAL_MS=20000",
            "class.STANDARD_IDLE_PING_THRESHOLD_MS=20000",
            "class.DEGRADED_IDLE_PING_THRESHOLD_MS=45000",
            "class.STANDARD_IDLE_RECONNECT_THRESHOLD_MS=180000",
            "class.DEGRADED_IDLE_RECONNECT_THRESHOLD_MS=180000",
            "class.RATE_LIMIT_RECONNECT_DELAY_MS=30000",
            "class.RATE_LIMIT_COOLDOWN_MS=60000",
            "get:RATE_LIMIT_COOLDOWN_MS",
            "class.RATE_LIMIT_FORCE_TOAST_AFTER_MS=60000",
            "class.RATE_LIMIT_RESUBSCRIBE_BUFFER_MS=2000",
            "class.RATE_LIMIT_PING_INTERVAL_MS=20000",
            "class.RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS=15000",
            "class.PRIVATE_STREAM_WARNING_AFTER_MS=90000",
            "class.PRIVATE_STREAM_WARNING_COOLDOWN_MS=60000",
            "class.NO_STREAM_RECOVERY_AFTER_MS=180000",
            "class.STALE_STREAM_RECOVERY_AFTER_MS=120000",
            "class.RECOVERY_ATTEMPT_COOLDOWN_MS=180000",
            "class.RECOVERY_RECONNECT_GRACE_MS=20000",
            "class.EARLY_CLOSE_WINDOW_MS=10000",
            "finish",
        ]
    );

    let owner =
        LighterPrivateWebsocket::construct(&realm, class.clone(), class, &[t("parentExchange")])
            .unwrap();
    assert!(H(host.clone()).strict_equals(&owner, &realm.owner));
    let log = realm.log.take();
    assert_eq!(log[0], "super(parentExchange)");
    assert_eq!(
        log[1..6],
        [
            "owner.recoversAfterOffline=false",
            "owner.treatAnyMessageAsPong=true",
            "owner.shouldEnforcePongMissReconnect=arrow:shouldEnforcePongMissReconnect/0",
            "owner.lastConnectTime=0",
            "owner.ordersThrottleTimeout=undefined",
        ]
    );
    assert!(log.contains(&"owner.accountAllOrdersChannelEnabled=true".to_string()));
    let message_at = log
        .iter()
        .position(|l| l.starts_with("owner.readonlyFallbackWarningMessage=Lighter georestrictions"))
        .unwrap();
    assert_eq!(log[message_at - 1], "owner.lastIdlePingAt=0");
    assert_eq!(
        log[message_at + 1],
        "owner.getRateLimitWarningMessage=arrow:getRateLimitWarningMessage/0"
    );
    let arrows: Vec<&String> = log.iter().filter(|l| l.contains("=arrow:")).collect();
    assert_eq!(arrows.len(), 49);
    assert!(
        log.contains(&"owner.connectAndSubscribe=arrow:connectAndSubscribe/0/async".to_string())
    );
    assert!(log.contains(&"owner.onClose=arrow:onClose/1".to_string()));
    assert!(log.contains(&"owner.scheduleThrottledTask=arrow:scheduleThrottledTask/4".to_string()));
    assert_eq!(log.last().unwrap(), "owner.dispose=arrow:dispose/0");

    // Dispatch reaches the ported bodies through the owner-bound contexts.
    let call = |m: PrivateWebsocketMethod, args: &[AccountValue]| {
        host.render(&m.invoke(&realm, owner.clone(), args).unwrap())
    };
    host.set("useReadonlyFallback", b(true));
    host.set("hasReceivedAccountAllOrdersAck", b(true));
    host.set("wsRateLimitedUntil", n(1_000_001.0));
    host.set("privateStreamRateLimitErrorCount", n(0.0));
    host.set("lastConnectTime", n(0.0));
    assert_eq!(
        call(PrivateWebsocketMethod::GetPingIntervalMs, &[]),
        "20000"
    );
    assert_eq!(
        call(PrivateWebsocketMethod::GetIdlePingThresholdMs, &[]),
        "45000"
    );
    assert_eq!(
        call(
            PrivateWebsocketMethod::ShouldUseRestOrderReconciliation,
            &[]
        ),
        "true"
    );
    assert_eq!(call(PrivateWebsocketMethod::IsWsRateLimited, &[]), "true");
    assert_eq!(
        call(PrivateWebsocketMethod::IsReadonlyFallbackActive, &[]),
        "true"
    );
    assert_eq!(
        call(PrivateWebsocketMethod::ShouldEnforcePongMissReconnect, &[]),
        "false"
    );
    call(
        PrivateWebsocketMethod::NotePrivateStreamRateLimitError,
        &[n(42.0)],
    );
    call(
        PrivateWebsocketMethod::NotePrivateStreamRateLimitError,
        &[n(99.0)],
    );
    assert_eq!(
        host.render(&host.read("privateStreamRateLimitFirstSeenAt")),
        "42"
    );
    assert_eq!(
        host.render(&host.read("privateStreamRateLimitErrorCount")),
        "2"
    );
    assert_eq!(
        call(
            PrivateWebsocketMethod::ShouldForceToastPrivateStreamRateLimit,
            &[n(60_042.0)]
        ),
        "true"
    );
    call(
        PrivateWebsocketMethod::ClearPrivateStreamRateLimitErrorState,
        &[],
    );
    assert_eq!(
        host.render(&host.read("privateStreamRateLimitErrorCount")),
        "0"
    );
    assert_eq!(
        call(
            PrivateWebsocketMethod::IsRecentStreamEvent,
            &[n(1_999_000.0)]
        ),
        "true"
    );
    assert_eq!(
        call(PrivateWebsocketMethod::IsRecentStreamEvent, &[n(0.0)]),
        "false"
    );
    assert_eq!(
        call(
            PrivateWebsocketMethod::FlattenByMarketPayload,
            &[t("x"), b(true)]
        ),
        "[]"
    );
    assert_eq!(
        call(PrivateWebsocketMethod::FlattenByMarketPayload, &[t("x")]),
        "[]"
    );
    assert_eq!(
        call(
            PrivateWebsocketMethod::FlattenOrdersByMarket,
            &[AccountValue::Undefined]
        ),
        "[]"
    );
    host.clear();
    host.set("isDisposed", b(true));
    assert_eq!(
        call(PrivateWebsocketMethod::ConnectAndSubscribe, &[]),
        "promise"
    );
    assert!(host.trace().is_empty());
    host.set("isDisposed", b(false));
    host.set("lastReceived", n(1_990_000.0));
    host.ret("getIdlePingThresholdMs", n(20_000.0));
    host.ret("getIdleReconnectThresholdMs", n(180_000.0));
    call(PrivateWebsocketMethod::CheckLastReceived, &[]);
    assert_eq!(
        host.trace(),
        vec!["getIdlePingThresholdMs()", "getIdleReconnectThresholdMs()"]
    );
    let bad_key = PrivateWebsocketMethod::ScheduleThrottledTask.invoke(
        &realm,
        owner.clone(),
        &[t("otherTimeout")],
    );
    assert!(bad_key.is_err());
}
