//! Spot markets merged into the perp account (TS `enableSpotMarkets`).
//! Documented spot metadata shape: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint/spot
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use serde_json::Value;
use support::{META, STATE, kind, message, reply};

/// UBTC is Unit-bridged (displays as BTC); UPUMP keeps its prefix because a native
/// PUMP token exists; HYPE quotes in USDH.
const SPOT_META: &str = r#"[{"tokens":[{"name":"USDC","index":0,"szDecimals":8,"fullName":null},{"name":"UBTC","index":1,"szDecimals":5,"fullName":"Unit Bitcoin"},{"name":"UPUMP","index":2,"szDecimals":0,"fullName":"Unit Pump Fun"},{"name":"PUMP","index":3,"szDecimals":0,"fullName":"Pump"},{"name":"HYPE","index":4,"szDecimals":2,"fullName":"Hyperliquid"},{"name":"USDH","index":5,"szDecimals":2,"fullName":"USDH"}],"universe":[{"name":"@1","tokens":[1,0],"index":1},{"name":"@2","tokens":[2,0],"index":2},{"name":"@4","tokens":[4,5],"index":4}]},[{"coin":"@4","midPx":"30","markPx":"30.1","prevDayPx":"29","dayNtlVlm":"300"},{"coin":"@1","midPx":"60000","markPx":"60010","prevDayPx":"59000","dayNtlVlm":"1000","oraclePx":"1","funding":"0.1"},{"coin":"@2","midPx":"0.01","markPx":"0.01"}]]"#;
const SPOT_STATE: &str = r#"{"balances":[{"coin":"USDC","token":0,"total":"500","hold":"0"},{"coin":"UBTC","token":1,"total":"0.5","hold":"0.1"},{"coin":"HYPE","token":4,"total":"10","hold":"0"}]}"#;
const KEY: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";

struct Run {
    e: Engine<Hyperliquid>,
    socket: SocketId,
    reads: Vec<String>,
    sends: Vec<String>,
    logs: Vec<String>,
    timers: Vec<(safe_cex_engine::TimerId, f64)>,
}

fn options(spot: bool) -> Options {
    Options {
        key: Some(KEY.into()),
        secret: Some("0x0123456789012345678901234567890123456789012345678901234567890123".into()),
        application_id: Some(KEY.into()),
        enable_spot_markets: spot,
        ..Options::default()
    }
}

/// Answers every read; `spot_meta` is the body for spotMetaAndAssetCtxs (`None`: HTTP 500).
fn run(spot: bool, spot_meta: Option<&str>, spot_first: bool) -> Run {
    let mut e = safe_cex_engine::test_support::seeded_engine(Hyperliquid::new(options(spot)));
    e.start(1000.0);
    let (mut socket, mut reads, mut sends) = (None, Vec::new(), Vec::new());
    let (mut logs, mut timers) = (Vec::new(), Vec::new());
    for _ in 0..20 {
        let mut actions = prepared_now(&mut e).into_iter().collect::<Vec<_>>();
        if actions.is_empty() {
            break;
        }
        if spot_first {
            actions.sort_by_key(
                |a| !matches!(a, Action::Http(r) if kind(r) == "spotMetaAndAssetCtxs"),
            );
        }
        for a in actions {
            match a {
                Action::Http(r) => {
                    let k = kind(&r);
                    reads.push(k.clone());
                    let body = match k.as_str() {
                        "spotMetaAndAssetCtxs" => match spot_meta {
                            Some(body) => body.to_owned(),
                            None => {
                                reply(&mut e, &r, 500, "down");
                                continue;
                            }
                        },
                        "metaAndAssetCtxs" => META.into(),
                        "perpDexs" => "[null]".into(),
                        "userAbstraction" => "\"disabled\"".into(),
                        "maxBuilderFee" => "30".into(),
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#.into(),
                        "clearinghouseState" => STATE.into(),
                        "frontendOpenOrders" => r#"[{"coin":"@1","oid":5,"side":"B","limitPx":"59000","origSz":"0.2","sz":"0.2","reduceOnly":false,"orderType":"Limit","tif":"Gtc","timestamp":1},{"coin":"BTC","oid":6,"side":"A","limitPx":"110","origSz":"1","sz":"1","reduceOnly":false,"orderType":"Limit","tif":"Gtc","timestamp":1}]"#.into(),
                        "spotClearinghouseState" => SPOT_STATE.into(),
                        other => panic!("unexpected read {other}"),
                    };
                    reply(&mut e, &r, 200, &body);
                }
                Action::WsConnect { socket: s, .. } => {
                    socket = Some(s);
                    e.handle(1000.0, Event::WsOpened { socket: s });
                }
                Action::WsSend { text, .. } => sends.push(text),
                Action::Notice(Notice::Event(event)) => logs.push(event.message),
                Action::SetTimer { timer, at_ms } => {
                    timers.push((timer, at_ms));
                    if at_ms == 1000.0 {
                        e.handle(at_ms, Event::TimerFired { timer });
                    }
                }
                _ => {}
            }
        }
    }
    Run {
        e,
        socket: socket.unwrap(),
        reads,
        sends,
        logs,
        timers,
    }
}

fn symbols(e: &Engine<Hyperliquid>) -> Vec<String> {
    e.store()
        .markets()
        .iter()
        .map(|m| m.symbol.clone())
        .collect()
}

#[test]
fn spot_pairs_list_after_perps_with_the_unit_alias_and_real_quotes() {
    // Even when the spot read answers first, TS lists perps before spot.
    let r = run(true, Some(SPOT_META), true);
    assert_eq!(
        symbols(&r.e),
        [
            "BTC-PERP",
            "ETH-PERP",
            "UBTC/USDC",
            "UPUMP/USDC",
            "HYPE/USDH"
        ]
    );
    let m = |s: &str| r.e.store().market(s).unwrap().clone();
    let ubtc = m("UBTC/USDC");
    assert_eq!(ubtc.id, "@1");
    assert_eq!(ubtc.base, "BTC");
    assert_eq!(ubtc.pretty_symbol.as_deref(), Some("BTC/USDC"));
    assert_eq!(ubtc.is_spot, Some(true));
    assert_eq!(ubtc.limits.leverage.max, 1.0);
    let upump = m("UPUMP/USDC");
    assert_eq!((upump.base.as_str(), upump.pretty_symbol), ("UPUMP", None));
    assert_eq!(m("HYPE/USDH").quote, "USDH");
    // TS `info.hlTokenName`/`hlTokenFullName`: the venue token behind each spot pair.
    let tokens = r.e.store().underlying_tokens().unwrap().to_vec();
    let token = |symbol: &str| {
        let index = r.e.store().market_index(symbol).unwrap();
        let row = tokens.iter().find(|t| t.market == index).unwrap();
        (row.name.clone(), row.full_name.clone())
    };
    assert_eq!(
        token("UBTC/USDC"),
        ("UBTC".into(), Some("Unit Bitcoin".into()))
    );
    assert_eq!(
        token("HYPE/USDH"),
        ("HYPE".into(), Some("Hyperliquid".into()))
    );
    assert_eq!(tokens.len(), 3, "spot pairs only");
    let t = r.e.store().ticker("UBTC/USDC").unwrap();
    assert_eq!(
        (t.last, t.mark, t.index, t.funding_rate),
        (60000.0, 60010.0, 0.0, 0.0)
    );
    // TS gives spot pairs no position, so no leverage row; and no activeAssetData.
    assert!(r.e.store().setting("UBTC/USDC").is_none());
    assert!(r.e.store().setting("BTC-PERP").is_some());
    assert!(
        !r.sends
            .iter()
            .any(|s| s.contains("activeAssetData") && s.contains("\"@1\""))
    );
    assert!(
        r.sends
            .iter()
            .any(|s| s.contains("\"activeAssetData\"") && s.contains("\"BTC\""))
    );
}

#[test]
fn a_failed_spot_read_warns_retries_and_never_holds_back_perps() {
    let mut r = run(true, None, false);
    assert_eq!(symbols(&r.e), ["BTC-PERP", "ETH-PERP"]);
    assert!(r.e.store().loaded().markets);
    assert!(
        r.logs
            .iter()
            .any(|m| m.starts_with("Failed to load Hyperliquid spot markets; retrying"))
    );
    // The retry fires on the discovery backoff and merges the spot pairs.
    let mut timers = r.timers.clone();
    timers.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut retry = None;
    for (timer, at) in timers {
        r.e.handle(at, Event::TimerFired { timer });
        retry = prepared_now(&mut r.e).into_iter().find_map(|a| match a {
            Action::Http(req) if kind(&req) == "spotMetaAndAssetCtxs" => Some(req),
            _ => None,
        });
        if retry.is_some() {
            break;
        }
    }
    let retry = retry.expect("spot discovery retried on the backoff");
    reply(&mut r.e, &retry, 200, SPOT_META);
    assert_eq!(
        symbols(&r.e),
        [
            "BTC-PERP",
            "ETH-PERP",
            "UBTC/USDC",
            "UPUMP/USDC",
            "HYPE/USDH"
        ]
    );
    // A late catalogue still adds no leverage rows for spot pairs.
    assert!(r.e.store().setting("UBTC/USDC").is_none());
}

#[test]
fn spot_orders_are_kept_and_cancel_by_their_spot_asset_index() {
    let mut r = run(true, Some(SPOT_META), false);
    let orders = r.e.store().orders().to_vec();
    assert!(
        orders
            .iter()
            .any(|o| o.id == "5" && o.symbol == "UBTC/USDC")
    );
    assert!(orders.iter().any(|o| o.id == "6" && o.symbol == "BTC-PERP"));
    r.e.handle(
        2000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: "UBTC/USDC".into(),
        }),
    );
    let write = exchange_write(&mut r.e);
    let body: Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["action"]["type"], "cancel");
    assert_eq!(
        body["action"]["cancels"],
        serde_json::json!([{"a": 10001, "o": 5}])
    );
}

#[test]
fn spot_holdings_publish_as_coins_while_standard_balance_stays_perp_only() {
    let r = run(true, Some(SPOT_META), false);
    assert!(r.sends.iter().any(|s| s.contains("\"spotState\"")));
    assert!(r.reads.contains(&"spotClearinghouseState".to_owned()));
    let balance = r.e.store().balance().clone();
    assert_eq!(balance.total, 1000.0);
    let coins = r.e.store().coin_balances().unwrap().to_vec();
    let coin = |c: &str| coins.iter().find(|b| b.base == c).unwrap().clone();
    assert_eq!(coin("USDC").notional, 500.0);
    // UBTC values off its spot pair's mark, HYPE off HYPE/USDH.
    assert_eq!(coin("UBTC").notional, 0.5 * 60010.0);
    assert_eq!(coin("HYPE").notional, 10.0 * 30.1);
}

#[test]
fn spot_fills_move_holdings_never_positions() {
    let mut r = run(true, Some(SPOT_META), false);
    let before = r.e.store().positions().to_vec();
    prepared_now(&mut r.e).into_iter().for_each(drop);
    message(
        &mut r.e,
        r.socket,
        2000.0,
        r#"{"channel":"userFills","data":{"isSnapshot":false,"fills":[{"coin":"@1","side":"B","px":"60000","sz":"0.1","time":2000,"tid":71,"oid":5,"dir":"Buy","startPosition":"0.5","closedPnl":"0","fee":"0.01"}]}}"#,
    );
    assert_eq!(r.e.store().positions(), before.as_slice());
    assert!(
        r.e.store()
            .positions()
            .iter()
            .all(|p| p.symbol != "UBTC/USDC")
    );
}

#[test]
fn spot_orders_use_spot_prices_never_reduce_only_and_ignore_leverage() {
    let mut r = run(true, Some(SPOT_META), false);
    prepared_now(&mut r.e).into_iter().for_each(drop);
    r.e.handle(
        2000.0,
        Event::Command(Command::SetLeverage {
            command: 3,
            symbol: "UBTC/USDC".into(),
            leverage: 5.0,
            side: None,
        }),
    );
    let done = prepared_now(&mut r.e).into_iter().collect::<Vec<_>>();
    assert!(!done.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(done.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone {
            command: 3,
            result: _
        })
    )));
    let opts: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"HYPE/USDH","type":"limit","side":"sell","amount":1,"price":30.123456789,"reduceOnly":true}"#,
    )
    .unwrap();
    r.e.handle(
        2000.0,
        Event::Command(Command::PlaceOrders {
            command: 4,
            orders: vec![opts],
        }),
    );
    let write = exchange_write(&mut r.e);
    let body: Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
    let wire = &body["action"]["orders"][0];
    assert_eq!(wire["a"], 10004);
    assert_eq!(wire["r"], false);
    // Spot allows 8 - szDecimals price decimals (perps 6 - szDecimals).
    assert_eq!(wire["p"], "30.123");
}

#[test]
fn a_spot_disabled_account_reads_and_subscribes_exactly_as_before() {
    let r = run(false, None, false);
    assert!(!r.reads.iter().any(|k| k.contains("spot")));
    assert!(!r.sends.iter().any(|s| s.contains("spotState")));
    assert_eq!(symbols(&r.e), ["BTC-PERP", "ETH-PERP"]);
    assert!(r.e.store().coin_balances().is_none());
    // Same reads and subscriptions as the shared harness, which panics on any spot read.
    let (_, _, baseline) =
        support::ready_with_policy("disabled", safe_cex_engine::PublicFeedPolicy::Direct);
    let strip = |sends: &[String]| {
        sends
            .iter()
            .filter(|s| !s.contains("\"dex\""))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(strip(&r.sends), strip(&baseline));
}

fn exchange_write(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    prepared_now(e)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("an /exchange write")
}

#[test]
fn spot_orders_read_before_the_spot_catalogue_never_hide_perp_orders() {
    // Spot discovery fails; the open-order snapshot still lists a spot row.
    let r = run(true, None, false);
    let orders = r.e.store().orders().to_vec();
    assert!(orders.iter().any(|o| o.id == "6" && o.symbol == "BTC-PERP"));
    assert!(!orders.iter().any(|o| o.id == "5"));
    // Holdings still publish, and the perp-only balance is not held back.
    assert_eq!(r.e.store().balance().total, 1000.0);
    assert!(
        !r.logs
            .iter()
            .any(|m| m.contains("collateral valuation unavailable"))
    );
}
