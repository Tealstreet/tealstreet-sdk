#[test]
fn start_closed_relay_defers_all_connection_work_until_force_open() {
    let calls = Rc::new(Cell::new(0));
    let provider_calls = calls.clone();
    let (socket, runtime, host) = managed_relay_options(Some(WebSocketConnectionOptions {
        start_closed: Some(true),
        headers: Some(UpgradeHeaders::Fresh(Rc::new(move || {
            provider_calls.set(provider_calls.get() + 1);
            Default::default()
        }))),
        ..Default::default()
    }));
    assert_eq!(
        runtime.jobs.borrow().len(),
        0,
        "START_CLOSED native construction schedules no connection work"
    );
    assert!(socket.has_websocket(), "START_CLOSED native wrapper exists");
    assert_eq!(
        socket.state.borrow().native.as_ref().unwrap().ready_state(),
        3
    );
    assert!(!socket.is_connected());
    assert_eq!(calls.get(), 0);
    assert_eq!(host.dials.borrow().len(), 0);
    assert!(socket.recover_after_offline(true).unwrap());
    runtime.pump();
    assert_eq!(
        host.dials.borrow().len(),
        0,
        "START_CLOSED offline recovery stays closed"
    );
    socket.force_close();
    assert!(socket.has_websocket());
    runtime.relaying.set(false);
    socket.force_open().unwrap();
    assert_eq!(
        calls.get(),
        0,
        "START_CLOSED native reopen defers fresh headers"
    );
    assert_eq!(host.dials.borrow().len(), 0);
    for _ in 0..4 {
        runtime.pump();
    }
    assert!(socket.is_connected());
    assert_eq!(host.dials.borrow().len(), 1);
    assert_eq!(calls.get(), 1);
    socket.dispose();
    let output = std::process::Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/dormant_socket_oracle.cjs"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "START_CLOSED live source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["branches"], 4);
    for index in 0..3 {
        assert_eq!(source["cases"][index]["initialReady"], 3);
        assert_eq!(source["cases"][index]["activatedDials"], 1);
        assert_eq!(source["cases"][index]["providerCalls"], 1);
    }
    assert_eq!(source["cases"][3]["initialDials"], 1);
}

#[test]
fn start_closed_managed_native_is_lazy_but_plain_native_ignores_option() {
    for managed in [false, true] {
        let (socket, _, runtime, _) = fixture();
        runtime.headers.set(managed);
        run(socket.create_websocket(
            "wss://venue".into(),
            vec![],
            Some(WebSocketConnectionOptions {
                start_closed: Some(true),
                headers: Some(UpgradeHeaders::Static(Default::default())),
                ..Default::default()
            }),
        ))
        .unwrap();
        assert_eq!(
            runtime.dial_options.borrow().len(),
            usize::from(!managed),
            "START_CLOSED native branch managed={managed}"
        );
        assert!(socket.has_websocket());
        if managed {
            assert_eq!(
                socket.state.borrow().native.as_ref().unwrap().ready_state(),
                3
            );
            runtime.inline_spawn.set(true);
            socket.force_open().unwrap();
            assert_eq!(
                runtime.dial_options.borrow().len(),
                0,
                "START_CLOSED native inline spawn must yield before dialing"
            );
            for _ in 0..3 {
                runtime.pump();
            }
            assert_eq!(runtime.dial_options.borrow().len(), 1);
            assert_ne!(
                runtime.dial_options.borrow()[0].start_closed,
                Some(true),
                "START_CLOSED inner dial must activate instead of nesting dormancy"
            );
        }
        socket.dispose();
    }
}

#[test]
fn start_closed_preflight_is_outer_eager_and_inner_deferred() {
    let (socket, parent, runtime, _) = fixture();
    let error = run(socket.create_websocket(
        "wss://venue".into(),
        vec![],
        Some(WebSocketConnectionOptions {
            start_closed: Some(true),
            headers_required: true,
            ..Default::default()
        }),
    ))
    .unwrap_err();
    assert_eq!(error.name, "HeaderCapableWebSocketUnavailableError");
    assert!(!socket.has_websocket());
    assert_eq!(runtime.jobs.borrow().len(), 0);
    runtime.headers.set(true);
    runtime.relaying.set(true);
    let host = Rc::new(RelayHost::new(&runtime));
    host.supports_headers.set(false);
    *runtime.tunnel.borrow_mut() = Some(host.clone());
    run(socket.create_websocket(
        "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue".into(),
        vec![],
        Some(WebSocketConnectionOptions {
            start_closed: Some(true),
            headers_required: true,
            ..Default::default()
        }),
    ))
    .unwrap();
    assert!(socket.has_websocket());
    assert_eq!(host.dials.borrow().len(), 0);
    socket.force_open().unwrap();
    for _ in 0..3 {
        runtime.pump();
    }
    assert_eq!(host.dials.borrow().len(), 0);
    assert_eq!(
        socket.owned_tasks.len(),
        0,
        "START_CLOSED failed activation releases operation"
    );
    assert!(socket.state.borrow().native.is_some());
    assert!(parent.logs.borrow().iter().any(|(message, severity)| {
        *severity == LogSeverity::Error && message ==
            "WebSocket error: Required WebSocket upgrade headers need a header-capable ws transport"
    }), "START_CLOSED deferred validation failure is observable");
    socket.dispose();
}

#[test]
fn start_closed_cancels_active_inner_attempt_without_losing_descriptor() {
    let (socket, _, runtime, _) = fixture();
    runtime.headers.set(true);
    runtime.pending_dial.set(true);
    run(socket.create_websocket(
        "wss://venue".into(),
        vec![],
        Some(WebSocketConnectionOptions {
            start_closed: Some(true),
            headers: Some(UpgradeHeaders::Static(Default::default())),
            ..Default::default()
        }),
    ))
    .unwrap();
    socket.force_open().unwrap();
    for _ in 0..3 {
        runtime.pump();
    }
    assert_eq!(runtime.dial_options.borrow().len(), 1);
    assert_eq!(runtime.dial_drops.get(), 0);
    socket.force_close();
    assert_eq!(
        runtime.dial_drops.get(),
        1,
        "START_CLOSED native close releases pending dial"
    );
    assert!(socket.has_websocket());
    assert_eq!(socket.owned_tasks.len(), 0);
    runtime.pending_dial.set(false);
    socket.force_open().unwrap();
    for _ in 0..3 {
        runtime.pump();
    }
    assert_eq!(runtime.dial_options.borrow().len(), 2);
    socket.dispose();
    assert_eq!(socket.owned_tasks.len(), 0);
}

#[test]
fn start_closed_discard_dispose_and_early_close_release_activation() {
    for action in ["close", "discard", "dispose"] {
        let (socket, runtime, host) = managed_relay_options(Some(WebSocketConnectionOptions {
            start_closed: Some(true),
            ..Default::default()
        }));
        socket.force_open().unwrap();
        assert_eq!(socket.owned_tasks.len(), 1);
        match action {
            "close" => socket.force_close(),
            "discard" => runtime.jobs.borrow_mut().clear(),
            "dispose" => socket.dispose(),
            _ => unreachable!(),
        }
        runtime.pump();
        assert_eq!(
            host.dials.borrow().len(),
            0,
            "START_CLOSED {action} avoids inner dial"
        );
        assert_eq!(
            socket.owned_tasks.len(),
            0,
            "START_CLOSED {action} releases owner slot"
        );
        if action != "dispose" {
            socket.force_open().unwrap();
            for _ in 0..4 {
                runtime.pump();
            }
            assert_eq!(
                host.dials.borrow().len(),
                1,
                "START_CLOSED {action} retains activation descriptor"
            );
        }
        socket.dispose();
    }
}

#[test]
fn start_closed_retains_source_outer_reconnect_then_force_close_race() {
    let (socket, runtime, host) = managed_relay_options(Some(WebSocketConnectionOptions {
        start_closed: Some(true),
        ..Default::default()
    }));
    socket.reconnect().unwrap();
    socket.force_close();
    runtime.advance(1_000.0);
    for _ in 0..4 {
        runtime.pump();
    }
    assert_eq!(
        host.dials.borrow().len(),
        1,
        "START_CLOSED outer delayed reconnect retains source force-close race"
    );
    assert!(socket.is_force_closed());
    socket.dispose();
}

#[test]
fn start_closed_old_task_cleanup_keeps_reentrant_activation_cancellable() {
    let (socket, _, runtime, _) = fixture();
    runtime.headers.set(true);
    run(socket.create_websocket(
        "wss://venue".into(),
        vec![],
        Some(WebSocketConnectionOptions {
            start_closed: Some(true),
            headers: Some(UpgradeHeaders::Static(Default::default())),
            ..Default::default()
        }),
    ))
    .unwrap();
    runtime.fail_sleep.set(true);
    socket.force_open().unwrap();
    let weak = Rc::downgrade(&socket);
    let weak_runtime = Rc::downgrade(&runtime);
    *runtime.task_drop.borrow_mut() = Some(Box::new(move || {
        let socket = weak.upgrade().unwrap();
        let runtime = weak_runtime.upgrade().unwrap();
        runtime.fail_sleep.set(false);
        runtime.pending_dial.set(true);
        socket.force_open().unwrap();
        for _ in 0..3 {
            runtime.pump();
        }
    }));
    for _ in 0..3 {
        runtime.pump();
    }
    assert_eq!(runtime.dial_options.borrow().len(), 1);
    assert_eq!(runtime.dial_drops.get(), 0);
    assert_eq!(socket.owned_tasks.len(), 1);
    socket.force_close();
    assert_eq!(
        runtime.dial_drops.get(),
        1,
        "START_CLOSED old cleanup must retain reentrant dial cancellation"
    );
    assert_eq!(socket.owned_tasks.len(), 0);
    socket.dispose();
}

#[test]
fn start_closed_reentrant_reopen_wins_during_prior_cancellation() {
    for pending_reentrant in [false, true] {
        let (socket, _, runtime, _) = fixture();
        runtime.headers.set(true);
        runtime.pending_dial.set(true);
        run(socket.create_websocket(
            "wss://venue".into(),
            vec![],
            Some(WebSocketConnectionOptions {
                start_closed: Some(true),
                headers: Some(UpgradeHeaders::Static(Default::default())),
                ..Default::default()
            }),
        ))
        .unwrap();
        socket.force_open().unwrap();
        for _ in 0..3 {
            runtime.pump();
        }
        assert_eq!(runtime.dial_options.borrow().len(), 1);
        let weak = Rc::downgrade(&socket);
        let weak_runtime = Rc::downgrade(&runtime);
        *runtime.task_drop.borrow_mut() = Some(Box::new(move || {
            let socket = weak.upgrade().unwrap();
            let runtime = weak_runtime.upgrade().unwrap();
            runtime.pending_dial.set(pending_reentrant);
            socket.force_open().unwrap();
            for _ in 0..3 {
                runtime.pump();
            }
        }));
        socket.force_open().unwrap();
        for _ in 0..3 {
            runtime.pump();
        }
        assert_eq!(
            runtime.dial_options.borrow().len(),
            2,
            "START_CLOSED newer reentrant activation must win pending={pending_reentrant}"
        );
        socket.force_close();
        assert_eq!(runtime.dial_drops.get(), 2);
        if pending_reentrant {
            assert_eq!(socket.owned_tasks.len(), 0);
        }
        socket.dispose();
        assert_eq!(socket.owned_tasks.len(), 0);
    }
}

#[test]
fn start_closed_repeated_reopen_keeps_one_owned_inner_attempt() {
    let (socket, runtime, host) = managed_relay_options(Some(WebSocketConnectionOptions {
        start_closed: Some(true),
        ..Default::default()
    }));
    socket.force_open().unwrap();
    socket.force_open().unwrap();
    for _ in 0..4 {
        runtime.pump();
    }
    assert_eq!(
        host.dials.borrow().len(),
        1,
        "START_CLOSED chosen activation policy keeps one pending inner attempt"
    );
    socket.dispose();
}
