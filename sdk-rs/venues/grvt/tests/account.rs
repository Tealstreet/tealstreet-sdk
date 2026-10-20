use safe_cex_engine::model::{OrderSide, PositionSide};
use safe_cex_engine::wire::{section, sections};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketChannel, SocketId};
use safe_cex_venue_grvt::{Grvt, Options};

const NOW: f64 = 1_700_000_000_000.;
const CATALOG: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
const SUMMARY: &str = r#"{"r":{"te":"100","ts2":"80","tu":"2","sas":[{"si":"777"}]}}"#;
const OPEN: &str = r#"{"oi":"0xabcd","im":false,"ro":false,"po":false,"ti":"GOOD_TILL_TIME","l":[{"i":"BTC_USDT_Perp","s":"2","lp":"100","ib":true}],"m":{"co":"9223372036854775807","ct":"1700000000000000000"},"s1":{"s":"OPEN","ts":["0"],"ut":"1700000000001000000"}}"#;
struct Harness {
    engine: Engine<Grvt>,
    requests: Vec<HttpRequest>,
    actions: Vec<Action>,
    socket: Option<SocketId>,
    now: f64,
}
impl Harness {
    fn new(proxy: bool) -> Self {
        Self::with_options(Options {
            session_proxy: proxy,
            ..Options::default()
        })
    }
    fn with_options(options: Options) -> Self {
        let mut engine = Engine::new(Grvt::new(Options {
            key: Some("fixture-api-key".into()),
            secret: Some(format!("0x{}", "01".repeat(32))),
            ..options
        }));
        engine.start(NOW);
        let mut h = Self {
            engine,
            requests: vec![],
            actions: vec![],
            socket: None,
            now: NOW,
        };
        h.collect();
        h.reply("all_instruments", CATALOG, "");
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            if let Action::Http(request) = action {
                self.requests.push(request);
            } else {
                self.actions.push(action);
            }
        }
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.ends_with(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.requests));
        self.requests.remove(index)
    }
    fn respond(&mut self, request: HttpRequest, body: &str, headers: &str) {
        self.now += 1.;
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers,
                    body,
                },
            },
        );
        self.collect();
    }
    fn reply(&mut self, path: &str, body: &str, headers: &str) {
        let request = self.take(path);
        self.respond(request, body, headers);
    }
    fn authenticate(&mut self) {
        self.reply(
            "login",
            r#"{"status":"success"}"#,
            "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
        );
        self.reply("aggregated_account_summary", SUMMARY, "");
        self.reply(
            "get_all_initial_leverage",
            r#"{"r":[{"i":"BTC_USDT_Perp","l":"20","ml":"1","ml1":"50"}]}"#,
            "",
        );
        self.reply("get_authorized_builders", r#"{"results":[]}"#, "");
        self.reply("positions", r#"{"r":[]}"#, "");
        self.reply("open_orders", r#"{"r":[]}"#, "");
        self.socket = self.actions.iter().find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            }
            | Action::WsConnectWithHeaders {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        });
    }
    fn open(&mut self) {
        let socket = self.socket.unwrap();
        self.engine.handle(self.now, Event::WsOpened { socket });
        self.collect();
        if self.actions.iter().any(|a|matches!(a,Action::WsSend {socket:s,text} if *s==socket && text.contains("session_cookie"))) {self.ws(r#"{"type":"auth_success"}"#);}
    }
    fn ws(&mut self, text: &str) {
        self.now += 1.;
        self.engine.handle(
            self.now,
            Event::WsMessage {
                socket: self.socket.unwrap(),
                text,
            },
        );
        self.collect();
    }
    fn timer(&mut self, timer: safe_cex_engine::TimerId, now: f64) {
        self.now = now;
        self.engine.handle(self.now, Event::TimerFired { timer });
        self.collect();
    }
}

#[test]
fn login_uses_edge_and_authenticated_reads_keep_real_urls() {
    let mut h = Harness::new(true);
    let login = h.take("login");
    assert_eq!(login.url, "https://edge.grvt.io/auth/api_key/login");
    assert_eq!(
        login.body.as_deref(),
        Some(r#"{"api_key":"fixture-api-key"}"#)
    );
    assert!(
        login
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("X-Grvt-Remember-Me") && v == "true")
    );
    h.respond(
        login,
        r#"{"status":"success"}"#,
        "X-Ts-Grvt-Session: fixture-session\nX-GRVT-Account-ID: 0xaccount",
    );
    let summary = h.take("aggregated_account_summary");
    assert_eq!(
        summary.url,
        "https://trades.grvt.io/lite/v1/aggregated_account_summary"
    );
    assert!(
        summary
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("cookie")
                && v == "rm=true; gravity=fixture-session")
    );
    assert!(
        summary
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("X-Ts-Grvt-Session") && v == "fixture-session")
    );
    assert!(
        summary
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("X-Grvt-Account-Id") && v == "0xaccount")
    );
}

#[test]
fn set_cookie_is_case_insensitive_and_preserves_equals_in_session() {
    let mut h = Harness::new(true);
    h.reply("login",r#"{"status":"success","sub_account_id":"888"}"#,"SeT-CoOkIe: another=ignored; Path=/\nSet-Cookie: gravity=abc==; Path=/; HttpOnly\nx-grvt-account-id: 0xaccount");
    let summary = h.take("aggregated_account_summary");
    assert!(
        summary
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("cookie") && v == "rm=true; gravity=abc==")
    );
    h.respond(summary, SUMMARY, "");
    let positions = h.take("positions");
    assert_eq!(positions.body.as_deref(), Some(r#"{"sa":"888"}"#));
}

#[test]
fn zero_available_is_zero_and_auto_detected_account_precedes_reads() {
    let mut h = Harness::new(true);
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
    );
    h.reply(
        "aggregated_account_summary",
        r#"{"r":{"te":"100","ts2":"0","sas":[{"si":"9007199254740993"}]}}"#,
        "",
    );
    let balance = h.engine.store().balance();
    assert_eq!(
        (balance.total, balance.free, balance.used),
        (100., 0., 100.)
    );
    let positions = h.take("positions");
    assert_eq!(
        positions.body.as_deref(),
        Some(r#"{"sa":"9007199254740993"}"#)
    );
    let leverage = h.take("get_all_initial_leverage");
    assert_eq!(leverage.body, positions.body);
}

#[test]
fn proxy_auth_is_first_and_native_defaults_use_upgrade_headers() {
    let mut proxy = Harness::new(true);
    proxy.authenticate();
    let socket = proxy.socket.unwrap();
    assert!(proxy.actions.iter().any(|a|matches!(a,Action::WsConnect {socket:s,url,channel:Some(SocketChannel::Private),..} if *s==socket && url=="wss://trades.grvt.io/ws")));
    proxy.engine.handle(proxy.now, Event::WsOpened { socket });
    proxy.collect();
    let messages: Vec<_> = proxy
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].contains(r#""session_cookie":"rm=true; gravity=fixture-session""#));
    proxy.ws(r#"{"type":"auth_success"}"#);
    let messages: Vec<_> = proxy
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 4);
    assert!(messages[1].contains(r#""stream":"v1.order""#));
    assert!(messages[1].contains(r#""feed":["777"]"#));
    let mut native = Harness::with_options(Options::default());
    native.authenticate();
    let socket = native.socket.unwrap();
    assert!(native.actions.iter().any(|a|matches!(a,Action::WsConnectWithHeaders {socket:s,url,headers,channel:Some(SocketChannel::Private),..} if *s==socket && url=="wss://trades.grvt.io/ws" && headers.iter().any(|(k,v)|*k=="Cookie" && v=="rm=true; gravity=fixture-session"))));
    native.open();
    let messages: Vec<_> = native
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket: s, text } if *s == socket => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 3);
    assert!(
        messages
            .iter()
            .all(|text| text.contains(r#""method":"subscribe""#))
    );
}

#[test]
fn orders_preserve_uint64_client_identity_and_terminal_events_beat_stale_reads() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("positions", r#"{"r":[]}"#, "");
    h.reply("open_orders", &format!("{{\"r\":[{OPEN}]}}"), "");
    let order = &h.engine.store().orders()[0];
    assert_eq!(order.id, "9223372036854775807");
    assert_eq!(order.side, OrderSide::Buy);
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.timer(timer, h.now + 1000.);
    let stale = h.take("open_orders");
    let closed = OPEN
        .replace("\"OPEN\"", "\"FILLED\"")
        .replace("\"ts\":[\"0\"]", "\"ts\":[\"2\"]");
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{closed}}}"));
    assert!(h.engine.store().orders().is_empty());
    h.respond(stale, &format!("{{\"r\":[{OPEN}]}}"), "");
    assert!(h.engine.store().orders().is_empty());
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn authoritative_empty_reconciliation_removes_orders_and_malformed_does_not() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("open_orders", &format!("{{\"r\":[{OPEN}]}}"), "");
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.timer(timer, h.now + 1000.);
    h.reply("open_orders", r#"{"r":[{"oi":"broken"}]}"#, "");
    assert_eq!(h.engine.store().orders().len(), 1);
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.timer(timer, h.now + 1000.);
    h.reply("open_orders", r#"{"r":[]}"#, "");
    assert!(h.engine.store().orders().is_empty());
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    assert!(h.engine.store().orders().is_empty());
}

#[test]
fn same_client_id_new_lifecycle_is_live_and_old_terminal_cannot_remove_it() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let closed = OPEN
        .replace("\"OPEN\"", "\"FILLED\"")
        .replace("\"ts\":[\"0\"]", "\"ts\":[\"2\"]");
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{closed}}}"));
    let next = OPEN
        .replace("0xabcd", "0xbeef")
        .replace("1700000000000000000", "1700000000100000000")
        .replace("1700000000001000000", "1700000000110000000");
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{next}}}"));
    assert_eq!(h.engine.store().orders().len(), 1);
    let delayed = closed.replace("1700000000001000000", "1700000000120000000");
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{delayed}}}"));
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(h.engine.store().orders()[0].filled, 0.);
}

#[test]
fn exact_nanoseconds_and_current_margin_survive_rest_then_older_stream() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let newer = r#"{"i":"BTC_USDT_Perp","s":"-2","ep":"100","ep1":"90","n":"200","l":"20","el":"140","up":"-2","mt":"CROSS","et":"1700000000000000900"}"#;
    h.reply("positions", &format!("{{\"r\":[{newer}]}}"), "");
    let old = newer
        .replace("\"s\":\"-2\"", "\"s\":\"-3\"")
        .replace("1700000000000000900", "1700000000000000100");
    h.ws(&format!("{{\"s\":\"v1.position\",\"f\":{old}}}"));
    assert_eq!(h.engine.store().positions()[0].contracts, 2.);
    assert_eq!(
        h.engine.store().positions()[0].margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
    assert_eq!(
        h.engine
            .store()
            .setting("BTC_USDT_PERP")
            .unwrap()
            .margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
}

#[test]
fn private_sequence_snapshot_ack_duplicates_and_gap_reconcile() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("positions", r#"{"r":[]}"#, "");
    h.reply("open_orders", r#"{"r":[]}"#, "");
    h.ws(r#"{"stream":"v1.order","subs":["777-BTC_USDT_Perp"],"num_snapshots":[1],"first_sequence_number":[100]}"#);
    h.ws(&format!(
        "{{\"s\":\"v1.order\",\"s1\":\"BTC_USDT_Perp\",\"sn\":\"0\",\"f\":{OPEN}}}"
    ));
    let filled = OPEN.replace("\"ts\":[\"0\"]", "\"ts\":[\"1\"]");
    h.ws(&format!(
        "{{\"s\":\"v1.order\",\"s1\":\"BTC_USDT_Perp\",\"sn\":\"100\",\"f\":{filled}}}"
    ));
    h.ws(&format!(
        "{{\"s\":\"v1.order\",\"s1\":\"BTC_USDT_Perp\",\"sn\":\"100\",\"f\":{OPEN}}}"
    ));
    assert_eq!(h.engine.store().orders()[0].filled, 1.);
    h.ws(&format!(
        "{{\"s\":\"v1.order\",\"s1\":\"BTC_USDT_Perp\",\"sn\":\"102\",\"f\":{OPEN}}}"
    ));
    assert_eq!(h.engine.store().orders()[0].filled, 1.);
    let socket = h.socket.unwrap();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose {socket:s} if *s==socket))
    );
    assert!(h.requests.iter().any(|r| r.url.ends_with("open_orders")));
    assert!(h.requests.iter().any(|r| r.url.ends_with("positions")));
}

#[test]
fn missing_expected_order_array_preserves_good_state() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    h.reply(
        "open_orders",
        r#"{"message":"upstream temporarily unavailable"}"#,
        "",
    );
    assert_eq!(h.engine.store().orders().len(), 1);
}

#[test]
fn position_entry_and_exit_fields_are_distinct_and_zero_close_cannot_reopen_from_old_event() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("positions", r#"{"r":[]}"#, "");
    let position = r#"{"i":"BTC_USDT_Perp","s":"-2","ep":"100","ep1":"90","n":"200","l":"20","el":"140","up":"-2","et":"1700000000100000000"}"#;
    h.ws(&format!("{{\"s\":\"v1.position\",\"f\":{position}}}"));
    let p = &h.engine.store().positions()[0];
    assert_eq!(
        (p.side, p.entry_price, p.liquidation_price, p.contracts),
        (PositionSide::Short, 100., 140., 2.)
    );
    assert_eq!(h.engine.store().balance().upnl, -2.);
    h.ws(r#"{"s":"v1.position","f":{"i":"BTC_USDT_Perp","s":"0","et":"1700000000200000000"}}"#);
    assert!(h.engine.store().positions().is_empty());
    h.ws(&format!("{{\"s\":\"v1.position\",\"f\":{position}}}"));
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(
        h.engine.store().setting("BTC_USDT_PERP").unwrap().leverage,
        20.
    );
}

#[test]
fn each_native_fill_emits_one_execution_and_duplicate_trade_ids_are_suppressed() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let fill = r#"{"s":"v1.fill","f":{"i":"BTC_USDT_Perp","ti":"9007199254740993","co":"9223372036854775807","oi":"0xabcd","ib":true,"s":"0.2","p":"100"}}"#;
    h.ws(fill);
    h.ws(fill);
    h.ws(r#"{"s":"v1.fill","f":{"i":"BTC_USDT_Perp","ti":"9007199254740994","co":"9223372036854775807","ib":true,"s":"0.3","p":"110"}}"#);
    h.ws(r#"{"s":"v1.fill","f":{"i":"BTC_USDT_Perp","ti":"9007199254740995","ib":"false","s":"2","p":"100"}}"#);
    let frame = h.engine.take_frame(h.now).unwrap();
    let (_, fills) = sections(frame)
        .find(|(kind, _)| *kind == section::FILLS)
        .unwrap();
    assert_eq!(u32::from_le_bytes(fills[..4].try_into().unwrap()), 2);
}

#[test]
fn expired_private_session_refreshes_once_then_fails_closed() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.ws(r#"{"error":{"code":1000}}"#);
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.url.ends_with("login"))
            .count(),
        1
    );
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: new-session\nx-grvt-account-id: 0xaccount",
    );
    h.socket = h.actions.iter().rev().find_map(|a| match a {
        Action::WsConnect {
            socket,
            channel: Some(SocketChannel::Private),
            ..
        } => Some(*socket),
        _ => None,
    });
    h.open();
    h.ws(r#"{"error":{"code":1000}}"#);
    assert!(h.requests.iter().all(|r| !r.url.ends_with("login")));
    let socket = h.socket.unwrap();
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose {socket:s} if *s==socket))
    );
}

#[test]
fn adversarial_old_request_rejection_reuses_fresh_session() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let old_orders = h.take("open_orders");
    let old_positions = h.take("positions");
    h.respond(old_orders, r#"{"c":1000,"m":"expired"}"#, "");
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: session-new\nx-grvt-account-id: 0xaccount",
    );
    h.actions.clear();
    h.respond(old_positions, r#"{"c":1000,"m":"expired old session"}"#, "");
    assert!(
        !h.requests.iter().any(|r| r.url.ends_with("login")),
        "late old-session rejection starts a second login and tears down fresh session"
    );
}
#[test]
fn adversarial_stale_leverage_read_cannot_revert_stream_margin() {
    let mut h = Harness::new(true);
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
    );
    h.reply("aggregated_account_summary", SUMMARY, "");
    h.socket = h.actions.iter().find_map(|a| match a {
        Action::WsConnect {
            socket,
            channel: Some(SocketChannel::Private),
            ..
        } => Some(*socket),
        _ => None,
    });
    h.open();
    h.ws(r#"{"s":"v1.position","f":{"i":"BTC_USDT_Perp","s":"1","ep":"100","n":"100","l":"20","el":"50","up":"0","mt":"CROSS","et":"1700000000000000000"}}"#);
    assert_eq!(
        h.engine
            .store()
            .setting("BTC_USDT_PERP")
            .unwrap()
            .margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
    h.reply(
        "get_all_initial_leverage",
        r#"{"r":[{"i":"BTC_USDT_Perp","l":"10","mt":"ISOLATED"}]}"#,
        "",
    );
    assert!(
        h.requests
            .iter()
            .any(|r| r.url.ends_with("get_all_initial_leverage")),
        "stale configuration read must refresh account rails"
    );
    assert_eq!(
        h.engine
            .store()
            .setting("BTC_USDT_PERP")
            .unwrap()
            .margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
}
#[test]
fn adversarial_login_network_failure_gets_bounded_read_retry() {
    let mut h = Harness::new(true);
    let login = h.take("login");
    h.engine.handle(
        h.now,
        Event::HttpFailed {
            request: login.id,
            error: "fixture connection reset",
        },
    );
    h.collect();
    assert!(
        h.requests.iter().any(|r| r.url.ends_with("login")),
        "login classified non-read and permanently fails authentication on transient network loss"
    );
}

fn fetch_leverage(h: &mut Harness, command: u32) {
    h.engine.handle(
        h.now,
        Event::Command(safe_cex_engine::Command::FetchLeverage {
            command,
            symbol: "BTC_USDT_PERP".into(),
        }),
    );
    h.collect();
}
fn assert_session(request: &HttpRequest, expected: &str) {
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("X-Ts-Grvt-Session") && v == expected),
        "unexpected headers: {:?}",
        request.headers
    );
}
#[test]
fn delayed_session_rejections_resend_once_and_new_queries_wait_for_login() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let orders = h.take("open_orders");
    let positions = h.take("positions");
    h.respond(orders, r#"{"c":1000,"m":"expired"}"#, "");
    fetch_leverage(&mut h, 700);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("get_all_initial_leverage"))
    );
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: session-new\nx-grvt-account-id: 0xaccount",
    );
    let leverage = h.take("get_all_initial_leverage");
    assert_session(&leverage, "session-new");
    h.respond(positions, r#"{"c":1000,"m":"old session"}"#, "");
    let resent = h.take("positions");
    assert_session(&resent, "session-new");
    h.respond(resent, r#"{"c":1000,"m":"still rejected"}"#, "");
    assert!(
        !h.requests.iter().any(|r| r.url.ends_with("login")),
        "second rejection must not reset resend budget"
    );
}
#[test]
fn old_successful_headers_cannot_replace_new_login_and_current_rotation_reconnects() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let orders = h.take("open_orders");
    let positions = h.take("positions");
    h.respond(orders, r#"{"c":1000,"m":"expired"}"#, "");
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: session-new\nx-grvt-account-id: 0xaccount",
    );
    h.actions.clear();
    h.respond(
        positions,
        r#"{"r":[]}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xold",
    );
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
    fetch_leverage(&mut h, 701);
    let read = h.take("get_all_initial_leverage");
    assert_session(&read, "session-new");
    h.respond(
        read,
        r#"{"r":[{"i":"BTC_USDT_Perp","l":"20"}]}"#,
        "x-ts-grvt-session: session-rotated\nx-grvt-account-id: 0xaccount",
    );
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Private),
            ..
        }
    )));
    fetch_leverage(&mut h, 702);
    assert_session(&h.take("get_all_initial_leverage"), "session-rotated");
}
#[test]
fn stale_headers_cannot_resurrect_session_while_refresh_login_is_incomplete() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let orders = h.take("open_orders");
    let positions = h.take("positions");
    h.respond(orders, r#"{"c":1000,"m":"expired"}"#, "");
    h.respond(
        positions,
        r#"{"r":[]}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xold",
    );
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: missing-account",
    );
    assert!(!h.requests.iter().any(|r| r.url.ends_with("open_orders")));
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Log {
            level: safe_cex_engine::LogLevel::Error,
            ..
        })
    )));
}
#[test]
fn login_network_retry_exhaustion_backs_off_and_recovers_without_terminal_auth_error() {
    let mut h = Harness::new(true);
    for _ in 0..6 {
        let login = h.take("login");
        h.engine.handle(
            h.now,
            Event::HttpFailed {
                request: login.id,
                error: "fixture reset",
            },
        );
        h.collect();
    }
    assert!(!h.requests.iter().any(|r| r.url.ends_with("login")));
    assert!(!h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Log {
            level: safe_cex_engine::LogLevel::Error,
            ..
        })
    )));
    let retry = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 5000. => Some(*timer),
            _ => None,
        })
        .expect("bounded retry timer");
    fetch_leverage(&mut h, 703);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.ends_with("login") || r.url.ends_with("get_all_initial_leverage"))
    );
    h.timer(retry, h.now + 5000.);
    h.reply(
        "login",
        r#"{"status":"success","sub_account_id":"777"}"#,
        "x-ts-grvt-session: recovered\nx-grvt-account-id: 0xaccount",
    );
    assert_session(&h.take("aggregated_account_summary"), "recovered");
}

fn pong_followup(h: &Harness) -> (safe_cex_engine::TimerId, f64) {
    h.actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 5_000. => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("pong schedules the next ping in five seconds")
}
fn private_pings(h: &Harness) -> Vec<SocketId> {
    h.actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } if text == "ping" => Some(*socket),
            _ => None,
        })
        .collect()
}
fn use_latest_private_socket(h: &mut Harness) {
    h.socket = h.actions.iter().rev().find_map(|action| match action {
        Action::WsConnect {
            socket,
            channel: Some(SocketChannel::Private),
            ..
        }
        | Action::WsConnectWithHeaders {
            socket,
            channel: Some(SocketChannel::Private),
            ..
        } => Some(*socket),
        _ => None,
    });
}

#[test]
fn pong_followup_is_one_shot_and_position_traffic_does_not_cancel_it() {
    for proxy in [true, false] {
        let mut h = Harness::new(proxy);
        h.authenticate();
        h.open();
        h.actions.clear();
        h.ws("pong");
        let (timer, deadline) = pong_followup(&h);
        assert!(private_pings(&h).is_empty());
        h.ws(r#"{"s":"v1.position","f":{"i":"BTC_USDT_Perp","s":"1","ep":"100","n":"100","l":"20","el":"50","up":"0","mt":"CROSS","et":"1700000000000000000"}}"#);
        assert_eq!(h.engine.store().positions().len(), 1);
        assert!(!h.actions.iter().any(
            |action| matches!(action, Action::ClearTimer { timer: cleared } if *cleared == timer)
        ));
        h.timer(timer, deadline);
        assert_eq!(private_pings(&h), [h.socket.unwrap()]);
        h.timer(timer, deadline + 5_000.);
        assert_eq!(
            private_pings(&h),
            [h.socket.unwrap()],
            "follow-up must not become a periodic timer"
        );
    }
}

#[test]
fn repeated_pong_replaces_pending_ping_instead_of_adding_another() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.actions.clear();
    h.ws("pong");
    let (old, old_deadline) = pong_followup(&h);
    h.now += 1_000.;
    h.ws("pong");
    let (current, deadline) = pong_followup(&h);
    assert_ne!(old, current);
    assert!(
        h.actions
            .iter()
            .any(|action| matches!(action, Action::ClearTimer { timer } if *timer == old))
    );
    h.timer(old, old_deadline);
    assert!(private_pings(&h).is_empty());
    h.timer(current, deadline);
    assert_eq!(private_pings(&h), [h.socket.unwrap()]);
}

#[test]
fn close_reauth_and_cookie_rotation_cancel_ping_and_fence_old_generation() {
    for reason in ["close", "reauth", "cookie rotation"] {
        let mut h = Harness::new(true);
        h.authenticate();
        h.open();
        h.actions.clear();
        h.ws("pong");
        let (old_timer, _) = pong_followup(&h);
        let old_socket = h.socket.unwrap();
        match reason {
            "close" => {
                h.engine.handle(
                    h.now,
                    Event::WsClosed {
                        socket: old_socket,
                        code: 1006,
                    },
                );
                h.collect();
                let reconnect = h
                    .actions
                    .iter()
                    .rev()
                    .find_map(|action| match action {
                        Action::SetTimer { timer, at_ms } if *at_ms == h.now + 1_000. => {
                            Some(*timer)
                        }
                        _ => None,
                    })
                    .expect("reconnect timer");
                h.timer(reconnect, h.now + 1_000.);
            }
            "reauth" => {
                h.ws(r#"{"type":"error","code":1000}"#);
                h.reply(
                    "login",
                    r#"{"status":"success"}"#,
                    "x-ts-grvt-session: replacement\nx-grvt-account-id: 0xaccount",
                );
            }
            _ => {
                fetch_leverage(&mut h, 704);
                let request = h.take("get_all_initial_leverage");
                h.respond(
                    request,
                    r#"{"r":[{"i":"BTC_USDT_Perp","l":"20"}]}"#,
                    "x-ts-grvt-session: rotated\nx-grvt-account-id: 0xaccount",
                );
            }
        }
        assert!(
            h.actions.iter().any(
                |action| matches!(action, Action::ClearTimer { timer } if *timer == old_timer)
            ),
            "{reason} must clear the old ping"
        );
        use_latest_private_socket(&mut h);
        assert_ne!(h.socket.unwrap(), old_socket, "{reason}");
        h.open();
        h.ws("pong");
        let (new_timer, deadline) = pong_followup(&h);
        h.engine.handle(
            h.now,
            Event::WsMessage {
                socket: old_socket,
                text: "pong",
            },
        );
        h.collect();
        assert!(
            !h.actions.iter().any(
                |action| matches!(action, Action::ClearTimer { timer } if *timer == new_timer)
            ),
            "stale socket pong cannot replace a new generation timer"
        );
        h.timer(old_timer, h.now + 1.);
        assert!(
            private_pings(&h).is_empty(),
            "{reason}: stale timer sent ping"
        );
        h.timer(new_timer, deadline);
        assert_eq!(private_pings(&h), [h.socket.unwrap()], "{reason}");
    }
}

#[test]
fn unauthenticated_or_rejected_private_stream_cannot_schedule_ping() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.engine.handle(
        h.now,
        Event::WsOpened {
            socket: h.socket.unwrap(),
        },
    );
    h.collect();
    h.actions.clear();
    h.ws("pong");
    assert!(
        !h.actions
            .iter()
            .any(|action| matches!(action, Action::SetTimer { .. }))
    );
    h.ws(r#"{"type":"error","code":1000}"#);
    h.reply(
        "login",
        r#"{"status":"success"}"#,
        "x-ts-grvt-session: replacement\nx-grvt-account-id: 0xaccount",
    );
    use_latest_private_socket(&mut h);
    h.open();
    h.ws("pong");
    let (timer, deadline) = pong_followup(&h);
    h.ws(r#"{"type":"error","code":1000}"#);
    assert!(h.actions.iter().any(
        |action| matches!(action, Action::ClearTimer { timer: cleared } if *cleared == timer)
    ));
    h.ws("pong");
    h.timer(timer, deadline);
    assert!(private_pings(&h).is_empty());
    assert!(
        !h.requests
            .iter()
            .any(|request| request.url.ends_with("login")),
        "second auth rejection must stay closed"
    );
}

#[test]
fn undiscovered_product_events_cannot_mutate_perpetual_account_or_consume_its_fill() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let position = r#"{"i":"BTC_USDT_Perp","s":"1","ep":"100","n":"100","l":"20","el":"50","up":"0","mt":"CROSS","et":"1700000000100000000"}"#;
    h.ws(&format!("{{\"s\":\"v1.position\",\"f\":{position}}}"));
    h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{OPEN}}}"));
    let fill = r#"{"i":"BTC_USDT_Perp","ti":"77","co":"9223372036854775807","oi":"0xabcd","ib":true,"s":"0.2","p":"100"}"#;
    h.engine.take_frame(h.now);

    // Deliberately share the supported market's base, quote, client ID and
    // trade ID: only exact discovered instrument identities may resolve.
    for instrument in [
        "BTC_USDT_20261225_Future",
        "BTC_USDT_20261225_100000_Call",
        "BTC_USDT_20261225_100000_Put",
    ] {
        let unknown_position = position
            .replace("BTC_USDT_Perp", instrument)
            .replace("\"s\":\"1\"", "\"s\":\"0\"")
            .replace("\"l\":\"20\"", "\"l\":\"99\"")
            .replace("CROSS", "ISOLATED");
        h.ws(&format!(
            "{{\"s\":\"v1.position\",\"f\":{unknown_position}}}"
        ));
        let unknown_order = OPEN
            .replace("BTC_USDT_Perp", instrument)
            .replace("\"OPEN\"", "\"FILLED\"")
            .replace("\"ts\":[\"0\"]", "\"ts\":[\"2\"]");
        h.ws(&format!("{{\"s\":\"v1.order\",\"f\":{unknown_order}}}"));
        let unknown_fill = fill.replace("BTC_USDT_Perp", instrument);
        h.ws(&format!("{{\"s\":\"v1.fill\",\"f\":{unknown_fill}}}"));
        assert!(
            h.engine
                .store()
                .setting(&instrument.to_uppercase())
                .is_none()
        );
    }
    assert_eq!(h.engine.store().positions().len(), 1);
    assert_eq!(h.engine.store().positions()[0].contracts, 1.);
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(h.engine.store().orders()[0].filled, 0.);
    let setting = h.engine.store().setting("BTC_USDT_PERP").unwrap();
    assert_eq!(setting.leverage, 20.);
    assert_eq!(
        setting.margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
    if let Some(frame) = h.engine.take_frame(h.now) {
        assert!(!sections(frame).any(|(kind, _)| kind == section::FILLS));
    }
    h.ws(&format!("{{\"s\":\"v1.fill\",\"f\":{fill}}}"));
    let frame = h.engine.take_frame(h.now).unwrap();
    let (_, fills) = sections(frame)
        .find(|(kind, _)| *kind == section::FILLS)
        .unwrap();
    assert_eq!(u32::from_le_bytes(fills[..4].try_into().unwrap()), 1);
}

#[test]
fn undiscovered_products_in_rest_snapshots_do_not_alias_perpetuals() {
    for instrument in [
        "BTC_USDT_20261225_Future",
        "BTC_USDT_20261225_100000_Call",
        "BTC_USDT_20261225_100000_Put",
    ] {
        let mut h = Harness::new(true);
        h.reply(
            "login",
            r#"{"status":"success"}"#,
            "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
        );
        h.reply("aggregated_account_summary", SUMMARY, "");
        h.reply(
            "get_all_initial_leverage",
            &format!(
                "{{\"r\":[{{\"i\":\"BTC_USDT_Perp\",\"l\":\"20\"}},{{\"i\":\"{instrument}\",\"l\":\"99\",\"mt\":\"ISOLATED\"}}]}}"
            ),
            "",
        );
        assert_eq!(
            h.engine.store().setting("BTC_USDT_PERP").unwrap().leverage,
            20.
        );
        assert!(
            h.engine
                .store()
                .setting(&instrument.to_uppercase())
                .is_none()
        );
        h.reply(
            "positions",
            &format!(
                "{{\"r\":[{{\"i\":\"{instrument}\",\"s\":\"3\",\"ep\":\"100\",\"n\":\"300\",\"l\":\"99\",\"el\":\"50\",\"up\":\"0\"}}]}}"
            ),
            "",
        );
        let unknown_order = OPEN.replace("BTC_USDT_Perp", instrument);
        h.reply("open_orders", &format!("{{\"r\":[{unknown_order}]}}"), "");
        assert!(h.engine.store().positions().is_empty());
        assert!(h.engine.store().orders().is_empty());
        assert_eq!(
            h.engine.store().setting("BTC_USDT_PERP").unwrap().leverage,
            20.
        );
    }
}
