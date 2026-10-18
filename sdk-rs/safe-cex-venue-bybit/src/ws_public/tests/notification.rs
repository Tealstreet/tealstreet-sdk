use super::*;

fn failure() -> ExchangeError {
    let mut error = ExchangeError::new("subscriber rejected");
    error.name = "TypeError".into();
    error.store_subscriber_ids = vec![71, 29];
    error.with_retained_response(json!({"code": "listener-identity"}).into())
}

/// Subscriber-cause Rc identity differs per store call; compare it by presence.
fn without_cause(mut error: ExchangeError) -> ExchangeError {
    assert!(
        error.store_subscriber_cause.take().is_some(),
        "subscriber cause retained"
    );
    error
}
fn expected_store_failure(store: &dyn Store) -> ExchangeError {
    let error = store
        .update_socket_health(&safe_cex_types::retained_json::RetainedJsonInput::Object(
            vec![],
        ))
        .unwrap_err();
    let safe_cex_store::StoreMutationError::Notification(notification) = error else {
        panic!("expected subscriber notification");
    };
    ExchangeError::from(safe_cex_store::StoreMutationError::Notification(
        notification,
    ))
}

#[test]
fn notification_remote_auth_reporter_failure_precedes_false_return() {
    let (socket, parent, runtime) = fixture();
    let expected = failure();
    *parent.failure.borrow_mut() = Some(("error", expected.clone()));
    let mut base = safe_cex_exchange::exchange::BaseExchange::new(
        safe_cex_types::ExchangeName::Bybit,
        parent.options.clone(),
        parent.store.clone(),
        runtime.clone(),
        Default::default(),
    );
    base.signers.remote_signer = Some(Rc::new(AuthSigner {
        native_extra_data: RefCell::default(),
        requests: RefCell::default(),
        response: r#"{"error":"unavailable"}"#.into(),
        wrong_variant: false,
        pending: false,
    }));
    assert_eq!(
        ready(crate::ws_private::local_auth::auth(
            &socket, &base, None, 1000.
        )),
        Err(expected),
        "remote auth reporter failure must not become false success"
    );
    assert!(runtime.wire.0.borrow().is_empty());
    base.dispose().unwrap();
}

#[test]
fn notification_failure_aborts_reauth_before_reconnect() {
    for inline in [false, true] {
        let (socket, parent, runtime) = fixture();
        runtime.inline.set(inline);
        let expected = failure();
        *parent.failure.borrow_mut() = Some(("error", expected.clone()));
        crate::ws_private::send_reconnect_sequence(
            &socket,
            runtime.clone(),
            || Box::pin(async { Ok(()) }),
            || Box::pin(async { Err(ExchangeError::new("signer rejected")) }),
            true,
            true,
        );
        reconnect_sequence::pump(&runtime);
        assert_eq!(
            *runtime.exceptions.borrow(),
            [expected],
            "reauth notification identity"
        );
        assert_eq!(
            runtime.wire.1.get(),
            0,
            "report failure must prevent reconnect"
        );
        assert!(runtime.wire.0.borrow().is_empty());
        assert!(
            !parent
                .events
                .borrow()
                .iter()
                .any(|(event, _)| event == "snapshot")
        );
    }
}

#[test]
fn notification_failure_from_no_socket_reconnect_reaches_owned_tail() {
    for inline in [false, true] {
        let (_, parent, runtime) = fixture();
        let socket = BaseWebSocket::new(2, parent.clone(), runtime.clone(), Rc::new(Protocol));
        runtime.inline.set(inline);
        let second = Rc::new(Cell::new(0));
        let observed = second.clone();
        let original = failure();
        let _first = Store::subscribe(
            parent.store.as_ref(),
            Box::new(move |_, _| Err(original.clone().into())),
        );
        let _second = Store::subscribe(
            parent.store.as_ref(),
            Box::new(move |_, _| {
                observed.set(observed.get() + 1);
                Ok(())
            }),
        );
        let expected = expected_store_failure(parent.store.as_ref());
        let writes = parent.health_writes.get();
        let tasks = runtime.tasks.borrow().len();
        crate::ws_private::send_reconnect_sequence(
            &socket,
            runtime.clone(),
            || Box::pin(async { Ok(()) }),
            || Box::pin(async { Ok(false) }),
            true,
            true,
        );
        reconnect_sequence::pump(&runtime);
        assert_eq!(
            runtime
                .exceptions
                .borrow()
                .iter()
                .cloned()
                .map(without_cause)
                .collect::<Vec<_>>(),
            [without_cause(expected)],
            "reconnect notification identity"
        );
        assert_eq!(
            parent.health_writes.get(),
            writes + 1,
            "one health publication"
        );
        assert_eq!(
            parent
                .store
                .socket_health()
                .unwrap()
                .unwrap()
                .into_value()
                .unwrap()["publicHealthy"],
            true,
            "health commits before subscriber failure"
        );
        assert_eq!(second.get(), 0, "health failure skips later subscriber");
        assert_eq!(
            runtime.tasks.borrow().len(),
            tasks + 1,
            "no retry task after failed health publication"
        );
        assert!(runtime.wire.0.borrow().is_empty());
    }
}

#[test]
fn notification_failure_prevents_message_dispatch_and_preserves_send_order() {
    let (socket, parent, runtime) = fixture();
    let expected = failure();
    *parent.failure.borrow_mut() = Some(("health", expected.clone()));
    assert_eq!(
        on_message(&socket, r#"{"topic":"tickers.BTCUSDT"}"#),
        Err(expected.clone()),
        "receive health notification identity"
    );
    *parent.failure.borrow_mut() = Some(("emit", expected.clone()));
    runtime.diagnostics.set(true);
    assert_eq!(
        send_ohlcv_diagnostic_payload(&socket, "subscribe", &["kline.1.BTCUSDT".into()]),
        Err(expected.clone()),
        "diagnostic send notification identity"
    );
    assert_eq!(
        runtime.wire.0.borrow().len(),
        1,
        "send precedes diagnostic notification"
    );
    assert_eq!(
        emit_ohlcv_diagnostic_receive(&socket, r#"{"topic":"kline.1.BTCUSDT"}"#),
        Err(expected.clone()),
        "diagnostic receive notification identity"
    );
}

#[test]
fn notification_no_socket_send_reports_unawaited_reconnect_and_continues() {
    let (_, parent, runtime) = fixture();
    let socket = BaseWebSocket::new(2, parent.clone(), runtime.clone(), Rc::new(Protocol));
    let original = failure();
    let _subscription = Store::subscribe(
        parent.store.as_ref(),
        Box::new(move |_, _| Err(original.clone().into())),
    );
    let expected = expected_store_failure(parent.store.as_ref());
    let topics: Vec<_> = (0..11).map(|i| format!("tickers.{i}")).collect();
    assert_eq!(
        ready(subscribe(
            &socket,
            json!({"tickers":topics}).as_object().unwrap(),
            runtime.as_ref()
        )),
        Ok(()),
        "source subscribe does not await its reconnect"
    );
    assert_eq!(
        *runtime.sleeps.borrow(),
        [10, 10],
        "both chunks and their delays still run"
    );
    runtime.diagnostics.set(true);
    parent.events.borrow_mut().clear();
    assert_eq!(
        crate::ws_private::subscribe(&socket),
        Ok(()),
        "private caller continues past the unawaited reconnect"
    );
    assert!(
        !parent.events.borrow().is_empty(),
        "private diagnostic still emits"
    );
    // The detached reporter yields its first poll before reporting.
    reconnect_sequence::pump(&runtime);
    reconnect_sequence::pump(&runtime);
    assert!(
        !runtime.exceptions.borrow().is_empty()
            && runtime
                .exceptions
                .borrow()
                .iter()
                .all(|e| without_cause(e.clone()) == without_cause(expected.clone())),
        "detached reconnect rejection reaches the reporter with its subscriber identity"
    );
}

#[test]
fn notification_no_socket_send_retains_fallback_subscriber_identity() {
    let (_, parent, runtime) = fixture();
    let socket = BaseWebSocket::new(2, parent.clone(), runtime.clone(), Rc::new(Protocol));
    let _subscription = Store::subscribe(
        parent.store.as_ref(),
        Box::new(|_, _| {
            Err(safe_cex_types::StoreSubscriberError::with_response(
                "fallback listener",
                json!({"code":419}),
            ))
        }),
    );
    let error = parent
        .store
        .update_socket_health(&safe_cex_types::retained_json::RetainedJsonInput::Object(
            vec![],
        ))
        .unwrap_err();
    let safe_cex_store::StoreMutationError::Notification(notification) = error else {
        panic!("expected fallback subscriber notification");
    };
    let mut expected =
        ExchangeError::new("fallback listener").with_retained_response(json!({"code":419}).into());
    expected.store_subscriber_ids = vec![notification.subscriber_id()];
    assert_eq!(
        ready(subscribe(
            &socket,
            json!({"tickers":["tickers.BTCUSDT"]}).as_object().unwrap(),
            runtime.as_ref()
        )),
        Ok(()),
        "fallback subscribe does not await its reconnect"
    );
    reconnect_sequence::pump(&runtime);
    reconnect_sequence::pump(&runtime);
    let reported = runtime.exceptions.borrow().first().cloned();
    let reported = reported.expect("fallback reconnect reaches the reporter");
    assert_eq!(
        (
            &reported.message,
            &reported.retained_response_input(),
            &reported.store_subscriber_ids
        ),
        (
            &expected.message,
            &expected.retained_response_input(),
            &expected.store_subscriber_ids
        ),
        "fallback reconnect notification identity"
    );
    assert!(
        reported.store_subscriber_cause.is_some() && reported.origin.is_some(),
        "fallback reconnect keeps its subscriber cause and Store origin"
    );
}
