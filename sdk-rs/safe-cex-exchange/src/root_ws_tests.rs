// Finite direct native witnesses for the three pending lifecycle rows.
#[test]
fn root_reconnect_now_clears_reference_without_cancelling_work() {
    let (s, p, r, protocol) = fixture();
    s.state.borrow_mut().reconnecting = true;
    s.schedule(Timer::Reconnect, 25.0);
    s.reconnect_now().unwrap();
    assert_eq!(protocol.connects.get(), 1, "ROOT reconnect native connect");
    assert!(!s.state.borrow().reconnecting, "ROOT reconnect clears flag");
    assert_eq!(
        s.state.borrow().last_close_time.to_bits(),
        1_000_000f64.to_bits(),
        "ROOT reconnect native timestamp"
    );
    assert!(!s.state.borrow().timer_ids.contains_key(&Timer::Reconnect));
    r.advance(25.0);
    assert_eq!(
        protocol.connects.get(),
        2,
        "ROOT direct reconnect retains old operation"
    );
    p.disposed.set(true);
    s.reconnect_now().unwrap();
    assert_eq!(protocol.connects.get(), 2);
    p.disposed.set(false);
    s.state.borrow_mut().force_closed = true;
    s.state.borrow_mut().reconnecting = true;
    s.reconnect_now().unwrap();
    assert_eq!(protocol.connects.get(), 2);
    assert!(!s.state.borrow().reconnecting);
    s.state.borrow_mut().force_closed = false;
    *protocol.connect_error.borrow_mut() = Some(ExchangeError::new("connect failed"));
    s.state.borrow_mut().reconnecting = true;
    assert_eq!(s.reconnect_now().unwrap_err().message, "connect failed");
    assert!(s.state.borrow().reconnecting);
    let source = root_ws_source("reconnect");
    assert_eq!(source["connects"], 1);
    assert_eq!(source["lastCloseTime"], 1_000_000);
    assert_eq!(source["cancelled"], 0);
}
#[test]
fn root_retry_failed_connect_announces_once_and_resets() {
    let (s, p, _, _) = fixture();
    *p.announcement_error.borrow_mut() = Some(ExchangeError::new("first reporter failed"));
    assert_eq!(
        s.retry_failed_connect("bad first").unwrap_err().message,
        "first reporter failed"
    );
    assert!(
        !s.state.borrow().connect_failure_announced,
        "ROOT retry first reporter failure retains flag"
    );
    assert_eq!(
        s.state.borrow().on_close_count,
        0,
        "ROOT retry first reporter failure skips close"
    );
    *p.announcement_error.borrow_mut() = None;
    s.retry_failed_connect("first").unwrap();
    s.retry_failed_connect("second").unwrap();
    assert_eq!(
        p.events
            .borrow()
            .iter()
            .filter(|(event, _)| event == "error")
            .count(),
        1,
        "ROOT retry native first announcement only"
    );
    assert_eq!(p.events.borrow()[0], ("error".into(), "first".into()));
    assert_eq!(
        p.logs
            .borrow()
            .iter()
            .filter(|(text, level)| text == "second" && *level == LogSeverity::Warning)
            .count(),
        1
    );
    assert!(s.state.borrow().connect_failure_announced);
    let close_count = s.state.borrow().on_close_count;
    *p.log_error.borrow_mut() = Some(ExchangeError::new("logger failed"));
    assert_eq!(
        s.retry_failed_connect("third").unwrap_err().message,
        "logger failed"
    );
    assert_eq!(
        s.state.borrow().on_close_count,
        close_count,
        "ROOT retry reporter fails before close"
    );
    *p.log_error.borrow_mut() = None;
    s.reset_failure_streak();
    assert!(!s.state.borrow().connect_failure_announced);
    s.retry_failed_connect("after reset").unwrap();
    assert_eq!(
        p.events
            .borrow()
            .iter()
            .filter(|(e, _)| e == "error")
            .count(),
        2
    );
    s.state.borrow_mut().force_closed = true;
    s.retry_failed_connect("ignored").unwrap();
    assert_eq!(
        p.events
            .borrow()
            .iter()
            .filter(|(e, _)| e == "error")
            .count(),
        2
    );
    let source = root_ws_source("retry");
    assert_eq!(source["announcements"], 2);
    assert_eq!(source["closes"], 3);
}
#[test]
fn root_offline_recovery_native_state_table_and_timer_cancel() {
    // Enumeration floor: managed readyState 0,1,2,3, then ordinary 0,1,2,3.
    for managed in [true, false] {
        for ready in [0, 1, 2, 3] {
            let (s, _, r, protocol) = fixture();
            let io = Rc::new(Socket::default());
            io.managed.set(managed);
            io.ready.set(ready);
            {
                let mut state = s.state.borrow_mut();
                state.native = Some(io.clone());
                state.close_failures = 8;
                state.on_close_count = 101;
                state.reconnect_count = 102;
                state.backoff_attempts = 9;
                state.connect_failure_announced = true;
                state.reconnecting = true;
            }
            assert!(s.recover_after_offline(true).unwrap());
            assert_eq!(
                io.reconnected.get(),
                usize::from(managed && ready == 1),
                "ROOT recovery managed OPEN only"
            );
            assert_eq!(
                io.closed.get(),
                usize::from(!managed && ready != 0),
                "ROOT recovery native CONNECTING preserved"
            );
            assert_eq!(
                s.state.borrow().on_close_count,
                if managed { 101 } else { 0 },
                "ROOT recovery native counters"
            );
            assert_eq!(
                s.state.borrow().reconnect_count,
                if managed { 102 } else { 0 }
            );
            assert_eq!(s.state.borrow().connect_failure_announced, managed);
            assert_eq!(protocol.connects.get(), usize::from(!managed && ready == 0));
            r.pump();
        }
    }
    let (s, p, r, protocol) = fixture();
    s.state.borrow_mut().on_close_count = 101;
    assert!(!s.recover_after_offline(false).unwrap());
    assert_eq!(s.state.borrow().on_close_count, 101);
    p.disposed.set(true);
    assert!(s.recover_after_offline(false).unwrap());
    p.disposed.set(false);
    s.state.borrow_mut().force_closed = true;
    assert!(s.recover_after_offline(false).unwrap());
    s.state.borrow_mut().force_closed = false;
    s.schedule(Timer::Reconnect, 25.0);
    assert!(s.recover_after_offline(true).unwrap());
    assert_eq!(protocol.connects.get(), 1);
    r.advance(25.0);
    assert_eq!(
        protocol.connects.get(),
        1,
        "ROOT recovery cancels old operation"
    );
    let relay = Rc::new(
        RelayWebSocket::new(
            Rc::new(RelayHost::new(&r)),
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue",
        )
        .unwrap(),
    );
    s.state.borrow_mut().relay = Some(relay.clone());
    s.state.borrow_mut().reconnecting = false;
    s.recover_after_offline(true).unwrap();
    assert_eq!(
        relay.ready_state(),
        0,
        "ROOT recovery relay CONNECTING preserved"
    );
    s.state.borrow_mut().reconnecting = true;
    s.recover_after_offline(true).unwrap();
    assert_eq!(protocol.connects.get(), 2);
    relay.on_inner_close(1000, "".into(), true);
    assert_eq!(relay.ready_state(), 3);
    s.state.borrow_mut().reconnecting = true;
    s.recover_after_offline(true).unwrap();
    assert_eq!(
        protocol.connects.get(),
        2,
        "ROOT recovery CLOSED relay closes without doubling connect"
    );
    assert_eq!(relay.ready_state(), 3);
    let host = Rc::new(RelayHost::new(&r));
    let relay = Rc::new(
        RelayWebSocket::new(
            host.clone(),
            "wss://relay/tunnel?ts-relay=wss%3A%2F%2Fvenue",
        )
        .unwrap(),
    );
    run(relay.on_inner_open()).unwrap();
    relay
        .on_inner_message(SocketFrame::Text(
            r#"{"type":"rpc.relay","status":"open"}"#.into(),
        ))
        .unwrap();
    assert_eq!(relay.ready_state(), 1, "ROOT recovery relay decoded OPEN");
    s.state.borrow_mut().relay = Some(relay.clone());
    s.recover_after_offline(true).unwrap();
    assert_eq!(
        *host.sockets.borrow()[0].close_codes.borrow(),
        vec![None],
        "ROOT recovery relay close code absent"
    );
    assert_eq!(
        relay.ready_state(),
        2,
        "ROOT recovery relay CLOSING after close"
    );
    s.recover_after_offline(true).unwrap();
    assert_eq!(
        *host.sockets.borrow()[0].close_codes.borrow(),
        vec![None],
        "ROOT recovery CLOSING relay close is inert"
    );
    assert_eq!(protocol.connects.get(), 2);
    let source = root_ws_source("recovery");
    assert_eq!(source["iterations"], 8);
}

fn root_ws_source(mode: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/root_rows_oracle.cjs"))
        .arg(mode)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ROOT WS actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn behavior_native_socket_redactor_omits_local_prototype_assignment() {
    let input = serde_json::json!({"__proto__":{"hidden":1},"ok":2});
    let mut native = input.clone();
    redact_json(&mut native, 0, false);
    assert!(
        native.get("__proto__").is_none(),
        "SOCKET REDACTOR native own-key omission"
    );
    assert_eq!(native["ok"], 2, "SOCKET REDACTOR native sibling");
    let (socket, parent, runtime, _) = fixture();
    runtime.diagnostics.set(true);
    socket
        .emit_ws_diagnostic_send(&input.to_string(), &WsDiagnosticSendOptions::default())
        .unwrap();
    socket
        .emit_ws_diagnostic_receive(&input, &WsDiagnosticReceiveOptions::default())
        .unwrap();
    assert_eq!(
        parent.events.borrow().len(),
        2,
        "SOCKET REDACTOR independent emitted event floor"
    );
    for (_, message) in parent.events.borrow().iter() {
        assert_eq!(
            message, "{\"ok\":2}",
            "SOCKET REDACTOR public emitted payload"
        );
    }
    assert_eq!(
        redact_payload(&input.to_string()),
        "{\"ok\":2}",
        "SOCKET REDACTOR emitted JSON projection"
    );
}
#[test]
fn recovery_policy_opt_out_keeps_timer_owned_until_disposal() {
    struct NoOfflineRecovery;
    impl WsProtocol for NoOfflineRecovery {
        fn recovers_after_offline(&self) -> bool {
            false
        }
        fn connect_and_subscribe(
            &self,
            _socket: Rc<BaseWebSocket>,
            _is_reconnect: bool,
        ) -> Result<(), ExchangeError> {
            Ok(())
        }
    }
    // Stop after unpolled and pending timer ownership; no broader scheduler claim.
    for pending in [false, true] {
        let (_, parent, runtime, _) = fixture();
        let socket = BaseWebSocket::new(2, parent, runtime.clone(), Rc::new(NoOfflineRecovery));
        socket.state.borrow_mut().on_close_count = 41;
        socket.schedule(Timer::Reconnect, 25.0);
        if pending {
            runtime.pump();
        }
        assert_eq!(
            socket.owned_tasks.len(),
            1,
            "RECOVERY-POLICY one owned timer"
        );
        assert!(
            !socket.recover_after_offline(true).unwrap(),
            "RECOVERY-POLICY protocol opt-out must skip recovery"
        );
        assert_eq!(
            socket.state.borrow().on_close_count,
            41,
            "RECOVERY-POLICY preserves state"
        );
        assert_eq!(
            socket.owned_tasks.len(),
            1,
            "RECOVERY-POLICY skipped work stays owned"
        );
        assert!(
            socket
                .state
                .borrow()
                .timer_ids
                .contains_key(&Timer::Reconnect)
        );
        socket.dispose();
        assert_eq!(
            socket.owned_tasks.len(),
            0,
            "RECOVERY-POLICY dispose cancels owned timer"
        );
        assert!(socket.state.borrow().timer_ids.is_empty());
        assert!(socket.state.borrow().tasks.is_empty());
        let weak = Rc::downgrade(&socket);
        drop(socket);
        runtime.pump();
        assert!(
            weak.upgrade().is_none(),
            "RECOVERY-POLICY no task retains socket after disposal"
        );
        runtime.advance(25.0);
        assert!(
            runtime.jobs.borrow().is_empty(),
            "RECOVERY-POLICY no timer job remains"
        );
    }
}
