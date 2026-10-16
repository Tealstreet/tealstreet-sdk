use safe_cex_venue_hyperliquid::{spot_json::JsString, types::WsUserEvent};
use serde_json::Value;
#[test]
fn core_event_retains_all_four_native_intersection_alternatives() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/finding-user-event-inputs.json")).unwrap();
    let mut completed = 0;
    let s = |value: &JsString, text: &str| {
        assert_eq!(
            value.0,
            text.encode_utf16().collect::<Vec<_>>(),
            "EVENT decoded native string {text}"
        )
    };
    let n = |value: f64, expected: f64| {
        assert_eq!(
            value.to_bits(),
            expected.to_bits(),
            "EVENT decoded native number {expected}"
        )
    };
    for (i, kind) in ["fills", "funding", "liquidation", "cancels"]
        .into_iter()
        .enumerate()
    {
        let row = rows.get(i).expect("EVENT independent native input index");
        assert_eq!(row["kind"], kind, "EVENT independent native variant order");
        let raw = row["raw"].as_str().unwrap();
        let user = JsString(vec![0xd800, 117, 115, 101, 114]);
        let event = match kind {
            "fills" => WsUserEvent::Fills {
                entries: serde_json::from_str(raw).unwrap(),
                user,
            },
            "funding" => WsUserEvent::Funding {
                entry: serde_json::from_str(raw).unwrap(),
                user,
            },
            "liquidation" => WsUserEvent::Liquidation {
                entry: serde_json::from_str(raw).unwrap(),
                user,
            },
            _ => WsUserEvent::NonUserCancels {
                entries: serde_json::from_str(raw).unwrap(),
                user,
            },
        };
        assert_eq!(
            event.user().0,
            [0xd800, 117, 115, 101, 114],
            "EVENT native owned user units"
        );
        match event {
            WsUserEvent::Fills { entries, .. } => {
                assert_eq!(entries.len(), 2, "EVENT independent fills length");
                for (i, f) in entries.iter().enumerate() {
                    let (coin, side, values): (&str, &str, [&str; 7]) = if i == 0 {
                        ("BTC", "B", ["11", "12", "13", "14", "15", "16", "18"])
                    } else {
                        ("ETH", "A", ["21", "22", "24", "25", "26", "27", "29"])
                    };
                    s(&f.coin, coin);
                    s(&f.side, side);
                    for (field, expected) in [
                        &f.px,
                        &f.sz,
                        &f.start_position,
                        &f.dir,
                        &f.closed_pnl,
                        &f.hash,
                        &f.fee,
                    ]
                    .into_iter()
                    .zip(values)
                    {
                        s(field, expected)
                    }
                    n(f.time, if i == 0 { -0.0 } else { 23.0 });
                    n(f.oid, if i == 0 { 17.0 } else { 28.0 });
                    n(f.tid, if i == 0 { 19.0 } else { 30.0 });
                    assert_eq!(f.crossed, i == 0, "EVENT native crossed");
                }
            }
            WsUserEvent::Funding { entry: f, .. } => {
                n(f.time, -0.0);
                for (field, expected) in [
                    (&f.coin, "BTC"),
                    (&f.usdc, "31"),
                    (&f.szi, "32"),
                    (&f.funding_rate, "33"),
                ] {
                    s(field, expected)
                }
            }
            WsUserEvent::Liquidation { entry: f, .. } => {
                n(f.lid, 34.0);
                for (field, expected) in [
                    (&f.liquidator, "35"),
                    (&f.liquidated_user, "36"),
                    (&f.liquidated_ntl_pos, "37"),
                    (&f.liquidated_account_value, "38"),
                ] {
                    s(field, expected)
                }
            }
            WsUserEvent::NonUserCancels { entries, .. } => {
                assert_eq!(entries.len(), 2, "EVENT independent cancels length");
                s(&entries[0].coin, "BTC");
                s(&entries[1].coin, "ETH");
                n(entries[0].oid, -0.0);
                n(entries[1].oid, 39.0);
            }
        }
        completed += 1;
    }
    assert_eq!(completed, 4, "EVENT independently completed native floor");
    // The source witness checks the actual core declaration, after native fields.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-user-event-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "EVENT actual core source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let marker: Value =
        serde_json::from_slice(&output.stdout).expect("EVENT source completion marker");
    assert_eq!(
        marker["count"], 4,
        "EVENT independent source completion floor"
    );
    assert_eq!(marker["last"], 3, "EVENT source stop3");
}
