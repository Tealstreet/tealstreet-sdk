mod support;
use safe_cex_engine::model::{Limits, Market, MinMax, Order, Precision};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Query, Response};
use safe_cex_venue_bitrue::Bitrue;
use support::Options;
const SYMBOL: &str = "E-BTC-USDT";
const ROW: &str = r#"{"tradeId":1558124009467904992,"price":60000,"qty":3,"amount":180,"contractName":"E-BTC-USDT","side":"BUY","fee":"-0.1","bidId":1558124009467904993,"askId":1558124009467904994,"isBuyer":true,"isMaker":true,"ctime":1700000000123}"#;
fn engine() -> Engine<Bitrue> {
    let mut e = support::engine(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    e.store_mut().set_markets(vec![Market {
        id: SYMBOL.into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
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
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some(SYMBOL.into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: None,
    }
}
fn query(e: &mut Engine<Bitrue>, query: Query) {
    e.handle(
        1700000000000.0,
        Event::Command(Command::Query { command: 99, query }),
    );
}
fn request(e: &mut Engine<Bitrue>) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/myTrades") => Some(r),
            _ => None,
        })
        .unwrap()
}
fn result(e: &mut Engine<Bitrue>) -> Result<String, String> {
    safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 99,
                result,
            }) => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { command: 99, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .unwrap()
}
fn response(e: &mut Engine<Bitrue>, r: HttpRequest, body: &str) {
    e.handle(
        1700000000000.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
#[test]
fn actual_executions_keep_exact_ids_base_units_and_seconds_without_state_changes() {
    let mut e = engine();
    query(&mut e, fills());
    let r = request(&mut e);
    assert!(
        r.url
            .ends_with("/fapi/v2/myTrades?contractName=E-BTC-USDT&limit=200")
    );
    assert!(r.headers.iter().any(|(k, _)| *k == "X-CH-SIGN"));
    let second = ROW
        .replace("1558124009467904992", "1558124009467904995")
        .replace("1700000000123", "1700000000789");
    response(
        &mut e,
        r,
        &format!(r#"{{"code":"0","data":[{ROW},{second},{ROW}]}}"#),
    );
    let out = result(&mut e).unwrap();
    assert_eq!(out.matches("commissionAsset").count(), 2);
    assert!(out.contains(r#""id":"1558124009467904995""#));
    assert!(out.contains(r#""orderId":"1558124009467904993""#));
    assert!(out.contains(r#""timestamp":1700000000.0"#));
    assert!(out.contains(r#""qty":0.003"#));
    assert!(out.contains(r#""positionSide":"both""#));
    assert!(out.contains(r#""commission":0.1"#));
    assert!(out.contains(r#""maker":true"#));
    assert!(!out.contains("realizedPnl"));
    assert!(out.find("1558124009467904995").unwrap() < out.find("1558124009467904992").unwrap());
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}
#[test]
fn filters_native_side_order_identity_symbol_and_exact_millisecond_boundaries() {
    let mut e = engine();
    query(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: Some("1558124009467904994".into()),
            from_ms: Some(1700000000123.0),
            to_ms: Some(1700000000123.0),
            limit: Some(5),
        },
    );
    let r = request(&mut e);
    assert!(
        r.url
            .contains("startTime=1700000000123&endTime=1700000000123")
    );
    assert!(!r.url.contains("orderId="));
    let sell = ROW.replace("\"BUY\"", "\"SELL\"");
    let older = sell
        .replace("1558124009467904992", "1558124009467904995")
        .replace("1700000000123", "1700000000122");
    let other = sell
        .replace("E-BTC-USDT", "E-ETH-USDT")
        .replace("1558124009467904992", "1558124009467904996");
    response(
        &mut e,
        r,
        &format!(r#"{{"code":0,"data":[{ROW},{sell},{older},{other}]}}"#),
    );
    let out = result(&mut e).unwrap();
    assert_eq!(out.matches("commissionAsset").count(), 1);
    assert!(out.contains(r#""orderId":"1558124009467904994""#));
    assert!(out.contains(r#""side":"sell""#));
}
#[test]
fn preserves_raw_base_qty_and_quote_inferred_qty_as_ts_does() {
    let mut e = engine();
    query(&mut e, fills());
    let r = request(&mut e);
    let base = ROW.replace("\"qty\":3", "\"qty\":0.003");
    let inferred = ROW
        .replace("\"qty\":3", "\"qty\":0")
        .replace("1558124009467904992", "1558124009467904995");
    response(
        &mut e,
        r,
        &format!(r#"{{"code":"0","data":[{base},{inferred}]}}"#),
    );
    assert_eq!(result(&mut e).unwrap().matches(r#""qty":0.003"#).count(), 2);
}
#[test]
fn malformed_rows_failed_envelopes_and_transport_failure_reject_partial_results() {
    for body in [
        format!(r#"{{"code":"510","msg":"rate limited","data":[{ROW}]}}"#),
        format!(r#"{{"code":"0","data":[{ROW},{{"tradeId":"bad"}}]}}"#),
        format!(
            r#"{{"code":"0","data":[{}]}}"#,
            ROW.replace("\"BUY\"", "\"other\"")
        ),
        r#"{"code":"0","data":[1,]}"#.into(),
        r#"{"code":"0"}"#.into(),
    ] {
        let mut e = engine();
        query(&mut e, fills());
        let r = request(&mut e);
        response(&mut e, r, &body);
        assert!(result(&mut e).is_err(), "{body}");
    }
    let mut e = engine();
    query(&mut e, fills());
    for _ in 0..4 {
        let r = request(&mut e);
        e.handle(
            1700000000000.0,
            Event::HttpFailed {
                request: r.id,
                error: "network",
            },
        );
    }
    assert!(result(&mut e).is_err());
}
#[test]
fn windows_require_known_symbols_and_unsupported_histories_do_not_fabricate_ledgers() {
    for q in [
        Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Income {
            symbol: Some(SYMBOL.into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(2.0),
            to_ms: Some(1.0),
            limit: None,
        },
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: None,
            from_ms: Some(f64::NAN),
            to_ms: None,
            limit: None,
        },
    ] {
        let mut e = engine();
        query(&mut e, q);
        assert!(result(&mut e).is_err());
    }
}
#[test]
fn public_only_fills_fail_before_private_requests() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    query(&mut e, fills());
    assert!(result(&mut e).is_err());
}

#[test]
fn explicit_signature_rejection_resigns_once_and_discards_untrusted_staged_rows() {
    let mut e = engine();
    query(&mut e, fills());
    let first = request(&mut e);
    response(
        &mut e,
        first,
        &format!(r#"{{"data":[{ROW}],"code":"-1022","msg":"bad signature"}}"#),
    );
    let next = request(&mut e);
    response(&mut e, next, r#"{"code":"0","data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
}

#[test]
fn interrupted_stream_read_retries_without_carrying_partial_executions() {
    let mut e = engine();
    query(&mut e, fills());
    let first = request(&mut e);
    e.handle(
        1700000000000.0,
        Event::HttpStart {
            request: first.id,
            status: 200,
            headers: "",
        },
    );
    let prefix = format!(r#"{{"code":"0","data":[{ROW},"#);
    e.handle(
        1700000000000.0,
        Event::HttpChunk {
            request: first.id,
            data: prefix.as_bytes(),
        },
    );
    e.handle(
        1700000000000.0,
        Event::HttpFailed {
            request: first.id,
            error: "connection reset",
        },
    );
    let next = request(&mut e);
    response(&mut e, next, r#"{"code":"0","data":[]}"#);
    assert_eq!(result(&mut e).unwrap(), "[]");
}

#[test]
fn missing_own_order_id_never_uses_counterparty_or_execution_identity() {
    for row in [
        ROW.replace("\"bidId\":1558124009467904993,", ""),
        ROW.replace("1558124009467904993", "0"),
        ROW.replace("1558124009467904993", "-1"),
        ROW.replace("1558124009467904993", "{}"),
        ROW.replace("1558124009467904993", "true"),
        ROW.replace("1558124009467904992", "0"),
        ROW.replace("1558124009467904992", "1.5"),
    ] {
        let mut e = engine();
        query(&mut e, fills());
        let r = request(&mut e);
        response(&mut e, r, &format!(r#"{{"code":"0","data":[{row}]}}"#));
        assert!(result(&mut e).is_err(), "{row}");
    }
}
#[test]
fn a_full_filtered_native_page_reports_incomplete_instead_of_false_absence() {
    let mut e = engine();
    query(
        &mut e,
        Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: Some("999".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    );
    let r = request(&mut e);
    response(&mut e, r, &format!(r#"{{"code":"0","data":[{ROW}]}}"#));
    assert!(result(&mut e).unwrap_err().starts_with("INCOMPLETE:"));
}

// TS oracle: the inherited client-ID helper checks the symbol-scoped
// store, then Bitrue.fetchOrdersForSymbol's documented v2 openOrders schema.
// Closed orders are not queried, so absence must never be definitive.
#[derive(serde::Deserialize)]
struct Lookup {
    found: bool,
    order: Option<LookupOrder>,
    definitive: Option<bool>,
}
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupOrder {
    id: String,
    symbol: String,
    client_order_id: Option<String>,
    filled: f64,
    remaining: f64,
    time_in_force: Option<String>,
    extra: Option<std::collections::BTreeMap<String, f64>>,
}
const LOOKUP_ROW: &str = r#"{"orderId":2334151155877155281,"clientOrderId":"caller-id","price":59000,"origQty":0.004,"executedQty":0.001,"status":"PART_FILLED","type":"LIMIT","side":"BUY","action":"OPEN","transactTime":1700000000123}"#;
fn lookup(e: &mut Engine<Bitrue>, symbol: &str, client_order_id: &str) {
    query(
        e,
        Query::OrderByClientId {
            symbol: symbol.into(),
            client_order_id: client_order_id.into(),
        },
    );
}
fn lookup_request(e: &mut Engine<Bitrue>) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn lookup_result(e: &mut Engine<Bitrue>) -> Lookup {
    serde_json::from_str(&result(e).unwrap()).unwrap()
}
fn fetched_order() -> Order {
    Order {
        id: "2334151155877155281".into(),
        parent_id: None,
        status: safe_cex_engine::model::OrderStatus::Open,
        symbol: SYMBOL.into(),
        kind: safe_cex_engine::model::OrderType::Limit,
        side: safe_cex_engine::model::OrderSide::Buy,
        price: 59000.0,
        amount: 0.004,
        filled: 0.001,
        remaining: 0.003,
        reduce_only: false,
        close: None,
        client_order_id: Some("caller-id".into()),
        time_in_force: None,
        last_update: 1700000000123.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: Some(safe_cex_engine::model::OrderExtra {
            remaining_native: Some(0.003),
            filled_in_placement_units: Some(0.001),
            ..Default::default()
        }),
    }
}
#[test]
fn client_lookup_reads_native_open_orders_exactly_without_mutating_account_state() {
    let mut e = engine();
    lookup(&mut e, SYMBOL, "caller-id");
    let r = lookup_request(&mut e);
    assert_eq!(r.method, safe_cex_engine::Method::Get);
    assert_eq!(
        r.url,
        "https://fapi.bitrue.com/fapi/v2/openOrders?contractName=E-BTC-USDT"
    );
    assert!(
        r.headers
            .iter()
            .any(|(key, value)| *key == "X-CH-APIKEY" && value == "key")
    );
    response(
        &mut e,
        r,
        &format!(r#"{{"code":"0","data":[{LOOKUP_ROW}]}}"#),
    );
    let found = lookup_result(&mut e);
    assert!(found.found);
    assert_eq!(found.definitive, None);
    let order = found.order.unwrap();
    assert_eq!(order.id, "2334151155877155281");
    assert_eq!(order.client_order_id.as_deref(), Some("caller-id"));
    assert_eq!(order.symbol, SYMBOL);
    assert_eq!(order.filled, 0.001);
    assert_eq!(order.remaining, 0.003);
    assert_eq!(order.time_in_force.as_deref(), Some("GoodTillCancel"));
    assert!(order.extra.is_none());
    assert!(e.store().orders().is_empty());
    assert!(!e.store().loaded().orders);
}
#[test]
fn client_lookup_cache_hit_does_not_send_a_request_and_is_symbol_scoped() {
    let mut e = engine();
    let order = fetched_order();
    e.store_mut().upsert_order(order.clone());
    lookup(&mut e, SYMBOL, "caller-id");
    let found = lookup_result(&mut e);
    assert!(found.found);
    assert_eq!(found.order.unwrap().id, order.id);
    lookup(&mut e, "E-ETH-USDT", "caller-id");
    let r = lookup_request(&mut e);
    assert!(r.url.ends_with("contractName=E-ETH-USDT"));
    response(&mut e, r, r#"{"code":"0","data":[]}"#);
    let missing = lookup_result(&mut e);
    assert!(!missing.found);
    assert_eq!(missing.definitive, Some(false));
    assert!(missing.order.is_none());
    assert_eq!(e.store().orders(), &[order]);
}
#[test]
fn client_lookup_checks_identity_exactly_and_never_claims_closed_order_absence() {
    for id in ["missing", "CALLER-ID", "", "0", "undefined"] {
        let mut e = engine();
        lookup(&mut e, SYMBOL, id);
        let r = lookup_request(&mut e);
        response(
            &mut e,
            r,
            &format!(r#"{{"code":"0","data":[{LOOKUP_ROW}]}}"#),
        );
        let missing = lookup_result(&mut e);
        assert!(!missing.found);
        assert_eq!(missing.definitive, Some(false));
    }
}
#[test]
fn client_lookup_rejects_partial_or_failed_snapshots_without_definitive_misses() {
    for body in [
        format!(r#"{{"code":"0","data":[{LOOKUP_ROW},{{"orderId":"malformed"}}]}}"#),
        format!(r#"{{"code":"0","data":[{LOOKUP_ROW},{LOOKUP_ROW}]}}"#),
        format!(
            r#"{{"code":"0","data":[{}]}}"#,
            LOOKUP_ROW.replace("\"side\":\"BUY\"", "\"side\":\"wrong\"")
        ),
        format!(
            r#"{{"code":"0","data":[{}]}}"#,
            LOOKUP_ROW.replace(
                "\"price\":59000",
                "\"contractName\":\"E-ETH-USDT\",\"price\":59000"
            )
        ),
        r#"{"code":"0"}"#.into(),
        r#"{"code":"-1","msg":"read denied"}"#.into(),
    ] {
        let mut e = engine();
        lookup(&mut e, SYMBOL, "caller-id");
        let r = lookup_request(&mut e);
        response(&mut e, r, &body);
        let missing = lookup_result(&mut e);
        assert!(!missing.found, "{body}");
        assert_eq!(missing.definitive, Some(false));
        assert!(e.store().orders().is_empty());
    }
}
#[test]
fn client_lookup_network_failure_falls_back_to_latest_cache_without_side_effects() {
    let mut e = engine();
    lookup(&mut e, SYMBOL, "caller-id");
    let order = fetched_order();
    e.store_mut().upsert_order(order.clone());
    for _ in 0..4 {
        let r = lookup_request(&mut e);
        e.handle(
            1700000000000.0,
            Event::HttpFailed {
                request: r.id,
                error: "network",
            },
        );
    }
    let found = lookup_result(&mut e);
    assert!(found.found);
    assert_eq!(found.order.unwrap().id, order.id);
    assert_eq!(e.store().orders(), &[order]);
}
#[test]
fn client_lookup_without_private_credentials_never_asserts_definitive_absence() {
    let mut e = support::engine(Options::default());
    lookup(&mut e, SYMBOL, "caller-id");
    let missing = lookup_result(&mut e);
    assert!(!missing.found);
    assert_eq!(missing.definitive, Some(false));
}

#[test]
fn client_lookup_preserves_ts_native_and_parent_identity_aliases() {
    let mut e = engine();
    let mut order = fetched_order();
    order.parent_id = Some("parent-id".into());
    e.store_mut().upsert_order(order);
    for id in ["2334151155877155281", "parent-id"] {
        lookup(&mut e, SYMBOL, id);
        assert!(lookup_result(&mut e).found);
    }
}

#[test]
fn lookup_copy_retains_other_extra_fields_and_does_not_clear_store_unit_metadata() {
    let mut e = engine();
    let mut order = fetched_order();
    order.extra.as_mut().unwrap().remaining_notional = Some(177.0);
    e.store_mut().upsert_order(order.clone());
    lookup(&mut e, SYMBOL, "caller-id");
    let found = lookup_result(&mut e).order.unwrap();
    assert_eq!(
        found.extra.unwrap(),
        std::collections::BTreeMap::from([("remainingNotional".into(), 177.0)])
    );
    assert_eq!(e.store().orders(), &[order]);
}

fn owned_failure(e: &mut Engine<Bitrue>) -> safe_cex_engine::VenueError {
    let mut failures = Vec::new();
    for action in
        safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.).into_iter()
    {
        match action {
            Action::Http(_) => panic!("unexpected continuation"),
            Action::Notice(Notice::CommandDone { .. }) => panic!("failed page became success"),
            Action::Notice(Notice::CommandFailed { command, error }) => {
                assert_eq!(command, 99);
                failures.push(error);
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE);
            }
            _ => {}
        }
    }
    assert_eq!(failures.len(), 1);
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
    let error = failures.pop().unwrap();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    error
}

#[test]
fn owner_types_terminal_native_refusal_without_accepting_its_prefix() {
    let mut e = engine();
    query(&mut e, fills());
    let r = request(&mut e);
    response(
        &mut e,
        r,
        &format!(r#"{{"code":"510","msg":"denied","data":[{ROW}]}}"#),
    );
    assert!(owned_failure(&mut e).message.contains("denied"));
}

#[test]
fn owner_types_malformed_and_missing_array_failures_without_empty_success() {
    for body in [
        format!(r#"{{"code":0,"data":[{ROW},{{"tradeId":"bad"}}]}}"#),
        r#"{"code":0}"#.into(),
        r#"{"code":0,"data":null}"#.into(),
    ] {
        let mut e = engine();
        query(&mut e, fills());
        let r = request(&mut e);
        response(&mut e, r, &body);
        owned_failure(&mut e);
    }
}

fn retry_request(e: &mut Engine<Bitrue>) -> HttpRequest {
    let mut requests = Vec::new();
    for action in
        safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.).into_iter()
    {
        match action {
            Action::Http(r) => requests.push(r),
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. }) => {
                panic!("retry attempt completed the command")
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE);
            }
            _ => {}
        }
    }
    assert_eq!(requests.len(), 1);
    requests.pop().unwrap()
}

fn interrupt(e: &mut Engine<Bitrue>, r: &HttpRequest, prefix: &str) {
    e.handle(
        1700000000000.0,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        1700000000000.0,
        Event::HttpChunk {
            request: r.id,
            data: prefix.as_bytes(),
        },
    );
    for action in
        safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.).into_iter()
    {
        match action {
            Action::Http(_)
            | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. }) => {
                panic!("unvalidated prefix escaped")
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE);
            }
            _ => {}
        }
    }
    e.handle(
        1700000000000.0,
        Event::HttpFailed {
            request: r.id,
            error: "network",
        },
    );
}

#[test]
fn owner_types_exhausted_transport_after_four_unadmitted_prefixes() {
    let mut e = engine();
    query(&mut e, fills());
    let mut r = retry_request(&mut e);
    for attempt in 0..4 {
        interrupt(&mut e, &r, &format!(r#"{{"code":0,"data":[{ROW},"#));
        if attempt < 3 {
            r = retry_request(&mut e);
        }
    }
    assert_eq!(owned_failure(&mut e).message, "network");
}

#[test]
fn owner_keeps_full_filtered_and_duplicate_page_coverage_strict_and_typed() {
    for (limit, order, rows) in [
        (1, Some("999".into()), ROW.to_owned()),
        (2, None, format!("{ROW},{ROW}")),
    ] {
        let mut e = engine();
        query(
            &mut e,
            Query::Fills {
                symbol: Some(SYMBOL.into()),
                order_id: order,
                from_ms: None,
                to_ms: None,
                limit: Some(limit),
            },
        );
        let r = retry_request(&mut e);
        response(&mut e, r, &format!(r#"{{"code":0,"data":[{rows}]}}"#));
        assert!(owned_failure(&mut e).message.starts_with("INCOMPLETE:"));
    }
}

#[test]
fn retry_resets_same_id_economics_and_keeps_signature_and_transport_budgets_separate() {
    for signature_first in [false, true] {
        let mut e = engine();
        query(&mut e, fills());
        let mut r = retry_request(&mut e);
        let mut stamps = Vec::new();
        // A signature resend is extra to the three GET retries, in either order.
        for step in 0..4 {
            stamps.push(
                r.headers
                    .iter()
                    .find(|(k, _)| *k == "X-CH-TS")
                    .unwrap()
                    .1
                    .clone(),
            );
            if (signature_first && step == 0) || (!signature_first && step == 3) {
                response(
                    &mut e,
                    r,
                    &format!(r#"{{"data":[{ROW}],"code":"-1022","msg":"signature"}}"#),
                );
            } else {
                interrupt(&mut e, &r, &format!(r#"{{"code":0,"data":[{ROW},"#));
            }
            r = retry_request(&mut e);
        }
        stamps.push(
            r.headers
                .iter()
                .find(|(k, _)| *k == "X-CH-TS")
                .unwrap()
                .1
                .clone(),
        );
        assert!(
            stamps
                .windows(2)
                .all(|w| w[0].parse::<u64>().unwrap() < w[1].parse::<u64>().unwrap())
        );
        let corrected = ROW.replace("60000", "61000").replace("180", "183");
        response(
            &mut e,
            r,
            &format!(r#"{{"code":200,"data":[{corrected}]}}"#),
        );
        let rows = owned_success(&mut e, 99);
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["price"], 61000.0);
        assert_eq!(rows[0]["qty"], 0.003);
    }
}

fn owned_success(e: &mut Engine<Bitrue>, expected_command: u32) -> serde_json::Value {
    let mut results = Vec::new();
    for action in
        safe_cex_engine::test_support::prepared_actions(&mut *e, 1700000000000.).into_iter()
    {
        match action {
            Action::Http(_) => panic!("single-page success continued"),
            Action::Notice(Notice::CommandFailed { .. }) => panic!("successful page rejected"),
            Action::Notice(Notice::CommandDone { command, result }) => {
                assert_eq!(command, expected_command);
                results.push(serde_json::from_str::<serde_json::Value>(&result).unwrap());
            }
            Action::Notice(Notice::Event(event)) => {
                assert_ne!(event.code, safe_cex_engine::log::code::HISTORY_INCOMPLETE);
            }
            _ => {}
        }
    }
    assert_eq!(results.len(), 1);
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
    results.pop().unwrap()
}

#[test]
fn single_page_hard_row_bound_is_checked_before_limit_and_admission() {
    for count in [1000, 1001] {
        let mut e = engine();
        query(
            &mut e,
            Query::Fills {
                symbol: Some(SYMBOL.into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(1000),
            },
        );
        let r = retry_request(&mut e);
        let rows = (1..=count)
            .map(|id| ROW.replace("1558124009467904992", &id.to_string()))
            .collect::<Vec<_>>()
            .join(",");
        response(&mut e, r, &format!(r#"{{"code":0,"data":[{rows}]}}"#));
        if count == 1000 {
            assert_eq!(owned_success(&mut e, 99).as_array().unwrap().len(), 1000);
        } else {
            assert_eq!(
                owned_failure(&mut e).message,
                "Malformed Bitrue execution history"
            );
        }
    }
}

#[test]
fn no_list_signature_resend_is_once_after_transport_retries() {
    let mut e = engine();
    query(&mut e, fills());
    let mut r = retry_request(&mut e);
    for _ in 0..3 {
        interrupt(&mut e, &r, &format!(r#"{{"code":0,"data":[{ROW},"#));
        r = retry_request(&mut e);
    }
    response(&mut e, r, r#"{"code":"-1022","msg":"signature"}"#);
    let r = retry_request(&mut e);
    response(&mut e, r, r#"{"code":"-1022","msg":"signature again"}"#);
    assert!(owned_failure(&mut e).message.contains("signature again"));
}

#[test]
fn signature_retry_does_not_replenish_get_retry_budget() {
    let mut e = engine();
    query(&mut e, fills());
    let mut r = retry_request(&mut e);
    for _ in 0..3 {
        interrupt(&mut e, &r, &format!(r#"{{"code":0,"data":[{ROW},"#));
        r = retry_request(&mut e);
    }
    response(&mut e, r, &format!(r#"{{"data":[{ROW}],"code":"-1022"}}"#));
    let r = retry_request(&mut e);
    interrupt(&mut e, &r, &format!(r#"{{"code":0,"data":[{ROW},"#));
    assert_eq!(owned_failure(&mut e).message, "network");
}

#[test]
fn utf8_and_missing_list_fail_without_retry_or_prefix_publication() {
    for (bytes, expected) in [
        (&[0xff, 0xfe][..], "response did not contain"),
        (
            br#"{"code":0,"data":{}}"#.as_slice(),
            "response did not contain",
        ),
    ] {
        let mut e = engine();
        query(&mut e, fills());
        let r = retry_request(&mut e);
        e.handle(
            1700000000000.,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            1700000000000.,
            Event::HttpChunk {
                request: r.id,
                data: bytes,
            },
        );
        e.handle(1700000000000., Event::HttpEnd { request: r.id });
        let error = owned_failure(&mut e);
        assert!(error.message.contains(expected), "{error:?}");
    }
}

#[test]
fn concurrent_single_page_owners_complete_independently_and_empty_is_valid() {
    let mut e = engine();
    query(&mut e, fills());
    let first = retry_request(&mut e);
    e.handle(
        1700000000000.0,
        Event::Command(Command::Query {
            command: 100,
            query: fills(),
        }),
    );
    let second = retry_request(&mut e);
    response(&mut e, second, r#"{"code":200,"data":[]}"#);
    assert_eq!(owned_success(&mut e, 100), serde_json::json!([]));
    let other = ROW.replace("E-BTC-USDT", "E-ETH-USDT");
    let duplicate = ROW.replace("60000", "61000").replace("180", "183");
    response(
        &mut e,
        first,
        &format!(r#"{{"code":0,"data":[{other},{ROW},{duplicate}]}}"#),
    );
    let rows = owned_success(&mut e, 99);
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["price"], 60000.0);
}
