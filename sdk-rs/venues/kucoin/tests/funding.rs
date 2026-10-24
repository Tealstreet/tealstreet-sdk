//! Oracles: Classic Futures Get Private Funding History, including its
//! recommendation to page by time rather than unstable offsets.
mod support;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query};
use safe_cex_venue_kucoin::Kucoin;
use serde::Deserialize;
use support::credentials::Options;

struct Harness {
    engine: Engine<Kucoin>,
    actions: Vec<Action>,
    now: f64,
}
impl Harness {
    fn new() -> Self {
        let mut engine = support::credentials::engine(
            Options {
                key: Some("fixture-key".into()),
                secret: Some("fixture-secret".into()),
                passphrase: Some("fixture-passphrase".into()),
                ..Default::default()
            },
            Kucoin::new,
        );
        engine.store_mut().set_markets(vec![Market {
            id: "XBTUSDTM".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 1.0,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 1.0,
                    max: 1000000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 100.0,
                },
                price: None,
            },
            contract_size: Some(0.001),
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        Self {
            engine,
            actions: vec![],
            now: 2_000_000_000_000.0,
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.actions
            .extend(safe_cex_engine::test_support::prepared_actions(
                &mut self.engine,
                self.now,
            ));
    }
    fn query(&mut self, from: f64, to: f64, limit: Option<u32>) {
        self.event(Event::Command(Command::Query {
            command: 1,
            query: Query::Income {
                symbol: Some("BTCUSDT".into()),
                kind: Some("FUNDING_FEE".into()),
                from_ms: Some(from),
                to_ms: Some(to),
                limit,
            },
        }));
    }
    fn take(&mut self) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a, Action::Http(_)))
            .unwrap_or_else(|| panic!("no request: {:?}", self.actions));
        let Action::Http(r) = self.actions.remove(i) else {
            unreachable!()
        };
        r
    }
    fn advance(&mut self) {
        let i = self
            .actions
            .iter()
            .enumerate()
            .filter_map(|(i, a)| {
                if let Action::SetTimer { at_ms, .. } = a {
                    Some((i, *at_ms))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap()
            .0;
        let Action::SetTimer { timer, at_ms } = self.actions.remove(i) else {
            unreachable!()
        };
        self.now = at_ms;
        self.event(Event::TimerFired { timer });
    }
    fn reply(&mut self, r: &HttpRequest, items: &str, more: bool) {
        let body =
            format!(r#"{{"code":"200000","data":{{"dataList":[{items}],"hasMore":{more}}}}}"#);
        self.event(Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        });
        for data in body.as_bytes().chunks(11) {
            self.event(Event::HttpChunk {
                request: r.id,
                data,
            });
        }
        self.event(Event::HttpEnd { request: r.id });
    }
    fn result(&self) -> Result<String, String> {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: 1, result }) => {
                    Some(Ok(result.clone()))
                }
                Action::Notice(Notice::CommandFailed { command: 1, error }) => {
                    Some(Err(error.legacy_text()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no completion: {:?}", self.actions))
    }
}
fn row(id: u64, time: u64) -> String {
    format!(
        r#"{{"id":{id},"symbol":"XBTUSDTM","timePoint":{time},"funding":-0.05585669,"settleCurrency":"USDT"}}"#
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: u64,
    tran_id: String,
    category: String,
}

#[test]
fn documented_funding_payload_preserves_sign_currency_and_exact_id() {
    let mut h = Harness::new();
    h.query(1731470300000.0, 1731470500000.0, Some(10));
    let r = h.take();
    assert!(r.url.contains("/api/v1/funding-history?symbol=XBTUSDTM"));
    assert!(r.url.contains("reverse=true&forward=false&maxCount=50"));
    assert!(!r.url.contains("offset="));
    h.reply(&r, &row(9007199254740993, 1731470400000), false);
    let rows: Vec<Income> = serde_json::from_str(h.result().as_ref().unwrap()).unwrap();
    let r = &rows[0];
    assert_eq!(r.symbol, "BTCUSDT");
    assert_eq!(r.income_type, "FUNDING_FEE");
    assert_eq!(r.income, -0.05585669);
    assert_eq!(r.asset, "USDT");
    assert_eq!(r.timestamp, 1731470400000);
    assert_eq!(r.tran_id, "9007199254740993");
    assert_eq!(r.category, "funding");
}
#[test]
fn inclusive_overlap_collects_additional_settlements_at_the_boundary() {
    let mut h = Harness::new();
    h.query(1000.0, 4000.0, Some(1));
    let r = h.take();
    h.reply(&r, &format!("{},{}", row(1, 3000), row(2, 2000)), true);
    h.advance();
    let r = h.take();
    assert!(r.url.contains("endAt=2000"));
    h.reply(
        &r,
        &format!("{},{},{}", row(2, 2000), row(3, 2000), row(4, 1000)),
        false,
    );
    let rows: Vec<Income> = serde_json::from_str(h.result().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows.iter().filter(|r| r.timestamp == 2000).count(), 2);
}
#[test]
fn saturated_same_timestamp_cannot_silently_skip_unseen_settlements() {
    let mut h = Harness::new();
    h.query(1000.0, 3000.0, None);
    let r = h.take();
    h.reply(&r, &row(1, 2000), true);
    h.advance();
    let r = h.take();
    h.reply(&r, &row(2, 2000), true);
    assert!(h.result().as_ref().unwrap_err().contains("same-timestamp"));
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn has_more_without_any_cursor_is_an_error() {
    let mut h = Harness::new();
    h.query(1000.0, 3000.0, None);
    let r = h.take();
    h.reply(&r, "", true);
    assert!(h.result().is_err());
}
#[test]
fn explicit_ranges_walk_small_windows_and_ignore_output_limits() {
    const DAY: u64 = 86_400_000;
    let mut h = Harness::new();
    h.query(0.0, (61 * DAY) as f64, Some(1));
    let r = h.take();
    assert!(r.url.contains(&format!("startAt={}", DAY + 1)));
    h.reply(&r, &row(1, DAY + 2), false);
    h.advance();
    let r = h.take();
    assert!(r.url.contains(&format!("startAt=0&endAt={DAY}")));
    h.reply(&r, &row(2, 1000), false);
    let rows: Vec<Income> = serde_json::from_str(h.result().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
}
#[test]
fn non_explicit_query_obeys_its_limit() {
    let mut h = Harness::new();
    h.event(Event::Command(Command::Query {
        command: 1,
        query: Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    }));
    let r = h.take();
    let now = h.now as u64;
    h.reply(
        &r,
        &format!("{},{}", row(1, now - 1000), row(2, now - 2000)),
        false,
    );
    let rows: Vec<Income> = serde_json::from_str(h.result().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tran_id, "1");
}
#[test]
fn default_funding_range_accepts_fractional_native_clock() {
    let mut h = Harness::new();
    h.now = 1_740_640_088_002.5;
    h.event(Event::Command(Command::Query {
        command: 1,
        query: Query::Income {
            symbol: Some("BTCUSDT".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }));
    let r = h.take();
    assert!(r.url.contains("endAt=1740640088002"));
    h.reply(&r, &row(1, 1_740_640_088_000), false);
    assert!(h.result().is_ok());
}
#[test]
fn explicit_fractional_funding_ranges_remain_invalid() {
    for (from, to) in [(1000.5, 2000.0), (1000.0, 2000.5)] {
        let mut h = Harness::new();
        h.query(from, to, None);
        assert!(h.result().is_err());
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}
#[test]
fn failed_partial_page_is_rolled_back_before_a_read_retry() {
    let mut h = Harness::new();
    h.query(1000.0, 3000.0, None);
    let r = h.take();
    h.event(Event::HttpStart {
        request: r.id,
        status: 200,
        headers: "",
    });
    let chunk = format!(
        r#"{{"code":"200000","data":{{"dataList":[{},"#,
        row(1, 2000)
    );
    h.event(Event::HttpChunk {
        request: r.id,
        data: chunk.as_bytes(),
    });
    h.event(Event::HttpFailed {
        request: r.id,
        error: "connection reset",
    });
    h.advance();
    let retry = h.take();
    assert_eq!(retry.url, r.url);
    h.reply(&retry, &row(2, 1500), false);
    let rows: Vec<Income> = serde_json::from_str(h.result().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tran_id, "2");
}
#[test]
fn funding_requires_a_symbol_before_any_network_call() {
    let mut h = Harness::new();
    h.event(Event::Command(Command::Query {
        command: 1,
        query: Query::Income {
            symbol: None,
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    }));
    assert!(
        h.result()
            .as_ref()
            .unwrap_err()
            .contains("requires a symbol")
    );
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn malformed_funding_amount_or_scope_never_becomes_zero_income() {
    for bad in [
        row(1, 2000).replace("-0.05585669", "null"),
        row(1, 2000).replace("XBTUSDTM", "ETHUSDTM"),
        row(1, 2000).replace("\"USDT\"", "\"\""),
    ] {
        let mut h = Harness::new();
        h.query(1000.0, 3000.0, None);
        let r = h.take();
        h.reply(&r, &bad, false);
        assert!(h.result().is_err());
    }
}
#[test]
fn funding_and_other_queries_share_one_paced_read_lane() {
    let mut h = Harness::new();
    h.query(1000.0, 3000.0, None);
    let funding = h.take();
    h.event(Event::Command(Command::Query {
        command: 2,
        query: Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        },
    }));
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    h.advance();
    let public = h.take();
    assert!(public.url.contains("/api/v1/trade/history"));
    assert_eq!(h.now, 2_000_000_000_350.0);
    h.reply(&funding, "", false);
}
#[test]
fn zero_width_requested_range_is_filtered_after_native_window_expansion() {
    let mut h = Harness::new();
    h.query(1000.0, 1000.0, None);
    let r = h.take();
    assert!(r.url.contains("startAt=999&endAt=1000"));
    h.reply(&r, &format!("{},{}", row(1, 1000), row(2, 999)), false);
    let rows: Vec<Income> = serde_json::from_str(h.result().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tran_id, "1");
}
