use safe_cex_engine::model::{OrderSide, PositionSide};
use safe_cex_engine::wire::{section, sections};
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, PublicFeedPolicy, Response, SocketChannel, SocketId,
};
use safe_cex_venue_grvt::{Grvt, Options};

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 60_000.0;

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
        Self::with_policy(options, PublicFeedPolicy::Direct)
    }
    fn with_policy(options: Options, policy: PublicFeedPolicy) -> Self {
        let mut engine = safe_cex_engine::test_support::seeded_engine(Grvt::new(Options {
            key: Some("fixture-api-key".into()),
            secret: Some(format!("0x{}", "01".repeat(32))),
            ..options
        }));
        engine.set_public_feed_policy(policy).unwrap();
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
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
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
fn shared_quotes_match_direct_private_pnl_and_survive_native_summary_and_catalog_refresh() {
    let mut results = Vec::new();
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Shared] {
        let mut h = Harness::with_policy(Options::default(), policy);
        if policy == PublicFeedPolicy::Shared {
            h.engine
                .apply_public_feed(
                    h.now,
                    serde_json::from_str(r#"{"tickers":[{"symbol":"BTC_USDT_PERP","last":120}]}"#)
                        .unwrap(),
                )
                .unwrap();
            assert!(!h.engine.store().loaded().balance);
            assert!(!h.engine.store().loaded().positions);
        }
        h.authenticate();
        h.open();
        h.reply("positions", r#"{"r":[]}"#, "");
        h.ws(r#"{"s":"v1.position","f":{"i":"BTC_USDT_Perp","s":"2","ep":"100","n":"200","l":"20","el":"50","up":"0","et":"1700000000100000000"}}"#);
        if policy == PublicFeedPolicy::Direct {
            let public = h
                .actions
                .iter()
                .find_map(|action| match action {
                    Action::WsConnect {
                        socket,
                        channel: Some(SocketChannel::Public),
                        ..
                    } => Some(*socket),
                    _ => None,
                })
                .unwrap();
            h.engine.handle(h.now, Event::WsOpened { socket: public });
            h.engine.handle(h.now, Event::WsMessage { socket: public, text: r#"{"s":"v1.ticker.d","s1":"BTC_USDT_Perp","sn":"0","f":{"i":"BTC_USDT_Perp","lp":"120"}}"# });
        } else {
            h.engine
                .apply_public_feed(
                    h.now,
                    serde_json::from_str(r#"{"tickers":[{"symbol":"BTC_USDT_PERP","last":120}]}"#)
                        .unwrap(),
                )
                .unwrap();
        }
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 40.);
        assert_eq!(h.engine.store().balance().upnl, 40.);
        let before = h.engine.store().balance().clone();

        // The native account poll reports stale server PnL, then values its local
        // positions from the same authoritative ticker cache as direct mode.
        let tick = h
            .actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms }
                    if *at_ms > NOW + 59_000. && *at_ms < NOW + 61_000. =>
                {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .unwrap();
        h.timer(tick.0, tick.1);
        h.reply("aggregated_account_summary", SUMMARY, "");
        assert_eq!(h.engine.store().balance(), &before);

        let refresh = h
            .actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms }
                    if *at_ms > NOW + 299_000. && *at_ms < NOW + 301_000. =>
                {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .unwrap();
        h.timer(refresh.0, refresh.1);
        h.reply("all_instruments", CATALOG, "");
        assert_eq!(h.engine.store().ticker("BTC_USDT_PERP").unwrap().last, 120.);
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 40.);
        assert_eq!(h.engine.store().balance(), &before);
        results.push((h.engine.store().positions()[0].unrealized_pnl, before));
    }
    assert_eq!(results[0], results[1]);
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
fn spot_balances_publish_summed_per_currency_balances() {
    let mut h = Harness::new(true);
    h.reply(
        "login",
        r#"{"status":"success","sub_account_id":"777"}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
    );
    h.reply(
        "aggregated_account_summary",
        r#"{"r":{"te":"100","ts2":"80","tu":"2","sb":[{"c":"USDT","b":"60.5"},{"c":"BTC","b":"0.25"},{"c":"USDT","b":"4.5"}]}}"#,
        "",
    );
    let coins: Vec<_> = h
        .engine
        .store()
        .currency_balances()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c.currency.as_str(),
                c.balance.total,
                c.balance.free,
                c.balance.used,
                c.balance.upnl,
            )
        })
        .collect();
    assert_eq!(
        coins,
        [("BTC", 0.25, 0.25, 0., 0.), ("USDT", 65., 65., 0., 0.)]
    );
}

#[test]
fn malformed_spot_balance_row_keeps_aggregate_and_other_coins() {
    let mut h = Harness::new(true);
    h.reply(
        "login",
        r#"{"status":"success","sub_account_id":"777"}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
    );
    h.reply(
        "aggregated_account_summary",
        r#"{"r":{"te":"100","ts2":"80","tu":"2","spot_balances":[{"currency":"ETH","balance":"bad"},{"b":"3"},{"currency":"USDT","balance":"65"}]}}"#,
        "",
    );
    let balance = h.engine.store().balance();
    assert_eq!(
        (balance.total, balance.free, balance.used),
        (100., 80., 20.)
    );
    let coins = h.engine.store().currency_balances().unwrap();
    assert_eq!(coins.len(), 1);
    assert_eq!(
        (coins[0].currency.as_str(), coins[0].balance.total),
        ("USDT", 65.)
    );
    assert_eq!(skip_warnings(&h, "spot balance"), 2);
}

#[test]
fn malformed_spot_balance_row_keeps_its_stored_coin_while_peers_update() {
    let mut h = Harness::new(true);
    h.reply(
        "login",
        r#"{"status":"success","sub_account_id":"777"}"#,
        "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
    );
    h.reply(
        "aggregated_account_summary",
        r#"{"r":{"te":"100","spot_balances":[{"c":"ETH","b":"2"},{"c":"USDT","b":"65"}]}}"#,
        "",
    );
    let tick = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 60_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.timer(tick, h.now + 60_000.);
    h.reply(
        "aggregated_account_summary",
        r#"{"r":{"te":"100","spot_balances":[{"c":"ETH","b":"bad"},{"c":"USDT","b":"70"}]}}"#,
        "",
    );
    let coins: Vec<_> = h
        .engine
        .store()
        .currency_balances()
        .unwrap()
        .iter()
        .map(|c| (c.currency.as_str(), c.balance.total))
        .collect();
    assert_eq!(coins, [("USDT", 70.), ("ETH", 2.)]);
}

#[test]
fn proxy_auth_is_first_and_native_defaults_use_upgrade_headers() {
    let mut proxy = Harness::new(true);
    proxy.authenticate();
    let socket = proxy.socket.unwrap();
    assert!(proxy.actions.iter().any(|a|matches!(a,Action::WsConnect {socket:s,url,headers,channel:Some(SocketChannel::Private),..} if *s==socket && url=="wss://trades.grvt.io/ws" && headers.is_empty())));
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
    assert!(native.actions.iter().any(|a|matches!(a,Action::WsConnect {socket:s,url,headers,channel:Some(SocketChannel::Private),..} if *s==socket && url=="wss://trades.grvt.io/ws" && headers.iter().any(|(k,v)|*k=="Cookie" && v=="rm=true; gravity=fixture-session"))));
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
    h.reply("open_orders", r#"{"r":[{"broken":true}]}"#, "");
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
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Error,
            ..
        }))
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
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Error,
            ..
        }))
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
                        Action::SetTimer { timer, at_ms }
                            if (*at_ms - h.now >= 750.) && (*at_ms - h.now <= 1000.) =>
                        {
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

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let mut h = Harness::new(true);
    h.authenticate();
    let mut e = h.engine;
    let now = NOW + 1000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let mut obsolete = stale;
    let mut current = None;
    for interval in [100_000_f64, 1_500.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(0.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending
            .iter()
            .any(|r| r.url.ends_with("aggregated_account_summary")),
        "actual native account request"
    );
    // Pause while REST is in flight. Completion may finish the current read
    // chain, but must not recreate an active recurring host timer.
    e.handle(
        due,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    e.drain_actions().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = SUMMARY;
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            pending.extend(e.drain_actions().filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            }));
        }
    }
    assert!(pending.is_empty(), "all fixture account reads complete");
    e.handle(
        due + 2.,
        Event::PollScaleChanged {
            scale: 10_000. / POLL_MS,
        },
    );
    let resumed: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(0.0));
}
#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn private_supervisor_retries_flaps_and_bounds_never_open_handshakes() {
    for proxy in [false, true] {
        let mut h = Harness::new(proxy);
        h.authenticate();
        h.open();
        socket_checks::flaps(h.engine, h.socket.unwrap(), h.now);
    }
}

#[test]
fn proxy_pongs_cannot_extend_private_authentication_deadline() {
    let mut h = Harness::new(true);
    h.authenticate();
    let socket = h.socket.unwrap();
    let opened = h.now;
    h.engine.handle(opened, Event::WsOpened { socket });
    h.collect();
    let mut timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == opened + 5000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    for offset in [5000., 10_000., 15_000.] {
        h.now = opened + offset;
        h.engine.handle(
            h.now,
            Event::WsMessage {
                socket,
                text: "pong",
            },
        );
        h.engine.handle(h.now, Event::TimerFired { timer });
        h.collect();
        if offset < 15_000. {
            timer = h
                .actions
                .iter()
                .rev()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == h.now + 5000. => Some(*timer),
                    _ => None,
                })
                .unwrap();
        }
    }
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    assert!(
        private_pings(&h).is_empty(),
        "no unauthenticated application ping"
    );
}
fn skip_warnings(h: &Harness, kind: &str) -> usize {
    h.actions
        .iter()
        .filter(|a| {
            matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Warn, message, .. })) if message.contains(&format!("GRVT {kind} row")))
        })
        .count()
}
fn next_orders_refresh(h: &mut Harness) {
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
}
#[test]
fn unknown_instrument_position_row_is_skipped_without_hiding_other_positions() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let btc = r#"{"i":"BTC_USDT_Perp","s":"-2","ep":"100","ep1":"90","n":"200","l":"20","el":"140","up":"-2","mt":"CROSS","et":"1700000000000000900"}"#;
    let foreign = btc.replace("BTC_USDT_Perp", "DOGE_USDT_Perp");
    h.reply("positions", &format!("{{\"r\":[{foreign},{btc}]}}"), "");
    let positions = h.engine.store().positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, "BTC_USDT_PERP");
    assert_eq!(skip_warnings(&h, "position"), 1);
}
#[test]
fn unknown_instrument_order_row_is_skipped_and_known_identity_keeps_stored_copy() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let foreign = OPEN
        .replace("BTC_USDT_Perp", "DOGE_USDT_Perp")
        .replace("0xabcd", "0xf00d")
        .replace("9223372036854775807", "77");
    h.reply("open_orders", &format!("{{\"r\":[{foreign},{OPEN}]}}"), "");
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(skip_warnings(&h, "order"), 1);
    next_orders_refresh(&mut h);
    // The known order's row is now unmappable; a new order arrives beside it.
    let unmappable = OPEN.replace("\"OPEN\"", "\"UNHEARD_OF\"");
    let fresh = OPEN
        .replace("0xabcd", "0xbeef")
        .replace("9223372036854775807", "88");
    h.reply(
        "open_orders",
        &format!("{{\"r\":[{unmappable},{fresh}]}}"),
        "",
    );
    let mut ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, ["88", "9223372036854775807"]);
    assert_eq!(skip_warnings(&h, "order"), 2);
}
#[test]
fn unidentified_order_row_keeps_stored_orders_and_commits_its_peers() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("open_orders", &format!("{{\"r\":[{OPEN}]}}"), "");
    next_orders_refresh(&mut h);
    let fresh = OPEN
        .replace("0xabcd", "0xbeef")
        .replace("9223372036854775807", "88");
    h.reply(
        "open_orders",
        &format!("{{\"r\":[{{\"broken\":true}},{fresh}]}}"),
        "",
    );
    let mut ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, ["88", "9223372036854775807"]);
}
#[test]
fn unidentified_position_row_keeps_stored_positions_and_commits_its_peers() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let btc = r#"{"i":"BTC_USDT_Perp","s":"-2","ep":"100","n":"200","l":"20","el":"140","up":"-2","mt":"CROSS","et":"1700000000000000900"}"#;
    h.ws(r#"{"s":"v1.position","f":{}}"#);
    let timer = |h: &Harness| {
        h.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now + 500. => Some(*timer),
                _ => None,
            })
            .unwrap()
    };
    h.timer(timer(&h), h.now + 500.);
    h.reply("positions", &format!("{{\"r\":[{btc}]}}"), "");
    assert_eq!(h.engine.store().positions()[0].contracts, 2.);
    h.ws(r#"{"s":"v1.position","f":{}}"#);
    h.timer(timer(&h), h.now + 500.);
    let grown = btc
        .replace("\"s\":\"-2\"", "\"s\":\"-3\"")
        .replace("1700000000000000900", "1700000000000001900");
    h.reply(
        "positions",
        &format!("{{\"r\":[{{\"broken\":true}},{grown}]}}"),
        "",
    );
    let positions = h.engine.store().positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].contracts, 3.);
}
#[test]
fn rejected_position_row_logs_an_error_only_when_no_stored_copy_is_kept() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let btc = r#"{"i":"BTC_USDT_Perp","s":"-2","ep":"100","n":"200","l":"20","el":"140","up":"-2","mt":"CROSS","et":"1700000000000000900"}"#;
    let timer = |h: &Harness| {
        h.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == h.now + 500. => Some(*timer),
                _ => None,
            })
            .unwrap()
    };
    let poll = |h: &mut Harness, rows: &str| {
        h.ws(r#"{"s":"v1.position","f":{}}"#);
        h.timer(timer(h), h.now + 500.);
        let start = h.actions.len();
        h.reply("positions", &format!("{{\"r\":[{rows}]}}"), "");
        h.actions[start..]
            .iter()
            .filter_map(|a| match a {
                Action::Notice(safe_cex_engine::Notice::Event(event))
                    if event.message.contains("malformed GRVT position row") =>
                {
                    Some(event.level)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    use safe_cex_engine::Level;
    assert_eq!(poll(&mut h, r#"{"broken":true}"#), [Level::Error]);
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(poll(&mut h, btc), []);
    assert_eq!(poll(&mut h, r#"{"broken":true}"#), [Level::Warn]);
    assert_eq!(poll(&mut h, btc), []);
    let bad = btc.replace("\"s\":\"-2\"", "\"s\":null");
    assert_eq!(poll(&mut h, &bad), [Level::Warn]);
    assert_eq!(h.engine.store().positions()[0].contracts, 2.);
}
const FOREIGN_MALFORMED: &str = r#"{"r":[{"oi":"0x999","m":{"co":"66"}}]}"#;
#[test]
fn readable_unmatched_order_row_is_foreign_once_every_native_id_is_known() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("open_orders", &format!("{{\"r\":[{OPEN}]}}"), "");
    next_orders_refresh(&mut h);
    h.reply("open_orders", FOREIGN_MALFORMED, "");
    assert!(h.engine.store().orders().is_empty());
    assert_eq!(skip_warnings(&h, "order"), 1);
}
#[test]
fn readable_unmatched_order_row_retains_orders_while_a_native_id_is_unknown() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("open_orders", &format!("{{\"r\":[{OPEN}]}}"), "");
    let mut unmapped = h.engine.store().orders()[0].clone();
    unmapped.id = "55".into();
    unmapped.client_order_id = Some("55".into());
    h.engine.store_mut().upsert_order(unmapped);
    next_orders_refresh(&mut h);
    h.reply("open_orders", FOREIGN_MALFORMED, "");
    let mut ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, ["55", "9223372036854775807"]);
}

#[test]
fn ip_whitelist_rejection_keeps_its_actionable_message() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.ws(r#"{"code":1008,"message":"IP not whitelisted","status":403}"#);
    assert!(h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::Event(event))
            if event.level == safe_cex_engine::Level::Error
                && event.message == "GRVT IP is not whitelisted. Please re-add your API key and whitelist every Tealstreet IP listed in the API key form."
    )));
}

fn reporter_summary(h: &mut Harness, coins: &str) {
    let (timer, at) = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= h.now + 50_000. && *at_ms <= h.now + 60_000. =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("native summary poll");
    h.timer(timer, at);
    h.reply(
        "aggregated_account_summary",
        &format!(r#"{{"r":{{"te":"100","sb":{coins}}}}}"#),
        "",
    );
}
#[test]
fn reporter_coin_duplicate_rearms_its_key_but_non_array_retains_coins_and_memory() {
    let mut h = Harness::new(true);
    h.authenticate();
    reporter_summary(&mut h, r#"[{"c":"ETH","b":"2"}]"#);
    let mixed = r#"[{"c":"ETH","b":"bad"},{"c":"ETH","b":"9"},{}]"#;
    reporter_summary(&mut h, mixed);
    assert_eq!(skip_warnings(&h, "spot balance"), 2);
    assert_eq!(
        h.engine.store().currency_balances().unwrap()[0]
            .balance
            .total,
        2.
    );
    reporter_summary(&mut h, mixed);
    // Native coin totals rearm ETH even though rejection retains the stored 2.
    assert_eq!(skip_warnings(&h, "spot balance"), 3);
    reporter_summary(&mut h, r#"[{"c":"ETH","b":"bad"},{}]"#);
    assert_eq!(skip_warnings(&h, "spot balance"), 4);
    reporter_summary(&mut h, "{}"); // Valid outer JSON, unreadable coin list.
    assert_eq!(
        h.engine.store().currency_balances().unwrap()[0]
            .balance
            .total,
        2.
    );
    reporter_summary(&mut h, r#"[{"c":"ETH","b":"bad"},{}]"#);
    assert_eq!(skip_warnings(&h, "spot balance"), 4);
    reporter_summary(&mut h, "null"); // Native clean-empty namespace rearm.
    assert!(h.engine.store().currency_balances().unwrap().is_empty());
    reporter_summary(&mut h, r#"[{"c":"ETH","b":"bad"},{}]"#);
    assert_eq!(skip_warnings(&h, "spot balance"), 6);
}
#[test]
fn reporter_clean_coin_namespace_does_not_rearm_rejected_duplicate_order_identity() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    h.reply("open_orders", &format!(r#"{{"r":[{OPEN}]}}"#), "");
    let bad = OPEN.replace(r#""lp":"100""#, r#""lp":"bad""#);
    let mixed = format!(r#"{{"r":[{bad},{OPEN},{{}}]}}"#);
    reporter_reconcile(&mut h, "v1.order", "open_orders", 1000., &mixed);
    assert_eq!(skip_warnings(&h, "order"), 2);
    assert_eq!(h.engine.store().orders().len(), 1);
    reporter_summary(&mut h, "[]");
    reporter_reconcile(&mut h, "v1.order", "open_orders", 1000., &mixed);
    // Unlike coin totals, rejected IDs are filtered out of order rearm.
    assert_eq!(skip_warnings(&h, "order"), 2);
    reporter_reconcile(&mut h, "v1.order", "open_orders", 1000., r#"{"r":[]}"#);
    reporter_reconcile(&mut h, "v1.order", "open_orders", 1000., &mixed);
    assert_eq!(skip_warnings(&h, "order"), 4);
}

fn reporter_reconcile(h: &mut Harness, stream: &str, path: &str, delay: f64, rows: &str) {
    h.ws(&format!(r#"{{"s":"{stream}","f":{{}}}}"#));
    let (timer, at) = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + delay => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("malformed event schedules native reconciliation");
    h.timer(timer, at);
    h.reply(path, rows, "");
}

#[test]
fn reporter_position_duplicate_filters_rearm_but_older_mapped_row_rearms_before_timestamp_check() {
    let mut h = Harness::new(true);
    h.authenticate();
    h.open();
    let old = r#"{"i":"BTC_USDT_Perp","s":"-2","ep":"100","n":"200","l":"20","el":"140","up":"-2","mt":"CROSS","et":"1700000000000000900"}"#;
    let newer = old
        .replace("1700000000000000900", "1700000001000000900")
        .replace(r#""s":"-2""#, r#""s":"-3""#);
    reporter_reconcile(
        &mut h,
        "v1.position",
        "positions",
        500.,
        &format!(r#"{{"r":[{newer}]}}"#),
    );
    let bad = old.replace(r#""s":"-2""#, r#""s":null"#);
    let foreign = bad.replace("BTC_USDT_Perp", "DOGE_USDT_Perp");
    let rejected = format!(r#"{{"r":[{bad},{foreign}]}}"#);
    reporter_reconcile(&mut h, "v1.position", "positions", 500., &rejected);
    assert_eq!(skip_warnings(&h, "position"), 2);
    reporter_reconcile(
        &mut h,
        "v1.position",
        "positions",
        500.,
        &format!(r#"{{"r":[{bad},{old},{foreign}]}}"#),
    );
    assert_eq!(skip_warnings(&h, "position"), 2);
    // Current response, but an older mapped row: native rearm precedes its
    // timestamp check. The unrelated named rejection remains remembered.
    reporter_reconcile(
        &mut h,
        "v1.position",
        "positions",
        500.,
        &format!(r#"{{"r":[{old},{foreign}]}}"#),
    );
    assert_eq!(h.engine.store().positions()[0].contracts, 3.);
    reporter_reconcile(&mut h, "v1.position", "positions", 500., &rejected);
    assert_eq!(skip_warnings(&h, "position"), 3);
}
