use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown, from_json},
    native_store::DefaultStore,
};
use safe_cex_venue_binance::ticker_capture::Capture;
use std::{cell::Cell, rc::Rc};

#[test]
fn ticker_capture_does_not_project_an_unselected_native_row() {
    let store = Rc::new(DefaultStore::new());
    let facade = store.facade().unwrap();
    let ticker = |symbol: &str| {
        from_json(&serde_json::json!({
            "id":symbol,"symbol":symbol,"bid":1,"ask":2,"last":3,"mark":4,"index":5,
            "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,
            "volume":0,"quoteVolume":0
        }))
    };
    let selected = ticker("SELECTED");
    let poison = ticker("UNSELECTED");
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "tickers",
                Js::array(vec![selected, poison.clone()]),
            )])],
        )
        .unwrap();
    let reads = Rc::new(Cell::new(0));
    let observed = reads.clone();
    poison
        .define_accessor(
            "bid",
            Some(Rc::new(move |_| {
                observed.set(observed.get() + 1);
                Err(Thrown::new(Js::utf16(&[0xd800])))
            })),
            None,
        )
        .unwrap();
    let capture =
        Capture::new(store).expect("capturing a native dictionary must not project unrelated DTOs");
    let dictionary = capture.dictionary().unwrap();
    assert!(dictionary.same_reference(&facade.get("tickersBySymbol").unwrap()));
    let selected = safe_cex_venue_binance::ticker_capture::lookup(&dictionary, "SELECTED")
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(
        selected.get("bid").unwrap().to_number().unwrap().to_bits(),
        1.0_f64.to_bits()
    );
    facade
        .call_method(
            "updateTicker",
            vec![
                selected.clone(),
                Js::object(vec![("bid", Js::Number(-0.0))]),
            ],
        )
        .unwrap();
    assert_eq!(
        selected.get("bid").unwrap().to_number().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(capture.dictionary().unwrap().same_reference(&dictionary));
    let replacement = ticker("SELECTED");
    replacement.set("id", Js::str("fresh")).unwrap();
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "tickers",
                Js::array(vec![replacement.clone()]),
            )])],
        )
        .unwrap();
    let fresh = capture.dictionary().unwrap();
    assert!(!fresh.same_reference(&dictionary));
    assert!(fresh.get("SELECTED").unwrap().same_reference(&replacement));
    assert!(
        dictionary
            .get("SELECTED")
            .unwrap()
            .same_reference(&selected)
    );
    assert_eq!(
        selected.get("bid").unwrap().to_number().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        reads.get(),
        0,
        "dictionary capture must not reach a row getter"
    );
}
