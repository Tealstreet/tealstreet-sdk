use super::*;
use safe_cex_exchange::js_runtime::value::from_json;

fn with_account(test: impl FnOnce(&Rc<NativeAccount>)) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: crate::native_host::hyperliquid_rate_limiter(),
            exchange: "binance".into(),
            options: json!({"key":"native-key","secret":"native-secret"}).into(),
            account_name: None,
            sign: None,
            proxy_token: None,
        })
        .unwrap();
        test(&account);
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}

#[test]
fn native_worker_log_arguments_keep_arity_and_single_array_identity() {
    with_account(|account| {
        let events = Rc::new(RefCell::new(vec![]));
        let captured = events.clone();
        let unwatch = native_account_watch(
            account,
            Rc::new(move |event, data| {
                captured.borrow_mut().push((event.to_owned(), data.clone()))
            }),
            Rc::new(|_| {}),
        );
        for event in [
            "log",
            "error",
            "warning",
            "grafana-info",
            "grafana-error",
            "grafana-http-request",
            "grafana-http-response",
        ] {
            for args in [
                vec![],
                vec![json!(["array-valued argument"])],
                vec![json!("message"), json!({"severity":"info"})],
            ] {
                account
                    .exchange
                    .base()
                    .emit_args(event, &args.iter().map(from_json).collect::<Vec<_>>())
                    .unwrap();
                let (name, actual) = events.borrow_mut().pop().unwrap();
                assert_eq!(name, event);
                assert_eq!(
                    actual,
                    NativeValue::Array(args.into_iter().map(NativeValue::from).collect()),
                    "{event}"
                );
            }
        }
        let fill = json!({"symbol":"BTCUSDT","price":10.0});
        account
            .exchange
            .base()
            .emit("fill", from_json(&fill))
            .unwrap();
        assert_eq!(
            events.borrow_mut().pop().unwrap(),
            ("fill".into(), NativeValue::from(fill))
        );
        unwatch();
    });
}

#[test]
fn native_worker_forwards_both_private_websocket_diagnostic_events() {
    with_account(|account| {
        let events = Rc::new(RefCell::new(vec![]));
        let captured = events.clone();
        let unwatch = native_account_watch(
            account,
            Rc::new(move |event, data| {
                captured.borrow_mut().push((event.to_owned(), data.clone()))
            }),
            Rc::new(|_| {}),
        );
        for event in ["grafana-pws", "grafana-pws-sub"] {
            account
                .exchange
                .base()
                .emit(event, Js::str("redacted frame"))
                .unwrap();
        }
        assert_eq!(events.borrow().len(), 2);
        assert_eq!(
            events.borrow()[0],
            (
                "grafana-pws".into(),
                NativeValue::Array(vec![NativeValue::string("redacted frame")])
            )
        );
        assert_eq!(
            events.borrow()[1],
            (
                "grafana-pws-sub".into(),
                NativeValue::Array(vec![NativeValue::string("redacted frame")])
            )
        );
        unwatch();
    });
}
