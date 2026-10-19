use super::*;

#[test]
fn dedicated_subscribe_starts_before_ping_and_retains_ten_ms_continuation() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-subscribe-delay-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 2, "maintained Spot and Inverse subscribe");
    for (name, category, expected) in [
        (ExchangeName::Bybitspot, "spot", &source[0]),
        (ExchangeName::Bybitinverse, "inverse", &source[1]),
    ] {
        let (h, socket) = setup(name.clone(), category);
        let symbols: Vec<_> = (0..25).map(|i| format!("SUB{i}USD")).collect();
        let markets: Vec<_> = symbols
            .iter()
            .map(|symbol| market(symbol, "USD", false))
            .collect();
        configure(&h, &markets);
        h.venue()
            .base()
            .set_active_ticker_symbols(&symbols)
            .unwrap();
        socket.connect_and_subscribe(false).unwrap();
        h.runtime.pump();
        let start = h.runtime.now.get();
        assert_eq!(wire_frames(&h)[0]["op"], "subscribe", "{name:?}");
        assert_eq!(wire_frames(&h)[1], json!({"op":"ping"}), "{name:?}");
        let mut sends = vec![];
        for (at, count) in [
            (0u64, 1),
            (9, 1),
            (10, 2),
            (19, 2),
            (20, 3),
            (29, 3),
            (30, 3),
        ] {
            h.runtime.now.set(start + at as f64);
            h.runtime.pump();
            let chunks = subscriptions(&h);
            assert_eq!(
                chunks.len(),
                count,
                "ten-ms subscribe boundary: {name:?}/{at}"
            );
            for args in chunks.iter().skip(sends.len()) {
                sends.push(json!({"at":at,"args":args}));
            }
        }
        assert_eq!(
            json!(sends),
            expected["sends"],
            "source chunk trace: {name:?}"
        );
        assert!(h.runtime.reports.borrow().is_empty());
        socket.dispose();
        h.venue().base().dispose().unwrap();
    }
}
