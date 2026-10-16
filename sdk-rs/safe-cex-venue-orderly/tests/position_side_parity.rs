use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_types::{PlaceOrderOpts, PositionMode};
use safe_cex_venue_orderly::utils::get_position_side_for_order;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::{Future, poll_fn},
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
fn opts(side: &str) -> PlaceOrderOpts {
    js_from_str(
        &json!({"symbol":"BTC","side":side,"type":"limit","amount":3.25,"reduceOnly":true})
            .to_string(),
    )
    .unwrap()
}
#[test]
fn actual_typescript_position_side_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/position-side-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("position-side runtime: {}", source["runtime"]);
    assert_eq!(
        source["rows"].as_array().unwrap().len(),
        12,
        "POSITION-SIDE source total"
    );
    assert_eq!(
        source["rejected"].as_array().unwrap().len(),
        2,
        "POSITION-SIDE rejection total"
    );
    let mut compared = 0;
    // Fixed independent walk: shrinking the source cannot shrink comparisons.
    for index in 0..12 {
        let row = &source["rows"][index];
        let label = row["label"]
            .as_str()
            .expect("POSITION-SIDE required source case");
        let opts: PlaceOrderOpts = js_from_str(&row["opts"].to_string()).unwrap();
        let mode: PositionMode = js_from_str(&row["mode"].to_string()).unwrap();
        let calls = Cell::new(0);
        let mut future = Box::pin(get_position_side_for_order(&opts, || {
            calls.set(calls.get() + 1);
            std::future::ready(Ok::<_, ()>(mode))
        }));
        let actual = poll(future.as_mut());
        assert_eq!(
            row["kind"],
            if row["result"].is_null() {
                "undefined"
            } else {
                "string"
            },
            "POSITION-SIDE source result kind/{label}"
        );
        assert_eq!(
            actual,
            Poll::Ready(Ok(row["result"].as_str())),
            "POSITION-SIDE native/{label}"
        );
        assert_eq!(calls.get(), 1, "POSITION-SIDE native getter calls/{label}");
        assert_eq!(row["calls"], 1, "POSITION-SIDE source getter calls/{label}");
        compared += 1;
    }
    assert_eq!(compared, 12, "POSITION-SIDE comparisons");
    for index in 0..2 {
        let row = &source["rejected"][index];
        let opts = opts(row["side"].as_str().unwrap());
        let error = Rc::new(format!("mode failed/{index}"));
        let calls = Cell::new(0);
        let mut future = Box::pin(get_position_side_for_order(&opts, || {
            calls.set(calls.get() + 1);
            std::future::ready(Err::<PositionMode, _>(error.clone()))
        }));
        let Poll::Ready(Err(actual)) = poll(future.as_mut()) else {
            panic!("POSITION-SIDE rejection required/{index}");
        };
        assert!(
            Rc::ptr_eq(&actual, &error),
            "POSITION-SIDE error identity/{index}"
        );
        assert_eq!(calls.get(), 1, "POSITION-SIDE rejected calls/{index}");
        assert_eq!(row["calls"], 1);
        assert_eq!(row["sameError"], true);
    }
    let opts = opts("buy");
    let ready = Cell::new(false);
    let calls = Cell::new(0);
    let mut future = Box::pin(get_position_side_for_order(&opts, || {
        calls.set(calls.get() + 1);
        poll_fn(|_| {
            if ready.get() {
                Poll::Ready(Ok::<_, ()>(PositionMode::Hedged))
            } else {
                Poll::Pending
            }
        })
    }));
    assert!(
        poll(future.as_mut()).is_pending(),
        "POSITION-SIDE awaits getter"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        source["pending"]["before"],
        json!({"calls":1,"settled":false})
    );
    ready.set(true);
    assert_eq!(
        poll(future.as_mut()),
        Poll::Ready(Ok(Some("SHORT"))),
        "POSITION-SIDE pending native"
    );
    assert_eq!(
        calls.get(),
        1,
        "POSITION-SIDE getter invoked once after resume"
    );
    assert_eq!(source["pending"]["result"], "SHORT");
    assert_eq!(source["pending"]["calls"], 1);
    assert_eq!(source["pending"]["settled"], true);
    // CHOSEN boundary: Rust futures start on poll and retain immutable options.
    // TS calls the getter eagerly and can observe caller changes after await.
    let stable_opts = crate::opts("buy");
    let calls = Cell::new(0);
    let mut future = Box::pin(get_position_side_for_order(&stable_opts, || {
        calls.set(calls.get() + 1);
        std::future::ready(Ok::<_, ()>(PositionMode::Hedged))
    }));
    assert_eq!(calls.get(), 0, "POSITION-SIDE native cold future");
    assert_eq!(
        poll(future.as_mut()),
        Poll::Ready(Ok(Some("SHORT"))),
        "POSITION-SIDE native immutable options"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(source["alias"]["immediateCalls"], 1);
    assert_eq!(source["alias"]["result"], "LONG");
}
