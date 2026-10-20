use safe_cex_engine::model::OrderSide;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_polymarket::{Options, Polymarket, PolymarketOptions, sign};

const KEY: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const SIGNER: &str = "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf";
const MARKETS: &str = r#"[{"conditionId":"0x123","slug":"election","question":"Who wins?","clobTokenIds":"[\"1\",\"2\"]","outcomes":"[\"Yes\",\"No\"]","orderPriceMinTickSize":0.01,"orderMinSize":5,"active":true,"closed":false}]"#;
const ORDER: &str = r#"{"id":"o1","asset_id":"1","side":"BUY","price":"0.5","original_size":"10","size_matched":"2","status":"LIVE","order_type":"GTC","created_at":"1700000000"}"#;
const POSITIONS: &str = r#"[{"asset":"1","size":10,"avgPrice":0.5,"curPrice":0,"cashPnl":-5,"redeemable":true,"mergeable":false}]"#;
struct H {
    e: Engine<Polymarket>,
    now: f64,
    http: Vec<HttpRequest>,
    private: Option<SocketId>,
    public: Option<SocketId>,
    opened: bool,
    sends: Vec<String>,
    logs: Vec<String>,
    timers: Vec<(TimerId, f64)>,
}
impl H {
    fn with(options: Options) -> Self {
        Self::with_seed(options, true)
    }
    fn with_seed(options: Options, seed: bool) -> Self {
        let mut h = Self {
            e: Engine::new(Polymarket::new(options)),
            now: 1_700_000_000_000.0,
            http: vec![],
            private: None,
            public: None,
            opened: false,
            sends: vec![],
            logs: vec![],
            timers: vec![],
        };
        h.e.start(h.now);
        h.collect();
        h.response("/markets", 200, MARKETS);
        if seed {
            h.response("/midpoints", 200, r#"{"1":"0.5","2":"0.5"}"#);
        }
        h
    }
    fn new() -> Self {
        Self::with(Options {
            key: Some(KEY.into()),
            polymarket: Some(PolymarketOptions {
                api_key: Some("api".into()),
                api_secret: Some("c2VjcmV0".into()),
                api_passphrase: Some("pass".into()),
                ..Default::default()
            }),
            ..Default::default()
        })
    }
    fn collect(&mut self) {
        for a in self.e.drain_actions() {
            match a {
                Action::Http(r) => self.http.push(r),
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => self.private = Some(socket),
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Public),
                    ..
                } => self.public = Some(socket),
                Action::WsSend { text, .. } => self.sends.push(text),
                Action::Notice(safe_cex_engine::Notice::Log { message, .. }) => {
                    self.logs.push(message)
                }
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        if matches!(&event, Event::WsOpened { .. }) {
            self.opened = true;
        }
        self.e.handle(self.now, event);
        self.collect();
    }
    fn response(&mut self, path: &str, status: u16, body: &str) {
        let wrapped = (path.contains("/markets") && body.trim_start().starts_with('['))
            .then(|| format!(r#"{{"markets":{body}}}"#));
        let body = wrapped.as_deref().unwrap_or(body);
        let i = self
            .http
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing{path}: {:?}", self.http));
        let r = self.http.remove(i);
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn initial(&mut self) {
        self.response(
            "/balance-allowance",
            200,
            r#"{"balance":"12345678","allowances":{}}"#,
        );
        self.response(
            "/data/orders",
            200,
            &format!(r#"{{"data":[{ORDER}],"next_cursor":"LTE="}}"#),
        );
        self.response("/positions", 200, POSITIONS);
    }
    fn ws(&mut self, text: &str) {
        if !self.opened {
            self.event(Event::WsOpened {
                socket: self.private.expect("private socket"),
            });
        }
        self.event(Event::WsMessage {
            socket: self.private.expect("private socket"),
            text,
        });
    }
    fn poll(&mut self) {
        let (timer, at_ms) = self.timers.pop().unwrap();
        self.now = at_ms;
        self.event(Event::TimerFired { timer });
    }
    fn fills(&mut self) -> Vec<(u32, u32, f64, f64)> {
        let Some(frame) = self.e.take_frame(self.now) else {
            return vec![];
        };
        let Some((_, b)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
        else {
            return vec![];
        };
        let count = u32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
        let offset = (4 + 12 * count).div_ceil(8) * 8;
        (0..count)
            .map(|i| {
                let at = offset + 16 * i;
                (
                    u32::from_le_bytes(
                        b[4 + 4 * count + 4 * i..8 + 4 * count + 4 * i]
                            .try_into()
                            .unwrap(),
                    ),
                    u32::from_le_bytes(
                        b[4 + 8 * count + 4 * i..8 + 8 * count + 4 * i]
                            .try_into()
                            .unwrap(),
                    ),
                    f64::from_le_bytes(b[at..at + 8].try_into().unwrap()),
                    f64::from_le_bytes(b[at + 8..at + 16].try_into().unwrap()),
                )
            })
            .collect()
    }
}
#[test]
fn provided_credentials_sign_exact_read_paths_and_preserve_zero_resolution_price() {
    let mut h = H::new();
    assert_eq!(h.http.len(), 3);
    assert!(h.private.is_some());
    let request = h
        .http
        .iter()
        .find(|r| r.url.contains("/data/orders"))
        .unwrap();
    let headers = sign::l2_headers(
        &sign::Credentials {
            api_key: "api".into(),
            secret: "c2VjcmV0".into(),
            passphrase: "pass".into(),
        },
        SIGNER,
        1700000000,
        safe_cex_engine::Method::Get,
        "/data/orders",
        "",
    )
    .unwrap();
    assert!(
        headers
            .iter()
            .all(|header| request.headers.contains(header))
    );
    h.event(Event::WsOpened {
        socket: h.private.unwrap(),
    });
    assert!(
        h.sends
            .iter()
            .any(|s| s.contains(r#""type":"user""#) && s.contains(r#""apiKey":"api""#))
    );
    h.initial();
    let balance = h.e.store().balance();
    assert_eq!(balance.total, 12.345678);
    assert_eq!(balance.free, balance.total);
    let p = &h.e.store().positions()[0];
    assert_eq!(p.contracts, 10.0);
    assert_eq!(p.notional, 0.0);
    assert_eq!(p.unrealized_pnl, -5.0);
    assert_eq!(p.leverage, 1.0);
    assert_eq!(p.liquidation_price, 0.0);
    assert!(h.e.store().prediction_holdings().unwrap()[0].redeemable);
}
#[test]
fn derives_credentials_using_server_time_before_any_private_read() {
    let mut h = H::with(Options {
        key: Some(KEY.into()),
        ..Default::default()
    });
    assert!(h.private.is_none());
    assert_eq!(h.http.len(), 1);
    h.response("/time", 200, "1770000000");
    let r = &h.http[0];
    assert!(r.url.ends_with("/auth/derive-api-key"));
    assert!(r.headers.contains(&("POLY_TIMESTAMP", "1770000000".into())));
    assert!(r.headers.contains(&(
        "POLY_SIGNATURE",
        sign::l1_signature(KEY, 1770000000, 0).unwrap()
    )));
    h.response(
        "/auth/derive-api-key",
        200,
        r#"{"apiKey":"api","secret":"c2VjcmV0","passphrase":"pass"}"#,
    );
    assert_eq!(h.http.len(), 3);
    assert!(h.private.is_some());
}
#[test]
fn credential_derivation_failure_retries_with_fresh_server_time() {
    let mut h = H::with(Options {
        key: Some(KEY.into()),
        ..Default::default()
    });
    h.response("/time", 200, "1770000000");
    h.response("/auth/derive-api-key", 503, r#"{"error":"unavailable"}"#);
    assert!(h.private.is_none());
    h.poll();
    h.response("/time", 200, "1770000002");
    assert!(
        h.http[0]
            .headers
            .contains(&("POLY_TIMESTAMP", "1770000002".into()))
    );
}
#[test]
fn public_only_and_invalid_funder_never_start_private_transport() {
    let h = H::with(Options {
        public_data_only: true,
        key: Some(KEY.into()),
        ..Default::default()
    });
    assert!(h.http.is_empty());
    assert!(h.private.is_none());
    assert!(h.e.store().loaded().positions);
    let h = H::with(Options {
        key: Some(KEY.into()),
        polymarket: Some(PolymarketOptions {
            signer_type: 3,
            ..Default::default()
        }),
        ..Default::default()
    });
    assert!(h.http.is_empty());
    assert!(h.private.is_none());
}
#[test]
fn partial_empty_or_malformed_credentials_reject_before_private_io() {
    for (api, secret, pass) in [
        (Some("api"), None, Some("pass")),
        (Some(""), Some("c2VjcmV0"), Some("pass")),
        (Some("api"), Some("%%%"), Some("pass")),
    ] {
        let h = H::with(Options {
            key: Some(KEY.into()),
            polymarket: Some(PolymarketOptions {
                api_key: api.map(str::to_string),
                api_secret: secret.map(str::to_string),
                api_passphrase: pass.map(str::to_string),
                ..Default::default()
            }),
            ..Default::default()
        });
        assert!(h.private.is_none());
        assert!(h.http.is_empty());
        assert!(!h.e.store().loaded().orders);
    }
}
#[test]
fn failed_reads_never_erase_known_snapshots() {
    let mut h = H::new();
    h.initial();
    h.poll();
    h.response("/balance-allowance", 500, r#"{"error":"outage"}"#);
    h.response("/data/orders", 200, r#"{"error":"outage"}"#);
    h.response("/positions", 502, "Bad Gateway");
    assert_eq!(h.e.store().balance().total, 12.345678);
    assert_eq!(h.e.store().positions()[0].contracts, 10.0);
    assert_eq!(h.e.store().orders()[0].id, "o1");
}
#[test]
fn malformed_money_and_missing_holding_identity_fail_closed() {
    let mut h = H::new();
    h.initial();
    h.poll();
    h.response("/balance-allowance", 200, r#"{"balance":"NaN"}"#);
    h.response("/data/orders", 200, r#"{"data":[{"id":"bad","asset_id":"1","side":"BUY","price":"bogus","original_size":"10"}],"next_cursor":"LTE="}"#);
    h.response(
        "/positions",
        200,
        r#"[{"asset":"","size":10,"avgPrice":0.5}]"#,
    );
    assert_eq!(h.e.store().balance().total, 12.345678);
    assert_eq!(h.e.store().orders()[0].id, "o1");
    assert_eq!(h.e.store().positions()[0].contracts, 10.0);
}
#[test]
fn cursor_orders_commit_atomically_and_cycles_fail_closed() {
    let mut h = H::new();
    h.initial();
    h.poll();
    h.response(
        "/data/orders",
        200,
        &format!(
            r#"{{"data":[{}],"next_cursor":"abc="}}"#,
            ORDER.replace("o1", "o2")
        ),
    );
    assert_eq!(h.e.store().orders()[0].id, "o1");
    assert!(h.http.iter().any(|r| r.url.ends_with("next_cursor=abc%3D")));
    h.response("/data/orders", 200, r#"{"data":[],"next_cursor":"abc="}"#);
    assert_eq!(h.e.store().orders()[0].id, "o1");
}
#[test]
fn offset_positions_commit_only_after_last_successful_page() {
    let mut h = H::new();
    h.initial();
    h.poll();
    let mut rows = vec![r#"{"asset":"1","size":99,"avgPrice":0.5,"curPrice":0.6}"#.to_string()];
    for i in 1..500 {
        rows.push(format!(r#"{{"asset":"zero{i}","size":0,"avgPrice":0.5}}"#));
    }
    h.response("/positions", 200, &format!("[{}]", rows.join(",")));
    assert_eq!(h.e.store().positions()[0].contracts, 10.0);
    assert!(h.http.iter().any(|r| r.url.contains("offset=500")));
    h.response("/positions", 500, r#"{"error":"offline"}"#);
    assert_eq!(h.e.store().positions()[0].contracts, 10.0);
}
#[test]
fn cancellation_fences_old_rest_and_delayed_placements() {
    let mut h = H::new();
    h.ws(r#"{"event_type":"order","type":"CANCELLATION","id":"o1","timestamp":"1700000001000"}"#);
    h.initial();
    assert!(h.e.store().orders().is_empty());
    h.ws(&format!(
        r#"{{"event_type":"order",{}}}"#,
        &ORDER[1..ORDER.len() - 1]
    ));
    assert!(h.e.store().orders().is_empty());
}
#[test]
fn cumulative_order_notifications_never_emit_fills_or_decrease_matched_size() {
    let mut h = H::new();
    h.initial();
    for (filled, time) in [
        (3, 1700000002000u64),
        (2, 1700000003000),
        (4, 1700000001000),
    ] {
        h.ws(&format!(
            r#"{{"event_type":"order","timestamp":"{time}",{}}}"#,
            ORDER[1..ORDER.len() - 1].replace(
                r#""size_matched":"2""#,
                &format!(r#""size_matched":"{filled}""#)
            )
        ));
    }
    assert_eq!(h.e.store().orders()[0].filled, 3.0);
    assert_eq!(h.e.store().orders()[0].remaining, 7.0);
    assert!(h.fills().is_empty());
}
#[test]
fn maker_executions_use_own_order_outcome_side_and_deduplicate_settlement_updates() {
    let mut h = H::new();
    h.initial();
    for status in ["MATCHED", "MINED", "CONFIRMED"] {
        h.ws(&format!(r#"{{"event_type":"trade","id":"t1","taker_order_id":"not-ours","asset_id":"1","side":"BUY","size":"99","price":"0.6","status":"{status}","owner":"api","trader_side":"MAKER","maker_orders":[{{"order_id":"maker-own","owner":"api","asset_id":"2","side":"SELL","matched_amount":"3","price":"0.4"}},{{"order_id":"maker-other","owner":"other","asset_id":"1","side":"SELL","matched_amount":"96","price":"0.6"}}]}}"#));
    }
    assert_eq!(h.fills(), vec![(1, OrderSide::Sell as u32, 0.4, 3.0)]);
}
#[test]
fn trade_during_positions_read_fences_old_holdings_and_refreshes_again() {
    let mut h = H::new();
    h.initial();
    h.poll();
    h.ws(r#"{"event_type":"trade","id":"t2","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"1","price":"0.5","status":"MATCHED","trader_side":"TAKER"}"#);
    h.response("/positions", 200, "[]");
    assert_eq!(h.e.store().positions()[0].contracts, 10.0);
    h.response("/balance-allowance", 200, r#"{"balance":"12345678"}"#);
    h.response("/data/orders", 200, r#"{"data":[],"next_cursor":"LTE="}"#);
    h.poll();
    assert!(h.http.iter().any(|r| r.url.contains("/positions")));
}
#[test]
fn maker_ownership_without_trader_side_does_not_emit_counterparty_taker_fill() {
    let mut h = H::new();
    h.initial();
    h.ws(r#"{"event_type":"trade","id":"legacy","taker_order_id":"counterparty","asset_id":"1","side":"BUY","size":"99","price":"0.5","status":"MATCHED","owner":"api","maker_orders":[{"order_id":"own","owner":"api","asset_id":"1","side":"SELL","matched_amount":"3","price":"0.5"}]}"#);
    assert_eq!(h.fills(), vec![(0, OrderSide::Sell as u32, 0.5, 3.0)]);
}

#[test]
fn aggregate_upnl_tracks_successful_holdings_for_either_read_order_and_survives_failures() {
    for positions_first in [false, true] {
        let mut h = H::new();
        if positions_first {
            h.response("/positions", 200, POSITIONS);
        }
        h.response("/balance-allowance", 200, r#"{"balance":"12345678"}"#);
        if !positions_first {
            h.response("/positions", 200, POSITIONS);
        }
        assert_eq!(h.e.store().balance().upnl, -5.0);
        assert_eq!(h.e.store().balance().total, 12.345678);
        h.response("/data/orders", 200, r#"{"data":[],"next_cursor":"LTE="}"#);
        h.poll();
        h.response("/positions", 500, "unavailable");
        h.response("/balance-allowance", 200, r#"{"balance":"20000000"}"#);
        assert_eq!(h.e.store().balance().upnl, -5.0);
    }
}

#[test]
fn trade_only_progress_updates_visible_order_and_late_cumulative_cannot_regress_it() {
    let mut h = H::new();
    h.initial();
    h.ws(r#"{"event_type":"trade","id":"trade-only","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"5","price":"0.5","status":"MATCHED","trader_side":"TAKER"}"#);
    assert_eq!(
        (
            h.e.store().orders()[0].filled,
            h.e.store().orders()[0].remaining
        ),
        (5.0, 5.0)
    );
    h.ws(&format!(
        r#"{{"event_type":"order","timestamp":"1700000001000",{}}}"#,
        &ORDER[1..ORDER.len() - 1]
    ));
    assert_eq!(
        (
            h.e.store().orders()[0].filled,
            h.e.store().orders()[0].remaining
        ),
        (5.0, 5.0)
    );
    assert_eq!(h.fills(), vec![(0, OrderSide::Buy as u32, 0.5, 5.0)]);
}

#[test]
fn unique_trade_progress_survives_inflight_empty_or_stale_order_snapshots() {
    for rows in ["[]".to_string(), format!("[{ORDER}]")] {
        let mut h = H::new();
        h.initial();
        h.poll();
        h.ws(r#"{"event_type":"trade","id":"trade-before-snapshot","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"5","price":"0.5","status":"MATCHED","trader_side":"TAKER"}"#);
        h.response(
            "/data/orders",
            200,
            &format!(r#"{{"data":{rows},"next_cursor":"LTE="}}"#),
        );
        assert_eq!(
            (
                h.e.store().orders()[0].filled,
                h.e.store().orders()[0].remaining
            ),
            (5.0, 5.0)
        );
    }
}

#[test]
fn full_trade_execution_removes_order_and_fences_later_snapshots() {
    let mut h = H::new();
    h.initial();
    h.poll();
    h.ws(r#"{"event_type":"trade","id":"complete","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"10","price":"0.5","status":"MATCHED","trader_side":"TAKER"}"#);
    assert!(h.e.store().orders().is_empty());
    h.response(
        "/data/orders",
        200,
        &format!(r#"{{"data":[{ORDER}],"next_cursor":"LTE="}}"#),
    );
    assert!(h.e.store().orders().is_empty());
    h.ws(&format!(
        r#"{{"event_type":"order","timestamp":"1700000001000",{}}}"#,
        &ORDER[1..ORDER.len() - 1]
    ));
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn first_snapshot_normalizes_prior_trade_evidence_without_order_details() {
    let mut h = H::new();
    h.ws(r#"{"event_type":"trade","id":"before-first-order","taker_order_id":"o1","asset_id":"1","side":"BUY","size":"5","price":"0.5","status":"MATCHED","trader_side":"TAKER"}"#);
    h.initial();
    assert_eq!(
        (
            h.e.store().orders()[0].filled,
            h.e.store().orders()[0].remaining
        ),
        (5.0, 5.0)
    );
}

#[test]
fn authoritative_empty_orders_cannot_be_reopened_by_older_live_event() {
    let mut h = H::new();
    h.initial();
    h.poll();
    h.response("/data/orders", 200, r#"{"data":[],"next_cursor":"LTE="}"#);
    assert!(h.e.store().orders().is_empty());
    h.ws(&format!(
        r#"{{"event_type":"order","timestamp":"1700000000000",{}}}"#,
        &ORDER[1..ORDER.len() - 1]
    ));
    assert!(
        h.e.store().orders().is_empty(),
        "old LIVE notification reopened an order absent from authoritative snapshot"
    );
}

#[test]
fn unknown_owned_token_is_visible_failure_and_retains_previous_holdings() {
    let mut h = H::new();
    h.initial();
    let before = h.e.store().positions().to_vec();
    h.poll();
    h.response(
        "/positions",
        200,
        &POSITIONS.replace("\"asset\":\"1\"", "\"asset\":\"unknown\""),
    );
    assert_eq!(h.e.store().positions(), before);
    let mut h = H::new();
    h.response(
        "/positions",
        200,
        &POSITIONS.replace("\"asset\":\"1\"", "\"asset\":\"unknown\""),
    );
    assert!(!h.e.store().loaded().positions);
    assert!(
        h.logs
            .iter()
            .any(|message| message.contains("owned holding has unavailable market metadata"))
    );
}

#[test]
fn explicitly_unselected_holding_does_not_block_selected_positions() {
    let mut h = H::with(Options {
        key: Some(KEY.into()),
        polymarket: Some(PolymarketOptions {
            market_slugs: Some(vec!["election".into()]),
            api_key: Some("api".into()),
            api_secret: Some("c2VjcmV0".into()),
            api_passphrase: Some("pass".into()),
            ..Default::default()
        }),
        ..Default::default()
    });
    let selected = &POSITIONS[1..POSITIONS.len() - 1];
    h.response("/positions", 200, &format!(r#"[{selected},{{"asset":"unselected-token","slug":"elsewhere","size":2,"avgPrice":0.5,"curPrice":0.6}}]"#));
    assert!(h.e.store().loaded().positions);
    assert_eq!(h.e.store().positions().len(), 1);
    assert_eq!(h.e.store().positions()[0].contracts, 10.0);
}

#[test]
fn bbo_only_peer_never_uses_placeholder_zero_to_value_funded_holdings() {
    for partial_seed in [false, true] {
        let mut h = H::with_seed(
            Options {
                key: Some(KEY.into()),
                polymarket: Some(PolymarketOptions {
                    api_key: Some("api".into()),
                    api_secret: Some("c2VjcmV0".into()),
                    api_passphrase: Some("pass".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            false,
        );
        h.event(Event::Command(
            safe_cex_engine::Command::SubscribeOrderBook {
                symbol: "election-No".into(),
            },
        ));
        let socket = h.public.unwrap();
        h.event(Event::WsOpened { socket });
        h.event(Event::WsMessage { socket, text: r#"{"event_type":"price_change","timestamp":"1700000000000","price_changes":[{"asset_id":"2","price":"0.4","size":"1","side":"BUY","best_bid":"0.4","best_ask":"0.6"}]}"# });
        assert_eq!(h.e.store().ticker("election-No").unwrap().last, 0.0);
        if partial_seed {
            h.response("/midpoints", 200, r#"{"1":"0.5"}"#);
            assert!(h.e.store().loaded().tickers);
        }
        h.response(
            "/positions",
            200,
            r#"[{"asset":"2","size":10,"avgPrice":0.5}]"#,
        );
        assert!(!h.e.store().loaded().positions);
        assert!(
            h.logs
                .iter()
                .any(|message| message.contains("owned holding price unavailable"))
        );
    }
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn user_supervisor_bounds_handshake_and_backs_off_without_changing_subscriptions() {
    let mut h = H::new();
    let socket = h.private.unwrap();
    h.event(Event::WsOpened { socket });
    socket_checks::flaps(h.e, socket, h.now);
}
