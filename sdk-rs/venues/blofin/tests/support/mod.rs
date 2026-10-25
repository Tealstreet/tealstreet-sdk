use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_blofin::{Blofin, Options};
pub const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
pub const POSITION: &str = r#"{"instId":"BTC-USDT","instType":"SWAP","positionId":"395624","positionSide":"net","marginMode":"cross","positions":"4","averagePrice":"59965.8","markPrice":"60000","maintenanceMargin":"0.72","unrealizedPnl":"0.5","leverage":"20","liquidationPrice":"26914.9"}"#;
pub struct Harness {
    pub e: Engine<Blofin>,
    pub pending: Vec<HttpRequest>,
    pub actions: Vec<Action>,
    /// Structured failures; `actions` holds their legacy `CommandDone` text.
    pub failures: Vec<safe_cex_engine::VenueError>,
    pub private: SocketId,
    pub tick: TimerId,
    /// The latest event time, so `collect` releases managed reads at it.
    pub now: f64,
    /// The construction time `respond` answers at, so managed contexts never
    /// see the clock run backwards behind it.
    pub start: f64,
}
impl Harness {
    pub fn new() -> Self {
        Self::new_at(0.0)
    }
    pub fn new_at(now: f64) -> Self {
        Self::with_instruments_and_broker_at(INSTRUMENT, false, now)
    }
    #[allow(dead_code)] // Used by the trading suite; other suites share this module.
    pub fn with_broker_attribution(broker_attribution: bool) -> Self {
        Self::with_instruments_and_broker_at(INSTRUMENT, broker_attribution, 0.0)
    }
    #[allow(dead_code)]
    pub fn with_instruments(instruments: &str) -> Self {
        Self::with_instruments_and_broker_at(instruments, false, 0.0)
    }
    fn with_instruments_and_broker_at(
        instruments: &str,
        broker_attribution: bool,
        now: f64,
    ) -> Self {
        Self::with_entropy(instruments, broker_attribution, now, None)
    }
    #[allow(dead_code)]
    pub fn with_signing_entropy(fill: safe_cex_engine::entropy::Fill) -> Self {
        Self::with_entropy(INSTRUMENT, false, 0.0, Some(fill))
    }
    /// Trusted setup with the venue's refs adjusted, e.g. a missing passphrase ref.
    #[allow(dead_code)]
    pub fn with_identities(adjust: impl FnOnce(&mut safe_cex_engine::signing::Identities)) -> Self {
        Self::build(INSTRUMENT, false, 0.0, None, adjust)
    }
    fn with_entropy(
        instruments: &str,
        broker_attribution: bool,
        now: f64,
        fill: Option<safe_cex_engine::entropy::Fill>,
    ) -> Self {
        Self::build(instruments, broker_attribution, now, fill, |_| {})
    }
    fn build(
        instruments: &str,
        broker_attribution: bool,
        now: f64,
        fill: Option<safe_cex_engine::entropy::Fill>,
        adjust: impl FnOnce(&mut safe_cex_engine::signing::Identities),
    ) -> Self {
        // Trusted setup registers the same opaque refs the venue holds.
        let (signer, mut identities) = safe_cex_crypto::signer::LocalSigner::from_json(
            r#"{"primary":{"key":"key","secret":"secret","passphrase":"pass"}}"#.into(),
        )
        .unwrap();
        adjust(&mut identities);
        let mut e = safe_cex_engine::test_support::seeded_engine(Blofin::new(Options {
            broker_attribution,
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            identities: identities.clone(),
            ..Options::default()
        }));
        e.set_signer(Some(std::sync::Arc::new(signer))).unwrap();
        e.set_identities(identities).unwrap();
        if let Some(fill) = fill {
            e.set_signing_entropy(fill).unwrap();
        }
        e.start(now);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            failures: vec![],
            private: SocketId(0),
            tick: TimerId(0),
            now,
            start: now,
        };
        h.collect_at(now);
        let instrument_request = h.take("instruments");
        h.respond_at(instrument_request, instruments, now);
        let tickers = h.take("tickers");
        h.respond_at(tickers,r#"{"code":"0","data":[{"instId":"BTC-USDT","last":"60000","open24h":"59000","bidPrice":"59999","askPrice":"60001","volCurrency24h":"1","vol24h":"60000"}]}"#,now);
        h.private = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.tick = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == now + 10_000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        h
    }
    /// Handle an event at `now`; a later `collect` releases at that time.
    #[allow(dead_code)]
    pub fn handle(&mut self, now: f64, event: Event<'_>) {
        self.now = now;
        self.e.handle(now, event);
    }
    pub fn collect(&mut self) {
        let now = self.now;
        self.collect_at(now);
    }
    pub fn collect_at(&mut self, now: f64) {
        self.now = now;
        let actions = safe_cex_engine::test_support::prepared_actions(&mut self.e, now);
        self.collect_actions(actions);
    }
    fn collect_actions(&mut self, actions: Vec<Action>) {
        let mut receipts = Vec::new();
        for a in actions {
            if let Action::Http(r) = a {
                let payload = r
                    .body
                    .as_deref()
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
                let rows: Vec<_> = payload
                    .as_ref()
                    .map(|value| {
                        value
                            .as_array()
                            .map_or_else(|| vec![value], |rows| rows.iter().collect())
                    })
                    .unwrap_or_default();
                let expected: Vec<_> = rows
                    .into_iter()
                    .filter_map(|row| {
                        Some(safe_cex_engine::OrderIdentityReceipt {
                            symbol: self
                                .e
                                .store()
                                .market_by_id(row["instId"].as_str()?)?
                                .symbol
                                .clone(),
                            kind: safe_cex_engine::OrderIdentityKind::ClientOrderId,
                            identity: row["clientOrderId"].as_str()?.into(),
                        })
                    })
                    .collect();
                assert_eq!(
                    std::mem::take(&mut receipts),
                    expected,
                    "receipt must precede the exact outgoing native identities: {}",
                    r.url
                );
                self.pending.push(r);
            } else {
                if let Action::Notice(safe_cex_engine::Notice::CommandProgress {
                    receipts: actual,
                    ..
                }) = &a
                {
                    receipts.extend(actual.clone());
                }
                let a = match a {
                    Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error }) => {
                        self.failures.push(error.clone());
                        Action::Notice(safe_cex_engine::Notice::CommandFailed {
                            command,
                            error: error.clone(),
                        })
                    }
                    a => a,
                };
                self.actions.push(a);
            }
        }
    }
    pub fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .pending
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}; pending {:?}", self.pending));
        self.pending.remove(i)
    }
    pub fn reply(&mut self, path: &str, body: &str) {
        let r = self.take(path);
        self.respond(r, body);
    }
    pub fn respond(&mut self, r: HttpRequest, body: &str) {
        let start = self.start;
        self.respond_at(r, body, start);
    }
    pub fn respond_at(&mut self, r: HttpRequest, body: &str, now: f64) {
        self.e.handle(
            now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect_at(now);
    }
    pub fn modes(&mut self) {
        self.reply(
            "margin-mode",
            r#"{"code":"0","data":{"marginMode":"cross"}}"#,
        );
        self.reply(
            "position-mode",
            r#"{"code":"0","data":{"positionMode":"net_mode"}}"#,
        );
    }
    pub fn seed(&mut self) {
        self.modes();
        self.reply(
            "account/positions",
            &format!("{{\"code\":\"0\",\"data\":[{POSITION}]}}"),
        );
        self.reply(
            "balances",
            r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"10"}]}"#,
        );
        for path in [
            "orders-pending",
            "orders-tpsl-pending",
            "orders-algo-pending",
        ] {
            self.reply(path, r#"{"code":"0","data":[]}"#);
        }
    }
    pub fn ws(&mut self, text: &str) {
        self.now = self.now.max(1.0);
        self.e.handle(
            1.0,
            Event::WsMessage {
                socket: self.private,
                text,
            },
        );
        self.collect();
    }
}
