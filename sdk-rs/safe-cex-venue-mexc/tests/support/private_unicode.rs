use super::observed_store::ObservedStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::Store;
use safe_cex_venue_mexc::ws_private::MexcPrivateHandlers;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc};
pub(super) fn native_unicode_boundaries() {
    const CASES: [(&str, &str, &[u16]); 4] = [
        (
            "lone",
            r"BTC_\ud800USDT",
            &[66, 84, 67, 55296, 85, 83, 68, 84],
        ),
        ("bmp", r"BTC_\u00e9USDT", &[66, 84, 67, 233, 85, 83, 68, 84]),
        (
            "astral",
            r"BTC_\ud83d\ude03USDT",
            &[66, 84, 67, 55357, 56835, 85, 83, 68, 84],
        ),
        (
            "space",
            r"\ufeff BTC_USDT \u3000",
            &[65279, 32, 66, 84, 67, 85, 83, 68, 84, 32, 12288],
        ),
    ];
    let (
        mut accepted,
        mut refused,
        mut reads_total,
        mut first_total,
        mut later_total,
        mut seen_total,
        mut shield_total,
    ) = (0, 0, 0, 0, 0, 0, 0);
    for method in [
        "order",
        "position",
        "plan",
        "stop",
        "direct-plan",
        "direct-stop",
    ] {
        let kind = method.strip_prefix("direct-").unwrap_or(method);
        for (label, escape, units) in CASES {
            let row = match kind {
                "order" => {
                    json!({"orderId":"row","symbol":"PRIVATE_SYMBOL","side":1,"state":1,"vol":2,"price":10,"createTime":1000})
                }
                "position" => {
                    json!({"symbol":"PRIVATE_SYMBOL","positionType":1,"holdVol":6,"holdAvgPrice":20,"createTime":1000})
                }
                "plan" => {
                    json!({"id":"row","symbol":"PRIVATE_SYMBOL","side":1,"state":1,"vol":2,"triggerPrice":21,"createTime":1000})
                }
                "stop" => {
                    json!({"stopPlanOrderId":"row","symbol":"PRIVATE_SYMBOL","positionType":1,"state":1,"vol":2,"stopLossPrice":5,"createTime":1000})
                }
                _ => unreachable!(),
            };
            let wire = serde_json::to_string(&json!({"data":[row]}))
                .unwrap()
                .replace("PRIVATE_SYMBOL", escape);
            let parsed = serde_json::from_str::<Value>(&wire);
            if label == "lone" {
                assert!(
                    parsed.is_err(),
                    "private native refuses materialized lone surrogate frame"
                );
                // No native Value exists and no handler is invoked in these six
                // cases. Initialized zero counters are not call evidence.
                refused += 1;
                continue;
            }
            let frame = parsed.expect("private native positive scalar frame accepted");
            let store = ObservedStore::new();
            let first = Rc::new(Cell::new(0));
            let later = Rc::new(Cell::new(0));
            let calls = first.clone();
            let _first = Store::subscribe(
                &store,
                Box::new(move |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(())
                }),
            );
            let calls = later.clone();
            let _later = Store::subscribe(
                &store,
                Box::new(move |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(())
                }),
            );
            let seen = Rc::new(Cell::new(0));
            let shields = Rc::new(Cell::new(0));
            let calls = seen.clone();
            let mark = Rc::new(move |id: String| {
                assert_eq!(id, "row");
                calls.set(calls.get() + 1);
            });
            let calls = shields.clone();
            let shield = Rc::new(move |id: &str| {
                assert_eq!(id, "row");
                calls.set(calls.get() + 1);
                false
            });
            let handlers = MexcPrivateHandlers::new(
                Rc::new(|v| v),
                None,
                Rc::new(|_, _| -> Result<(), ExchangeError> { panic!("no Unicode reject hook") }),
                mark,
                shield,
                Rc::new(|_| -> Result<(), ExchangeError> { panic!("no Unicode fill hook") }),
            );
            match method {
                "order" => handlers.handle_order(&store, &frame),
                "position" => handlers.handle_position(&store, &frame),
                "plan" => handlers.handle_plan_order(&store, &frame),
                "stop" => handlers.handle_stop_order(&store, &frame),
                "direct-plan" => handlers.apply_stop_like_frame(&store, &frame, true),
                "direct-stop" => handlers.apply_stop_like_frame(&store, &frame, false),
                _ => unreachable!(),
            }
            .unwrap();
            assert_eq!(
                store.market_reads.get(),
                1,
                "private native Unicode getter count"
            );
            assert_eq!(
                first.get(),
                1,
                "private native Unicode first subscriber count"
            );
            assert_eq!(
                later.get(),
                1,
                "private native Unicode later subscriber count"
            );
            if kind == "position" {
                let positions = store.positions().unwrap();
                assert_eq!(positions.len(), 1);
                assert_eq!(
                    positions[0].symbol.encode_utf16().collect::<Vec<_>>(),
                    units,
                    "private native positive position symbol units"
                );
                assert_eq!(positions[0].contracts.to_bits(), 6f64.to_bits());
            } else {
                let orders = store.orders().unwrap();
                assert_eq!(orders.len(), 1);
                assert_eq!(
                    orders[0].symbol.encode_utf16().collect::<Vec<_>>(),
                    units,
                    "private native positive order symbol units"
                );
                assert_eq!(orders[0].amount.to_bits(), 2f64.to_bits());
            }
            assert_eq!(
                seen.get(),
                usize::from(matches!(kind, "plan" | "stop")),
                "private native Unicode seen calls"
            );
            assert_eq!(
                shields.get(),
                usize::from(matches!(kind, "plan" | "stop")),
                "private native Unicode shield calls"
            );
            accepted += 1;
            reads_total += store.market_reads.get();
            first_total += first.get();
            later_total += later.get();
            seen_total += seen.get();
            shield_total += shields.get();
        }
    }
    assert_eq!(
        (
            accepted,
            refused,
            reads_total,
            first_total,
            later_total,
            seen_total,
            shield_total
        ),
        (18, 6, 18, 18, 18, 12, 12),
        "private native independent Unicode boundary floors"
    );
}
pub(super) fn assert_source_unicode_floor() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private_unicode_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual private Unicode Source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows.len(),
        24,
        "private Source independent Unicode root floor"
    );
    for (i, method) in [
        "order",
        "position",
        "plan",
        "stop",
        "direct-plan",
        "direct-stop",
    ]
    .into_iter()
    .enumerate()
    {
        for (j, label) in ["lone", "bmp", "astral", "space"].into_iter().enumerate() {
            let row = &rows[i * 4 + j];
            assert_eq!(row["method"], method);
            assert_eq!(row["label"], label);
            assert_eq!(row["reads"], 1);
            assert_eq!(row["first"], 1);
            assert_eq!(row["later"], 1);
            if label == "lone" {
                assert_eq!(
                    row["units"][3], 0xd800,
                    "private Source program-visible stored lone unit"
                );
            }
        }
    }
}
