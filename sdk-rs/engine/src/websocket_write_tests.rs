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
    assert!(engine.claim_websocket_write(0., binding).is_err());
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
                engine
                    .claim_websocket_write(now, binding)
                    .ok()
                    .map(|(socket, text)| Action::WsSend { socket, text })
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
        engine.claim_websocket_write(0., binding),
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
