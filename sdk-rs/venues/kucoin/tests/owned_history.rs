//! Shared-owner contracts: original failures discard already validated pages.
mod support;
use safe_cex_engine::log::code;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Level, Notice, Query, Value, Venue,
};
use safe_cex_venue_kucoin::Kucoin;
use serde_json::{Value as Json, json};
use std::collections::HashSet;
use support::credentials::Options;

struct H<V: Venue> {
    engine: Engine<V>,
    actions: Vec<Action>,
    now: f64,
    commands: HashSet<u32>,
}
impl<V: Venue> H<V> {
    fn new(venue: V) -> Self {
        Self {
            engine: safe_cex_engine::test_support::seeded_engine(venue),
            actions: vec![],
            now: 2000.0,
            commands: HashSet::new(),
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.actions.extend(self.engine.drain_actions());
    }
    fn query(&mut self, command: u32, query: Query) {
        self.commands.insert(command);
        self.event(Event::Command(Command::Query { command, query }));
    }
    fn take(&mut self) -> HttpRequest {
        for _ in 0..20 {
            let settled: HashSet<_> = self
                .actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(
                        Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. },
                    ) => Some(*command),
                    _ => None,
                })
                .collect();
            assert!(
                !self.commands.is_subset(&settled),
                "dispatch after all commands completed"
            );
            if let Some(i) = self
                .actions
                .iter()
                .position(|a| matches!(a, Action::Http(_)))
            {
                let Action::Http(r) = self.actions.remove(i) else {
                    unreachable!()
                };
                return r;
            }
            let i = self
                .actions
                .iter()
                .enumerate()
                .filter_map(|(i, a)| match a {
                    Action::SetTimer { at_ms, .. } => Some((i, *at_ms)),
                    _ => None,
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("continuation timer")
                .0;
            let Action::SetTimer { timer, at_ms } = self.actions.remove(i) else {
                unreachable!()
            };
            self.now = at_ms;
            self.event(Event::TimerFired { timer });
        }
        panic!("no HTTP request: {:?}", self.actions)
    }
    fn reply(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.event(Event::HttpStart {
            request: request.id,
            status,
            headers: "",
        });
        for data in body.as_bytes().chunks(11) {
            self.event(Event::HttpChunk {
                request: request.id,
                data,
            });
        }
        self.event(Event::HttpEnd {
            request: request.id,
        });
    }
    fn done(&self, id: u32) -> Json {
        assert!(!self.actions.iter().any(
            |a| matches!(a, Action::Notice(Notice::CommandFailed {command,..}) if *command == id)
        ));
        let results: Vec<_> = self
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                    Some(result)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            results.len(),
            1,
            "completion: {:?}",
            self.actions
                .iter()
                .filter_map(|a| if let Action::Notice(n) = a {
                    Some(n)
                } else {
                    None
                })
                .collect::<Vec<_>>()
        );
        serde_json::from_str(results[0]).unwrap()
    }
    fn rejected(&self, id: u32) {
        let errors: Vec<_> = self
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                    Some(error)
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            errors.len(),
            1,
            "failure: {:?}",
            self.actions
                .iter()
                .filter_map(|a| if let Action::Notice(n) = a {
                    Some(n)
                } else {
                    None
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
        assert!(!self.actions.iter().any(
            |a| matches!(a, Action::Notice(Notice::CommandDone { command, .. }) if *command == id)
        ));
        self.markers(id, 0);
    }
    fn markers(&self, id: u32, expected: usize) {
        let markers: Vec<_> = self
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(e)) if e.code == code::HISTORY_INCOMPLETE => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(markers.len(), expected, "markers: {:?}", self.actions);
        for marker in markers {
            let mark_at = self
                .actions
                .iter()
                .position(
                    |a| matches!(a, Action::Notice(Notice::Event(e)) if std::ptr::eq(e,marker)),
                )
                .unwrap();
            let done_at = self.actions.iter().position(|a| matches!(a, Action::Notice(Notice::CommandDone {command,..}) if *command == id)).expect("marker needs successful completion");
            assert!(mark_at < done_at, "marker must precede completion");
            assert_eq!(marker.level, Level::Warn);
            assert_eq!(marker.fields, vec![("command", Value::Num(f64::from(id)))]);
        }
    }
}
fn options() -> Options {
    Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        passphrase: Some("fixture-passphrase".into()),
        ..Default::default()
    }
}
fn futures() -> H<Kucoin> {
    H::new(Kucoin::new(options()))
}
fn income(funding: bool, symbol: Option<String>) -> Query {
    Query::Income {
        symbol,
        kind: funding.then(|| "FUNDING_FEE".into()),
        from_ms: Some(1000.0),
        to_ms: Some(2000.0),
        limit: Some(100),
    }
}
fn ledger(id: u64, time: u64) -> Json {
    json!({"time":time,"type":"RealisedPNL","amount":"2","offset":id,"currency":"USDT","status":"Completed","remark":""})
}
fn ledger_page(items: Vec<Json>, more: bool) -> String {
    json!({"code":"200000","data":{"dataList":items,"hasMore":more}}).to_string()
}
fn funding_row(id: u64, time: u64) -> Json {
    json!({"id":id,"symbol":"XBTUSDTM","timePoint":time,"funding":"-0.1","settleCurrency":"USDT"})
}
fn funding() -> H<Kucoin> {
    let mut h = futures();
    let market = safe_cex_engine::model::Market {
        id: "XBTUSDTM".into(),
        symbol: "BTCUSDT".into(),
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: safe_cex_engine::model::Precision {
            amount: 1.0,
            price: 0.1,
        },
        limits: safe_cex_engine::model::Limits {
            amount: safe_cex_engine::model::MinMax { min: 1.0, max: 1e6 },
            leverage: safe_cex_engine::model::MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        pretty_symbol: None,
        contract_size: Some(0.001),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        maker_fee: None,
        taker_fee: None,
    };
    h.engine.store_mut().set_markets(vec![market]);
    h
}
#[test]
fn owned_history_first_page_failures_are_typed_and_never_empty_success() {
    for body in [
        r#"{"code":"400100","msg":"refused","data":{"dataList":[]}}"#.to_owned(),
        ledger_page(vec![ledger(9, 1500), json!({"invalid":true})], false),
    ] {
        let mut h = futures();
        h.query(7, income(false, None));
        let r = h.take();
        h.reply(&r, 200, &body);
        h.rejected(7);
    }
}
#[test]
fn owned_history_later_failure_returns_only_admitted_ledger_and_owned_marker() {
    for body in [
        r#"{"code":"400100","msg":"refused","data":{"dataList":[]}}"#.to_owned(),
        ledger_page(vec![ledger(8, 1400), json!({"invalid":true})], false),
    ] {
        let mut h = futures();
        h.query(7, income(false, None));
        let r = h.take();
        h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
        let r = h.take();
        assert!(r.url.contains("offset=9"));
        h.reply(&r, 200, &body);
        assert_eq!(h.done(7).as_array().unwrap().len(), 1);
        assert_eq!(h.done(7)[0]["tranId"], "9");
        h.markers(7, 1);
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
    }
}
#[test]
fn owned_history_retry_discards_only_unvalidated_page_and_keeps_identity_eligible() {
    let mut h = futures();
    h.query(7, income(false, None));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
    let r = h.take();
    h.reply(&r, 503, &ledger_page(vec![ledger(8, 1400)], false));
    let retry = h.take();
    assert_eq!(r.url, retry.url);
    h.reply(
        &retry,
        200,
        &ledger_page(vec![ledger(9, 1500), ledger(8, 1400)], false),
    );
    let out = h.done(7);
    assert_eq!(out.as_array().unwrap().len(), 2);
    assert_eq!(out[0]["tranId"], "9");
    assert_eq!(out[1]["tranId"], "8");
    h.markers(7, 0);
}
#[test]
fn owned_history_filtered_rows_do_not_authorize_partial_success() {
    let mut h = funding();
    h.query(7, income(false, Some("BTCUSDT".into())));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
    let r = h.take();
    h.reply(
        &r,
        400,
        r#"{"code":"400100","msg":"refused","data":{"dataList":[]}}"#,
    );
    h.rejected(7);
}
#[test]
fn owned_history_ledger_cursor_proof_stays_strict_after_acceptance() {
    let mut h = futures();
    h.query(7, income(false, None));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
    h.rejected(7);
}
#[test]
fn owned_history_funding_first_and_later_failures_obey_the_same_owner_policy() {
    for accepted in [true, false] {
        let mut h = funding();
        h.query(8, income(true, Some("BTCUSDT".into())));
        let mut r = h.take();
        if accepted {
            h.reply(&r, 200, &ledger_page(vec![funding_row(9, 1500)], true));
            r = h.take();
        }
        h.reply(
            &r,
            200,
            &ledger_page(vec![funding_row(8, 1400), json!({"invalid":true})], false),
        );
        if accepted {
            assert_eq!(h.done(8).as_array().unwrap().len(), 1);
            assert_eq!(h.done(8)[0]["tranId"], "9");
            h.markers(8, 1);
        } else {
            h.rejected(8);
        }
    }
}
#[test]
fn owned_history_funding_cursor_proof_stays_strict_after_acceptance() {
    let mut h = funding();
    h.query(8, income(true, Some("BTCUSDT".into())));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![funding_row(9, 1500)], true));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![funding_row(8, 1500)], true));
    h.rejected(8);
}
#[test]
fn owned_history_successful_empty_is_complete_for_both_futures_routes() {
    for fund in [false, true] {
        let mut h = funding();
        h.query(9, income(fund, fund.then(|| "BTCUSDT".into())));
        let r = h.take();
        h.reply(&r, 200, &ledger_page(vec![], false));
        assert_eq!(h.done(9), json!([]));
        h.markers(9, 0);
    }
}
#[cfg(feature = "spot")]
#[test]
fn owned_history_spot_fills_initial_failures_are_typed_without_partial_output() {
    for body in [
        r#"{"code":"400100","msg":"refused","data":{"dataList":[]}}"#,
        r#"{"code":"200000","data":{"items":[{"symbol":"BTC-USDT","orderId":"o","side":"buy","price":"100","size":"1","createdAt":1500},{}]}}"#,
    ] {
        let mut h = H::new(safe_cex_venue_kucoin::spot::KucoinSpot::new(options()));
        h.query(
            10,
            Query::Fills {
                symbol: Some("BTC-USDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(5),
            },
        );
        let r = h.take();
        h.reply(&r, 200, body);
        h.rejected(10);
    }
}

#[test]
fn owned_history_numbered_fills_and_orders_recover_prior_pages_without_failed_prefix() {
    for orders in [false, true] {
        let mut h = funding();
        let q = if orders {
            Query::OrderHistory {
                symbol: Some("BTCUSDT".into()),
                from_ms: Some(1000.0),
                to_ms: Some(2000.0),
                limit: Some(100),
            }
        } else {
            Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: Some(1000.0),
                to_ms: Some(2000.0),
                limit: Some(100),
            }
        };
        h.query(11, q);
        let r = h.take();
        let row = if orders {
            json!({"id":"kept","symbol":"XBTUSDTM","side":"buy","type":"limit","price":"100","size":"1","dealSize":"1","createdAt":1500})
        } else {
            json!({"tradeId":"kept","orderId":"o","symbol":"XBTUSDTM","side":"buy","liquidity":"maker","price":"100","size":"1","fee":"0","feeCurrency":"USDT","createdAt":1500})
        };
        let body=json!({"code":"200000","data":{"currentPage":1,"totalPage":2,"totalNum":2,"items":[row]}}).to_string();
        h.reply(&r, 200, &body);
        let r = h.take();
        assert!(r.url.contains("currentPage=2"));
        let prefix = row
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    if k == "id" || k == "tradeId" {
                        json!("discarded")
                    } else {
                        v.clone()
                    },
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let body=json!({"code":"200000","data":{"currentPage":2,"totalPage":2,"totalNum":3,"items":[prefix,json!({"invalid":true})]}}).to_string();
        h.reply(&r, 200, &body);
        let out = h.done(11);
        assert_eq!(out.as_array().unwrap().len(), 1);
        assert_eq!(out[0]["id"], "kept");
        h.markers(11, 1);
    }
}
#[test]
fn owned_history_numbered_census_refusal_is_strict_after_accepted_rows() {
    let mut h = funding();
    h.query(
        11,
        Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms: Some(1000.0),
            to_ms: Some(2000.0),
            limit: Some(100),
        },
    );
    let r = h.take();
    let row = json!({"id":"kept","symbol":"XBTUSDTM","side":"buy","type":"limit","price":"100","size":"1","dealSize":"1","createdAt":1500});
    h.reply(
        &r,
        200,
        &json!({"code":"200000","data":{"currentPage":1,"totalPage":2,"totalNum":2,"items":[row]}})
            .to_string(),
    );
    let r = h.take();
    h.reply(
        &r,
        200,
        r#"{"code":"200000","data":{"currentPage":1,"totalPage":2,"totalNum":2,"items":[]}}"#,
    );
    h.rejected(11);
}
#[test]
fn owned_history_concurrent_funding_cannot_inherit_ledger_partial_marker() {
    let mut h = funding();
    h.query(7, income(false, None));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
    h.query(8, income(true, Some("BTCUSDT".into())));
    let r = h.take();
    assert!(r.url.contains("transaction-history"));
    h.reply(
        &r,
        400,
        r#"{"code":"400100","msg":"refused","data":{"dataList":[]}}"#,
    );
    assert_eq!(h.done(7)[0]["tranId"], "9");
    h.markers(7, 1);
    let r = h.take();
    assert!(r.url.contains("funding-history"));
    h.reply(&r, 200, &ledger_page(vec![funding_row(3, 1400)], false));
    assert_eq!(h.done(8)[0]["tranId"], "3");
    h.markers(7, 1);
}

#[test]
fn owned_history_metadata_decode_failure_recovers_only_previous_ledger_or_funding_pages() {
    for fund in [false, true] {
        let mut h = funding();
        h.query(8, income(fund, fund.then(|| "BTCUSDT".into())));
        let r = h.take();
        let row = if fund {
            funding_row(9, 1500)
        } else {
            ledger(9, 1500)
        };
        h.reply(&r, 200, &ledger_page(vec![row], true));
        let r = h.take();
        let row = if fund {
            funding_row(8, 1400)
        } else {
            ledger(8, 1400)
        };
        h.reply(
            &r,
            200,
            &json!({"code":"200000","data":{"dataList":[row],"hasMore":"invalid"}}).to_string(),
        );
        assert_eq!(h.done(8).as_array().unwrap().len(), 1);
        assert_eq!(h.done(8)[0]["tranId"], "9");
        h.markers(8, 1);
    }
}
#[test]
fn owned_history_retry_exhaustion_recovers_the_prior_page_without_dispatching_again() {
    let mut h = futures();
    h.query(7, income(false, None));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![ledger(9, 1500)], true));
    for attempt in 0..=3 {
        let r = h.take();
        let before = h.now;
        h.reply(&r, 503, &ledger_page(vec![ledger(8, 1400)], false));
        if attempt < 3 {
            assert!(!h.actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
            )));
            let next = h
                .actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { at_ms, .. } => Some(*at_ms),
                    _ => None,
                })
                .unwrap();
            assert_eq!(next - before, 1000.0 * f64::from(attempt + 1));
        }
    }
    assert_eq!(h.done(7)[0]["tranId"], "9");
    h.markers(7, 1);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn owned_history_funding_failed_page_retry_keeps_raw_cursor_and_reused_identity() {
    let mut h = funding();
    h.query(8, income(true, Some("BTCUSDT".into())));
    let r = h.take();
    h.reply(&r, 200, &ledger_page(vec![funding_row(9, 1500)], true));
    let r = h.take();
    h.reply(&r, 503, &ledger_page(vec![funding_row(8, 1400)], false));
    let retry = h.take();
    assert_eq!(r.url, retry.url);
    h.reply(
        &retry,
        200,
        &ledger_page(vec![funding_row(9, 1500), funding_row(8, 1400)], false),
    );
    assert_eq!(h.done(8).as_array().unwrap().len(), 2);
    h.markers(8, 0);
}
#[test]
fn owned_history_public_trades_failure_is_typed_without_private_credentials() {
    let mut h = funding();
    h.engine = support::credentials::engine(Options::default(), Kucoin::new);
    let market = funding().engine.store().markets().to_vec();
    h.engine.store_mut().set_markets(market);
    h.query(
        12,
        Query::PublicTrades {
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        },
    );
    let r = h.take();
    assert!(!r.headers.iter().any(|(name, _)| *name == "KC-API-KEY"));
    h.reply(&r, 400, r#"{"code":"400100","msg":"refused"}"#);
    h.rejected(12);
}
#[cfg(feature = "spot")]
#[test]
fn owned_history_spot_transport_failure_is_typed_with_the_existing_diagnostic() {
    let mut h = H::new(safe_cex_venue_kucoin::spot::KucoinSpot::new(options()));
    h.query(
        10,
        Query::Fills {
            symbol: Some("BTC-USDT".into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(5),
        },
    );
    let r = h.take();
    h.event(Event::HttpFailed {
        request: r.id,
        error: "fixture transport refusal",
    });
    h.rejected(10);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandError { command: 10, .. })))
    );
}
