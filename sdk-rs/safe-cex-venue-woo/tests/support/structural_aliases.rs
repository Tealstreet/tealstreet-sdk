use crate::binding_test_harness as harness;
use crate::legacy_metadata::{JsString, Rows};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, from_json},
};
use safe_cex_types::{ExchangeName, MarginType, PositionMode, PositionSide};
use serde_json::json;
use std::rc::Rc;

fn invoke(h: &harness::Harness, member: &str, args: Vec<Js>) -> Js {
    let value = h.exchange.rt.this.call_method(member, args).unwrap();
    let runtime = h.exchange.rt.exchange_runtime();
    h.drive(Box::pin(async move {
        await_js(&runtime, value)
            .await
            .map_err(|e| e.to_exchange_error())
    }))
    .unwrap()
}

#[test]
fn metadata_aliases_keep_native_units_cache_identity_and_signed_fields() {
    const PATH: &str = "/v1/client/futures_leverage";
    let h = harness::harness(ExchangeName::Woo, harness::options(), |http| {
        http.ok("GET", PATH, json!({"details":[{"leverage":"-0"}]}));
        http.ok("POST", PATH, json!({"success":true}));
    });
    let symbol: JsString = vec![66, 84, 67, 0xd800, 0xdc00, 0xdc00];
    let mode = Js::str("hedged");
    let rows = h
        .exchange
        .rt
        .this
        .call_method(
            "buildPositionMetadataFromLeverage",
            vec![
                Js::utf16(&symbol),
                mode.clone(),
                from_json(&json!({"data":{"details":[{"leverage":"-0"}]}})),
            ],
        )
        .unwrap()
        .array_values()
        .expect("fixture array property reads")
        .unwrap();
    assert_eq!(
        rows.len(),
        2,
        "live metadata builder retains both hedge sides"
    );
    for row in &rows {
        assert_eq!(
            row.get("symbol").unwrap().as_str().unwrap().as_ref(),
            symbol.as_slice()
        );
        assert_eq!(
            row.get("leverage").unwrap().as_f64().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert!(row.get("positionMode").unwrap().strict_equals(&mode));
        assert!(
            row.get("marginType")
                .unwrap()
                .strict_equals(&Js::str("cross"))
        );
    }
    assert!(rows[0].get("side").unwrap().strict_equals(&Js::str("long")));
    assert!(
        rows[1]
            .get("side")
            .unwrap()
            .strict_equals(&Js::str("short"))
    );
    let first = invoke(
        &h,
        "fetchPositionMetadataForSymbol",
        vec![Js::str("BTCUSDT"), mode.clone()],
    );
    assert_eq!(
        first
            .array_values()
            .expect("fixture array property reads")
            .unwrap()
            .len(),
        2
    );
    let state = h.exchange.rt.shared.extra.metadata.get().unwrap();
    let key: JsString = "BTCUSDT:hedged".encode_utf16().collect();
    let cached: Rows = state.cache.borrow().get(&key).unwrap().metadata;
    let cached_rows = cached.borrow();
    assert_eq!(cached_rows.len(), 2);
    for (index, row) in cached_rows.iter().enumerate() {
        assert_eq!(row.symbol, "BTCUSDT".encode_utf16().collect::<Vec<_>>());
        assert_eq!(
            row.side,
            if index == 0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            }
        );
        assert_eq!(row.position_mode, PositionMode::Hedged);
        assert_eq!(row.margin_type, MarginType::Cross);
        assert_eq!(row.leverage.to_bits(), (-0.0f64).to_bits());
    }
    let child = cached_rows[0].clone();
    drop(cached_rows);
    invoke(
        &h,
        "fetchPositionMetadataForSymbol",
        vec![Js::str("BTCUSDT"), mode],
    );
    let again: Rows = state.cache.borrow().get(&key).unwrap().metadata;
    assert!(
        Rc::ptr_eq(&cached, &again),
        "cache hit must retain the actual Rows handle"
    );
    assert!(
        Rc::ptr_eq(&child, &again.borrow()[0]),
        "cache hit must retain the Metadata child"
    );
    let calls = h.http.calls(PATH);
    assert_eq!(calls.len(), 1, "cache hit must not resend GET");
    assert_eq!(calls[0].method, "GET");
    assert_eq!(calls[0].params["symbol"], "PERP_BTC_USDT");
    assert_eq!(calls[0].params["position_mode"], "ONE_WAY");
    assert_eq!(calls[0].params["margin_mode"], "CROSS");
    assert!(calls[0].query.as_ref().is_some_and(|q| !q.is_empty()));
    let result = h.drive(h.venue().set_leverage("BTCUSDT".into(), 17.0, None));
    assert!(result.is_ok(), "live Fields POST result: {result:?}");
    let calls = h.http.calls(PATH);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].method, "POST");
    assert_eq!(
        calls[1].data.as_str(),
        Some("leverage=17&margin_mode=CROSS&position_side=BOTH&symbol=PERP_BTC_USDT")
    );
    let replaced: Rows = state.cache.borrow().get(&key).unwrap().metadata;
    assert!(
        !Rc::ptr_eq(&cached, &replaced),
        "leverage update replaces metadata rows"
    );
    assert_eq!(cached.borrow()[0].leverage.to_bits(), (-0.0f64).to_bits());
    assert_eq!(replaced.borrow()[0].leverage.to_bits(), 17.0f64.to_bits());
    h.venue().dispose().unwrap();
}
