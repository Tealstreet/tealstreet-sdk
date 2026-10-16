use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_venue_woo::{
    v3_orderbook_recovery::*,
    v3_public::{
        WooV3MappedOrderBookSnapshot, WooV3OrderBookLevel, WooV3OrderBookSnapshot,
        map_woo_v3_order_book,
    },
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/orderbook-recovery-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "RECOVERY source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value, key: &str) -> Option<JsNumberInput> {
    row[key].as_str().map(|s| js_from_str(s).unwrap())
}
fn number(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        expected["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "{label}"
    );
}
fn result<T>(actual: Result<T, String>, expected: &Value, check: impl FnOnce(T, &Value)) {
    if let Some(error) = expected.get("error") {
        assert_eq!(error["name"], "Error");
        assert_eq!(
            actual.err().unwrap(),
            error["message"].as_str().unwrap(),
            "RECOVERY native first error"
        );
    } else {
        check(actual.ok().unwrap(), &expected["ok"]);
    }
}
fn levels(actual: &[Vec<f64>], expected: &Value) {
    let rows = expected.as_array().unwrap();
    assert_eq!(actual.len(), rows.len(), "RECOVERY native levels count");
    for (v, e) in actual.iter().zip(rows) {
        assert_eq!(v.len(), 2);
        number(v[0], &e[0], "RECOVERY native price");
        number(v[1], &e[1], "RECOVERY native amount");
    }
}
fn wires(actual: &[[String; 2]], expected: &Value) {
    let rows = expected.as_array().unwrap();
    assert_eq!(actual.len(), rows.len());
    for (v, e) in actual.iter().zip(rows) {
        assert_eq!(v[0], e[0].as_str().unwrap());
        assert_eq!(v[1], e[1].as_str().unwrap());
    }
}
fn publication(actual: &WooV3RecoveredOrderBook, expected: &Value) {
    number(
        actual.snapshot_timestamp,
        &expected["snapshotTimestamp"],
        "RECOVERY native snapshot provenance",
    );
    number(
        actual.timestamp,
        &expected["timestamp"],
        "RECOVERY native current time",
    );
    for (side, key) in [(&actual.book.bids, "bids"), (&actual.book.asks, "asks")] {
        let rows = expected[key].as_array().unwrap();
        assert_eq!(
            side.len(),
            rows.len(),
            "RECOVERY native publication side count"
        );
        for (v, e) in side.iter().zip(rows) {
            number(v.price, &e["price"], "RECOVERY native book price");
            number(v.amount, &e["amount"], "RECOVERY native book amount");
            number(v.total, &e["total"], "RECOVERY native book total");
        }
    }
}
fn count(actual: usize, expected: &Value, label: &str) {
    number(actual as f64, expected, label);
}
fn recovery(actual: &WooV3OrderBookRecoveryResult, expected: &Value) {
    use WooV3OrderBookRecoveryResult::*;
    let status = expected["status"].as_str().unwrap();
    match (actual, status) {
        (Buffered { buffered_updates }, "buffered") => count(
            *buffered_updates,
            &expected["bufferedUpdates"],
            "RECOVERY native buffered count",
        ),
        (
            AwaitingJoin {
                snapshot_timestamp,
                stale_updates_discarded,
            },
            "awaiting-join",
        ) => {
            number(
                *snapshot_timestamp,
                &expected["snapshotTimestamp"],
                "RECOVERY native awaiting snapshot time",
            );
            count(
                *stale_updates_discarded,
                &expected["staleUpdatesDiscarded"],
                "RECOVERY native stale count",
            );
        }
        (
            Joined {
                snapshot_timestamp,
                update_timestamp,
                applied_updates,
                stale_updates_discarded,
                duplicates_ignored,
                publication: p,
            },
            "joined",
        ) => {
            number(
                *snapshot_timestamp,
                &expected["snapshotTimestamp"],
                "RECOVERY native joined snapshot time",
            );
            number(
                *update_timestamp,
                &expected["updateTimestamp"],
                "RECOVERY native joined update time",
            );
            count(
                *applied_updates,
                &expected["appliedUpdates"],
                "RECOVERY native applied count",
            );
            count(
                *stale_updates_discarded,
                &expected["staleUpdatesDiscarded"],
                "RECOVERY native stale count",
            );
            count(
                *duplicates_ignored,
                &expected["duplicatesIgnored"],
                "RECOVERY native duplicate count",
            );
            publication(p, &expected["publication"]);
        }
        (
            Applied {
                snapshot_timestamp,
                update_timestamp,
                publication: p,
            },
            "applied",
        ) => {
            number(
                *snapshot_timestamp,
                &expected["snapshotTimestamp"],
                "RECOVERY native applied snapshot time",
            );
            number(
                *update_timestamp,
                &expected["updateTimestamp"],
                "RECOVERY native applied update time",
            );
            publication(p, &expected["publication"]);
        }
        (
            Ignored {
                reason,
                update_timestamp,
            },
            "ignored",
        ) => {
            assert_eq!(
                reason,
                expected["reason"].as_str().unwrap(),
                "RECOVERY native ignored reason"
            );
            if let Some(t) = update_timestamp {
                number(
                    *t,
                    &expected["updateTimestamp"],
                    "RECOVERY native ignored time",
                );
            } else {
                assert!(
                    expected.get("updateTimestamp").is_none(),
                    "RECOVERY native ignored time absence"
                );
            }
        }
        (
            Recover {
                reason,
                requires_fresh_snapshot,
            },
            "recover",
        )
        | (
            Reset {
                reason,
                requires_fresh_snapshot,
            },
            "reset",
        ) => {
            assert_eq!(
                reason,
                expected["reason"].as_str().unwrap(),
                "RECOVERY native recovery reason"
            );
            assert_eq!(
                *requires_fresh_snapshot,
                expected["requiresFreshSnapshot"].as_bool().unwrap(),
                "RECOVERY native recovery flag"
            );
            assert!(*requires_fresh_snapshot);
        }
        _ => panic!("RECOVERY native result variant {status}: {actual:?}"),
    }
}
#[test]
fn decoded_native_numeric_helpers_and_validation_order() {
    let data = source();
    assert_eq!(WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES, 500);
    assert_eq!(data["records"].as_array().unwrap().len(), 14);
    for row in data["records"].as_array().unwrap() {
        let v = input(row, "input");
        assert_eq!(
            is_record(v.as_ref()),
            row["ok"].as_bool().unwrap(),
            "RECOVERY native record predicate"
        );
    }
    assert_eq!(data["timestamps"].as_array().unwrap().len(), 14);
    for row in data["timestamps"].as_array().unwrap() {
        let v = input(row, "input");
        result(
            require_positive_timestamp(v.as_ref(), "probe"),
            row,
            |v, e| number(v, e, "RECOVERY native positive timestamp"),
        );
    }
    assert_eq!(data["levels"].as_array().unwrap().len(), 24);
    for row in data["levels"].as_array().unwrap() {
        let v = input(row, "input");
        result(require_numeric_level(v.as_ref(), "probe"), row, |v, e| {
            number(
                v.numeric[0],
                &e["numeric"][0],
                "RECOVERY native level price",
            );
            number(
                v.numeric[1],
                &e["numeric"][1],
                "RECOVERY native level quantity",
            );
            assert_eq!(v.wire[0], e["wire"][0].as_str().unwrap());
            assert_eq!(v.wire[1], e["wire"][1].as_str().unwrap());
        });
    }
    assert_eq!(data["sides"].as_array().unwrap().len(), 6);
    for row in data["sides"].as_array().unwrap() {
        let v = input(row, "input");
        result(normalize_side(v.as_ref(), "probe"), row, |v, e| {
            levels(&v.numeric, &e["numeric"]);
            wires(&v.wire, &e["wire"]);
        });
    }
    assert_eq!(data["updates"].as_array().unwrap().len(), 12);
    for row in data["updates"].as_array().unwrap() {
        let v = input(row, "input");
        result(
            normalize_update(v.as_ref(), "PERP_BTC_USDT"),
            row,
            |v, e| {
                assert_eq!(v.symbol, e["symbol"].as_str().unwrap());
                number(
                    v.previous_timestamp,
                    &e["previousTimestamp"],
                    "RECOVERY native predecessor",
                );
                number(v.timestamp, &e["timestamp"], "RECOVERY native update time");
                levels(&v.asks, &e["asks"]);
                levels(&v.bids, &e["bids"]);
                assert_eq!(
                    v.fingerprint,
                    e["fingerprint"].as_str().unwrap(),
                    "RECOVERY native exact lexical fingerprint"
                );
            },
        );
    }
    let level = |p: &str, q: &str| WooV3OrderBookLevel {
        price: p.into(),
        quantity: q.into(),
    };
    let snapshot: WooV3MappedOrderBookSnapshot = map_woo_v3_order_book(&WooV3OrderBookSnapshot {
        timestamp: 100.0,
        bids: vec![level("99", "2"), level("98", "3")],
        asks: vec![level("103", "4"), level("102", "5")],
    })
    .unwrap();
    assert_eq!(data["snapshotSides"].as_array().unwrap().len(), 2);
    for row in data["snapshotSides"].as_array().unwrap() {
        levels(
            &snapshot_levels(&snapshot, row["side"] == "bids"),
            &row["ok"],
        );
    }
}
#[test]
fn native_coordinator_construction_and_complete_trace_floor() {
    let data = source();
    assert_eq!(data["constructors"].as_array().unwrap().len(), 10);
    for row in data["constructors"].as_array().unwrap() {
        let s = input(row, "symbol");
        let d = input(row, "depth");
        result(
            WooV3OrderBookRecoveryCoordinator::new(s.as_ref(), d.as_ref()),
            row,
            |c, e| assert_eq!(c.requires_fresh_snapshot(), e.as_bool().unwrap()),
        );
    }
    let names = [
        "late-join",
        "buffered-join",
        "buffer-duplicate",
        "buffer-conflict",
        "buffer-out-of-order",
        "missing-join-queued",
        "missing-join-late",
        "sequence-gap-queued",
        "sequence-gap-live",
        "joined-duplicate-stale",
        "joined-conflict",
        "waiting-stale",
        "request-replacement",
        "request-typed-refusal",
        "invalid-snapshot-consumes",
        "invalid-snapshot-shapes",
        "invalid-delta-resets",
        "overflow-precedes-invalid",
        "snapshot-overflow",
        "reconnect-retains-nonce",
        "numeric-price-collision",
        "decimal-snapshot",
        "invalid-delta-level",
        "buffer-ceiling",
        "cross-depth-200",
    ];
    assert_eq!(data["scenarios"].as_array().unwrap().len(), 25);
    let step_counts = [
        3, 5, 4, 4, 4, 3, 3, 4, 4, 6, 4, 5, 6, 7, 6, 8, 7, 3, 4, 8, 3, 3, 6, 503, 4,
    ];
    for (index, scenario) in data["scenarios"].as_array().unwrap().iter().enumerate() {
        assert_eq!(scenario["name"], names[index]);
        let s: JsNumberInput = js_from_str("\"PERP_BTC_USDT\"").unwrap();
        let d: JsNumberInput =
            js_from_str(&scenario["depth"].as_u64().unwrap().to_string()).unwrap();
        let mut c = WooV3OrderBookRecoveryCoordinator::new(Some(&s), Some(&d)).unwrap();
        assert!(c.requires_fresh_snapshot());
        let steps = scenario["steps"].as_array().unwrap();
        assert_eq!(
            steps.len(),
            step_counts[index],
            "RECOVERY independent trace step floor"
        );
        for step_index in 0..step_counts[index] {
            let step = &steps[step_index];
            let v = input(step, "input");
            match step["op"].as_str().unwrap() {
                "begin" => number(
                    c.begin_snapshot_request().unwrap(),
                    &step["expected"],
                    "RECOVERY native request token",
                ),
                "push" => recovery(&c.push_update(v.as_ref()), &step["expected"]),
                "snapshot" => {
                    let token = input(step, "token");
                    recovery(
                        &c.accept_rest_snapshot(v.as_ref(), token.as_ref()),
                        &step["expected"],
                    );
                }
                "reconnect" => recovery(&c.reconnect(), &step["expected"]),
                _ => unreachable!(),
            };
            assert_eq!(
                c.requires_fresh_snapshot(),
                step["requiresFreshSnapshot"].as_bool().unwrap(),
                "RECOVERY native phase"
            );
        }
        if names[index] == "buffer-ceiling" {
            assert_eq!(
                scenario["steps"].as_array().unwrap().len(),
                503,
                "RECOVERY independent queue-bound trace floor"
            );
        }
    }
}
#[test]
fn publications_own_book_levels_and_arrays() {
    let data = source();
    let s: JsNumberInput = js_from_str("\"PERP_BTC_USDT\"").unwrap();
    let d: JsNumberInput = js_from_str("50").unwrap();
    let mut c = WooV3OrderBookRecoveryCoordinator::new(Some(&s), Some(&d)).unwrap();
    c.begin_snapshot_request().unwrap();
    let scenario = &data["scenarios"][0];
    let snap = input(&scenario["steps"][1], "input");
    let t: JsNumberInput = js_from_str("1").unwrap();
    c.accept_rest_snapshot(snap.as_ref(), Some(&t));
    let first_input = input(&scenario["steps"][2], "input");
    let first = c.push_update(first_input.as_ref());
    let WooV3OrderBookRecoveryResult::Joined {
        publication: mut owned_publication,
        ..
    } = first
    else {
        panic!("RECOVERY native ownership first join")
    };
    let next:JsNumberInput=js_from_str(r#"{"symbol":"PERP_BTC_USDT","previousTimestamp":101,"timestamp":102,"asks":[["102","99"]],"bids":[["99","100"]]}"#).unwrap();
    c.push_update(Some(&next));
    publication(&owned_publication, &data["publicationOwnership"]["first"]);
    owned_publication
        .book
        .asks
        .push(safe_cex_types::OrderBookOrders {
            price: 999.0,
            amount: 999.0,
            total: 999.0,
        });
    let next: JsNumberInput = js_from_str(
        r#"{"symbol":"PERP_BTC_USDT","previousTimestamp":102,"timestamp":103,"asks":[],"bids":[]}"#,
    )
    .unwrap();
    let WooV3OrderBookRecoveryResult::Applied { publication: p, .. } = c.push_update(Some(&next))
    else {
        panic!("RECOVERY native ownership final apply")
    };
    publication(&p, &data["publicationOwnership"]["afterArrayMutation"]);
}
