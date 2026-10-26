use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId,
};
use safe_cex_venue_coinbase::{Coinbase, Options};

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 5_000.0;

const SPOT: &str = r#"{"products":[{"product_id":"BTC-USD","product_type":"SPOT","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1"}]}"#;
const INTX: &str = r#"{"products":[{"product_id":"BTC-PERP-INTX","product_type":"FUTURE","base_currency_id":"BTC","quote_currency_id":"USDC","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1","future_product_details":{"contract_expiry_type":"PERPETUAL","contract_root_unit":"BTC","contract_size":"1"}}]}"#;
const CFM: &str = r#"{"products":[{"product_id":"BTC-CDE","product_type":"FUTURE","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"1","price_increment":"0.01","base_min_size":"1","base_max_size":"100","price":"100","volume_24h":"2","price_percentage_change_24h":"1","future_product_details":{"contract_expiry_type":"EXPIRING","contract_root_unit":"BTC","contract_size":"0.01","perpetual_details":{"funding_time":"2025-12-15T12:00:00Z","open_interest":"30"}}}]}"#;
const CFM_BALANCE: &str = r#"{"balance_summary":{"total_usd_balance":{"value":"10"},"available_margin":{"value":"8"},"unrealized_pnl":{"value":"1.5"}}}"#;
const BREAKDOWN: &str = r#"{"breakdown":{"spot_positions":[{"asset":"BTC","total_balance_crypto":1.5,"available_to_trade_crypto":1,"total_balance_fiat":150}]}}"#;
const INTX_BALANCE: &str =
    r#"{"summary":{"total_balance":{"value":"1000"},"buying_power":{"value":"800"}}}"#;
const INTX_POSITIONS: &str = r#"{"positions":[{"symbol":"BTC-PERP-INTX","position_side":"POSITION_SIDE_LONG","net_size":"2","entry_vwap":{"value":"100"},"mark_price":{"value":"110"},"liquidation_price":{"value":"50"},"leverage":"10"}]}"#;
const CFM_POSITIONS: &str = r#"{"positions":[{"product_id":"BTC-CDE","product_type":"FUTURE","side":"SHORT","number_of_contracts":"2","avg_entry_price":"100","current_price":"90","unrealized_pnl":"0.2"}]}"#;

struct Session {
    e: Engine<Coinbase>,
    timers: Vec<(f64, TimerId)>,
    now: f64,
    private: Option<SocketId>,
    urls: Vec<String>,
    kind: &'static str,
    positions: Option<String>,
    products: Option<String>,
    orders: Option<String>,
    breakdown: Option<String>,
    logs: Vec<String>,
    /// Logs at `Level::Error`, also present in `logs`.
    errors: Vec<String>,
    sends: Vec<(SocketId, String)>,
    // Private sockets only; the unopened public feed has its own supervisor.
    closes: Vec<SocketId>,
    privates: Vec<SocketId>,
}
impl Session {
    fn new(kind: &'static str, spot: bool) -> Self {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            Coinbase::new(Options {
                key: "organizations/test/apiKeys/test".into(),
                secret: STANDARD.encode([7; 32]),
                spot_only: spot,
                ..Default::default()
            })
            .unwrap(),
        );
        e.start(1000.0);
        Self {
            e,
            timers: vec![],
            now: 1000.0,
            private: None,
            urls: vec![],
            kind,
            positions: None,
            products: None,
            orders: None,
            breakdown: None,
            logs: vec![],
            errors: vec![],
            sends: vec![],
            closes: vec![],
            privates: vec![],
        }
    }
    fn reply(&mut self, r: HttpRequest, status: u16, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        )
    }
    fn collect(&mut self) -> Vec<HttpRequest> {
        let mut requests = vec![];
        for a in self.e.drain_actions().collect::<Vec<_>>() {
            match a {
                Action::Http(r) => {
                    self.urls.push(r.url.clone());
                    requests.push(r)
                }
                Action::SetTimer { timer, at_ms } => self.timers.push((at_ms, timer)),
                Action::ClearTimer { timer } => self.timers.retain(|(_, t)| *t != timer),
                Action::WsConnect { socket, url, .. } if url.contains("ws-user") => {
                    self.private = Some(socket);
                    self.privates.push(socket)
                }
                Action::Notice(safe_cex_engine::Notice::Event(event)) => {
                    if event.level == safe_cex_engine::Level::Error {
                        self.errors.push(event.message.clone());
                    }
                    self.logs.push(event.message)
                }
                Action::WsSend { socket, text } => self.sends.push((socket, text)),
                Action::WsClose { socket } if self.privates.contains(&socket) => {
                    self.closes.push(socket)
                }
                _ => {}
            }
        }
        requests
    }
    fn fire(&mut self) {
        self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, t) = self.timers.remove(0);
        self.now = self.now.max(at);
        self.e.handle(self.now, Event::TimerFired { timer: t });
    }
    fn answer(&mut self, r: HttpRequest) {
        let url = &r.url;
        let body = if url.ends_with("/portfolios") {
            format!(
                r#"{{"portfolios":[{{"uuid":"portfolio-one","type":"{}"}}]}}"#,
                self.kind
            )
        } else if url.contains("/cfm/balance_summary") {
            CFM_BALANCE.into()
        } else if url.contains("/portfolios/portfolio-one") {
            self.breakdown.clone().unwrap_or_else(|| BREAKDOWN.into())
        } else if url.contains("/intx/portfolio/") {
            INTX_BALANCE.into()
        } else if url.contains("/intx/positions/") {
            self.positions
                .clone()
                .unwrap_or_else(|| INTX_POSITIONS.into())
        } else if url.contains("/cfm/positions") {
            self.positions
                .clone()
                .unwrap_or_else(|| CFM_POSITIONS.into())
        } else if url.contains("/orders/historical/batch") {
            self.orders
                .clone()
                .unwrap_or_else(|| r#"{"orders":[],"has_next":false}"#.into())
        } else if url.contains("/products") {
            if url.contains("contract_expiry_type=PERPETUAL") {
                self.products.clone().unwrap_or_else(|| INTX.into())
            } else if url.contains("product_type=FUTURE") {
                CFM.into()
            } else {
                SPOT.into()
            }
        } else if url.contains("international") {
            "[]".into()
        } else {
            panic!("unhandled {}", url)
        };
        self.reply(r, 200, &body)
    }
    fn ready(mut self) -> Self {
        for _ in 0..250 {
            let requests = self.collect();
            if !requests.is_empty() {
                for r in requests {
                    self.answer(r)
                }
                continue;
            }
            let l = self.e.store().loaded();
            if l.markets && l.balance && l.orders && l.positions {
                let socket = self.private.unwrap();
                self.e.handle(self.now, Event::WsOpened { socket });
                return self;
            }
            self.fire()
        }
        panic!("private startup did not finish")
    }
    fn message(&mut self, seq: u64, kind: &str, orders: &str) {
        let text = format!(
            r#"{{"channel":"user","sequence_num":{seq},"events":[{{"type":"{kind}","orders":{orders}}}]}}"#
        );
        self.e.handle(
            self.now + 1.0,
            Event::WsMessage {
                socket: self.private.unwrap(),
                text: &text,
            },
        );
    }
}
impl Session {
    fn next_request(&mut self) -> HttpRequest {
        for _ in 0..10 {
            if let Some(r) = self.collect().pop() {
                return r;
            }
            self.fire();
        }
        panic!("no request");
    }
}
fn user_order(id: &str, product: &str, status: &str, filled: &str, left: &str) -> String {
    format!(
        r#"{{"order_id":"{id}","client_order_id":"client-{id}","product_id":"{product}","status":"{status}","order_side":"BUY","order_type":"Limit","cumulative_quantity":"{filled}","leaves_quantity":"{left}","limit_price":"100","stop_price":"0","avg_price":"100"}}"#
    )
}

fn fetch_account(session: &mut Session) -> Result<String, String> {
    session.e.handle(
        session.now,
        Event::Command(Command::FetchAccount { command: 9 }),
    );
    session
        .e
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone { command: 9, result }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { command: 9, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("FetchAccount completes")
}

#[test]
fn account_identity_tracks_selected_portfolio_after_discovery() {
    assert_eq!(
        fetch_account(&mut Session::new("DEFAULT", true)),
        Err("Coinbase private portfolio ID is unavailable".into())
    );
    let mut session = Session::new("DEFAULT", true).ready();
    assert_eq!(
        fetch_account(&mut session),
        Ok(r#"{"userId":"portfolio-one"}"#.into())
    );
}
#[test]
fn spot_selection_does_not_probe_cfm_and_preserves_held_currency() {
    let s = Session::new("DEFAULT", true).ready();
    assert!(s.urls.iter().all(|u| !u.contains("/cfm/")));
    let b = &s.e.store().currency_balances().unwrap()[0];
    assert_eq!(
        (b.balance.total, b.balance.free, b.balance.used),
        (1.5, 1.0, 0.5)
    );
    assert!(s.e.store().positions().is_empty());
}
#[test]
fn cfm_contracts_are_normalized_and_settings_are_cross_oneway() {
    let s = Session::new("DEFAULT", false).ready();
    assert_eq!(s.e.store().ticker("BTC-CDE").unwrap().funding_time, 0.0);
    let p = s.e.store().position("BTC-CDE").unwrap();
    assert_eq!(
        (p.contracts, p.notional, p.unrealized_pnl),
        (0.02, 1.8, 0.2)
    );
    assert_eq!(
        p.position_mode,
        Some(safe_cex_engine::model::PositionMode::Oneway)
    );
    assert_eq!(
        p.margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
    assert!(s.e.store().market("BTC-USD").is_some());
}
#[test]
fn intx_balance_upnl_is_recomputed_from_positions() {
    let s = Session::new("INTX", false).ready();
    assert_eq!(s.e.store().balance().upnl, 20.0);
    assert_eq!(s.e.store().balance().total, 1000.0);
}
#[test]
fn user_subscriptions_have_distinct_jwts_without_uri_claims() {
    let mut s = Session::new("INTX", false).ready();
    let sent =
        s.e.drain_actions()
            .filter_map(|a| {
                if let Action::WsSend { text, .. } = a {
                    Some(text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
    assert_eq!(sent.len(), 2);
    assert_ne!(sent[0], sent[1]);
    for text in sent {
        #[derive(serde::Deserialize)]
        struct Sub {
            jwt: String,
        }
        let sub: Sub = serde_json::from_str(&text).unwrap();
        let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(sub.jwt.split('.').nth(1).unwrap())
            .unwrap();
        assert!(!String::from_utf8(claims).unwrap().contains("\"uri\""));
    }
}
#[test]
fn user_snapshot_and_updates_preserve_base_quantities_and_terminal_identity() {
    let mut s = Session::new("DEFAULT", false).ready();
    let row = user_order("order-a", "BTC-CDE", "OPEN", "1", "2");
    s.message(0, "snapshot", &format!("[{row}]"));
    let o = s.e.store().order("order-a").unwrap();
    assert_eq!((o.amount, o.filled, o.remaining), (0.03, 0.01, 0.02));
    assert_eq!(o.extra.as_ref().unwrap().remaining_native, Some(2.0));
    let row = user_order("order-a", "BTC-CDE", "FILLED", "3", "0");
    s.message(1, "update", &format!("[{row}]"));
    let stale = user_order("order-a", "BTC-CDE", "OPEN", "1", "2");
    s.message(2, "update", &format!("[{stale}]"));
    assert_eq!(
        s.e.store().order("order-a").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
}
#[test]
fn invalid_nonempty_snapshot_preserves_prior_orders() {
    let mut s = Session::new("INTX", false).ready();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    s.message(1, "snapshot", r#"[{"order_id":"broken"}]"#);
    assert!(s.e.store().order("order-a").is_some());
    s.message(2, "snapshot", "[]");
    assert!(s.e.store().orders().is_empty());
}
#[test]
fn user_sequence_gap_closes_socket_and_preserves_orders() {
    let mut s = Session::new("INTX", false).ready();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    s.message(2, "update", "[]");
    assert!(s.e.store().order("order-a").is_some());
    let socket = s.private.unwrap();
    assert!(
        s.e.drain_actions()
            .any(|a| matches!(a,Action::WsClose{socket:id} if id==socket))
    );
}
#[test]
fn transient_portfolio_failure_is_retried_without_committing_empty() {
    let mut s = Session::new("INTX", false);
    let r = s.collect().pop().unwrap();
    s.reply(r, 429, "too many requests");
    assert!(!s.e.store().loaded().markets);
    s.collect();
    s.fire();
    let r = s.collect().pop().unwrap();
    assert!(r.url.ends_with("/portfolios"));
}
#[test]
fn connection_sequence_includes_subscriptions_and_heartbeats() {
    let mut s = Session::new("INTX", false).ready();
    for text in [
        r#"{"channel":"subscriptions","sequence_num":0,"events":[{"subscriptions":{"user":[],"heartbeats":[]}}]}"#,
        r#"{"channel":"heartbeats","sequence_num":1,"events":[{"heartbeat_counter":1}]}"#,
    ] {
        s.e.handle(
            s.now + 1.0,
            Event::WsMessage {
                socket: s.private.unwrap(),
                text,
            },
        );
    }
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(2, "snapshot", &format!("[{row}]"));
    assert!(s.e.store().order("order-a").is_some());
    assert!(
        !s.e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}
#[test]
fn fifty_row_snapshot_preserves_update_before_last_batch() {
    let mut s = Session::new("INTX", false).ready();
    let rows = (0..50)
        .map(|i| user_order(&format!("order-{i}"), "BTC-PERP-INTX", "OPEN", "0", "1"))
        .collect::<Vec<_>>()
        .join(",");
    s.message(0, "snapshot", &format!("[{rows}]"));
    assert!(s.e.store().orders().is_empty());
    let update = user_order("order-0", "BTC-PERP-INTX", "OPEN", "0.5", "0.5");
    s.message(1, "update", &format!("[{update}]"));
    let stale = user_order("order-0", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(2, "snapshot", &format!("[{stale}]"));
    assert_eq!(s.e.store().orders().len(), 50);
    assert_eq!(s.e.store().order("order-0").unwrap().filled, 0.5);
}
fn fills(e: &mut Engine<Coinbase>) -> Vec<f64> {
    let Some(bytes) = e.take_frame(100000.0) else {
        return vec![];
    };
    let mut at = 16;
    let count = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    for _ in 0..count {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        if kind == 11 {
            let n = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
            let data = (start + 4 + n * 12 + 7) & !7;
            return (0..n)
                .map(|i| {
                    f64::from_le_bytes(
                        bytes[data + i * 16 + 8..data + i * 16 + 16]
                            .try_into()
                            .unwrap(),
                    )
                })
                .collect();
        }
        at = start + len
    }
    vec![]
}
#[test]
fn cumulative_user_fills_emit_only_new_quantity_once() {
    let mut s = Session::new("INTX", false).ready();
    let initial = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "3");
    s.message(0, "snapshot", &format!("[{initial}]"));
    fills(&mut s.e);
    for (seq, filled, left, status) in [
        (1, "1", "2", "OPEN"),
        (2, "2", "1", "OPEN"),
        (3, "3", "0", "FILLED"),
        (4, "3", "0", "FILLED"),
    ] {
        let row = user_order("order-a", "BTC-PERP-INTX", status, filled, left);
        s.message(seq, "update", &format!("[{row}]"));
    }
    assert_eq!(fills(&mut s.e), vec![1.0, 1.0, 1.0]);
}
#[test]
fn first_live_fill_after_empty_snapshot_is_emitted_once() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    fills(&mut s.e);
    let row = user_order("external-order", "BTC-PERP-INTX", "FILLED", "1", "0");
    s.message(1, "update", &format!("[{row}]"));
    s.message(2, "update", &format!("[{row}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
}
fn held_scan() -> (Session, HttpRequest) {
    let mut s = Session::new("INTX", false);
    for _ in 0..150 {
        let requests = s.collect();
        if requests.is_empty() {
            assert!(!s.timers.is_empty(), "stalled: {:?}", s.urls);
            s.fire();
            continue;
        }
        for r in requests {
            if r.url.contains("orders/historical/batch") {
                let socket = s.private.unwrap();
                s.e.handle(s.now, Event::WsOpened { socket });
                return (s, r);
            }
            s.answer(r)
        }
    }
    panic!("missing order scan")
}
fn complete_scan(s: &mut Session, held: HttpRequest, body: &str) {
    s.reply(held, 200, body);
    for _ in 0..150 {
        let requests = s.collect();
        if requests.is_empty() {
            assert!(!s.timers.is_empty(), "stalled: {:?}", s.urls);
            s.fire();
            continue;
        }
        for r in requests {
            let final_page =
                r.url.contains("orders/historical/batch") && r.url.contains("EDIT_QUEUED");
            if r.url.contains("orders/historical/batch") {
                s.reply(r, 200, body)
            } else {
                s.answer(r)
            }
            if final_page {
                return;
            }
        }
    }
    panic!("scan did not finish")
}
#[test]
fn user_snapshot_survives_older_rest_scan() {
    let (mut s, held) = held_scan();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    complete_scan(&mut s, held, r#"{"orders":[],"has_next":false}"#);
    assert!(s.e.store().order("order-a").is_some());
}
#[test]
fn empty_user_snapshot_removal_survives_older_rest_scan() {
    let (mut s, held) = held_scan();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    s.message(1, "snapshot", "[]");
    complete_scan(
        &mut s,
        held,
        &format!(r#"{{"orders":[{row}],"has_next":false}}"#),
    );
    assert!(s.e.store().orders().is_empty());
}
#[test]
fn incomplete_execution_price_does_not_consume_later_valid_fill() {
    let mut s = Session::new("INTX", false).ready();
    let initial = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{initial}]"));
    fills(&mut s.e);
    let missing = user_order("order-a", "BTC-PERP-INTX", "FILLED", "1", "0")
        .replace(",\"avg_price\":\"100\"", "");
    s.message(1, "update", &format!("[{missing}]"));
    assert!(fills(&mut s.e).is_empty());
    assert_eq!(s.e.store().order("order-a").unwrap().filled, 0.0);
    let valid = user_order("order-a", "BTC-PERP-INTX", "FILLED", "1", "0")
        .replace("\"avg_price\":\"100\"", "\"avg_price\":\"90\"");
    s.message(2, "update", &format!("[{valid}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
}

#[test]
fn cfm_automatic_timers_finish_initial_and_fallback_scans_within_the_request_budget() {
    cfm_timed_scans(false);
}

#[test]
fn cfm_automatic_timers_retry_an_initial_order_scan_failure() {
    cfm_timed_scans(true);
}

fn cfm_timed_scans(fail_initial_scan: bool) {
    // Exercise the actual paced scheduler: dispatch every scheduled timer in
    // deadline order, without granting order continuations priority. Withhold
    // user snapshots so REST must keep reconciling both account families.
    let mut s = Session::new("DEFAULT", false);
    let started = s.now;
    let mut loaded_at = None;
    let mut last_http = None;
    let mut scan_started = None;
    let mut scan_pages = 0;
    let mut completed = Vec::new();
    let mut initial_reads = Vec::new();
    let mut failure = None;
    let mut private_sequence = 0u64;
    let mut opened_socket = None;
    while s.now - started < 90000.0 {
        let requests = s.collect();
        if requests.is_empty() {
            s.fire();
            if let Some(socket) = s.private {
                if opened_socket != Some(socket) {
                    s.e.handle(s.now, Event::WsOpened { socket });
                    opened_socket = Some(socket);
                    private_sequence = 0;
                }
                // Initial order retries must not depend on a socket watchdog.
                // Keep the connection healthy without providing user snapshots.
                let text = format!(
                    r#"{{"channel":"heartbeats","sequence_num":{private_sequence},"events":[]}}"#
                );
                private_sequence += 1;
                s.e.handle(
                    s.now,
                    Event::WsMessage {
                        socket,
                        text: &text,
                    },
                );
            }
            continue;
        }
        for request in requests {
            if let Some(previous) = last_http {
                assert!(s.now - previous >= 334.0, "shared REST pacing violated");
            }
            last_http = Some(s.now);
            if loaded_at.is_none() {
                initial_reads.push(request.url.clone());
            }
            if request.url.contains("/orders/historical/batch") {
                if fail_initial_scan && failure.is_none() {
                    failure = Some(s.now);
                    s.reply(request, 503, "temporary failure");
                    continue;
                }
                if request.url.contains("product_type=FUTURE")
                    && request.url.contains("order_status=PENDING")
                {
                    assert!(scan_started.is_none(), "overlapping REST order scans");
                    scan_started = Some(s.now);
                    scan_pages = 0;
                }
                scan_pages += 1;
                let first_open_page = request.url.contains("product_type=FUTURE")
                    && request.url.contains("order_status=OPEN")
                    && !request.url.contains("cursor=");
                let last_page = request.url.contains("product_type=SPOT")
                    && request.url.contains("order_status=EDIT_QUEUED");
                let body = if first_open_page {
                    format!(
                        r#"{{"orders":[{}],"has_next":true,"cursor":"second-open-page"}}"#,
                        user_order("resting", "BTC-CDE", "OPEN", "0", "1")
                    )
                } else {
                    r#"{"orders":[],"has_next":false,"cursor":""}"#.into()
                };
                s.reply(request, 200, &body);
                if last_page {
                    assert_eq!(scan_pages, 11, "must scan every status and both families");
                    let duration = s.now - scan_started.take().unwrap();
                    assert!(
                        duration <= 9000.0,
                        "order continuation starved: {duration}ms"
                    );
                    completed.push((s.now, duration));
                }
            } else {
                s.answer(request);
            }
            if s.e.store().loaded().orders && loaded_at.is_none() {
                loaded_at = Some(s.now);
            }
        }
    }
    assert!(loaded_at.unwrap() - started <= 12000.0);
    // No periodic product/balance/position poll may compete with the initial
    // eleven-page scan. Each initial startup read appears exactly once (CFM
    // balance has a separate discovery probe and its initial account read).
    if fail_initial_scan {
        assert!(failure.is_some());
        assert!(
            completed[0].0 - failure.unwrap() <= 10000.0,
            "initial scan did not retry independently of socket failure"
        );
    } else {
        assert_eq!(
            initial_reads
                .iter()
                .filter(|u| u.contains("/products?"))
                .count(),
            2
        );
        assert_eq!(
            initial_reads
                .iter()
                .filter(|u| u.contains("/cfm/positions"))
                .count(),
            1
        );
        assert_eq!(
            initial_reads
                .iter()
                .filter(|u| u.contains("/cfm/balance_summary"))
                .count(),
            2
        );
    }
    assert!(
        completed.len() >= 5,
        "fallback scans stopped making progress"
    );
    for pair in completed.windows(2) {
        assert!(
            pair[1].0 - pair[0].0 <= 20000.0,
            "poll queue grew without bound"
        );
    }
}

fn next_read(s: &mut Session, suffix: &str) -> HttpRequest {
    for _ in 0..200 {
        let requests = s.collect();
        for r in requests {
            if r.url.contains(suffix) {
                return r;
            }
            s.answer(r);
        }
        s.fire();
    }
    panic!("missing read {suffix}");
}

#[test]
fn rest_fill_observation_does_not_consume_delayed_user_execution() {
    let mut s = Session::new("INTX", false).ready();
    let initial = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "2");
    s.message(0, "snapshot", &format!("[{initial}]"));
    assert!(fills(&mut s.e).is_empty());
    let held = next_read(&mut s, "/orders/historical/batch");
    let filled = user_order("order-a", "BTC-PERP-INTX", "OPEN", "1", "1");
    complete_scan(
        &mut s,
        held,
        &format!(r#"{{"orders":[{filled}],"has_next":false}}"#),
    );
    assert_eq!(s.e.store().order("order-a").unwrap().filled, 1.0);
    assert!(fills(&mut s.e).is_empty());
    s.message(1, "update", &format!("[{filled}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
    s.message(2, "update", &format!("[{filled}]"));
    assert!(fills(&mut s.e).is_empty());
}

#[test]
fn rest_open_order_removal_retains_previously_emitted_fill_baseline() {
    let mut s = Session::new("INTX", false).ready();
    s.message(0, "snapshot", "[]");
    fills(&mut s.e);
    let partial = user_order("order-a", "BTC-PERP-INTX", "OPEN", "1", "1");
    s.message(1, "update", &format!("[{partial}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
    let held = next_read(&mut s, "/orders/historical/batch");
    complete_scan(&mut s, held, r#"{"orders":[],"has_next":false}"#);
    assert!(s.e.store().order("order-a").is_none());
    let terminal = user_order("order-a", "BTC-PERP-INTX", "FILLED", "2", "0");
    s.message(2, "update", &format!("[{terminal}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
    s.message(3, "update", &format!("[{terminal}]"));
    assert!(fills(&mut s.e).is_empty());
}

fn size_detail(id: &str, config: &str) -> String {
    format!(
        r#"{{"order":{{"order_id":"{id}","product_id":"BTC-USD","order_configuration":{config}}}}}"#
    )
}

#[test]
fn external_quote_sized_limit_waits_for_denomination_and_uses_base_remaining() {
    let mut s = Session::new("DEFAULT", true).ready();
    let row = user_order("quote-limit", "BTC-USD", "OPEN", "0.1", "50");
    s.message(0, "snapshot", &format!("[{row}]"));
    assert!(s.e.store().orders().is_empty());
    let request = next_read(&mut s, "/orders/historical/quote-limit");
    s.reply(
        request,
        200,
        &size_detail(
            "quote-limit",
            r#"{"limit_limit_gtc":{"quote_size":"60","limit_price":"100"}}"#,
        ),
    );
    let order = s.e.store().order("quote-limit").unwrap();
    assert_eq!(
        (order.amount, order.filled, order.remaining),
        (0.6, 0.1, 0.5)
    );
    assert!(fills(&mut s.e).is_empty());
}

#[test]
fn external_quote_sized_market_does_not_add_quote_leaves_to_base_fills() {
    let mut s = Session::new("DEFAULT", true).ready();
    let row = user_order("quote-market", "BTC-USD", "OPEN", "0.1", "50")
        .replace("\"order_type\":\"Limit\"", "\"order_type\":\"Market\"");
    s.message(0, "snapshot", &format!("[{row}]"));
    let request = next_read(&mut s, "/orders/historical/quote-market");
    s.reply(
        request,
        200,
        &size_detail(
            "quote-market",
            r#"{"market_market_ioc":{"quote_size":"60"}}"#,
        ),
    );
    let order = s.e.store().order("quote-market").unwrap();
    assert_eq!(
        (order.amount, order.filled, order.remaining),
        (0.6, 0.1, 0.5)
    );
}

#[test]
fn failed_external_denomination_read_preserves_prior_snapshot_until_recovery() {
    let mut s = Session::new("DEFAULT", true).ready();
    let initial = user_order("base-order", "BTC-USD", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{initial}]"));
    let request = next_read(&mut s, "/orders/historical/base-order");
    s.reply(
        request,
        200,
        &size_detail(
            "base-order",
            r#"{"limit_limit_gtc":{"base_size":"1","limit_price":"100"}}"#,
        ),
    );
    assert!(s.e.store().order("base-order").is_some());
    let quote = user_order("quote-order", "BTC-USD", "OPEN", "0", "50");
    s.message(1, "snapshot", &format!("[{quote}]"));
    let request = next_read(&mut s, "/orders/historical/quote-order");
    s.reply(request, 503, r#"{"error":"temporary"}"#);
    assert!(s.e.store().order("base-order").is_some());
    assert!(s.e.store().order("quote-order").is_none());
    s.message(2, "snapshot", &format!("[{quote}]"));
    let request = next_read(&mut s, "/orders/historical/quote-order");
    s.reply(
        request,
        200,
        &size_detail(
            "quote-order",
            r#"{"limit_limit_gtc":{"quote_size":"50","limit_price":"100"}}"#,
        ),
    );
    assert!(s.e.store().order("base-order").is_none());
    assert_eq!(s.e.store().order("quote-order").unwrap().amount, 0.5);
}

#[test]
fn zero_fill_terminal_uses_existing_order_size_or_fetches_native_configuration() {
    let mut s = Session::new("DEFAULT", true).ready();
    let initial = user_order("known-order", "BTC-USD", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{initial}]"));
    let request = next_read(&mut s, "/orders/historical/known-order");
    s.reply(
        request,
        200,
        &size_detail(
            "known-order",
            r#"{"limit_limit_gtc":{"base_size":"1","limit_price":"100"}}"#,
        ),
    );
    let canceled = user_order("known-order", "BTC-USD", "CANCELLED", "0", "0");
    s.message(1, "update", &format!("[{canceled}]"));
    let order = s.e.store().order("known-order").unwrap();
    assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Canceled);
    assert_eq!(order.amount, 1.0);

    let unknown = user_order("unknown-order", "BTC-USD", "CANCELLED", "0", "0");
    s.message(2, "update", &format!("[{unknown}]"));
    assert!(s.e.store().order("unknown-order").is_none());
    let request = next_read(&mut s, "/orders/historical/unknown-order");
    s.reply(
        request,
        200,
        &size_detail(
            "unknown-order",
            r#"{"limit_limit_gtc":{"base_size":"2","limit_price":"100"}}"#,
        ),
    );
    let order = s.e.store().order("unknown-order").unwrap();
    assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Canceled);
    assert_eq!(order.amount, 2.0);
    assert!(fills(&mut s.e).is_empty());
}

#[test]
fn completed_wire_snapshot_emits_new_fills_while_another_order_waits_for_sizing() {
    let mut s = Session::new("DEFAULT", true).ready();
    let initial = user_order("pending-size", "BTC-USD", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{initial}]"));
    let request = next_read(&mut s, "/orders/historical/pending-size");
    let filled = user_order("new-fill", "BTC-USD", "FILLED", "1", "0");
    s.message(1, "update", &format!("[{filled}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
    s.reply(
        request,
        200,
        &size_detail(
            "pending-size",
            r#"{"limit_limit_gtc":{"base_size":"1","limit_price":"100"}}"#,
        ),
    );
    assert!(fills(&mut s.e).is_empty());
    s.message(2, "update", &format!("[{filled}]"));
    assert!(fills(&mut s.e).is_empty());
}

#[test]
fn deferred_pre_snapshot_update_does_not_emit_historical_fill_after_wire_completion() {
    let mut s = Session::new("DEFAULT", true).ready();
    let partial = user_order("pending-size", "BTC-USD", "OPEN", "1", "1");
    s.message(0, "update", &format!("[{partial}]"));
    let request = next_read(&mut s, "/orders/historical/pending-size");
    s.message(1, "snapshot", &format!("[{partial}]"));
    s.reply(
        request,
        200,
        &size_detail(
            "pending-size",
            r#"{"limit_limit_gtc":{"base_size":"2","limit_price":"100"}}"#,
        ),
    );
    assert!(fills(&mut s.e).is_empty());
    let filled = user_order("pending-size", "BTC-USD", "FILLED", "2", "0");
    s.message(2, "update", &format!("[{filled}]"));
    assert_eq!(fills(&mut s.e), vec![1.0]);
}

#[test]
fn rest_sizing_proof_does_not_let_new_user_row_overtake_queued_row() {
    let mut s = Session::new("DEFAULT", true).ready();
    s.message(0, "snapshot", "[]");
    let initial = user_order("pending-size", "BTC-USD", "OPEN", "0", "1");
    s.message(1, "update", &format!("[{initial}]"));
    let detail = next_read(&mut s, "/orders/historical/pending-size");
    let scan = next_read(&mut s, "/orders/historical/batch");
    let rest = r#"{"orders":[{"order_id":"pending-size","product_id":"BTC-USD","status":"OPEN","order_type":"LIMIT","side":"BUY","filled_size":"0","order_configuration":{"limit_limit_gtc":{"base_size":"1","limit_price":"100"}}}],"has_next":false}"#;
    complete_scan(&mut s, scan, rest);
    let edited = initial.replace("\"limit_price\":\"100\"", "\"limit_price\":\"110\"");
    s.message(2, "update", &format!("[{edited}]"));
    s.reply(
        detail,
        200,
        &size_detail(
            "pending-size",
            r#"{"limit_limit_gtc":{"base_size":"1","limit_price":"100"}}"#,
        ),
    );
    assert_eq!(s.e.store().order("pending-size").unwrap().price, 110.0);
}

#[test]
fn stale_sizing_failure_cannot_deactivate_a_replacement_socket() {
    let mut s = Session::new("DEFAULT", true).ready();
    let row = user_order("stale-detail", "BTC-USD", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    let request = next_read(&mut s, "/orders/historical/stale-detail");
    s.e.handle(
        s.now,
        Event::WsClosed {
            socket: s.private.unwrap(),
            code: 1006,
        },
    );
    s.e.drain_actions().for_each(drop);
    s.reply(request, 401, r#"{"error":"expired"}"#);
    assert!(
        !s.e.drain_actions()
            .any(|action| matches!(action, Action::Notice(safe_cex_engine::Notice::Deactivate)))
    );
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let s = Session::new("DEFAULT", true).ready();
    let mut e = s.e;
    let now = 20_000.;
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
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
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
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
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
            .any(|r| r.url.contains("/portfolios/portfolio-one")),
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
            let body = if r.url.contains("/portfolios/portfolio-one") {
                BREAKDOWN
            } else if r.url.contains("/orders/historical/batch") {
                r#"{"orders":[],"has_next":false}"#
            } else {
                panic!("unexpected account read {}", r.url)
            };
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
#[test]
fn cfm_product_failure_survives_spot_success_during_seeding() {
    let mut s = Session::new("DEFAULT", false);
    let mut failed = false;
    for _ in 0..100 {
        if failed {
            break;
        }
        let requests = s.collect();
        if requests.is_empty() {
            assert!(!s.timers.is_empty(), "stalled: {:?}", s.urls);
            s.fire();
            continue;
        }
        for r in requests {
            let cfm =
                r.url.contains("/brokerage/products?") && r.url.contains("product_type=FUTURE");
            if cfm && !failed {
                failed = true;
                s.reply(r, 503, "temporary failure");
            } else {
                s.answer(r);
            }
        }
    }
    assert!(failed, "CFM products were never requested");
    let s = s.ready();
    assert!(s.e.store().market("BTC-CDE").is_some());
    assert!(s.e.store().market("BTC-USD").is_some());
}

#[test]
fn unmappable_cfm_position_row_neither_blocks_startup_nor_invents_a_flat_position() {
    let mut s = Session::new("DEFAULT", false);
    s.positions = Some(
        r#"{"positions":[{"product_id":"BTC-CDE","side":"UNKNOWN","number_of_contracts":"2","avg_entry_price":"100","current_price":"90","unrealized_pnl":"0.2"}]}"#.into(),
    );
    let s = s.ready();
    assert!(s.e.store().loaded().positions);
    assert!(s.e.store().position("BTC-CDE").is_none());
    assert!(
        s.logs
            .iter()
            .any(|l| l.contains("skipped unmappable position BTC-CDE"))
    );
}
#[test]
fn unmappable_cfm_position_row_keeps_the_known_position() {
    let mut s = Session::new("DEFAULT", false).ready();
    s.positions = Some(
        r#"{"positions":[{"product_id":"BTC-CDE","side":"UNKNOWN","number_of_contracts":"5","avg_entry_price":"100","current_price":"90","unrealized_pnl":"0.2"}]}"#.into(),
    );
    let before = s.urls.len();
    while !s.urls[before..]
        .iter()
        .any(|u| u.contains("/cfm/positions"))
    {
        for r in s.collect() {
            s.answer(r);
        }
        s.fire();
    }
    for r in s.collect() {
        s.answer(r);
    }
    assert_eq!(s.e.store().position("BTC-CDE").unwrap().contracts, 0.02);
}
#[test]
fn portfolio_probe_advances_past_a_missing_portfolio() {
    let mut s = Session::new("INTX", false);
    let r = s.collect().pop().unwrap();
    assert!(r.url.ends_with("/portfolios"));
    s.reply(
        r,
        200,
        r#"{"portfolios":[{"uuid":"gone","type":"DEFAULT"},{"uuid":"portfolio-one","type":"INTX"}]}"#,
    );
    let r = s.next_request();
    assert!(r.url.ends_with("/portfolios/gone"));
    s.reply(r, 404, r#"{"error":"NOT_FOUND"}"#);
    let r = s.next_request();
    assert!(r.url.ends_with("/portfolios/portfolio-one"), "{}", r.url);
}
#[test]
fn portfolio_probe_retries_the_same_portfolio_on_server_failure() {
    let mut s = Session::new("INTX", false);
    let r = s.collect().pop().unwrap();
    s.reply(
        r,
        200,
        r#"{"portfolios":[{"uuid":"busy","type":"DEFAULT"},{"uuid":"portfolio-one","type":"INTX"}]}"#,
    );
    let r = s.next_request();
    s.reply(r, 503, "unavailable");
    let r = s.next_request();
    assert!(r.url.ends_with("/portfolios/busy"), "{}", r.url);
}
#[test]
fn flat_intx_row_without_a_side_clears_the_position() {
    let mut s = Session::new("INTX", false).ready();
    assert_eq!(
        s.e.store().position("BTC-PERP-INTX").unwrap().contracts,
        2.0
    );
    s.positions = Some(
        r#"{"positions":[{"symbol":"BTC-PERP-INTX","position_side":"POSITION_SIDE_UNKNOWN","net_size":"0","entry_vwap":{"value":"0"},"mark_price":{"value":"110"},"liquidation_price":{"value":"0"},"leverage":"10"}]}"#.into(),
    );
    for _ in 0..20 {
        for r in s.collect() {
            s.answer(r);
        }
        if s.e.store().position("BTC-PERP-INTX").unwrap().contracts == 0.0 {
            return;
        }
        s.fire();
    }
    panic!("flat position row was not applied");
}
#[test]
fn unmappable_update_for_an_unknown_order_does_not_spoil_the_user_snapshot() {
    let mut s = Session::new("INTX", false).ready();
    let site = r#"{"order_id":"twap-1","client_order_id":"","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"SELL","order_type":"Twap","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"120","stop_price":"0","avg_price":"0"}"#;
    s.message(0, "update", &format!("[{site}]"));
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(1, "snapshot", &format!("[{row}]"));
    assert!(s.e.store().order("order-a").is_some());
}
#[test]
fn snapshot_holding_only_a_site_order_still_removes_ended_orders() {
    let mut s = Session::new("INTX", false).ready();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    assert!(s.e.store().order("order-a").is_some());
    let site = r#"{"order_id":"twap-1","client_order_id":"","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"SELL","order_type":"Twap","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"120","stop_price":"0","avg_price":"0"}"#;
    s.message(1, "snapshot", &format!("[{site}]"));
    assert!(s.e.store().order("order-a").is_none());
}
#[test]
fn snapshot_holding_only_an_order_on_an_unloaded_market_still_removes_ended_orders() {
    let mut s = Session::new("INTX", false).ready();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    s.message(0, "snapshot", &format!("[{row}]"));
    assert!(s.e.store().order("order-a").is_some());
    let other = user_order("spot-1", "ETH-USD", "OPEN", "0", "1");
    s.message(1, "snapshot", &format!("[{other}]"));
    assert!(s.e.store().order("order-a").is_none());
}
#[test]
fn unmappable_update_for_a_staged_order_spoils_the_snapshot() {
    let mut s = Session::new("INTX", false).ready();
    let rows = (0..50)
        .map(|i| user_order(&format!("ours-{i}"), "BTC-PERP-INTX", "OPEN", "0", "1"))
        .collect::<Vec<_>>()
        .join(",");
    s.message(0, "snapshot", &format!("[{rows}]"));
    let update = r#"{"order_id":"ours-0","client_order_id":"","product_id":"BTC-PERP-INTX","status":"OPEN","order_side":"BUY","order_type":"Twap","cumulative_quantity":"0","leaves_quantity":"1","limit_price":"100","stop_price":"0","avg_price":"0"}"#;
    s.message(1, "update", &format!("[{update}]"));
    s.message(2, "snapshot", "[]");
    assert!(s.e.store().order("ours-0").is_none());
}
#[test]
fn skipped_position_row_beside_a_mapped_row_does_not_duplicate_the_position() {
    let mut s = Session::new("INTX", false).ready();
    s.positions = Some(
        r#"{"positions":[{"symbol":"BTC-PERP-INTX","position_side":"POSITION_SIDE_LONG","net_size":"3","entry_vwap":{"value":"100"},"mark_price":{"value":"110"},"liquidation_price":{"value":"50"},"leverage":"10"},{"symbol":"BTC-PERP-INTX","position_side":"BOGUS","net_size":"1","entry_vwap":{"value":"100"},"mark_price":{"value":"110"},"liquidation_price":{"value":"50"},"leverage":"10"}]}"#.into(),
    );
    for _ in 0..20 {
        for r in s.collect() {
            s.answer(r);
        }
        if s.e.store().position("BTC-PERP-INTX").unwrap().contracts == 3.0 {
            break;
        }
        s.fire();
    }
    let held =
        s.e.store()
            .positions()
            .iter()
            .filter(|p| p.symbol == "BTC-PERP-INTX")
            .count();
    assert_eq!(held, 1);
}

impl Session {
    fn event(&mut self, event: Event<'_>) -> Vec<(f64, TimerId)> {
        let before: Vec<_> = self.timers.iter().map(|(_, t)| *t).collect();
        self.e.handle(self.now, event);
        self.collect();
        self.timers
            .iter()
            .filter(|(_, t)| !before.contains(t))
            .copied()
            .collect()
    }
    fn advance(&mut self, until: f64) {
        self.collect();
        loop {
            self.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
            match self.timers.first() {
                Some(&(at, timer)) if at <= until => {
                    self.timers.remove(0);
                    self.now = self.now.max(at);
                    self.e.handle(self.now, Event::TimerFired { timer });
                    self.collect();
                }
                _ => break,
            }
        }
        self.now = until;
    }
    fn frame(&mut self, text: &str) {
        let socket = self.private.unwrap();
        self.event(Event::WsMessage { socket, text });
    }
    fn heartbeat(&mut self, seq: u64) {
        self.frame(&format!(
            r#"{{"channel":"heartbeats","sequence_num":{seq},"events":[]}}"#
        ));
    }
    fn snapshot(&mut self) {
        self.frame(
            r#"{"channel":"user","sequence_num":0,"events":[{"type":"snapshot","orders":[]}]}"#,
        );
    }
    /// Closes the current private socket; returns the scheduled reconnect timer.
    fn drop_private(&mut self) -> (TimerId, f64) {
        let socket = self.private.unwrap();
        let timers = self.event(Event::WsClosed { socket, code: 1006 });
        self.reconnect_timer(timers)
    }
    fn reconnect_timer(&self, timers: Vec<(f64, TimerId)>) -> (TimerId, f64) {
        let retries: Vec<_> = timers
            .into_iter()
            .filter(|(at, _)| *at - self.now >= 1000.0)
            .collect();
        assert_eq!(retries.len(), 1, "one reconnect timer");
        (retries[0].1, retries[0].0 - self.now)
    }
    fn reconnect(&mut self, timer: TimerId, delay: f64) -> SocketId {
        let old = self.private.unwrap();
        self.now += delay;
        self.timers.retain(|(_, t)| *t != timer);
        self.event(Event::TimerFired { timer });
        let socket = self.private.unwrap();
        assert_ne!(socket, old);
        self.closes.clear();
        socket
    }
}

#[test]
fn private_flaps_back_off_exponentially_even_after_valid_snapshots() {
    let mut s = Session::new("INTX", false).ready();
    for attempt in 0..7 {
        s.snapshot();
        let (timer, delay) = s.drop_private();
        let nominal = (2000.0 * 2f64.powi(attempt)).min(30_000.0);
        assert!(
            (nominal * 0.75..=nominal).contains(&delay),
            "attempt {attempt}: {delay}"
        );
        let socket = s.reconnect(timer, delay);
        s.event(Event::WsOpened { socket });
    }
}

#[test]
fn private_reconnect_resubscribes_and_ignores_the_retired_socket() {
    let mut s = Session::new("INTX", false).ready();
    s.collect();
    let old = s.private.unwrap();
    let (timer, delay) = s.drop_private();
    let fresh = s.reconnect(timer, delay);
    s.sends.clear();
    s.event(Event::WsOpened { socket: fresh });
    let channels: Vec<_> = s
        .sends
        .iter()
        .map(|(socket, text)| {
            assert_eq!(*socket, fresh);
            assert!(text.contains(r#""type":"subscribe""#) && text.contains(r#""jwt":""#));
            text.split(r#""channel":""#)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(channels, ["user", "heartbeats"]);
    let row = user_order("retired", "BTC-PERP-INTX", "OPEN", "0", "1");
    let text = format!(
        r#"{{"channel":"user","sequence_num":0,"events":[{{"type":"snapshot","orders":[{row}]}}]}}"#
    );
    s.event(Event::WsMessage {
        socket: old,
        text: &text,
    });
    s.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    assert!(s.e.store().order("retired").is_none());
    assert!(s.closes.is_empty());
    s.frame(&text);
    assert!(s.e.store().order("retired").is_some());
}

#[test]
fn private_connect_timeout_closes_a_socket_that_never_opens() {
    let mut s = Session::new("INTX", false).ready();
    let (timer, delay) = s.drop_private();
    let fresh = s.reconnect(timer, delay);
    let connected = s.now;
    s.advance(connected + 9_999.0);
    assert!(s.closes.is_empty());
    s.advance(connected + 10_000.0);
    assert_eq!(s.closes, [fresh]);
    assert!(s.logs.iter().any(|l| l.contains("watchdog expired")));
}

#[test]
fn private_snapshot_deadline_ignores_heartbeats() {
    let mut s = Session::new("INTX", false).ready();
    let (timer, delay) = s.drop_private();
    let fresh = s.reconnect(timer, delay);
    s.event(Event::WsOpened { socket: fresh });
    let opened = s.now;
    for seq in 0..6 {
        s.advance(opened + 5_000.0 * seq as f64 + 4_000.0);
        s.heartbeat(seq);
    }
    s.advance(opened + 29_999.0);
    assert!(s.closes.is_empty());
    s.advance(opened + 30_000.0);
    assert_eq!(s.closes, [fresh]);
}

#[test]
fn private_idle_timeout_counts_only_sequence_valid_frames_and_sends_no_pings() {
    let mut s = Session::new("INTX", false).ready();
    let socket = s.private.unwrap();
    s.collect();
    s.sends.clear();
    s.snapshot();
    let mut seq = 1;
    for _ in 0..12 {
        s.advance(s.now + 9_000.0);
        s.heartbeat(seq);
        seq += 1;
    }
    let last = s.now;
    s.advance(last + 20_000.0);
    s.heartbeat(seq - 1);
    s.advance(last + 29_999.0);
    assert!(s.closes.is_empty());
    assert!(
        s.sends.is_empty(),
        "Coinbase relies on its heartbeats channel"
    );
    s.advance(last + 30_000.0);
    assert_eq!(s.closes, [socket]);
}

#[test]
fn private_stable_traffic_resets_backoff() {
    let mut s = Session::new("INTX", false).ready();
    for _ in 0..3 {
        s.snapshot();
        let (timer, delay) = s.drop_private();
        let socket = s.reconnect(timer, delay);
        s.event(Event::WsOpened { socket });
    }
    s.snapshot();
    for seq in 1..5 {
        s.advance(s.now + 9_000.0);
        s.heartbeat(seq);
    }
    let (_, delay) = s.drop_private();
    assert!((1500.0..=2000.0).contains(&delay), "{delay}");
}

#[test]
fn private_pending_snapshot_survives_while_user_frames_arrive() {
    let mut s = Session::new("INTX", false).ready();
    let (timer, delay) = s.drop_private();
    let fresh = s.reconnect(timer, delay);
    s.event(Event::WsOpened { socket: fresh });
    let opened = s.now;
    for seq in 0..7 {
        s.advance(opened + 10_000.0 * (seq + 1) as f64);
        s.message(seq, "update", "[]");
    }
    let last = s.now;
    assert!(s.closes.is_empty());
    for seq in 7..13 {
        s.advance(last + 5_000.0 * (seq - 6) as f64 - 1_000.0);
        s.heartbeat(seq);
    }
    s.advance(last + 29_999.0);
    assert!(s.closes.is_empty());
    s.advance(last + 35_000.0);
    assert_eq!(s.closes, [fresh]);
}

#[test]
fn private_resent_snapshot_on_the_same_connection_expires_without_user_frames() {
    let mut s = Session::new("INTX", false).ready();
    let socket = s.private.unwrap();
    s.snapshot();
    let rows: Vec<_> = (0..50)
        .map(|i| user_order(&format!("page-{i}"), "BTC-PERP-INTX", "OPEN", "0", "1"))
        .collect();
    s.message(1, "snapshot", &format!("[{}]", rows.join(",")));
    let resent = s.now + 1.0;
    for seq in 2..8 {
        s.advance(resent + 5_000.0 * (seq - 1) as f64 - 1_000.0);
        s.heartbeat(seq);
    }
    s.advance(resent + 29_000.0);
    assert!(s.closes.is_empty());
    s.advance(resent + 35_000.0);
    assert_eq!(s.closes, [socket]);
}

impl Session {
    /// Answers reads and fires timers until `done` holds.
    fn poll_until(&mut self, done: impl Fn(&Self) -> bool) {
        for _ in 0..80 {
            let requests = self.collect();
            if requests.is_empty() {
                if done(self) {
                    return;
                }
                self.fire();
            }
            for r in requests {
                self.answer(r);
            }
        }
        panic!("condition never held");
    }
}

#[test]
fn position_row_without_a_symbol_keeps_stored_positions_while_valid_peers_commit() {
    let mut s = Session::new("INTX", false);
    s.products = Some(INTX.replace(
        "}]}",
        &format!(
            "}},{}]}}",
            INTX.trim_start_matches(r#"{"products":["#)
                .trim_end_matches("]}")
                .replace("BTC", "ETH")
        ),
    ));
    let row = |symbol: &str, side: &str, size: &str| {
        format!(
            r#"{{"symbol":"{symbol}","position_side":"POSITION_SIDE_{side}","net_size":"{size}","entry_vwap":{{"value":"100"}},"mark_price":{{"value":"110"}},"liquidation_price":{{"value":"50"}},"leverage":"10"}}"#
        )
    };
    let positions = |rows: &[String]| Some(format!(r#"{{"positions":[{}]}}"#, rows.join(",")));
    s.positions = positions(&[
        row("BTC-PERP-INTX", "LONG", "2"),
        row("ETH-PERP-INTX", "LONG", "1"),
    ]);
    let mut s = s.ready();
    let eth = s.e.store().position("ETH-PERP-INTX").unwrap().clone();
    assert_eq!(eth.contracts, 1.0);
    s.logs.clear();

    s.positions = positions(&[
        row("BTC-PERP-INTX", "LONG", "3"),
        r#"{"position_side":"POSITION_SIDE_LONG","net_size":"7"}"#.into(),
    ]);
    s.poll_until(|s| s.e.store().position("BTC-PERP-INTX").unwrap().contracts == 3.0);
    assert_eq!(s.e.store().position("ETH-PERP-INTX"), Some(&eth));
    assert!(
        s.logs
            .iter()
            .any(|l| l == "Coinbase skipped unmappable position without a symbol")
    );
    assert!(!s.logs.iter().any(|l| l.contains('7')));

    s.positions = positions(&[row("BTC-PERP-INTX", "LONG", "3")]);
    s.poll_until(|s| s.e.store().position("ETH-PERP-INTX").unwrap().contracts == 0.0);
}

#[test]
fn order_row_without_an_id_keeps_stored_orders_while_valid_peers_commit() {
    let mut s = Session::new("INTX", false).ready();
    let orders = |rows: &[String]| {
        Some(format!(
            r#"{{"orders":[{}],"has_next":false}}"#,
            rows.join(",")
        ))
    };
    let a = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    let b = user_order("order-b", "BTC-PERP-INTX", "OPEN", "0", "2");
    s.orders = orders(&[a.clone(), b]);
    s.poll_until(|s| s.e.store().order("order-b").is_some());
    let stored = s.e.store().order("order-b").unwrap().clone();
    s.logs.clear();

    let moved = a.replace(r#""limit_price":"100""#, r#""limit_price":"101""#);
    s.orders = orders(&[
        moved.clone(),
        r#"{"status":"OPEN","limit_price":"7"}"#.into(),
    ]);
    s.poll_until(|s| s.e.store().order("order-a").unwrap().price == 101.0);
    assert_eq!(s.e.store().order("order-b"), Some(&stored));
    assert!(
        s.logs
            .iter()
            .any(|l| l == "Coinbase skipped unmappable order without an id")
    );

    s.orders = orders(&[moved]);
    s.poll_until(|s| s.e.store().order("order-b").is_none());
    assert_eq!(s.e.store().orders().len(), 1);
}

#[test]
fn one_invalid_spot_holding_keeps_its_stored_copy_while_valid_peers_commit() {
    let holding = |asset: &str, total: &str, free: &str| {
        format!(
            r#"{{"asset":"{asset}","total_balance_crypto":{total},"available_to_trade_crypto":{free}}}"#
        )
    };
    let breakdown = |rows: &[String]| {
        Some(format!(
            r#"{{"breakdown":{{"spot_positions":[{}]}}}}"#,
            rows.join(",")
        ))
    };
    let total = |s: &Session, asset: &str| {
        s.e.store()
            .currency_balances()
            .unwrap()
            .iter()
            .find(|c| c.currency == asset)
            .map(|c| c.balance.total)
    };
    let mut s = Session::new("DEFAULT", true);
    s.breakdown = breakdown(&[holding("BTC", "1.5", "1"), holding("ETH", "3", "3")]);
    let mut s = s.ready();
    assert_eq!(total(&s, "ETH"), Some(3.0));
    s.logs.clear();

    s.breakdown = breakdown(&[holding("BTC", "2", "1"), holding("ETH", "4", "9")]);
    s.poll_until(|s| total(s, "BTC") == Some(2.0));
    assert_eq!(total(&s, "ETH"), Some(3.0));
    assert!(
        s.logs
            .iter()
            .any(|l| l == "Coinbase skipped invalid spot holding ETH")
    );
    assert!(!s.logs.iter().any(|l| l.contains('9')));

    s.breakdown = breakdown(&[holding("BTC", "2", "1")]);
    s.poll_until(|s| total(s, "ETH").is_none());
}

fn intx_position(symbol: &str, size: &str) -> String {
    format!(
        r#"{{"symbol":"{symbol}","position_side":"POSITION_SIDE_LONG","net_size":"{size}","entry_vwap":{{"value":"100"}},"mark_price":{{"value":"110"}},"liquidation_price":{{"value":"50"}},"leverage":"10"}}"#
    )
}
fn intx_positions(rows: &[String]) -> Option<String> {
    Some(format!(r#"{{"positions":[{}]}}"#, rows.join(",")))
}
const SYMBOLLESS_POSITION: &str = r#"{"position_side":"POSITION_SIDE_LONG","net_size":"7"}"#;
impl Session {
    /// Answers every read, positions last in each batch, until `done` holds.
    fn poll_positions_last_until(&mut self, done: impl Fn(&Self) -> bool) {
        for _ in 0..80 {
            let (positions, mut requests): (Vec<_>, Vec<_>) = self
                .collect()
                .into_iter()
                .partition(|r| r.url.contains("/intx/positions/"));
            requests.extend(positions);
            if requests.is_empty() {
                if done(self) {
                    return;
                }
                self.fire();
            }
            for r in requests {
                self.answer(r);
                if done(self) {
                    return;
                }
            }
        }
        panic!("condition never held");
    }
}

#[test]
fn startup_position_row_without_a_symbol_is_an_error_logged_once_until_a_clean_snapshot() {
    let mut s = Session::new("INTX", false);
    s.positions = intx_positions(&[SYMBOLLESS_POSITION.into()]);
    let mut s = s.ready();
    let message = "Coinbase skipped unmappable position without a symbol";
    assert_eq!(s.errors, [message]);
    let reads = |s: &Session| {
        s.urls
            .iter()
            .filter(|u| u.contains("/intx/positions/"))
            .count()
    };
    let seen = reads(&s);
    s.poll_until(|s| reads(s) > seen + 1);
    assert_eq!(s.logs.iter().filter(|l| *l == message).count(), 1);

    s.positions = intx_positions(&[]);
    let seen = reads(&s);
    s.poll_until(|s| reads(s) > seen + 1);
    s.positions = intx_positions(&[SYMBOLLESS_POSITION.into()]);
    let seen = reads(&s);
    s.poll_until(|s| reads(s) > seen + 1);
    assert_eq!(s.logs.iter().filter(|l| *l == message).count(), 2);
    // The flat copies it now keeps make the recurrence a warning.
    assert_eq!(s.errors.len(), 1);
}

#[test]
fn intx_upnl_sums_committed_positions_including_retained_copies_across_a_balance_poll() {
    let mut s = Session::new("INTX", false);
    s.products = Some(INTX.replace(
        "}]}",
        &format!(
            "}},{}]}}",
            INTX.trim_start_matches(r#"{"products":["#)
                .trim_end_matches("]}")
                .replace("BTC", "ETH")
        ),
    ));
    s.positions = intx_positions(&[
        intx_position("BTC-PERP-INTX", "2"),
        intx_position("ETH-PERP-INTX", "1"),
    ]);
    let mut s = s.ready();
    assert_eq!(s.e.store().balance().upnl, 30.0);

    // BTC now 3 (UPnL 30); the symbol-less row keeps ETH's stored copy (UPnL 10).
    s.positions = intx_positions(&[
        intx_position("BTC-PERP-INTX", "3"),
        SYMBOLLESS_POSITION.into(),
    ]);
    s.poll_positions_last_until(|s| {
        s.e.store().position("BTC-PERP-INTX").unwrap().contracts == 3.0
    });
    assert_eq!(s.e.store().balance().upnl, 40.0);

    let reads = |s: &Session| {
        s.urls
            .iter()
            .filter(|u| u.contains("/intx/portfolio/"))
            .count()
    };
    let seen = reads(&s);
    s.poll_until(|s| reads(s) > seen);
    assert_eq!(s.e.store().balance().upnl, 40.0);
}

#[test]
fn a_skipped_order_is_an_error_only_when_no_stored_copy_is_kept() {
    let mut s = Session::new("INTX", false).ready();
    let orders = |rows: &[String]| {
        Some(format!(
            r#"{{"orders":[{}],"has_next":false}}"#,
            rows.join(",")
        ))
    };
    let a = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    let b = user_order("order-b", "BTC-PERP-INTX", "OPEN", "0", "2");
    let unmappable = |row: &str| row.replace(r#""order_type":"Limit""#, r#""order_type":"Odd""#);
    s.orders = orders(std::slice::from_ref(&a));
    s.poll_until(|s| s.e.store().order("order-a").is_some());
    s.errors.clear();

    // order-a keeps its stored copy; order-b has none and drops out.
    s.orders = orders(&[unmappable(&a), unmappable(&b)]);
    s.poll_until(|s| {
        s.logs
            .iter()
            .any(|l| l.starts_with("Coinbase skipped unmappable order order-b"))
    });
    s.poll_until(|s| {
        s.logs
            .iter()
            .any(|l| l.starts_with("Coinbase skipped unmappable order order-a"))
    });
    assert!(s.e.store().order("order-a").is_some());
    assert!(s.e.store().order("order-b").is_none());
    assert_eq!(s.errors.len(), 1);
    assert!(s.errors[0].starts_with("Coinbase skipped unmappable order order-b"));
}

#[test]
fn a_skipped_spot_holding_is_an_error_only_when_no_stored_copy_is_kept() {
    let holding = |asset: &str, total: &str, free: &str| {
        format!(
            r#"{{"asset":"{asset}","total_balance_crypto":{total},"available_to_trade_crypto":{free}}}"#
        )
    };
    let breakdown = |rows: &[String]| {
        Some(format!(
            r#"{{"breakdown":{{"spot_positions":[{}]}}}}"#,
            rows.join(",")
        ))
    };
    let mut s = Session::new("DEFAULT", true);
    s.breakdown = breakdown(&[holding("BTC", "1.5", "1")]);
    let mut s = s.ready();
    s.errors.clear();

    // BTC keeps its stored copy; SOL has none and drops out.
    s.breakdown = breakdown(&[holding("BTC", "1", "2"), holding("SOL", "1", "2")]);
    s.poll_until(|s| {
        ["BTC", "SOL"].iter().all(|asset| {
            s.logs
                .contains(&format!("Coinbase skipped invalid spot holding {asset}"))
        })
    });
    assert_eq!(s.errors, ["Coinbase skipped invalid spot holding SOL"]);
    s.e.store_mut().set_currency_balances(vec![]);
    s.poll_until(|s| s.errors.len() == 2);
    assert_eq!(s.errors[1], "Coinbase skipped invalid spot holding BTC");
    s.breakdown = breakdown(&[holding("BTC", "1", "1"), holding("SOL", "1", "1")]);
    s.poll_until(|s| {
        s.e.store()
            .currency_balances()
            .unwrap()
            .iter()
            .any(|c| c.currency == "SOL")
    });
    s.breakdown = breakdown(&[holding("BTC", "1", "2"), holding("SOL", "1", "2")]);
    s.e.store_mut().set_currency_balances(vec![]);
    s.poll_until(|s| s.errors.len() == 4);
    assert_eq!(s.errors.iter().filter(|m| m.ends_with("BTC")).count(), 2);
    assert_eq!(s.errors.iter().filter(|m| m.ends_with("SOL")).count(), 2);
}

#[test]
fn user_snapshot_skip_errors_escalate_once_and_rearm_after_mapping() {
    let mut s = Session::new("INTX", false).ready();
    let row = user_order("order-a", "BTC-PERP-INTX", "OPEN", "0", "1");
    let bad = row.replace(r#""order_type":"Limit""#, r#""order_type":"Odd""#);
    let report = |s: &mut Session, seq, row: &str| {
        s.message(seq, "snapshot", &format!("[{row}]"));
        s.collect();
    };
    report(&mut s, 0, &row);
    let stored = s.e.store().order("order-a").unwrap().clone();
    s.errors.clear();
    s.logs.clear();
    report(&mut s, 1, &bad);
    assert_eq!(s.logs.len(), 1);
    assert!(s.errors.is_empty());
    report(&mut s, 2, &bad);
    assert_eq!(s.logs.len(), 1);
    s.e.store_mut().set_orders(vec![]);
    report(&mut s, 3, &bad);
    assert_eq!(
        s.errors.len(),
        1,
        "a warned row losing its copy upgrades once"
    );
    // Reappearing cached state must not reset the one Error upgrade.
    s.e.store_mut().set_orders(vec![stored]);
    report(&mut s, 4, &bad);
    assert_eq!((s.logs.len(), s.errors.len()), (2, 1));
    s.e.store_mut().set_orders(vec![]);
    report(&mut s, 5, &bad);
    assert_eq!(s.errors.len(), 1);
    report(&mut s, 6, &row);
    s.e.store_mut().set_orders(vec![]);
    report(&mut s, 7, &bad);
    assert_eq!(s.errors.len(), 2, "mapping re-arms the missing-copy error");
}

#[test]
fn deferred_snapshot_rejections_report_coded_errors_once() {
    use safe_cex_engine::{Level, log::code};
    for wrong_product in [false, true] {
        let mut s = Session::new("DEFAULT", true).ready();
        let row = user_order("pending-size", "BTC-USD", "OPEN", "0", "1");
        let detail = if wrong_product {
            size_detail(
                "pending-size",
                r#"{"limit_limit_gtc":{"base_size":"1","limit_price":"100"}}"#,
            )
            .replace("BTC-USD", "ETH-USD")
        } else {
            size_detail("pending-size", "{}")
        };
        for seq in 0..2 {
            s.message(seq, "snapshot", &format!("[{row}]"));
            let request = next_read(&mut s, "/orders/historical/pending-size");
            s.reply(request, 200, &detail);
            let events =
                s.e.drain_actions()
                    .filter_map(|a| match a {
                        Action::Notice(Notice::Event(event)) if event.code == code::ROW_SKIPPED => {
                            Some(event)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
            assert_eq!(events.len(), usize::from(seq == 0));
            if let Some(event) = events.first() {
                assert_eq!(event.level, Level::Error);
                assert!(
                    event
                        .message
                        .starts_with("Coinbase skipped unmappable order pending-size")
                );
            }
            assert!(s.e.store().order("pending-size").is_none());
        }
    }
}

fn execution_prices_and_amounts(e: &mut Engine<Coinbase>) -> Vec<(f64, f64)> {
    let Some(frame) = e.take_market_frame(100000.0) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == safe_cex_engine::wire::section::FILLS)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let values = (4 + count * 12).next_multiple_of(8);
    (0..count)
        .map(|i| {
            let offset = values + i * 16;
            (
                f64::from_le_bytes(payload[offset..offset + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[offset + 8..offset + 16].try_into().unwrap()),
            )
        })
        .collect()
}

#[test]
fn cumulative_quote_prices_and_equal_endpoint_corrections_preserve_native_units() {
    for (kind, spot, nonunit_intx, native_scale) in [
        ("DEFAULT", true, false, 1),
        ("INTX", false, false, 1),
        ("INTX", false, true, 1),
        ("DEFAULT", false, false, 100),
    ] {
        let mut s = Session::new(kind, spot);
        if nonunit_intx {
            s.products =
                Some(INTX.replace("\"contract_size\":\"1\"", "\"contract_size\":\"0.01\""));
        }
        let mut s = s.ready();
        let product = if spot {
            "BTC-USD"
        } else if kind == "INTX" {
            "BTC-PERP-INTX"
        } else {
            "BTC-CDE"
        };
        let row = |filled: usize, average: &str| {
            let mut row = user_order(
                "quote-progress",
                product,
                "OPEN",
                &(filled * native_scale).to_string(),
                &((4 - filled) * native_scale).to_string(),
            )
            .replace(
                "\"avg_price\":\"100\"",
                &format!("\"avg_price\":\"{average}\""),
            );
            if spot {
                row.pop();
                row.push_str(r#","order_configuration":{"limit_limit_gtc":{"base_size":"4","limit_price":"100"}}}"#);
            }
            row
        };
        s.message(0, "snapshot", &format!("[{}]", row(1, "100")));
        assert!(execution_prices_and_amounts(&mut s.e).is_empty());
        s.message(1, "update", &format!("[{}]", row(2, "110")));
        assert_eq!(execution_prices_and_amounts(&mut s.e), [(120.0, 1.0)]);
        // An equal-quantity receipt refreshes the quote without redelivering.
        s.message(2, "update", &format!("[{}]", row(2, "115")));
        assert!(execution_prices_and_amounts(&mut s.e).is_empty());
        s.message(3, "update", &format!("[{}]", row(3, "120")));
        s.message(4, "update", &format!("[{}]", row(3, "120")));
        assert_eq!(execution_prices_and_amounts(&mut s.e), [(130.0, 1.0)]);
        assert_eq!(s.e.store().order("quote-progress").unwrap().filled, 3.0);
    }
}

#[test]
fn invalid_incremental_quote_preserves_eligibility_for_its_corrected_receipt() {
    let mut s = Session::new("INTX", false).ready();
    let row = |filled: &str, average: &str| {
        user_order("quote-retry", "BTC-PERP-INTX", "OPEN", filled, "2").replace(
            "\"avg_price\":\"100\"",
            &format!("\"avg_price\":\"{average}\""),
        )
    };
    s.message(0, "snapshot", &format!("[{}]", row("1", "100")));
    assert!(execution_prices_and_amounts(&mut s.e).is_empty());
    s.message(1, "update", &format!("[{}]", row("2", "40")));
    assert!(execution_prices_and_amounts(&mut s.e).is_empty());
    assert_eq!(s.e.store().order("quote-retry").unwrap().filled, 1.0);
    s.message(2, "update", &format!("[{}]", row("2", "110")));
    s.message(3, "update", &format!("[{}]", row("2", "110")));
    assert_eq!(execution_prices_and_amounts(&mut s.e), [(120.0, 1.0)]);
}
