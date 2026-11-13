fn websocket_fixture(signed: bool) -> (Engine<Probe>, Context, SocketId) {
    let (mut engine, context, socket) = websocket_connecting_fixture(signed);
    engine.handle(0., Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    (engine, context, socket)
}

fn websocket_connecting_fixture(signed: bool) -> (Engine<Probe>, Context, SocketId) {
    let mut engine = crate::test_support::seeded_engine(Probe::default());
    engine.enable_websocket_write_host();
    engine.start(0.);
    engine.io.core.request_contexts.dispatch = Some((71, crate::classify::Intent::Place));
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let socket =
        cx.connect(SocketSpec::new("wss://venue.test/orders").channel(SocketChannel::Business));
    let context = cx
        .http_context(
            Scope::Command(71),
            Policy {
                operation: Operation::Mutation,
                credential: signed.then(|| crate::request_context::Credential {
                    slot: CredentialSlot::Primary,
                    reference: crate::signing::KeyRef("fixture".into()),
                }),
                timeout_ms: 1_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
        .unwrap();
    cx.io.core.request_contexts.dispatch = None;
    engine.drain_actions().for_each(drop);
    (engine, context, socket)
}

#[test]
fn websocket_write_claim_before_handshake_is_not_sent() {
    let (mut engine, context, socket) = websocket_connecting_fixture(false);
    let request = queue_websocket_write(&mut engine, &context, socket, false);
    let binding = engine
        .drain_actions()
        .find_map(WebSocketWriteBinding::from_action)
        .expect("bound write on a connecting socket");
    assert!(engine.claim_websocket_write(0., binding, &mut |_, _, _: &mut crate::WebSocketCommit<'_>| panic!("must not hand off")).is_err());
    assert!(engine.venue.releases.is_empty());
    assert!(engine.http_release(request).is_none());
    assert_eq!(engine.venue.failures.len(), 1);
    assert_eq!(engine.venue.failures[0].1, Some(crate::Outcome::NotSent));
    engine.handle(1., Event::WsOpened { socket });
    assert!(websocket_frames(&drain_websocket_actions(&mut engine)).is_empty());
}

fn queue_websocket_write(
    engine: &mut Engine<Probe>,
    context: &Context,
    socket: SocketId,
    signed: bool,
) -> RequestId {
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let write = cx
        .websocket_write(context.clone(), socket, "order".into())
        .unwrap();
    cx.send_websocket_write(
        write,
        signed.then(|| Box::new(WsRecipe) as Box<dyn crate::SignStep>),
        Pending(context.clone()),
    )
    .unwrap()
}

fn websocket_frames(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.as_str()),
            Action::Http(_) => panic!("unreleased transport work escaped"),
            Action::ManagedHttp(packet)
                if !matches!(packet.as_ref(), HttpAction::Cancel { .. }) =>
            {
                panic!("socket mutation escaped to HTTP executor")
            }
            _ => None,
        })
        .collect()
}

fn release_websocket_actions(
    engine: &mut Engine<Probe>,
    actions: Vec<Action>,
    now: f64,
) -> Vec<Action> {
    actions
        .into_iter()
        .filter_map(|action| match action {
            action @ Action::SignedOutbound {
                attempt: RequestId(0),
                ..
            } => {
                let binding = WebSocketWriteBinding::from_action(action).expect("typed binding");
                let mut frame = None;
                let _ = engine.claim_websocket_write(now, binding, &mut |socket, text, commit: &mut crate::WebSocketCommit<'_>| {
                    let mut text = Some(text);
                    commit(&mut || { frame = Some(Action::WsSend {socket, text:text.take().expect("one handoff")}); true })
                });
                frame
            }
            action => Some(action),
        })
        .collect()
}

fn drain_websocket_actions(engine: &mut Engine<Probe>) -> Vec<Action> {
    let actions = engine.drain_actions().collect();
    release_websocket_actions(engine, actions, engine.io.core.dispatch_now)
}

#[test]
fn websocket_write_requires_the_same_dispatch_admission_as_http() {
    for denied in [false, true] {
        let (mut engine, context, socket) = websocket_fixture(false);
        engine.venue.deny = denied;
        let request = queue_websocket_write(&mut engine, &context, socket, false);
        assert!(engine.venue.releases.is_empty());
        let actions = drain_websocket_actions(&mut engine);
        assert_eq!(
            websocket_frames(&actions),
            if denied { vec![] } else { vec!["order"] }
        );
        assert_eq!(engine.venue.releases.len(), 1);
        assert_eq!(engine.http_release(request).is_some(), !denied);
        if denied {
            assert_eq!(engine.venue.failures.len(), 1);
            assert_eq!(engine.venue.failures[0].1, Some(crate::Outcome::NotSent));
        }
    }
}

#[test]
fn websocket_write_signing_cannot_release_a_frame_before_authorization() {
    for denied in [false, true] {
        let (mut engine, context, socket) = websocket_fixture(true);
        queue_websocket_write(&mut engine, &context, socket, true);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(websocket_frames(&actions).is_empty());
        let (attempt, work) = work(&actions);
        assert!(engine.venue.releases.is_empty());
        engine.venue.deny = denied;
        let actions = signed(&mut engine, 10., attempt, work);
        let actions = release_websocket_actions(&mut engine, actions, 10.);
        assert_eq!(
            websocket_frames(&actions),
            if denied { vec![] } else { vec!["signed auth"] }
        );
        assert_eq!(engine.venue.releases.len(), 1);
    }
}

#[test]
fn websocket_write_rechecks_expiry_owner_and_socket_after_issuance() {
    for refusal in 0..3 {
        let (mut engine, context, socket) = websocket_fixture(false);
        queue_websocket_write(&mut engine, &context, socket, false);
        match refusal {
            0 => engine.prepare_dispatch(context.deadline()),
            1 => {
                Cx {
                    now: 1.,
                    store: &mut engine.store,
                    io: &mut engine.io,
                }
                .done_context(&context, "null".into())
                .unwrap();
            }
            _ => {
                Cx {
                    now: 1.,
                    store: &mut engine.store,
                    io: &mut engine.io,
                }
                .close(socket);
            }
        }
        let actions = drain_websocket_actions(&mut engine);
        assert!(websocket_frames(&actions).is_empty());
        assert!(engine.venue.releases.is_empty());
    }
}

#[test]
fn websocket_write_refuses_a_context_from_another_engine() {
    let (_, foreign, _) = websocket_fixture(false);
    let (mut engine, _, socket) = websocket_fixture(false);
    let cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    assert!(matches!(
        cx.websocket_write(foreign, socket, "order".into()),
        Err(Error::Revoked)
    ));
    assert!(cx.io.core.actions.is_empty());
}

fn assert_websocket_binding_rejects_previous_engine(signed_write: bool) {
    fn queue(
        engine: &mut Engine<Probe>,
        context: &Context,
        socket: SocketId,
        text: &str,
        signed_write: bool,
    ) -> (RequestId, WebSocketWriteBinding) {
        let mut cx = Cx {
            now: 0.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        let write = cx
            .websocket_write(context.clone(), socket, text.into())
            .unwrap();
        let request = cx
            .send_websocket_write(
                write,
                signed_write.then(|| Box::new(WsRecipe) as Box<dyn crate::SignStep>),
                Pending(context.clone()),
            )
            .unwrap();
        let actions = engine.drain_actions().collect::<Vec<_>>();
        let actions = if signed_write {
            let (attempt, work) = work(&actions);
            signed(engine, 1., attempt, work)
        } else {
            actions
        };
        let binding = actions
            .into_iter()
            .find_map(WebSocketWriteBinding::from_action)
            .expect("deferred final handoff");
        (request, binding)
    }

    let (mut a, context_a, socket_a) = websocket_fixture(signed_write);
    let (request_a, binding_a) = queue(&mut a, &context_a, socket_a, "A-order", signed_write);
    drop(a);
    let (mut b, context_b, socket_b) = websocket_fixture(signed_write);
    let (request_b, binding_b) = queue(&mut b, &context_b, socket_b, "B-order", signed_write);
    assert!(!context_a.same_issuer(&context_b));
    assert_eq!(context_a.owner(), context_b.owner());
    assert_eq!(socket_a, socket_b);
    assert_eq!(request_a, request_b);
    assert_eq!(
        binding_a.preparation().attempt(),
        binding_b.preparation().attempt()
    );
    assert_ne!(binding_a.preparation(), binding_b.preparation());
    assert_eq!(b.websocket_write_socket(&binding_b), Some(socket_b));
    let foreign_socket = b.websocket_write_socket(&binding_a);
    let mut frames = Vec::new();
    let mut handoff = |socket: SocketId, text: String, commit: &mut crate::WebSocketCommit<'_>| {
        commit(&mut || {
            frames.push((socket, text.clone()));
            true
        })
    };
    assert_eq!(
        b.claim_websocket_write(2., binding_a, &mut handoff),
        Err(Error::Revoked),
        "a deferred A frame cannot borrow B's repeated counters"
    );
    assert_eq!(foreign_socket, None);
    assert!(b.venue.releases.is_empty(), "foreign work never admits");
    assert!(b.venue.failures.is_empty(), "B's pending work stays live");
    assert!(b.http_release(request_b).is_none());
    assert_eq!(b.websocket_write_socket(&binding_b), Some(socket_b));
    assert_eq!(
        b.claim_websocket_write(2., binding_b, &mut handoff),
        Ok(())
    );
    assert_eq!(
        frames,
        vec![(
            socket_b,
            if signed_write { "signed auth" } else { "B-order" }.into()
        )],
        "only B's current binding reaches the physical handoff"
    );
    assert_eq!(b.venue.releases.len(), 1);
    assert!(b.http_release(request_b).is_some());
}

#[test]
fn websocket_write_binding_rejects_previous_engine_with_repeated_counters() {
    assert_websocket_binding_rejects_previous_engine(false);
}

#[test]
fn websocket_write_signed_binding_rejects_previous_engine_with_repeated_counters() {
    assert_websocket_binding_rejects_previous_engine(true);
}

#[test]
fn websocket_write_cannot_reuse_a_consumed_permit() {
    let (mut engine, context, socket) = websocket_fixture(false);
    let request = queue_websocket_write(&mut engine, &context, socket, false);
    // Hold the binding so the test can clone the internal single-use token.
    let prepare = engine
        .io
        .core
        .actions
        .iter()
        .find_map(|action| match action {
            Action::SignedOutbound {
                attempt: RequestId(0),
                outbound,
            } => match outbound.as_ref() {
                Action::ManagedHttp(binding) => match binding.as_ref() {
                    HttpAction::Prepare { preparation, .. } => Some(*preparation),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    engine.io.core.actions.clear();
    engine.http_prepared(
        0.,
        prepare,
        Prepared {
            id: socket.0,
            expires_at: None,
        },
    );
    let bind = engine
        .io
        .core
        .actions
        .iter()
        .find(|action| {
            matches!(
                action,
                Action::SignedOutbound {
                    attempt: RequestId(0),
                    ..
                }
            )
        })
        .unwrap()
        .clone();
    engine.io.core.actions.push(bind);
    let actions = drain_websocket_actions(&mut engine);
    assert_eq!(websocket_frames(&actions), vec!["order"]);
    assert_eq!(engine.venue.releases.len(), 1);
    assert!(engine.http_release(request).is_some());
}

#[test]
fn websocket_write_response_uses_the_shared_write_fact_journal() {
    let (mut engine, context, socket) = websocket_fixture(false);
    let request = queue_websocket_write(&mut engine, &context, socket, false);
    assert_eq!(
        websocket_frames(&drain_websocket_actions(&mut engine)),
        vec!["order"]
    );
    engine.handle(
        10.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "ok",
            },
        },
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 71, .. })
    )));
    assert!(engine.io.core.managed_http.is_empty());
}

#[test]
fn websocket_control_frames_keep_their_existing_path() {
    let (mut engine, _, socket) = websocket_fixture(false);
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.send(socket, "subscribe");
    cx.send_signed(socket, "auth".into(), Box::new(WsRecipe));
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(websocket_frames(&actions), vec!["subscribe"]);
    let (attempt, work) = work(&actions);
    let actions = signed(&mut engine, 10., attempt, work);
    assert_eq!(websocket_frames(&actions), vec!["signed auth"]);
    assert!(engine.venue.releases.is_empty());
}

#[test]
fn websocket_write_host_claim_rechecks_authority_after_drain() {
    for refusal in 0..3 {
        let (mut engine, context, socket) = websocket_fixture(false);
        queue_websocket_write(&mut engine, &context, socket, false);
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(engine.venue.releases.is_empty());
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::WsSend { .. }))
        );
        let at = if refusal == 0 { context.deadline() } else { 1. };
        if refusal == 1 {
            Cx {
                now: at,
                store: &mut engine.store,
                io: &mut engine.io,
            }
            .done_context(&context, "null".into())
            .unwrap();
        } else if refusal == 2 {
            Cx {
                now: at,
                store: &mut engine.store,
                io: &mut engine.io,
            }
            .close(socket);
        }
        let actions = release_websocket_actions(&mut engine, actions, at);
        assert!(websocket_frames(&actions).is_empty());
        assert!(engine.venue.releases.is_empty());
    }
}

#[test]
fn websocket_write_hosts_without_a_claim_handoff_fail_before_queueing() {
    let (mut engine, context, socket) = websocket_fixture(true);
    engine.io.core.websocket_write_host = false;
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let write = cx
        .websocket_write(context.clone(), socket, "order".into())
        .unwrap();
    assert!(matches!(
        cx.send_websocket_write(write, Some(Box::new(WsRecipe)), Pending(context)),
        Err((_, Error::Invalid))
    ));
    assert!(cx.io.core.actions.is_empty());
    assert!(cx.io.core.managed_http.is_empty());
    assert!(cx.io.pending.is_empty());
    assert!(cx.io.core.signing.is_empty());
}

#[test]
fn websocket_write_rechecks_the_grant_between_construction_and_submission() {
    let (mut engine, context, socket) = websocket_fixture(false);
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let write = cx
        .websocket_write(context.clone(), socket, "order".into())
        .unwrap();
    cx.now = context.deadline();
    assert!(matches!(
        cx.send_websocket_write(write, None, Pending(context)),
        Err((_, Error::TimedOut))
    ));
    assert!(cx.io.core.managed_http.is_empty());
    assert!(cx.io.core.actions.is_empty());
}

#[test]
fn websocket_write_requires_the_same_signing_policy_as_http() {
    for credentialed in [false, true] {
        let (mut engine, context, socket) = websocket_fixture(credentialed);
        let mut cx = Cx {
            now: 0.,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        let write = cx
            .websocket_write(context.clone(), socket, "order".into())
            .unwrap();
        let recipe = (!credentialed).then(|| Box::new(WsRecipe) as Box<dyn crate::SignStep>);
        assert!(matches!(
            cx.send_websocket_write(write, recipe, Pending(context)),
            Err((_, Error::Invalid))
        ));
        assert!(cx.io.core.managed_http.is_empty());
        assert!(cx.io.core.actions.is_empty());
    }
}

#[test]
fn websocket_write_cannot_borrow_a_read_only_http_permit() {
    let (mut engine, tokens) = start(Probe {
        count: 1,
        ..Probe::default()
    });
    engine.http_prepared(
        0.,
        tokens[0],
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let packet = engine.drain_actions().find(|action|
        matches!(action, Action::ManagedHttp(packet) if matches!(packet.as_ref(), HttpAction::Bind { .. }))
    ).expect("read-only HTTP binding");
    let binding = WebSocketWriteBinding::from_action(Action::SignedOutbound {
        attempt: RequestId(0),
        outbound: Box::new(packet),
    })
    .unwrap();
    assert_eq!(engine.websocket_write_socket(&binding), None);
    assert_eq!(
        engine.claim_websocket_write(0., binding, &mut |_, _, _: &mut crate::WebSocketCommit<'_>| panic!("must not hand off")),
        Err(Error::Revoked)
    );
    assert!(engine.venue.releases.is_empty());
}

#[test]
fn websocket_write_uses_shared_pacing_without_implicit_resend() {
    let (mut engine, root, socket) = websocket_fixture(false);
    let mut contexts = Vec::new();
    for _ in 0..2 {
        let context = Cx {
            now: 0.,
            store: &mut engine.store,
            io: &mut engine.io,
        }
        .http_leg_context(
            &root,
            Policy {
                operation: Operation::Mutation,
                credential: None,
                timeout_ms: 1_000,
                pacing: Some(GROUP),
                retry: RetryPolicy::Never,
            },
        )
        .unwrap();
        contexts.push(context);
    }
    for context in &contexts {
        queue_websocket_write(&mut engine, context, socket, false);
    }
    let actions = drain_websocket_actions(&mut engine);
    assert_eq!(websocket_frames(&actions), vec!["order"]);
    assert_eq!(engine.venue.releases.len(), 1);
    assert_eq!(engine.venue.failures.len(), 1);
    assert!(websocket_frames(&drain_websocket_actions(&mut engine)).is_empty());
}

#[test]
fn websocket_write_signing_recipe_cannot_replace_the_authorized_socket() {
    struct Retarget;
    impl crate::SignStep for Retarget {
        fn prepare(
            &mut self,
            action: &mut Action,
            cx: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            let Action::WsSend { socket, .. } = action else {
                panic!("expected socket recipe");
            };
            *socket = SocketId(999);
            crate::SignStep::prepare(&mut WsRecipe, action, cx)
        }
        fn apply(
            &mut self,
            _: &mut Action,
            _: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            unreachable!()
        }
        fn freshness_ms(&self) -> f64 {
            3_000.
        }
    }
    let (mut engine, context, socket) = websocket_fixture(true);
    let mut cx = Cx {
        now: 0.,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    let write = cx
        .websocket_write(context.clone(), socket, "order".into())
        .unwrap();
    cx.send_websocket_write(write, Some(Box::new(Retarget)), Pending(context))
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(websocket_frames(&actions).is_empty());
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Sign { .. }))
    );
    assert!(engine.venue.releases.is_empty());
    assert_eq!(engine.venue.failures.len(), 1);
}


#[test]
fn websocket_write_cannot_commit_through_http_claim() {
    let (mut engine, context, socket)=websocket_fixture(false);
    let request=queue_websocket_write(&mut engine,&context,socket,false);
    let permit=engine.drain_actions().find_map(|action| match action {
        Action::SignedOutbound {attempt:RequestId(0),outbound} => match *outbound {
            Action::ManagedHttp(packet) => match *packet { HttpAction::Bind {permit,..}=>Some(permit), _=>None },
            _=>None,
        },
        _=>None,
    }).expect("bound socket permit");
    assert_eq!(engine.claim_http(0.,permit),Err(Error::Revoked));
    assert!(engine.http_release(request).is_none());
    assert!(engine.venue.releases.is_empty());
    assert_eq!(engine.venue.failures[0].1,Some(crate::Outcome::NotSent));
}
