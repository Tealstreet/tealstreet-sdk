#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
#[allow(dead_code)]
#[path = "support/rest_host.rs"]
mod native;
#[path = "support/spot_account_host.rs"]
mod support;
use cache::{N, arr, eq, object, s};
use native::{assert_value, decode};
use safe_cex_venue_hyperliquid::ws_ping_lifecycle::WsPingHost;
use safe_cex_venue_hyperliquid::{helper_tasks::HelperObserver, spot_account_reads::*};
use serde_json::Value;
use std::{
    future::Future,
    pin::Pin,
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
use support::{Host, flag, get, input};
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .env("TZ", "UTC")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/spot_account_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["sourceHashes"].as_object().unwrap().len(),
            6,
            "ACCOUNT independent source floor"
        );
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            62,
            "ACCOUNT independent case floor through61"
        );
        v
    })
}
fn complete(h: &Host, mut future: HelperObserver<N, N>) -> Result<N, N> {
    h.drain();
    let mut cx = Context::from_waker(Waker::noop());
    match Pin::new(&mut future).poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("ACCOUNT observer unresolved"),
    }
}
fn check(family: &str, count: usize) {
    let rows = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        count,
        "ACCOUNT independent family floor {family}"
    );
    for row in rows {
        let i = input(row);
        let mut h = Host::new(
            i.clone(),
            decode(&oracle()["enums"]),
            decode(&oracle()["endpoints"]),
        );
        let e = h.exchange.clone();
        let label = format!("ACCOUNT native {family} {}", row["name"]);
        let mut pending = None;
        let mut concurrent = None;
        let mut immediate = None;
        let mut native_pending = None;
        let mut flight_alias = false;
        let mut outer_distinct = true;
        match family {
            "validate" => pending = Some(validate_account(h.clone(), e.clone())),
            "state" => pending = Some(fetch_spot_clearinghouse_state(h.clone(), e.clone())),
            "refresh" => {
                let first = h
                    .call(&N::Builtin("refresh".into()), &e, &[get(&i, "force")])
                    .unwrap();
                let flight = get(&e, "spotBalanceRefreshInFlight");
                if flag(&i, "concurrent") {
                    let second = h
                        .call(&N::Builtin("refresh".into()), &e, &[N::B(true)])
                        .unwrap();
                    flight_alias = eq(&flight, &get(&e, "spotBalanceRefreshInFlight"));
                    outer_distinct = !eq(&first, &second);
                    concurrent = Some(second);
                }
                native_pending = Some(first);
            }
            "start" => pending = Some(start(h.clone(), e.clone())),
            "tick" => pending = Some(tick(h.clone(), e.clone())),
            "ohlcv" => {
                let opts = if matches!(get(&i, "opts"), N::U) {
                    object(vec![
                        ("symbol", s("BTC/USDC")),
                        ("interval", s("1m")),
                        ("to", N::Num(100000.)),
                        ("limit", N::Num(2.)),
                    ])
                } else {
                    get(&i, "opts")
                };
                pending = Some(fetch_ohlcv(h.clone(), e.clone(), opts));
            }
            "orders" => pending = Some(fetch_orders(h.clone(), e.clone(), get(&i, "symbols"))),
            "fake" => {
                let markets = h.markets.clone();
                immediate = Some(generate_fake_positions_from_markets(&mut h, &e, &markets))
            }
            "positions" => {
                pending = Some(fetch_positions(h.clone(), e.clone(), get(&i, "symbols")))
            }
            _ => panic!("family"),
        }
        assert_value(
            &h.effects(),
            &row["prefix"],
            &format!("ACCOUNT native eager prefix {family} {}", row["name"]),
        );
        let result = if let Some(first) = native_pending {
            h.drain();
            let result = h.native_result(&first);
            if let Some(second) = concurrent {
                result.and_then(|a| h.native_result(&second).map(|b| arr(vec![a, b])))
            } else {
                result
            }
        } else if let Some(pending) = pending {
            complete(&h, pending)
        } else {
            immediate.unwrap()
        };
        assert_eq!(
            result.is_err(),
            row["rejected"].as_bool().unwrap(),
            "{label}: explicit rejected discriminator"
        );
        let result_value = match &result {
            Ok(v) => {
                assert_value(v, &row["result"], &label);
                v.clone()
            }
            Err(v) => {
                assert_value(v, &row["error"], &label);
                N::U
            }
        };
        if flag(&i, "fireLoop") {
            h.fire_loop();
        }
        if family == "orders" {
            let second =
                complete(&h, fetch_orders(h.clone(), e.clone(), get(&i, "symbols"))).unwrap();
            h.mark_orders_fresh(!eq(&result_value, &second));
        }
        assert_value(&h.effects(), &row["effects"], &label);
        assert_value(
            &h.final_state(flight_alias, outer_distinct),
            &row["state"],
            &label,
        );
        assert_value(&h.aliases(family, &result_value), &row["aliases"], &label);
    }
}
#[test]
fn account_validation() {
    check("validate", 6)
}
#[test]
fn spot_state_read() {
    check("state", 6)
}
#[test]
fn balance_refresh() {
    check("refresh", 11)
}
#[test]
fn account_start() {
    check("start", 7)
}
#[test]
fn account_tick() {
    check("tick", 6)
}
#[test]
fn candle_reads() {
    check("ohlcv", 12)
}
#[test]
fn empty_orders() {
    check("orders", 3)
}
#[test]
fn fake_positions() {
    check("fake", 5)
}
#[test]
fn position_reads() {
    check("positions", 6)
}

#[test]
fn dropped_refresh_observer_continues() {
    let row = oracle()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["family"] == "refresh" && r["name"] == "ordinary")
        .unwrap();
    let h = Host::new(
        input(row),
        decode(&oracle()["enums"]),
        decode(&oracle()["endpoints"]),
    );
    let future = refresh_spot_balance(h.clone(), h.exchange.clone(), N::U);
    assert_value(
        &h.effects(),
        &row["prefix"],
        "ACCOUNT dropped refresh native prefix",
    );
    drop(future);
    h.drain();
    assert_value(
        &h.effects(),
        &row["effects"],
        "ACCOUNT dropped refresh native effects",
    );
    assert_value(
        &h.final_state(false, true),
        &row["state"],
        "ACCOUNT dropped refresh native state",
    );
}
