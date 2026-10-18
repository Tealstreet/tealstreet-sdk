fn set_maximum_size(h: &Harness, max: f64) {
    let mut markets = h.store.markets().unwrap().to_vec();
    markets[0].limits.amount.max = (max).into();
    Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

#[test]
fn zero_maximum_size_matches_source_empty_lots_and_schedules_order_refresh() {
    let h = fixture();
    set_maximum_size(&h, 0.0);
    let mut opts = placed();
    opts.client_order_id = None;
    let ids = h
        .drive(h.exchange.base().place_order(h.exchange.as_ref(), opts))
        .expect("lodash times(Infinity) produces no lots");
    assert!(ids.is_empty());
    assert!(h.http.calls("/v3/order").is_empty());
    assert!(h.http.calls("/v3/open-orders").is_empty());
    h.runtime.advance(1_001.0);
    assert_eq!(
        h.http.calls("/v3/open-orders").len(),
        1,
        "empty submission still schedules source order reconciliation"
    );
    done(&h);
}

#[test]
fn stop_order_keeps_zero_adjusted_price_as_the_trigger_fallback() {
    let h = fixture();
    Store::update(h.store.as_ref(), serde_json::from_value(json!({"tickers":[{
        "id":"BTC-USDC","symbol":"BTCUSDC","bid":99,"ask":101,"last":100,"mark":100,"index":100,
        "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0
    }]})).unwrap(), false).unwrap();
    let mut opts = placed();
    opts.r#type = OrderType::StopLoss;
    opts.client_order_id = None;
    opts.price = Some(0.00001);
    h.drive(h.exchange.base().place_order(h.exchange.as_ref(), opts))
        .unwrap();
    let requests = h.http.calls("/v3/order");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].data["triggerPrice"], "0",
        "triggerPrice || price keeps a final zero"
    );
    assert_eq!(requests[0].data["signature"].as_str().unwrap().len(), 128);
    done(&h);
}

#[test]
fn split_orders_post_every_lot_before_projecting_a_numeric_response_id() {
    let h = fixture();
    set_maximum_size(&h, 1.0);
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "POST" || path != "/v3/order");
    h.http.ok("POST", "/v3/order", json!({"id":123}));
    h.http.ok("POST", "/v3/order", json!({"id":"second"}));
    h.http.ok("POST", "/v3/order", json!({"id":"third"}));
    let mut opts = placed();
    opts.client_order_id = None;
    opts.amount = 3.0;
    let result = h.drive(h.exchange.base().place_order(h.exchange.as_ref(), opts));
    let requests = h.http.calls("/v3/order");
    assert_eq!(
        requests.len(),
        3,
        "TS maps IDs only after posting every lot"
    );
    for request in requests {
        assert_eq!(request.data["size"], "1");
        assert_eq!(request.data["signature"].as_str().unwrap().len(), 128);
    }
    assert_eq!(
        result.expect("numeric live order IDs must be returned"),
        ["123", "second", "third"]
    );
    h.runtime.advance(1_001.0);
    assert_eq!(
        h.http.calls("/v3/open-orders").len(),
        1,
        "numeric IDs do not skip source reconciliation"
    );
    done(&h);
}

#[test]
fn missing_response_data_throws_only_after_all_lots_are_submitted() {
    let h = fixture();
    set_maximum_size(&h, 1.0);
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "POST" || path != "/v3/order");
    h.http.route("POST", "/v3/order", 200.0, json!({"code":0}));
    h.http.ok("POST", "/v3/order", json!({"id":"second"}));
    h.http.ok("POST", "/v3/order", json!({"id":"third"}));
    let mut opts = placed();
    opts.client_order_id = None;
    opts.amount = 3.0;
    let result = h.drive(h.exchange.base().place_order(h.exchange.as_ref(), opts));
    assert_eq!(h.http.calls("/v3/order").len(), 3);
    let error = result.unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert_eq!(
        error.message,
        "Cannot read properties of undefined (reading 'id')"
    );
    done(&h);
}

#[test]
fn placement_ids_use_js_string_and_keep_missing_ids_visible() {
    for (raw, expected) in [
        (json!(0), "0"),
        (json!(-0.0), "0"),
        (json!(123.5), "123.5"),
        (json!(1e21), "1e+21"),
        (json!(9007199254740993u64), "9007199254740992"),
        (json!("kept-id"), "kept-id"),
        (json!(true), "true"),
        (json!(null), "null"),
        (json!([12, 34]), "12,34"),
        (json!({}), "[object Object]"),
    ] {
        let h = fixture();
        h.http
            .routes
            .borrow_mut()
            .retain(|(method, path, _)| method != "POST" || path != "/v3/order");
        h.http.ok("POST", "/v3/order", json!({"id":raw}));
        let mut opts = placed();
        opts.client_order_id = None;
        let ids = h
            .drive(h.exchange.base().place_order(h.exchange.as_ref(), opts))
            .unwrap_or_else(|error| panic!("already live order id {raw}: {error:?}"));
        assert_eq!(ids, [expected], "JS String({raw})");
        let requests = h.http.calls("/v3/order");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].data["signature"].as_str().unwrap().len(), 128);
        done(&h);
    }
    let h = fixture();
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "POST" || path != "/v3/order");
    h.http.ok("POST", "/v3/order", json!({}));
    let mut opts = placed();
    opts.client_order_id = None;
    let error = h
        .drive(h.exchange.base().place_order(h.exchange.as_ref(), opts))
        .unwrap_err();
    assert_eq!(error.message, "ApexOmni placement response is missing id");
    assert_eq!(h.http.calls("/v3/order").len(), 1);
    h.runtime.advance(1_001.0);
    assert_eq!(
        h.http.calls("/v3/open-orders").len(),
        1,
        "missing result still reconciles the submitted order"
    );
    done(&h);
}

#[test]
fn missing_id_reports_after_every_signed_lot_and_schedules_reconciliation() {
    let h = fixture();
    set_maximum_size(&h, 1.0);
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "POST" || path != "/v3/order");
    h.http.ok("POST", "/v3/order", json!({}));
    h.http.ok("POST", "/v3/order", json!({"id":"second"}));
    h.http.ok("POST", "/v3/order", json!({"id":"third"}));
    let mut opts = placed();
    opts.client_order_id = None;
    opts.amount = 3.;
    let error = h
        .drive(h.exchange.base().place_order(h.exchange.as_ref(), opts))
        .unwrap_err();
    assert_eq!(error.message, "ApexOmni placement response is missing id");
    let requests = h.http.calls("/v3/order");
    assert_eq!(requests.len(), 3);
    for request in requests {
        assert_eq!(request.data["size"], "1");
        assert_eq!(request.data["signature"].as_str().unwrap().len(), 128);
    }
    h.runtime.advance(1_001.0);
    assert_eq!(h.http.calls("/v3/open-orders").len(), 1);
    done(&h);
}
