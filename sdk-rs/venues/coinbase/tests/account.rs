use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_coinbase::{Coinbase, Options};

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
    logs: Vec<String>,
}
impl Session {
    fn new(kind: &'static str, spot: bool) -> Self {
        let mut e = Engine::new(
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
            logs: vec![],
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
                    self.private = Some(socket)
                }
                Action::Notice(safe_cex_engine::Notice::Log { message, .. }) => {
                    self.logs.push(message.to_string())
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
            BREAKDOWN.into()
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
            r#"{"orders":[],"has_next":false}"#.into()
        } else if url.contains("/products") {
            if url.contains("contract_expiry_type=PERPETUAL") {
                INTX.into()
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

#[test]
fn account_identity_tracks_selected_portfolio_after_discovery() {
    let session = Session::new("DEFAULT", true);
    assert_eq!(session.e.account_id(), None);
    let session = session.ready();
    assert_eq!(session.e.account_id(), Some("portfolio-one"));
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
