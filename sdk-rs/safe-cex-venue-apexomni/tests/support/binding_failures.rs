fn reset_route(h: &Harness, path: &str) {
    h.http.routes.borrow_mut().retain(|(_, p, _)| p != path);
}
#[test]
fn signature_envelope_retries_once_using_legacy_signing() {
    let h = fixture();
    reset_route(&h, "/v3/account");
    h.runtime.now.set(1700000000123.);
    h.http.advance_ms.set(250.);
    h.http.route(
        "GET",
        "/v3/account",
        200.,
        json!({"code":20016,"msg":"signature rejected"}),
    );
    h.http.ok("GET", "/v3/account", json!({"id":"16909060"}));
    let before = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
    assert_eq!(
        h.drive(h.venue().get_account()).unwrap().user_id,
        "16909060"
    );
    let calls = h.http.calls("/v3/account");
    assert_eq!(calls.len(), 2);
    let after = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
    let stamp = |index: usize| {
        calls[index].headers["APEX-TIMESTAMP"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
    };
    assert!((before..=after).contains(&stamp(0)));
    assert!(
        ((before / 1000.).floor() * 1000. ..=(after / 1000.).floor() * 1000.).contains(&stamp(1))
    );
    assert_eq!(
        stamp(1) % 1000.,
        0.,
        "legacy UTCString truncates milliseconds"
    );
    assert_ne!(
        calls[0].headers["APEX-SIGNATURE"],
        calls[1].headers["APEX-SIGNATURE"]
    );
    done(&h);
}
#[test]
fn repeated_signature_refusal_stops_after_one_resend() {
    let h = fixture();
    reset_route(&h, "/v3/account");
    h.http.route(
        "GET",
        "/v3/account",
        200.,
        json!({"code":20016,"msg":"signature rejected"}),
    );
    assert!(
        h.drive(h.venue().get_account())
            .unwrap_err()
            .message
            .contains("20016")
    );
    assert_eq!(h.http.calls("/v3/account").len(), 2);
    done(&h);
}
#[test]
fn http_status_failure_preserves_envelope_and_does_not_use_legacy_resend() {
    let h = fixture();
    reset_route(&h, "/v3/account");
    h.http.route(
        "GET",
        "/v3/account",
        400.,
        json!({"code":20016,"msg":"signature rejected"}),
    );
    let error = h.drive(h.venue().get_account()).unwrap_err();
    assert_eq!(error.response.as_ref().unwrap()["status"], 400.);
    assert_eq!(
        error.response.as_ref().unwrap()["data"]["msg"],
        "signature rejected"
    );
    assert!(error.config.is_some());
    assert_eq!(h.http.calls("/v3/account").len(), 1);
    done(&h);
}
#[test]
fn ambiguous_write_is_never_replayed() {
    let h = fixture();
    reset_route(&h, "/v3/order");
    h.http.route(
        "POST",
        "/v3/order",
        503.,
        json!({"code":500,"msg":"lost response"}),
    );
    assert!(
        h.drive(h.exchange.base().place_order(h.exchange.as_ref(), placed()))
            .is_err()
    );
    assert_eq!(h.http.calls("/v3/order").len(), 1);
    done(&h);
}
#[test]
fn read_disposal_guard_prevents_late_config_publication() {
    let h = fixture();
    h.store.update_zk_link_config(None).unwrap();
    let exchange = h.exchange.clone();
    *h.http.after_send.borrow_mut() = Some(Box::new(move || exchange.dispose().unwrap()));
    assert!(h.drive(h.venue().fetch_positions(None)).unwrap().is_empty());
    assert!(h.store.zk_link_config().is_none());
    assert!(h.store.positions().is_empty());
    done(&h);
}
#[test]
fn orders_disposal_guard_prevents_late_results() {
    let h = fixture();
    let exchange = h.exchange.clone();
    *h.http.after_send.borrow_mut() = Some(Box::new(move || exchange.dispose().unwrap()));
    assert!(h.drive(h.venue().fetch_orders(None)).unwrap().is_empty());
    assert!(h.store.orders().is_empty());
    done(&h);
}
