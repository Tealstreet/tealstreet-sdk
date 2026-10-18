//! Hyperliquid-owned state over the shared exchange core. Transport/WS wiring is incomplete.
use safe_cex_exchange::{capabilities::ExchangeCapabilities, exchange::BaseExchange};
use safe_cex_types::{Market, OrderSide, OrderTimeInForce, PlaceOrderOpts, PositionSide};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};

pub struct HyperliquidExchange {
    pub(crate) base: Rc<BaseExchange>,
    last_nonce: Cell<f64>,
    pub(crate) capabilities: ExchangeCapabilities,
    placed_tif: RefCell<Vec<(String, OrderTimeInForce)>>,
    cancelled: RefCell<BTreeMap<String, f64>>,
    pub(crate) binding: RefCell<Option<crate::binding::Binding>>,
}
impl HyperliquidExchange {
    /// Incomplete constructor composition: API clients and sockets are not attached yet.
    pub fn from_base(base: BaseExchange) -> Self {
        Self {
            base: Rc::new(base),
            last_nonce: Cell::new(0.0),
            capabilities: ExchangeCapabilities::default(),
            placed_tif: RefCell::new(Vec::new()),
            cancelled: RefCell::new(BTreeMap::new()),
            binding: RefCell::new(None),
        }
    }
    pub fn generate_nonce(&self) -> f64 {
        let now = self.base.runtime.now_ms();
        let nonce = safe_cex_num::math_max(now, self.last_nonce.get() + 1.0);
        self.last_nonce.set(nonce);
        nonce
    }
    pub fn get_order_base_price(
        &self,
        market: &Market,
        purpose: &str,
    ) -> Result<f64, safe_cex_exchange::runtime::ExchangeError> {
        let candidates = self
            .base
            .store
            .tickers_by_symbol()
            .get(&market.symbol)
            .map(|ticker| {
                if market.is_spot == Some(true) {
                    [
                        ticker.last,
                        ticker.mid_price.unwrap_or(f64::NAN),
                        ticker.mark,
                        ticker.index,
                    ]
                } else {
                    [
                        ticker.mark,
                        ticker.last,
                        ticker.mid_price.unwrap_or(f64::NAN),
                        ticker.index,
                    ]
                }
            });
        candidates
            .into_iter()
            .flatten()
            .find(|price| price.is_finite() && *price > 0.0)
            .ok_or_else(|| {
                safe_cex_exchange::runtime::ExchangeError::new(format!(
                    "Cannot place Hyperliquid {purpose} for {}: missing valid ticker price data",
                    market.symbol
                ))
            })
    }
    pub fn should_use_position_tpsl(
        &self,
        opts: &PlaceOrderOpts,
        market: &Market,
        order_size: f64,
    ) -> bool {
        if opts.amount == 0.0 {
            return true;
        }
        if opts.reduce_only != Some(true) || !order_size.is_finite() || order_size <= 0.0 {
            return false;
        }
        let target_side = if opts.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        self.base
            .store
            .positions()
            .iter()
            .find(|position| {
                position.symbol == market.symbol
                    && position.side == target_side
                    && position.contracts > 0.0
            })
            .is_some_and(|position| order_size >= position.contracts)
    }
    pub fn is_public_data_only(&self) -> bool {
        self.base.options().public_data_only == Some(true) || !self.has_private_session_identity()
    }
    pub fn has_private_session_identity(&self) -> bool {
        let options = self.base.options();
        options.vault.as_ref().is_some_and(|s| !s.is_empty())
            || options
                .application_id
                .as_ref()
                .is_some_and(|s| !s.is_empty())
    }
    pub fn remember_limit_order_time_in_force(&self, id: String, tif: OrderTimeInForce) {
        let mut cache = self.placed_tif.borrow_mut();
        cache.retain(|(key, _)| key != &id);
        cache.push((id, tif));
        while cache.len() > 500 {
            cache.remove(0);
        }
    }
    pub fn get_placed_limit_order_time_in_force(&self, id: &str) -> Option<OrderTimeInForce> {
        self.placed_tif
            .borrow()
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, tif)| *tif)
    }
    pub fn forget_limit_order_time_in_force(&self, id: &str) {
        self.placed_tif.borrow_mut().retain(|(key, _)| key != id);
    }
    pub fn is_order_cancelled(&self, id: &str) -> bool {
        let time = self.cancelled.borrow().get(id).copied();
        let Some(time) = time else {
            return false;
        };
        if self.base.runtime.now_ms() - time > 120_000.0 {
            self.cancelled.borrow_mut().remove(id);
            return false;
        }
        true
    }
    pub fn suppress_cancelled_order(&self, id: String) {
        // The host clock can reenter venue code; do not hold state across it.
        let now = self.base.runtime.now_ms();
        self.cancelled.borrow_mut().insert(id, now);
    }
    pub fn unsuppress_cancelled_order(&self, id: &str) {
        self.cancelled.borrow_mut().remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_exchange::{
        exchange::{Exchange, ExchangeSettings},
        runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    };
    use safe_cex_store::store::DefaultStore;
    use safe_cex_types::ExchangeName;
    use serde_json::{Value, json};
    use std::{cell::Cell, future::Future, pin::Pin, rc::Rc};
    #[test]
    fn source_nonce_bits_match_before_serialization() {
        let output = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/exchange-nonce.cjs"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "nonce source oracle must execute {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let corpus: Value =
            serde_json::from_str(include_str!("../tests/fixtures/exchange-nonce.json")).unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 15, "NONCE-CARDINALITY source total");
        let mut sequence = None;
        let clock = Rc::new(Clock::default());
        let mut exchange = subject(serde_json::json!({}), clock.clone());
        let mut comparisons = 0;
        // The native walk stays fixed even if the source-count guard is bypassed.
        for index in 0..15 {
            let row = cases
                .get(index)
                .expect("NONCE-CARDINALITY native index coverage");
            let next = row["sequence"].as_u64().unwrap();
            if sequence != Some(next) {
                exchange = subject(serde_json::json!({}), clock.clone());
                sequence = Some(next);
            }
            clock.0.set(safe_cex_num::number_from_string(
                row["input"].as_str().unwrap(),
            ));
            let actual = exchange.generate_nonce();
            let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
            assert_eq!(
                if actual.is_nan() {
                    0x7ff8000000000000
                } else {
                    actual.to_bits()
                },
                expected,
                "source nonce bits for {row}"
            );
            assert_eq!(
                if exchange.last_nonce.get().is_nan() {
                    0x7ff8000000000000
                } else {
                    exchange.last_nonce.get().to_bits()
                },
                expected,
                "stored nonce bits"
            );
            comparisons += 2;
        }
        assert_eq!(comparisons, 30, "NONCE-CARDINALITY native bit comparisons");
    }
    mod read_trace {
        include!("../tests/fixtures/store-read-observer.rs");
        pub(super) fn new(inner: Rc<safe_cex_store::store::DefaultStore>) -> Rc<ReadTrace> {
            Rc::new(ReadTrace {
                inner,
                position_reads: Cell::new(0),
                ticker_reads: Cell::new(0),
            })
        }
    }
    #[test]
    fn placement_reads_match_source_values_and_errors() {
        use safe_cex_store::{Store, StoreUpdate};
        let corpus: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/exchange-placement-reads.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 30, "PLACEMENT-CARDINALITY source total");
        for (kind, count) in [("price", 10), ("tpsl", 20)] {
            assert_eq!(
                cases.iter().filter(|case| case["kind"] == kind).count(),
                count,
                "PLACEMENT-CARDINALITY source category {kind}"
            );
        }
        let mut market: Market = serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":10}}})).unwrap();
        let mut comparisons = 0;
        for index in 0..30 {
            let row = cases
                .get(index)
                .expect("PLACEMENT-CARDINALITY native index coverage");
            let store = Rc::new(DefaultStore::new());
            let trace = read_trace::new(store.clone());
            let state = subject_with(json!({}), Rc::new(Clock::default()), trace.clone());
            if row["kind"] == "price" {
                let nums: Vec<f64> = row["values"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| safe_cex_num::number_from_string(v.as_str().unwrap()))
                    .collect();
                assert_eq!(nums.len(), 4, "PLACEMENT-CARDINALITY price input fields");
                let mut ticker: safe_cex_types::Ticker = serde_json::from_value(json!({"id":"BTC","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
                ticker.last = nums[0];
                ticker.mid_price = Some(nums[1]);
                ticker.mark = nums[2];
                ticker.index = nums[3];
                Store::update(
                    store.as_ref(),
                    StoreUpdate {
                        tickers: Some(vec![ticker]),
                        ..Default::default()
                    },
                    false,
                )
                .expect("placement fixture Store update must succeed");
                market.is_spot = Some(row["isSpot"].as_bool().unwrap());
                let actual = state.get_order_base_price(&market, "limit");
                assert_eq!(trace.ticker_reads.get(), 1, "source ticker map read count");
                if let Some(error) = row.get("error") {
                    assert_eq!(
                        actual.unwrap_err().message,
                        error["message"].as_str().unwrap(),
                        "source price error"
                    );
                } else {
                    assert_eq!(
                        actual.unwrap().to_bits(),
                        u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap(),
                        "source price precedence bits"
                    );
                }
            } else {
                let mut positions = Vec::new();
                for contracts in [2.0, 1.0] {
                    let position: safe_cex_types::Position = serde_json::from_value(json!({"id":contracts.to_string(),"symbol":"BTC","side":"short","entryPrice":0,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":contracts,"liquidationPrice":0})).unwrap();
                    positions.push(position);
                }
                Store::update(
                    store.as_ref(),
                    StoreUpdate {
                        positions: Some(positions),
                        ..Default::default()
                    },
                    false,
                )
                .expect("placement fixture Store update must succeed");
                let opts: PlaceOrderOpts=serde_json::from_value(json!({"symbol":"BTC","type":"market","side":"buy","amount":row["amount"],"reduceOnly":row["reduceOnly"]})).unwrap();
                let size = safe_cex_num::number_from_string(row["size"].as_str().unwrap());
                assert_eq!(
                    state.should_use_position_tpsl(&opts, &market, size),
                    row["result"].as_bool().unwrap(),
                    "source first position and TPSL guards"
                );
                assert_eq!(
                    trace.position_reads.get(),
                    row["reads"].as_u64().unwrap() as usize,
                    "source position read count"
                );
            }
            comparisons += 2;
        }
        assert_eq!(
            comparisons, 60,
            "PLACEMENT-CARDINALITY native value and effect comparisons"
        );
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/exchange-placement-reads.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "placement source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn placement_string_boundary_matches_source_acceptance() {
        #[derive(serde::Deserialize)]
        struct Case {
            value: Box<serde_json::value::RawValue>,
            tpsl: bool,
        }
        #[derive(serde::Deserialize)]
        struct Capture {
            cases: Vec<Case>,
        }
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/exchange-placement-reads.cjs"
            ))
            .arg("--string-boundary")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source placement string boundary {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let capture: Capture = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(capture.cases.len(), 5);
        let mut refusals = 0;
        for case in capture.cases {
            assert!(case.tpsl, "source accepts zero amount before string lookup");
            match serde_json::from_str::<String>(case.value.get()) {
                Ok(value) => assert!(
                    ["BTC", "é", "😀"].contains(&value.as_str()),
                    "ordinary scalar strings remain accepted"
                ),
                Err(_) => {
                    assert!(
                        [r#""\ud800""#, r#""\udfff""#].contains(&case.value.get()),
                        "UTF-8 refusal must remain limited to lone surrogate witnesses"
                    );
                    refusals += 1;
                }
            }
        }
        assert_eq!(refusals, 2, "placement UTF-8 representation witnesses");
    }
    #[derive(Default)]
    struct Clock(Cell<f64>, RefCell<Option<Box<dyn Fn()>>>);
    impl ExchangeRuntime for Clock {
        fn monotonic_now_ms(&self) -> f64 {
            panic!("state helpers must never use monotonic timers")
        }
        fn now_ms(&self) -> f64 {
            let hook = self.1.borrow_mut().take();
            if let Some(hook) = hook {
                hook();
            }
            self.0.get()
        }
        fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
            Box::pin(async { Err(ExchangeError::new("unexpected sleep")) })
        }
        fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
            panic!("state helpers must never spawn")
        }
        fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
        fn unique_id(&self) -> String {
            "state-test".into()
        }
        fn is_worker(&self) -> bool {
            false
        }
        fn random(&self) -> f64 {
            0.5
        }
        fn report_raw_exception(
            &self,
            _: &safe_cex_exchange::js_runtime::value::Js,
            _: &serde_json::Value,
        ) {
            panic!("unexpected raw exception report in this test runtime");
        }
        fn report_exception(&self, e: &ExchangeError, _: &Value) {
            panic!("unexpected background error {}", e.message)
        }
        fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
            Box::pin(async { Err(ExchangeError::new("unexpected worker request")) })
        }
    }
    fn subject(options: Value, clock: Rc<Clock>) -> HyperliquidExchange {
        subject_with(options, clock, Rc::new(DefaultStore::new()))
    }
    fn subject_with(
        options: Value,
        clock: Rc<Clock>,
        store: Rc<dyn safe_cex_store::Store>,
    ) -> HyperliquidExchange {
        let mut options = options.as_object().unwrap().clone();
        options.entry("key").or_insert(json!(""));
        options.entry("secret").or_insert(json!(""));
        HyperliquidExchange::from_base(BaseExchange::new(
            ExchangeName::Hyperliquid,
            serde_json::from_value(Value::Object(options)).unwrap(),
            store,
            clock,
            ExchangeSettings::default(),
        ))
    }
    #[test]
    fn actual_source_identity_tif_and_cancel_state_matches() {
        let corpus: Value =
            serde_json::from_str(include_str!("../tests/fixtures/exchange-state-typed.json"))
                .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 533, "STATE-CARDINALITY source total");
        for (kind, operation, count) in [
            ("privacy", None, 10),
            ("tif", Some("remember"), 506),
            ("tif", Some("get"), 7),
            ("tif", Some("forget"), 1),
            ("cancel", Some("get"), 6),
            ("cancel", Some("suppress"), 2),
            ("cancel", Some("unsuppress"), 1),
        ] {
            assert_eq!(
                cases
                    .iter()
                    .filter(|case| case["kind"] == kind && case["op"].as_str() == operation)
                    .count(),
                count,
                "STATE-CARDINALITY source category {kind}/{operation:?}"
            );
        }
        let clock = Rc::new(Clock::default());
        let state = subject(json!({}), clock.clone());
        let mut comparisons = 0;
        let mut field_comparisons = 0;
        for index in 0..533 {
            let c = cases
                .get(index)
                .expect("STATE-CARDINALITY native index coverage");
            match c["kind"].as_str().unwrap() {
                "privacy" => {
                    let s = subject(c["options"].clone(), clock.clone());
                    assert_eq!(
                        s.has_private_session_identity(),
                        c["identity"].as_bool().unwrap(),
                        "private identity {c}"
                    );
                    let dynamic: &dyn Exchange = &s;
                    assert_eq!(
                        dynamic.is_public_data_only(),
                        c["public"].as_bool().unwrap(),
                        "venue privacy dispatch {c}"
                    );
                    field_comparisons += 2;
                }
                "tif" => {
                    let id = c["id"].as_str().unwrap();
                    match c["op"].as_str().unwrap() {
                        "remember" => state.remember_limit_order_time_in_force(
                            id.into(),
                            serde_json::from_value(c["tif"].clone()).unwrap(),
                        ),
                        "forget" => state.forget_limit_order_time_in_force(id),
                        "get" => {
                            let actual = state.get_placed_limit_order_time_in_force(id);
                            assert_eq!(
                                actual.is_some(),
                                c["present"].as_bool().unwrap(),
                                "tif presence before serialization {c}"
                            );
                            assert_eq!(
                                actual.map(|t| serde_json::to_value(t).unwrap()),
                                c.get("value").cloned(),
                                "tif return {c}"
                            );
                            field_comparisons += 2;
                        }
                        _ => panic!("unknown tif operation"),
                    }
                    let keys: Vec<String> = state
                        .placed_tif
                        .borrow()
                        .iter()
                        .map(|(id, _)| id.clone())
                        .collect();
                    assert_eq!(
                        serde_json::to_value(keys).unwrap(),
                        c["order"],
                        "tif insertion-order and eviction {c}"
                    );
                    field_comparisons += 1;
                }
                "cancel" => {
                    clock.0.set(c["time"].as_f64().unwrap());
                    let id = c["id"].as_str().unwrap();
                    match c["op"].as_str().unwrap() {
                        "suppress" => state.suppress_cancelled_order(id.into()),
                        "unsuppress" => state.unsuppress_cancelled_order(id),
                        "get" => {
                            assert_eq!(
                                state.is_order_cancelled(id),
                                c["value"].as_bool().unwrap(),
                                "cancel expiry {c}"
                            );
                            field_comparisons += 1;
                        }
                        _ => panic!("unknown cancel operation"),
                    }
                    let keys: Vec<String> = state.cancelled.borrow().keys().cloned().collect();
                    assert_eq!(
                        serde_json::to_value(keys).unwrap(),
                        c["keys"],
                        "expired suppression must be removed {c}"
                    );
                    field_comparisons += 1;
                }
                _ => panic!("unknown source state case"),
            }
            comparisons += 1;
        }
        assert_eq!(comparisons, 533, "STATE-CARDINALITY completed native cases");
        assert_eq!(
            field_comparisons, 563,
            "STATE-CARDINALITY native field comparisons"
        );
    }
    #[test]
    fn actual_source_state_oracle_and_controls_execute() {
        let script =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/exchange-state.cjs");
        for (flag, diagnostic) in [
            (None, None),
            (
                Some("--control-age"),
                Some("age exactly max remains suppressed"),
            ),
            (
                Some("--control-identity"),
                Some("signing without identity is public"),
            ),
        ] {
            let mut command = std::process::Command::new("node");
            command.arg(&script).arg("--typed");
            if let Some(flag) = flag {
                command.arg(flag);
            }
            let output = command.output().unwrap();
            if let Some(diagnostic) = diagnostic {
                assert!(!output.status.success());
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains(diagnostic),
                    "source control must fail by name: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                assert!(
                    output.status.success(),
                    "source oracle must execute {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    #[test]
    fn state_string_representation_refusal_is_separate_from_normal_unicode() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/exchange-state-strings.json"
        ))
        .unwrap();
        for c in corpus["cases"].as_array().unwrap() {
            assert_eq!(c["sourceAccepted"], true);
            let decoded = serde_json::from_str::<String>(c["raw"].as_str().unwrap());
            let units = c["units"].as_array().unwrap();
            let lone = units.len() == 1 && (0xd800..=0xdfff).contains(&units[0].as_u64().unwrap());
            if lone {
                assert!(
                    decoded.is_err(),
                    "shared UTF-8 representation refuses the source-accepted code unit"
                );
                continue;
            }
            let id = decoded.unwrap();
            let clock = Rc::new(Clock::default());
            let state = subject(json!({"vault":id}), clock);
            state.remember_limit_order_time_in_force(id.clone(), OrderTimeInForce::GoodTillCancel);
            state.suppress_cancelled_order(id.clone());
            assert_eq!(
                state.get_placed_limit_order_time_in_force(&id),
                Some(OrderTimeInForce::GoodTillCancel)
            );
            assert_eq!(
                state.is_order_cancelled(&id),
                c["cancelled"].as_bool().unwrap()
            );
            assert_eq!(
                state.has_private_session_identity(),
                c["identity"].as_bool().unwrap()
            );
            assert_eq!(
                Exchange::is_public_data_only(&state),
                c["public"].as_bool().unwrap()
            );
            state.forget_limit_order_time_in_force(&id);
            state.unsuppress_cancelled_order(&id);
            assert!(state.get_placed_limit_order_time_in_force(&id).is_none());
            assert!(!state.is_order_cancelled(&id));
        }
        let output = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/exchange-state-strings.cjs"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "state string source oracle must execute {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn host_clock_reentry_does_not_hold_a_state_borrow() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/exchange-state-reentry.json"
        ))
        .unwrap();
        let clock = Rc::new(Clock::default());
        clock.0.set(42.0);
        let state = Rc::new(subject(json!({}), clock.clone()));
        let weak = Rc::downgrade(&state);
        *clock.1.borrow_mut() = Some(Box::new(move || {
            weak.upgrade()
                .unwrap()
                .suppress_cancelled_order("nested".into())
        }));
        state.suppress_cancelled_order("outer".into());
        assert_eq!(
            state.is_order_cancelled("nested"),
            fixture["nested"].as_bool().unwrap()
        );
        assert_eq!(
            state.is_order_cancelled("outer"),
            fixture["outer"].as_bool().unwrap()
        );
        let output = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/exchange-state-reentry.cjs"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source clock reentry must execute {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn caller_options_snapshot_bound_is_distinct_from_internal_identity_patch() {
        use safe_cex_exchange::exchange::{AccountIdentityPatch, IdentityFieldPatch};
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/exchange-state-alias.json"))
                .unwrap();
        assert_eq!(fixture["sameReference"], true);
        let state = subject(json!({}), Rc::new(Clock::default()));
        let mut snapshot = state.base.options();
        assert_eq!(
            state.has_private_session_identity(),
            fixture["before"]["identity"].as_bool().unwrap()
        );
        snapshot.vault = Some("vault".into());
        assert!(
            !state.has_private_session_identity(),
            "owned snapshot changes do not reach the private-session getter"
        );
        assert_ne!(
            state.has_private_session_identity(),
            fixture["after"]["identity"].as_bool().unwrap(),
            "source retained caller reference changes the getter; Rust snapshot does not"
        );
        state.base.patch_account_identity(AccountIdentityPatch {
            vault: IdentityFieldPatch::Set(snapshot.vault.unwrap()),
            application_id: IdentityFieldPatch::Unchanged,
        });
        assert_eq!(
            state.has_private_session_identity(),
            fixture["after"]["identity"].as_bool().unwrap(),
            "internal account-owner patch must reach getter"
        );
        assert_eq!(
            Exchange::is_public_data_only(&state),
            fixture["after"]["public"].as_bool().unwrap()
        );
        let output = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/exchange-state-alias.cjs"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source ownership/getter witness must execute {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
