//! Concrete socket pump witnesses; source topic/lifecycle helpers alone cannot pass these.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_exchange::{socket_lifecycle::SocketSlot, websocket::WsEvent};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Market};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn market(symbol: &str, quote: &str, option: bool) -> Value {
    let mut value = json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":quote,"active":true,
        "precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":100},"leverage":{"min":1,"max":100}}});
    if option {
        value["option"] = json!({"underlying":"BTC","expiry":1793952000000.0,"strike":100.0,"right":"call","style":"european","settlement":"cash","premiumAsset":"USDC","settleAsset":"USDC"});
    }
    value
}
fn markets() -> Vec<Value> {
    (0..14)
        .map(|i| {
            market(
                &format!("M{i}"),
                if i % 3 == 0 {
                    "BTC"
                } else if i % 3 == 1 {
                    "USDT"
                } else {
                    "USDC"
                },
                i == 13,
            )
        })
        .collect()
}
fn configure(h: &support::Harness, markets: &[Value]) {
    let markets: Vec<Market> = serde_json::from_value(json!(markets)).unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}
fn wire_frames(h: &support::Harness) -> Vec<Value> {
    h.runtime.sockets.borrow()[0]
        .1
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn subscriptions(h: &support::Harness) -> Vec<Vec<String>> {
    wire_frames(h)
        .into_iter()
        .filter(|v| v["op"] == "subscribe")
        .map(|v| serde_json::from_value(v["args"].clone()).unwrap())
        .collect()
}
fn setup(
    name: ExchangeName,
    category: &str,
) -> (
    support::Harness,
    std::rc::Rc<safe_cex_exchange::websocket::BaseWebSocket>,
) {
    let mut options = support::options();
    options.enable_inverse_markets = Some(category == "inverse");
    let h = support::harness_as(name, options, |_| {});
    configure(&h, &markets());
    let socket = h
        .venue()
        .base()
        .get_socket(if category == "inverse" {
            SocketSlot::PublicSecondary
        } else {
            SocketSlot::Public
        })
        .unwrap();
    (h, socket)
}
#[test]
fn primary_public_topics_stay_category_wide_and_follow_market_refreshes() {
    for category in ["linear", "inverse"] {
        let (h, socket) = setup(ExchangeName::Bybit, category);
        h.venue()
            .base()
            .set_active_ticker_symbols(&["M1".into()])
            .unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        h.advance(100.);
        let initial = if category == "linear" {
            vec!["M1", "M2", "M4", "M5", "M7", "M8", "M10", "M11"]
        } else {
            vec!["M0", "M3", "M6", "M9", "M12"]
        };
        assert_eq!(
            subscriptions(&h),
            vec![
                initial
                    .into_iter()
                    .map(|s| format!("tickers.{s}"))
                    .collect::<Vec<_>>()
            ],
            "primary initial category policy"
        );
        let before = wire_frames(&h);
        h.venue()
            .base()
            .set_active_ticker_symbols(&["M2".into()])
            .unwrap();
        assert_eq!(
            wire_frames(&h),
            before,
            "primary has no active-symbol callback"
        );
        h.venue().base().emit("markets:changed",safe_cex_exchange::js_runtime::value::from_json(&json!({"added":[market("NEW-L","USDT",false),market("NEW-I","BTC",false),market("OPT","USDT",true)],"removed":[]}))).unwrap();
        assert_eq!(
            subscriptions(&h).last().unwrap(),
            &[if category == "linear" {
                "tickers.NEW-L"
            } else {
                "tickers.NEW-I"
            }],
            "market callback runs synchronously through first send"
        );
        h.advance(100.);
        assert!(h.runtime.reports.borrow().is_empty());
    }
}
#[test]
fn spot_public_topics_use_active_selection_and_keep_prior_subscriptions() {
    let (h, socket) = setup(ExchangeName::Bybitspot, "spot");
    // Source constructor listener syncs topics before any WebSocket exists.
    h.venue()
        .base()
        .set_active_ticker_symbols(&["M1".into()])
        .unwrap();
    h.venue()
        .base()
        .set_active_ticker_symbols(&["M2".into()])
        .unwrap();
    socket.connect_and_subscribe(false).unwrap();
    h.runtime.pump();
    h.advance(100.);
    assert_eq!(
        subscriptions(&h),
        [vec!["tickers.M1", "tickers.M2"]],
        "Spot initial active topic union"
    );
    h.venue()
        .base()
        .set_active_ticker_symbols(&["M3".into()])
        .unwrap();
    assert_eq!(
        subscriptions(&h).last().unwrap(),
        &["tickers.M3"],
        "live active-symbol callback"
    );
    let before = wire_frames(&h).len();
    h.runtime.sockets.borrow()[0].1.closed.set(true);
    h.venue()
        .base()
        .set_active_ticker_symbols(&["M4".into()])
        .unwrap();
    assert_eq!(
        wire_frames(&h).len(),
        before,
        "offline change retains topics without sending"
    );
    h.runtime.sockets.borrow()[0].1.closed.set(false);
    h.runtime.sockets.borrow()[0]
        .1
        .events
        .borrow_mut()
        .push_back(WsEvent::Open);
    h.runtime.pump();
    h.advance(100.);
    assert_eq!(
        subscriptions(&h).last().unwrap(),
        &["tickers.M1", "tickers.M2", "tickers.M3", "tickers.M4"],
        "reopen retains offline topics"
    );
    h.venue()
        .base()
        .emit(
            "markets:changed",
            safe_cex_exchange::js_runtime::value::from_json(&json!({"added":[market("NEW","USDT",false),market("NEW","USDT",false)],"removed":[]})),
        )
        .unwrap();
    assert_eq!(
        subscriptions(&h).last().unwrap(),
        &["tickers.NEW", "tickers.NEW"],
        "market callback is replaced on reopen and preserves duplicate added rows"
    );
    h.advance(100.);
    socket.dispose();
    let before = wire_frames(&h);
    h.venue()
        .base()
        .set_active_ticker_symbols(&["M5".into()])
        .unwrap();
    assert_eq!(wire_frames(&h), before, "dispose removes active listener");
    assert!(h.runtime.reports.borrow().is_empty());
}
fn cases() -> Vec<Value> {
    let mut cases = vec![];
    let active = [
        json!([]),
        json!(["M0"]),
        json!(["M1"]),
        json!(["M1", "M2"]),
        json!(["UNKNOWN"]),
        json!(["M13"]),
        json!([" M3 ", "M3", ""]),
        json!(["M12", "M0"]),
    ];
    for category in ["linear", "inverse", "spot"] {
        for seed in active.clone() {
            let added: Vec<_> = (0..23)
                .map(|i| {
                    market(
                        &format!("N{i}"),
                        if i % 2 == 0 { "USDT" } else { "BTC" },
                        i == 22,
                    )
                })
                .collect();
            cases.push(
                json!({"category":category,"active":seed,"markets":markets(),"actions":[
                    {"op":"active","symbols":["M1"]},{"op":"open"},
                    {"op":"active","symbols":["M2"]},{"op":"offline"},
                    {"op":"active","symbols":["M3"]},{"op":"open"},
                    {"op":"markets","added":added},{"op":"markets","added":[]},
                    {"op":"active","symbols":[]},{"op":"open"},
                    {"op":"markets","added":[market("N0","USDT",false),market("N0","USDT",false)]},
                    {"op":"dispose"},{"op":"active","symbols":["M4"]},
                    {"op":"parentDispose"},{"op":"markets","added":[market("LATE","USDT",false)]}
                ]}),
            );
        }
    }
    assert_eq!(cases.len(), 24);
    cases
}
fn native(input: &Value) -> Value {
    let category = input["category"].as_str().unwrap();
    let mut opts = support::options();
    opts.enable_inverse_markets = Some(category == "inverse");
    opts.tuning =
        Some(serde_json::from_value(json!({"activeTickerSymbols":input["active"]})).unwrap());
    let h = support::harness_as(
        if category == "spot" {
            ExchangeName::Bybitspot
        } else {
            ExchangeName::Bybit
        },
        opts,
        |_| {},
    );
    configure(&h, input["markets"].as_array().unwrap());
    let socket = h
        .venue()
        .base()
        .get_socket(if category == "inverse" {
            SocketSlot::PublicSecondary
        } else {
            SocketSlot::Public
        })
        .unwrap();
    let mut snapshots = vec![];
    for action in input["actions"].as_array().unwrap() {
        match action["op"].as_str().unwrap() {
            "active" => h
                .venue()
                .base()
                .set_active_ticker_symbols(
                    &serde_json::from_value::<Vec<String>>(action["symbols"].clone()).unwrap(),
                )
                .unwrap(),
            "open" => {
                if h.runtime.sockets.borrow().is_empty() {
                    socket.connect_and_subscribe(false).unwrap();
                    h.runtime.pump();
                } else {
                    let ws = h.runtime.sockets.borrow()[0].1.clone();
                    ws.closed.set(false);
                    ws.events.borrow_mut().push_back(WsEvent::Open);
                    h.runtime.pump();
                }
            }
            "offline" => h.runtime.sockets.borrow()[0].1.closed.set(true),
            "markets" => h
                .venue()
                .base()
                .emit(
                    "markets:changed",
                    safe_cex_exchange::js_runtime::value::from_json(
                        &json!({"added":action["added"],"removed":[]}),
                    ),
                )
                .unwrap(),
            "dispose" => socket.dispose(),
            "parentDispose" => h.venue().base().set_disposed_flag(true),
            other => panic!("{other}"),
        }
        h.advance(150.);
        snapshots.push(if h.runtime.sockets.borrow().is_empty() {
            vec![]
        } else {
            wire_frames(&h)
        });
    }
    assert_eq!(snapshots.len(), 15);
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    json!(snapshots)
}
#[test]
fn production_public_callbacks_match_twenty_four_actual_source_compositions() {
    let cases = cases();
    let mut child = Command::new("node")
        .arg(format!(
            "{}/tests/public-topic-binding-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&cases).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 24);
    let mut executed = 0;
    for (i, input) in cases.iter().enumerate() {
        let actual = native(input);
        assert_eq!(actual, expected[i], "public composition {i}: {input}");
        executed += 1;
    }
    assert_eq!(executed, 24);
}

#[test]
fn disposal_drops_pending_public_subscription_tails_and_weak_venue_references() {
    for (name, category) in [
        (ExchangeName::Bybitspot, "spot"),
        (ExchangeName::Bybitinverse, "inverse"),
    ] {
        for phase in ["open", "markets"] {
            let (h, socket) = setup(name.clone(), category);
            if category == "inverse" {
                let inverse_markets: Vec<_> = (0..14)
                    .map(|i| market(&format!("I{i}USD"), "USD", false))
                    .collect();
                configure(&h, &inverse_markets);
            }
            socket.connect_and_subscribe(false).unwrap();
            h.runtime.pump();
            if phase == "markets" {
                h.advance(100.);
                let added: Vec<_> = (0..23)
                    .map(|i| market(&format!("C{i}"), "USDT", false))
                    .collect();
                h.venue()
                    .base()
                    .emit(
                        "markets:changed",
                        safe_cex_exchange::js_runtime::value::from_json(
                            &json!({"added":added,"removed":[]}),
                        ),
                    )
                    .unwrap();
                assert_eq!(subscriptions(&h).last().unwrap().len(), 10);
            } else {
                assert_eq!(subscriptions(&h).len(), 1);
                assert_eq!(subscriptions(&h)[0].len(), 10);
                assert_eq!(wire_frames(&h)[1], json!({"op":"ping"}));
            }
            let before = wire_frames(&h);
            socket.dispose();
            h.advance(200.);
            assert_eq!(wire_frames(&h), before, "{phase} pending tail cancelled");
            assert!(h.runtime.reports.borrow().is_empty());
            let venue = std::rc::Rc::downgrade(&h.exchange);
            drop(h);
            assert!(
                venue.upgrade().is_none(),
                "{phase} listener/tail retains no venue"
            );
            drop(socket);
        }
    }
}

#[test]
fn market_additions_wait_five_ms_between_real_socket_chunks() {
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-market-delay-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let profiles: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        profiles.len(),
        2,
        "both maintained Spot/Inverse methods execute"
    );
    for (name, category) in [
        (ExchangeName::Bybitspot, "spot"),
        (ExchangeName::Bybitinverse, "inverse"),
    ] {
        let (h, socket) = setup(name.clone(), category);
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        h.advance(100.);
        h.advance(100.);
        let initial = subscriptions(&h).len();
        let added: Vec<_> = (0..23)
            .map(|i| market(&format!("DELAY{i}"), "USD", false))
            .collect();
        h.venue()
            .base()
            .emit(
                "markets:changed",
                safe_cex_exchange::js_runtime::value::from_json(
                    &json!({"added":added,"removed":[]}),
                ),
            )
            .unwrap();
        let chunks = |count: usize| {
            let actual = subscriptions(&h);
            assert_eq!(
                actual.len(),
                initial + count,
                "five-ms market boundary: {name:?}/{count}"
            );
            let expected: Vec<_> = (0..23).map(|i| format!("tickers.DELAY{i}")).collect();
            for (actual, expected) in actual[initial..].iter().zip(expected.chunks(10)) {
                assert_eq!(actual, expected, "original ordered topic chunks: {name:?}");
            }
        };
        chunks(1); // The first send is the source's synchronous prefix.
        h.runtime.pump();
        chunks(1); // Polling the continuation must not advance the clock.
        for expected in [2, 3] {
            h.runtime.now.set(h.runtime.now.get() + 4.);
            h.runtime.pump();
            chunks(expected - 1);
            h.runtime.now.set(h.runtime.now.get() + 1.);
            h.runtime.pump();
            chunks(expected);
        }
        assert!(h.runtime.reports.borrow().is_empty());
        socket.dispose();
        h.venue().base().dispose().unwrap();
    }
}
