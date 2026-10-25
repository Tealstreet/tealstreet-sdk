//! Original remaining-funnel custody gaps and native compatibility controls.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Query,
    Response, SocketId,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use serde_json::{Value, json};
use support::Options;

type TestEngine = Engine<PhemexPerpetuals>;
const NOW: f64 = 1_710_000_000_000.;
const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"1","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"69000","askRp":"69001","lastRp":"69000","markRp":"69000","indexRp":"69000","openRp":"69000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;
const FILL: &str = r#"{"transactTimeNs":1669407633999999999,"symbol":"BTCUSDT","currency":"USDT","side":"Sell","tradeType":"Trade","execQtyRq":"0.001","execPriceRp":"16600","execFeeRv":"0.00166","execID":"execution-a","orderID":"order-a"}"#;

fn engine(primary: bool, secondary: bool) -> TestEngine {
    support::engine(Options {
        key: primary.then(|| "fixture-primary".into()),
        secret: primary.then(|| "fixture-material".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "fixture-secondary".into(),
            secret: "fixture-read-material".into(),
            passphrase: None,
        }),
        ..Default::default()
    })
}
fn seeded(primary: bool, secondary: bool) -> TestEngine {
    let mut e = engine(primary, secondary);
    e.store_mut().set_markets(vec![Market {
        id: "BTC/USDT:USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            price: 0.1,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 1.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}
fn at(e: &mut TestEngine, now: f64, event: Event<'_>) -> Vec<Action> {
    e.handle(now, event);
    e.prepare_dispatch(now);
    e.drain_actions().collect()
}
fn reply(
    e: &mut TestEngine,
    now: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    at(
        e,
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    )
}
fn release(e: &mut TestEngine, now: f64, actions: Vec<Action>) -> Vec<Action> {
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from(actions);
    e.prepare_dispatch(now);
    queue.extend(e.drain_actions());
    while let Some(action) = queue.pop_front() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation, .. } => e.http_prepared(
                    now,
                    preparation,
                    Prepared {
                        id: preparation.attempt().request.0,
                        expires_at: None,
                    },
                ),
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    if matches!(e.claim_http(now, permit), Ok(Claim::Released(_))) {
                        out.push(Action::Http(request));
                    }
                }
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => {
                    panic!("remaining-funnel original uses serial requests")
                }
            },
            action => out.push(action),
        }
        queue.extend(e.drain_actions());
    }
    out
}
fn requests(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect()
}
fn wire(actions: &[Action], path: &str) -> HttpRequest {
    requests(actions)
        .into_iter()
        .find(|r| r.url.contains(path))
        .expect("one native request")
}
fn prepared(actions: &[Action], path: &str) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "physical HTTP must await actual route preparation"
    );
    assert_eq!(actions.iter().filter(|a|matches!(a,Action::ManagedHttp(a) if matches!(a.as_ref(),HttpAction::Prepare{request,..} if request.url.contains(path)))).count(),1,"one genuine managed prepare");
}
fn capture(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { request, .. } if request.url.contains(path) => {
                    Some(request.clone())
                }
                _ => None,
            },
            _ => None,
        })
        .expect("actual original request or preparation")
}
fn terminal(actions: &[Action], id: u32) -> Vec<Result<&str, &safe_cex_engine::VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(Ok(result.as_str()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(Err(error))
            }
            _ => None,
        })
        .collect()
}
fn fills() -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(2),
    }
}
fn income() -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(0.),
        to_ms: Some(1_000.),
        limit: None,
    }
}
fn candles(id: u32) -> Command {
    Command::FetchCandles {
        command: id,
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: Some(0.),
        to_ms: Some(180_000.),
    }
}
fn ask(e: &mut TestEngine, now: f64, id: u32, query: Query) -> Vec<Action> {
    at(
        e,
        now,
        Event::Command(Command::Query { command: id, query }),
    )
}
fn opt(id: &str) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount: 0.01,
        price: Some(69000.),
        trigger_price: None,
        max_slippage_pct: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: Some(id.into()),
        close: None,
        stp_mode: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
        implied_volatility: None,
    }
}
fn value(r: &HttpRequest, key: &str) -> String {
    r.url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .find_map(|s| {
            s.split_once('=')
                .filter(|(k, _)| *k == key)
                .map(|(_, v)| v.to_owned())
        })
        .unwrap()
}
fn started() -> (TestEngine, SocketId) {
    let mut e = engine(true, true);
    e.start(NOW);
    let a = e.drain_actions().collect();
    let a = release(&mut e, NOW, a);
    let products = wire(&a, "products");
    let a = reply(&mut e, NOW + 1., &products, 200, PRODUCTS);
    let a = release(&mut e, NOW + 1., a);
    let socket = a
        .iter()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .expect("native private connection");
    for r in requests(&a)
        .into_iter()
        .filter(|r| r.url.contains("/g-accounts/"))
    {
        let currency = if r.url.contains("USDC") {
            "USDC"
        } else {
            "USDT"
        };
        let rows = if currency == "USDT" {
            json!([{"symbol":"BTCUSDT","currency":"USDT","posMode":"OneWay","posSide":"Merged","size":"0","leverageRr":"-10","crossMargin":true}])
        } else {
            json!([])
        };
        let body=json!({"code":0,"data":{"account":{"currency":currency,"accountBalanceRv":"1000","totalUsedBalanceRv":"0"},"positions":rows}}).to_string();
        let a = reply(&mut e, NOW + 2., &r, 200, &body);
        let _ = release(&mut e, NOW + 2., a);
    }
    let daily = wire(&a, "24hr/all");
    let a = reply(&mut e, NOW + 3., &daily, 200, DAILY);
    let _ = release(&mut e, NOW + 3., a);
    let a = at(&mut e, NOW + 4., Event::WsOpened { socket });
    let _ = release(&mut e, NOW + 4., a);
    let a = at(
        &mut e,
        NOW + 5.,
        Event::WsMessage {
            socket,
            text: r#"{"id":999,"result":{"status":"success"}}"#,
        },
    );
    let _ = release(&mut e, NOW + 5., a);
    let a = at(
        &mut e,
        NOW + 6.,
        Event::WsMessage {
            socket,
            text: r#"{"type":"snapshot","orders_p":[]}"#,
        },
    );
    let _ = release(&mut e, NOW + 6., a);
    (e, socket)
}
fn placement(id: u32) -> Command {
    Command::PlaceOrders {
        command: id,
        orders: vec![opt("caller-a")],
    }
}

#[test]
fn products_wait_for_route_preparation() {
    let mut e = engine(false, false);
    e.start(NOW);
    prepared(&e.drain_actions().collect::<Vec<_>>(), "/public/products");
}
#[test]
fn unclaimed_products_cannot_publish_catalogue() {
    let mut e = engine(false, false);
    e.start(NOW);
    let a: Vec<_> = e.drain_actions().collect();
    let r = capture(&a, "products");
    let output = reply(&mut e, NOW + 1., &r, 200, PRODUCTS);
    assert!(
        e.store().markets().is_empty(),
        "unclaimed DATA cannot populate markets"
    );
    assert!(
        !output.iter().any(|a| matches!(a, Action::Http(_))),
        "unclaimed catalogue cannot start ticker followups"
    );
}
#[test]
fn ticker_waits_for_route_preparation() {
    let mut e = engine(false, false);
    e.start(NOW);
    let a = e.drain_actions().collect();
    let a = release(&mut e, NOW, a);
    let output = reply(&mut e, NOW + 1., &wire(&a, "products"), 200, PRODUCTS);
    prepared(&output, "24hr/all");
}
#[test]
fn credentialed_candles_wait_for_route_preparation() {
    let mut e = seeded(true, true);
    let a = at(&mut e, NOW, Event::Command(candles(10)));
    prepared(&a, "kline/list");
}
#[test]
fn signed_history_waits_for_route_preparation() {
    let mut e = seeded(true, true);
    let a = ask(&mut e, NOW, 11, fills());
    prepared(&a, "g-futures/trades");
}
#[test]
fn lookup_waits_for_primary_route_preparation() {
    let mut e = seeded(true, true);
    let a = ask(
        &mut e,
        NOW,
        12,
        Query::OrderByClientId {
            symbol: "BTCUSDT".into(),
            client_order_id: "caller-a".into(),
        },
    );
    prepared(&a, "by-order-id");
}
#[test]
fn income_waits_for_route_preparation() {
    let mut e = seeded(true, true);
    let a = ask(&mut e, NOW, 13, income());
    prepared(&a, "tradeAccountDetail");
}
#[test]
fn unclaimed_history_cannot_publish_terminal_or_rows() {
    let mut e = seeded(true, true);
    let a = ask(&mut e, NOW, 14, fills());
    let r = capture(&a, "g-futures/trades");
    let output = reply(&mut e, NOW + 1., &r, 200, &format!("[{FILL}]"));
    assert!(
        terminal(&output, 14).is_empty(),
        "an unclaimed response has no terminal authority"
    );
}
#[test]
fn history_uses_original_caller_deadline() {
    let mut e = seeded(true, true);
    e.handle_command(
        NOW,
        Command::Query {
            command: 15,
            query: fills(),
        },
        Some(5),
    )
    .unwrap();
    let a: Vec<_> = e.drain_actions().collect();
    let _ = release(&mut e, NOW, a);
    let output = at(
        &mut e,
        NOW + 5.,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    let results = terminal(&output, 15);
    assert_eq!(
        results.len(),
        1,
        "original caller bound seals once at equality"
    );
    assert_eq!(
        results[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
#[test]
fn income_uses_original_caller_deadline() {
    let mut e = seeded(true, true);
    e.handle_command(
        NOW,
        Command::Query {
            command: 16,
            query: income(),
        },
        Some(5),
    )
    .unwrap();
    let a: Vec<_> = e.drain_actions().collect();
    let _ = release(&mut e, NOW, a);
    let output = at(
        &mut e,
        NOW + 5.,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    let results = terminal(&output, 16);
    assert_eq!(results.len(), 1, "income retains its original receipt");
    assert_eq!(
        results[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}
#[test]
fn trade_waits_for_route_preparation() {
    let (mut e, _) = started();
    let a = at(&mut e, NOW + 10., Event::Command(placement(20)));
    prepared(&a, "g-orders/create");
}
#[test]
fn rejected_trade_admission_has_no_optimistic_effect_and_preserves_peers() {
    let (mut e, _) = started();
    let mut peers: Vec<Preparation> = Vec::new();
    for id in 1000..1256 {
        let a = at(
            &mut e,
            NOW + 10.,
            Event::Command(Command::FetchAccount { command: id }),
        );
        let mut added = false;
        for action in a {
            if let Action::ManagedHttp(a) = action
                && let HttpAction::Prepare { preparation, .. } = *a
            {
                peers.push(preparation);
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    assert!(!peers.is_empty());
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
    let before = e.store().orders().to_vec();
    let a = at(&mut e, NOW + 11., Event::Command(placement(21)));
    assert_eq!(
        e.store().orders(),
        before,
        "refused admission cannot publish speculative order state"
    );
    assert!(
        !a.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandProgress { command: 21, .. }) | Action::Http(_)
        )),
        "no progress or I/O before admission"
    );
    let results = terminal(&a, 21);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].as_ref().unwrap_err().outcome,
        Some(Outcome::NotSent)
    );
    assert!(peers.iter().all(|p| e.http_preparation_pending(*p)));
}

#[test]
fn native_optional_public_signing_accepts_secondary_without_primary() {
    let mut e = engine(false, true);
    e.start(NOW);
    let a = e.drain_actions().collect();
    let a = release(&mut e, NOW, a);
    let r = wire(&a, "products");
    assert_eq!(
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-phemex-access-token"))
            .unwrap()
            .1,
        "fixture-secondary"
    );
}
#[test]
fn native_signed_history_uses_secondary_but_lookup_stays_primary() {
    for (id, q, key, path) in [
        (31, fills(), "fixture-secondary", "g-futures/trades"),
        (
            32,
            Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "caller-a".into(),
            },
            "fixture-primary",
            "by-order-id",
        ),
    ] {
        let mut e = seeded(true, true);
        let a = ask(&mut e, NOW, id, q);
        if id == 32 {
            assert!(a.iter().any(|a| matches!(a,Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare { request, intent:safe_cex_engine::request_context::RoutingIntent::Primary,.. } if request.url.contains("/by-order-id")))));
        }
        let a = release(&mut e, NOW, a);
        let r = wire(&a, path);
        assert_eq!(
            r.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("x-phemex-access-token"))
                .unwrap()
                .1,
            key
        );
        if id == 32 {
            assert_eq!(r.route, None);
        }
    }
}
#[test]
fn native_history_dedup_and_numeric_values_survive() {
    let mut e = seeded(true, true);
    let a = ask(&mut e, NOW, 33, fills());
    let a = release(&mut e, NOW, a);
    let r = wire(&a, "g-futures/trades");
    let output = reply(&mut e, NOW + 1., &r, 200, &format!("[{FILL},{FILL}]"));
    let rows: Value = serde_json::from_str(terminal(&output, 33)[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "execution-a");
    assert_eq!(rows[0]["price"], 16600.);
    assert_eq!(rows[0]["qty"], 0.001);
    assert_eq!(rows[0]["commission"], 0.00166);
}
#[test]
fn native_place_bytes_progress_and_primary_signing_survive() {
    let (mut e, _) = started();
    let a = at(&mut e, NOW + 10., Event::Command(placement(34)));
    let a = release(&mut e, NOW + 10., a);
    let r = wire(&a, "g-orders/create");
    assert_eq!(r.method, Method::Put);
    assert_eq!(r.body, None);
    assert_eq!(value(&r, "clOrdID"), "caller-a");
    assert_eq!(value(&r, "orderQtyRq"), "0.01");
    assert_eq!(
        r.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("x-phemex-access-token"))
            .unwrap()
            .1,
        "fixture-primary"
    );
    let progress = a
        .iter()
        .position(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandProgress { command: 34, .. })
            )
        })
        .unwrap();
    let send = a.iter().position(|a| matches!(a, Action::Http(_))).unwrap();
    assert!(progress < send);
}
#[test]
fn native_unknown_write_stops_without_resend_and_keeps_signing_expiry_fence() {
    let (mut e, _) = started();
    let a = at(&mut e, NOW + 10., Event::Command(placement(35)));
    let a = release(&mut e, NOW + 10., a);
    let r = wire(&a, "g-orders/create");
    let output = at(
        &mut e,
        NOW + 11.,
        Event::HttpFailed {
            request: r.id,
            error: "physical transport loss",
        },
    );
    let results = terminal(&output, 35);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    assert!(!requests(&output).iter().any(|r| r.method != Method::Get));
    let expiry = value_header(&r, "x-phemex-request-expiry")
        .parse::<f64>()
        .unwrap()
        * 1000.;
    assert!(
        output
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms>=expiry+5000.)),
        "reconcile uses actual released signature expiry plus skew"
    );
}
fn value_header<'a>(r: &'a HttpRequest, key: &str) -> &'a str {
    r.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .unwrap()
        .1
        .as_str()
}

fn public_book_pending() -> (TestEngine, Vec<Action>) {
    let mut e = engine(false, false);
    e.start(NOW);
    let a = e.drain_actions().collect();
    let a = release(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &wire(&a, "products"), 200, PRODUCTS);
    let a = release(&mut e, NOW + 1., a);
    let a = reply(&mut e, NOW + 2., &wire(&a, "24hr/all"), 200, DAILY);
    let a = release(&mut e, NOW + 2., a);
    let socket = a
        .iter()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Public),
                ..
            } = a
            {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    let _ = at(&mut e, NOW + 3., Event::WsOpened { socket });
    let _ = at(
        &mut e,
        NOW + 4.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let a = at(
        &mut e,
        NOW + 5.,
        Event::WsMessage {
            socket,
            text: r#"{"symbol":"BTCUSDT","sequence":102,"type":"incremental","orderbook_p":{"bids":[["20690","2"]],"asks":[]}}"#,
        },
    );
    (e, a)
}
#[test]
fn depth_snapshot_waits_for_route_preparation() {
    let (_, a) = public_book_pending();
    prepared(&a, "fullbook");
}
#[test]
fn native_depth_snapshot_applies_newer_buffered_diff() {
    let (mut e, a) = public_book_pending();
    let a = release(&mut e, NOW + 5., a);
    let r = wire(&a, "fullbook");
    let _ = reply(
        &mut e,
        NOW + 6.,
        &r,
        200,
        r#"{"result":{"sequence":101,"orderbook_p":{"bids":[["20690","1"],["20689","1"]],"asks":[["20691","1"]]}}}"#,
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().bids(),
        &[(20690., 2.), (20689., 1.)]
    );
}

#[test]
fn native_empty_batch_keeps_credential_precedence_without_dummy_http() {
    for private in [false, true] {
        let (mut e, _) = started();
        if !private {
            e = engine(false, false);
        }
        let a = at(
            &mut e,
            NOW + 10.,
            Event::Command(Command::PlaceOrders {
                command: 50,
                orders: vec![],
            }),
        );
        assert!(requests(&a).is_empty());
        let results = terminal(&a, 50);
        assert_eq!(results.len(), 1);
        if private {
            assert_eq!(*results[0].as_ref().unwrap(), "[]");
        } else {
            assert_eq!(
                results[0].as_ref().unwrap_err().kind,
                Some(ErrorKind::AuthInvalid)
            );
        }
    }
}
#[test]
fn native_thousand_split_lots_complete_with_every_id_in_order() {
    let (mut e, _) = started();
    let mut order = opt("caller-a");
    order.client_order_id = None;
    order.amount = 1000.;
    let a = at(
        &mut e,
        NOW + 10.,
        Event::Command(Command::PlaceOrders {
            command: 51,
            orders: vec![order],
        }),
    );
    let mut a = release(&mut e, NOW + 10., a);
    let expected: Vec<_> = (0..1000).map(|i| format!("placed-{i}")).collect();
    for (i, id) in expected.iter().enumerate() {
        let request = wire(&a, "g-orders/create");
        assert_eq!(value(&request, "orderQtyRq"), "1");
        let body=json!({"code":0,"data":{"orderID":id,"clOrdID":value(&request,"clOrdID"),"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","orderType":"Limit","orderQtyRq":"1","priceRp":"69000","ordStatus":"New","execStatus":"New","timeInForce":"GoodTillCancel"}}).to_string();
        let now = NOW + 11. + i as f64;
        a = reply(&mut e, now, &request, 200, &body);
        a = release(&mut e, now, a);
    }
    let results = terminal(&a, 51);
    assert_eq!(results.len(), 1);
    assert_eq!(
        serde_json::from_str::<Vec<String>>(results[0].as_ref().unwrap()).unwrap(),
        expected
    );
}

fn cached_order(id: &str) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 69000.,
        amount: 0.1,
        filled: 0.,
        remaining: 0.1,
        reduce_only: false,
        close: None,
        client_order_id: Some("old".into()),
        time_in_force: Some(TimeInForce::PostOnly),
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

#[test]
fn native_mixed_cancel_rows_stop_unknown_and_restore_only_refused_rows() {
    let (mut e, _) = started();
    e.store_mut().upsert_order(cached_order("accepted"));
    e.store_mut().upsert_order(cached_order("refused"));
    let a = at(
        &mut e,
        NOW + 10.,
        Event::Command(Command::CancelOrders {
            command: 52,
            ids: vec!["accepted".into(), "refused".into()],
        }),
    );
    let a = release(&mut e, NOW + 10., a);
    let r = wire(&a, "g-orders");
    let body=json!({"code":0,"data":[{"orderID":"accepted","ordStatus":"Canceled","cumQtyRq":"0"},{"orderID":"refused","bizError":11001}]}).to_string();
    let a = reply(&mut e, NOW + 11., &r, 200, &body);
    let a = release(&mut e, NOW + 11., a);
    let results = terminal(&a, 52);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].as_ref().unwrap_err().outcome,
        Some(Outcome::Unknown)
    );
    assert_eq!(
        e.store().order("accepted").unwrap().status,
        OrderStatus::Canceled
    );
    assert_eq!(
        e.store().order("refused").unwrap().status,
        OrderStatus::Open
    );
    assert!(!requests(&a).iter().any(|r| r.method != Method::Get));
}

fn corrupt_body(e: &mut TestEngine, now: f64, request: &HttpRequest) -> Vec<Action> {
    let mut out = at(
        e,
        now,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    out.extend(at(
        e,
        now,
        Event::HttpChunk {
            request: request.id,
            data: &[0xff],
        },
    ));
    out.extend(at(
        e,
        now,
        Event::HttpEnd {
            request: request.id,
        },
    ));
    out
}
#[test]
fn managed_private_integrity_failures_keep_the_typed_cause_without_retry() {
    for q in [fills(), income()] {
        let mut e = seeded(true, true);
        let a = ask(&mut e, NOW, 801, q);
        let path=if a.iter().any(|a| matches!(a,Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare {request,..} if request.url.contains("tradeAccountDetail")))) {"tradeAccountDetail"} else {"g-futures/trades"};
        let a = release(&mut e, NOW, a);
        let output = corrupt_body(&mut e, NOW + 1., &wire(&a, path));
        let result = terminal(&output, 801);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::Internal)
        );
        assert!(
            !output
                .iter()
                .any(|action| matches!(action, Action::Http(_)) || matches!(action, Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare {..} | HttpAction::Bind {..} | HttpAction::BindGroup {..})))
        );
    }
}
#[test]
fn income_network_and_integrity_failures_preserve_accepted_prefix_and_marker() {
    for integrity in [false, true] {
        let mut e = seeded(true, true);
        let a = ask(&mut e, NOW, 802, income());
        let a = release(&mut e, NOW, a);
        let row = r#"[{"typeDesc":"REALIZED_PNL","amountRv":"1","balanceRv":"101","createTime":500,"currency":"USDT"}]"#;
        let mut a = reply(&mut e, NOW + 1., &wire(&a, "tradeAccountDetail"), 200, row);
        let mut pages = 0;
        let second = loop {
            a = release(&mut e, NOW + 1., a);
            let current = wire(&a, "tradeAccountDetail");
            if current.url.contains("currency=USDC") {
                break current;
            }
            pages += 1;
            assert!(pages <= 4, "native USDT coverage must finish before USDC");
            a = reply(&mut e, NOW + 1., &current, 200, "[]");
        };
        let output = if integrity {
            corrupt_body(&mut e, NOW + 2., &second)
        } else {
            at(
                &mut e,
                NOW + 2.,
                Event::HttpFailed {
                    request: second.id,
                    error: "physical transport loss",
                },
            )
        };
        let result = terminal(&output, 802);
        assert_eq!(result.len(), 1);
        let rows: Value = serde_json::from_str(result[0].as_ref().unwrap()).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["income"], 1.);
        assert!(output.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
                && event.level == safe_cex_engine::Level::Warn && event.message.contains("INCOMPLETE"))));
        assert!(
            !output
                .iter()
                .any(|a| matches!(a, Action::Http(_)) || matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare {..} | HttpAction::Bind {..} | HttpAction::BindGroup {..})))
        );
    }
}
#[test]
fn original_root_equality_is_strict_in_both_event_orders_and_reuse_keeps_peers() {
    for query in [fills(), income()] {
        for response_first in [false, true] {
            let mut e = seeded(true, true);
            e.handle_command(
                NOW,
                Command::Query {
                    command: 803,
                    query: query.clone(),
                },
                Some(5),
            )
            .unwrap();
            let a = e.drain_actions().collect();
            let a = release(&mut e, NOW, a);
            let old = requests(&a).into_iter().next().unwrap();
            let timer = a
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == NOW + 5. => Some(*timer),
                    _ => None,
                })
                .unwrap();
            let mut output = if response_first {
                reply(&mut e, NOW + 5., &old, 200, "[]")
            } else {
                at(&mut e, NOW + 5., Event::TimerFired { timer })
            };
            if response_first {
                output.extend(at(&mut e, NOW + 5., Event::TimerFired { timer }));
            } else {
                output.extend(reply(&mut e, NOW + 5., &old, 200, "[]"));
            }
            assert_eq!(terminal(&output, 803).len(), 1);
            assert_eq!(
                terminal(&output, 803)[0].as_ref().unwrap_err().kind,
                Some(ErrorKind::Timeout)
            );
            let peer = ask(&mut e, NOW + 6., 804, query.clone());
            let peer = release(&mut e, NOW + 6., peer);
            let peer = requests(&peer).into_iter().next().unwrap();
            let reused = ask(&mut e, NOW + 6., 803, query.clone());
            let reused = release(&mut e, NOW + 6., reused);
            let reused = requests(&reused).into_iter().next().unwrap();
            assert!(
                reply(&mut e, NOW + 7., &old, 200, "[]")
                    .iter()
                    .all(|a| !matches!(
                        a,
                        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
                    ))
            );
            let peer_id = peer.id;
            for request in [peer, reused] {
                let next = reply(&mut e, NOW + 8., &request, 200, "[]");
                if request.url.contains("tradeAccountDetail") {
                    let next = release(&mut e, NOW + 8., next);
                    let second = wire(&next, "currency=USDC");
                    let end = reply(&mut e, NOW + 9., &second, 200, "[]");
                    assert_eq!(
                        terminal(&end, if request.id == peer_id { 804 } else { 803 }).len(),
                        1
                    );
                } else {
                    assert_eq!(
                        terminal(&next, if request.id == peer_id { 804 } else { 803 }).len(),
                        1
                    );
                }
            }
        }
    }
}
#[test]
fn physical_read_timeout_seals_once_while_the_original_root_is_live() {
    for query in [fills(), income()] {
        let mut e = seeded(true, true);
        let a = ask(&mut e, NOW, 805, query);
        let a = release(&mut e, NOW, a);
        let timer = a
            .iter()
            .filter_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .next_back()
            .unwrap();
        let request = requests(&a).into_iter().next().unwrap();
        let output = at(&mut e, NOW + 20_000., Event::TimerFired { timer });
        let result = terminal(&output, 805);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::Timeout)
        );
        assert!(terminal(&reply(&mut e, NOW + 20_001., &request, 200, "[]"), 805).is_empty());
    }
}

#[test]
fn released_unproven_write_cleanup_precedes_immediate_cancel_all() {
    for status in [408, 500] {
        let (mut e, _) = started();
        e.store_mut().upsert_order(cached_order("old-live"));
        let actions = at(&mut e, NOW + 10., Event::Command(placement(850)));
        let actions = release(&mut e, NOW + 10., actions);
        let create = wire(&actions, "g-orders/create");
        let failure = reply(
            &mut e,
            NOW + 11.,
            &create,
            status,
            r#"{"code":11001,"msg":"insufficient balance","data":null}"#,
        );
        let results = terminal(&failure, 850);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].as_ref().unwrap_err().outcome,
            Some(Outcome::Unknown)
        );
        let next = at(
            &mut e,
            NOW + 12.,
            Event::Command(Command::CancelSymbolOrders {
                command: 851,
                symbol: "BTCUSDT".into(),
            }),
        );
        assert!(!next.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed {error,..}) if error.kind==Some(ErrorKind::InvalidInput))));
        assert!(
            e.store()
                .orders()
                .iter()
                .all(|order| order.status != OrderStatus::Creating)
        );
        let next = release(&mut e, NOW + 12., next);
        let cancel = wire(&next, "/g-orders");
        assert_eq!(cancel.method, Method::Delete);
        assert_eq!(value(&cancel, "orderID"), "old-live");
        let expiry = value_header(&create, "x-phemex-request-expiry")
            .parse::<f64>()
            .unwrap()
            * 1000.;
        assert!(
            next.iter()
                .any(|action| matches!(action, Action::SetTimer{at_ms,..} if *at_ms==expiry+5000.))
        );
        assert!(next.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.message.contains("caller-a"))));
        assert!(terminal(&next, 850).is_empty());
        assert!(
            requests(&next)
                .iter()
                .all(|request| !request.url.contains("g-orders/create"))
        );
    }
}

fn unsupported(id: u32) -> Command {
    Command::FetchLeverage {
        command: id,
        symbol: "BTCUSDT".into(),
    }
}
fn assert_refusal(actions: &[Action], id: u32, message: &str, kind: Option<ErrorKind>) {
    let results = terminal(actions, id);
    assert_eq!(results.len(), 1);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.message, message);
    assert_eq!(error.kind, kind);
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandError{command,message:diagnostic}) if *command==id && diagnostic==message)));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
}
#[test]
fn unsupported_trading_keeps_native_key_and_input_precedence() {
    let mut public = support::engine(Options {
        public_data_only: true,
        key: Some("fixture-primary".into()),
        secret: Some("fixture-material".into()),
        ..Default::default()
    });
    let output = at(&mut public, NOW, Event::Command(unsupported(860)));
    assert_refusal(
        &output,
        860,
        "Phemex private credentials are required",
        Some(ErrorKind::AuthInvalid),
    );
    let mut private = engine(true, false);
    let output = at(&mut private, NOW, Event::Command(unsupported(861)));
    assert_refusal(
        &output,
        861,
        "Phemex command is unsupported",
        Some(ErrorKind::InvalidInput),
    );
}
#[test]
fn unsupported_trading_keeps_native_thirty_two_job_precedence() {
    let (mut e, _) = started();
    for i in 0..32 {
        let output = at(
            &mut e,
            NOW + 10. + i as f64,
            Event::Command(Command::PlaceOrders {
                command: 900 + i,
                orders: vec![opt(&format!("held-{i}"))],
            }),
        );
        prepared(&output, "g-orders/create");
    }
    let output = at(&mut e, NOW + 50., Event::Command(unsupported(950)));
    assert_refusal(
        &output,
        950,
        "Phemex too many active trading commands",
        None,
    );
}
#[test]
fn retired_place_settles_from_a_non_command_callback_once() {
    let (mut e, socket) = started();
    let output = at(&mut e, NOW + 10., Event::Command(placement(870)));
    let output = release(&mut e, NOW + 10., output);
    let request = wire(&output, "g-orders/create");
    let failed = reply(&mut e, NOW + 11., &request, 408, "{}");
    assert_eq!(terminal(&failed, 870).len(), 1);
    assert!(
        e.store()
            .orders()
            .iter()
            .any(|order| order.status == OrderStatus::Creating)
    );
    let settled = at(
        &mut e,
        NOW + 12.,
        Event::WsMessage {
            socket,
            text: r#"{"type":"incremental","orders_p":[]}"#,
        },
    );
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|order| order.status != OrderStatus::Creating)
    );
    let expiry = value_header(&request, "x-phemex-request-expiry")
        .parse::<f64>()
        .unwrap()
        * 1000.;
    assert!(
        settled
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==expiry+5000.))
    );
    assert!(terminal(&settled, 870).is_empty());
    let again = at(
        &mut e,
        NOW + 13.,
        Event::WsMessage {
            socket,
            text: r#"{"type":"incremental","orders_p":[]}"#,
        },
    );
    assert!(
        !again
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==expiry+5000.))
    );
    assert!(terminal(&again, 870).is_empty());
}
#[test]
fn retired_cancel_restores_unchanged_canceling_row_before_next_command() {
    let (mut e, _) = started();
    e.store_mut().upsert_order(cached_order("old-live"));
    let output = at(
        &mut e,
        NOW + 10.,
        Event::Command(Command::CancelOrders {
            command: 871,
            ids: vec!["old-live".into()],
        }),
    );
    let output = release(&mut e, NOW + 10., output);
    assert_eq!(
        e.store().order("old-live").unwrap().status,
        OrderStatus::Canceling
    );
    let failed = reply(&mut e, NOW + 11., &wire(&output, "/g-orders"), 500, "{}");
    assert_eq!(terminal(&failed, 871).len(), 1);
    let next = at(&mut e, NOW + 12., Event::Command(unsupported(872)));
    assert_eq!(
        e.store().order("old-live").unwrap().status,
        OrderStatus::Open
    );
    assert!(terminal(&next, 871).is_empty());
    assert_refusal(
        &next
            .iter()
            .filter(|a| matches!(a, Action::Notice(_)))
            .cloned()
            .collect::<Vec<_>>(),
        872,
        "Phemex command is unsupported",
        Some(ErrorKind::InvalidInput),
    );
}
fn create_ack(request: &HttpRequest, id: &str) -> String {
    json!({"code":0,"data":{"orderID":id,"clOrdID":value(request,"clOrdID"),"symbol":"BTCUSDT","side":"Buy","posSide":"Merged","orderType":"Limit","orderQtyRq":value(request,"orderQtyRq"),"priceRp":"69000","ordStatus":"New","execStatus":"New","timeInForce":"GoodTillCancel"}}).to_string()
}
#[test]
fn retirement_mid_split_keeps_accepted_prefix_and_fences_only_released_lost_lot() {
    let (mut e, _) = started();
    let mut order = opt("unused");
    order.client_order_id = None;
    order.amount = 1.1;
    let output = at(
        &mut e,
        NOW + 10.,
        Event::Command(Command::PlaceOrders {
            command: 873,
            orders: vec![order],
        }),
    );
    let output = release(&mut e, NOW + 10., output);
    let first = wire(&output, "g-orders/create");
    let next = reply(
        &mut e,
        NOW + 11.,
        &first,
        200,
        &create_ack(&first, "accepted-prefix"),
    );
    let next = release(&mut e, NOW + 11., next);
    let second = wire(&next, "g-orders/create");
    let failed = reply(&mut e, NOW + 12., &second, 408, "{}");
    let results = terminal(&failed, 873);
    let error = results[0].as_ref().unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["accepted-prefix"]
    );
    let next = at(
        &mut e,
        NOW + 13.,
        Event::Command(Command::CancelSymbolOrders {
            command: 874,
            symbol: "BTCUSDT".into(),
        }),
    );
    let next = release(&mut e, NOW + 13., next);
    assert_eq!(
        value(&wire(&next, "/g-orders"), "orderID"),
        "accepted-prefix"
    );
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|order| order.status != OrderStatus::Creating)
    );
    let warnings: Vec<_> = next
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) => Some(event.message.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        warnings
            .iter()
            .any(|message| message.contains(&value(&second, "clOrdID")))
    );
    assert!(
        warnings
            .iter()
            .all(|message| !message.contains(&value(&first, "clOrdID")))
    );
    assert!(terminal(&next, 873).is_empty());
}
#[test]
fn native_write_error_messages_keep_http_body_and_codeless_error_text() {
    for (status, body, expected) in [
        (
            400,
            r#"{"code":11001,"msg":"balance","data":null}"#,
            r#"HTTP 400: {"code":11001,"msg":"balance","data":null}"#,
        ),
        (
            200,
            r#"{"error":{"message":"bad"},"msg":"native failure"}"#,
            "Phemex -1 native failure",
        ),
    ] {
        let (mut e, _) = started();
        let output = at(&mut e, NOW + 10., Event::Command(placement(875)));
        let output = release(&mut e, NOW + 10., output);
        let output = reply(
            &mut e,
            NOW + 11.,
            &wire(&output, "g-orders/create"),
            status,
            body,
        );
        let result = terminal(&output, 875);
        assert_eq!(result.len(), 1);
        assert!(result[0].as_ref().unwrap_err().message.contains(expected));
    }
}

#[test]
fn income_later_provider_loss_preserves_prefix_and_first_page_loss_stays_typed() {
    for accepted in [false, true] {
        let mut e = seeded(true, true);
        if !accepted {
            e.set_signer(None).unwrap();
        }
        let output = ask(&mut e, NOW, 880, income());
        let mut output = release(&mut e, NOW, output);
        if accepted {
            let first = wire(&output, "tradeAccountDetail");
            output = reply(
                &mut e,
                NOW + 1.,
                &first,
                200,
                r#"[{"typeDesc":"REALIZED_PNL","amountRv":"1","balanceRv":"101","createTime":500,"currency":"USDT"}]"#,
            );
            let mut count = 0;
            while !output.iter().any(|action| matches!(action,Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare{request,..} if request.url.contains("currency=USDC")))) {
                count+=1;assert!(count<=4);
                output=release(&mut e,NOW+1.,output);
                let next=wire(&output,"tradeAccountDetail");
                output=reply(&mut e,NOW+1.,&next,200,"[]");
            }
            e.set_signer(None).unwrap();
            output = release(&mut e, NOW + 2., output);
        }
        let attempt = output
            .iter()
            .find_map(|action| match action {
                Action::Sign { attempt, .. } => Some(*attempt),
                _ => None,
            })
            .expect("real delegated signing refusal");
        output.extend(at(
            &mut e,
            if accepted { NOW + 2. } else { NOW },
            Event::Signed {
                attempt,
                result: Err(safe_cex_engine::signing::SignError::Refused),
            },
        ));
        let results = terminal(&output, 880);
        assert_eq!(results.len(), 1);
        let markers=output.iter().filter(|action| matches!(action,Action::Notice(Notice::Event(event)) if event.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE && event.level==safe_cex_engine::Level::Warn && event.message.contains("INCOMPLETE"))).count();
        if accepted {
            let rows: Value = serde_json::from_str(results[0].as_ref().unwrap()).unwrap();
            assert_eq!(rows.as_array().unwrap().len(), 1);
            assert_eq!(rows[0]["income"], 1.);
            assert_eq!(markers, 1);
        } else {
            assert_eq!(
                results[0].as_ref().unwrap_err().kind,
                Some(ErrorKind::InvalidInput)
            );
            assert_eq!(
                results[0].as_ref().unwrap_err().outcome,
                Some(Outcome::NotSent)
            );
            assert_eq!(markers, 0);
        }
        assert!(requests(&output).is_empty());
    }
}
