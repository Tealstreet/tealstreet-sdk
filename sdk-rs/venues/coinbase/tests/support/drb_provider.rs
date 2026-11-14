//! Synthetic provider custody controls for both gateway authentication channels.
use super::*;
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Capture {
    local: Provider,
    ids: safe_cex_engine::signing::Identities,
    requests: Mutex<Vec<SignRequest>>,
    deferred: AtomicBool,
}
impl Capture {
    fn run(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        self.requests.lock().unwrap().push(request.clone());
        let local = self.local.sign_ready(request).unwrap()?;
        let [
            safe_cex_engine::signing::Operation::Present { key },
            safe_cex_engine::signing::Operation::Sign { payload, .. },
        ] = request.operations.as_slice()
        else {
            panic!("Coinbase JWT signing graph");
        };
        let [safe_cex_engine::signing::PayloadPart::Bytes { bytes }] = payload.as_slice() else {
            panic!("Coinbase JWS signing bytes");
        };
        let identity = self
            .ids
            .primary
            .as_ref()
            .filter(|identity| identity.credential == *key)
            .or_else(|| {
                self.ids
                    .secondary
                    .as_ref()
                    .filter(|identity| identity.credential == *key)
            })
            .unwrap()
            .clone();
        let ids = safe_cex_engine::signing::Identities {
            primary: Some(identity),
            secondary: None,
        };
        let (remote, kind) = safe_cex_venues::hosted_signing::prepare(
            "coinbase",
            &ids,
            safe_cex_venues::hosted_signing::Input::WsAuth {
                to_sign: String::from_utf8(bytes.clone()).unwrap(),
                output_format: serde_json::from_str("\"coinbase-jws\"").unwrap(),
            },
        )?;
        assert_eq!(
            serde_json::to_value(&remote.operations).unwrap(),
            serde_json::to_value(&request.operations).unwrap()
        );
        let hosted = self.local.sign_ready(&remote).unwrap()?;
        assert_eq!(local.values, hosted.values);
        safe_cex_venues::hosted_signing::finish(kind, &hosted)?;
        Ok(local)
    }
}
impl Signer for Capture {
    fn immediate(&self) -> bool {
        !self.deferred.load(Ordering::Relaxed)
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        (!self.deferred.load(Ordering::Relaxed)).then(|| self.run(request))
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        if self.deferred.load(Ordering::Relaxed) {
            Box::pin(std::future::pending())
        } else {
            Box::pin(std::future::ready(self.run(&request)))
        }
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
}

fn provider_rig(read_key: bool) -> (Rig, Arc<Capture>) {
    provider_rig_for(read_key, "primary")
}
fn provider_rig_for(read_key: bool, user: &str) -> (Rig, Arc<Capture>) {
    let mut setup: safe_cex_venues::EngineOptions = serde_json::from_str("{}").unwrap();
    setup.key = Some(["organizations", "provider-test", "apiKeys", user].join("/"));
    setup.secret = Some(STANDARD.encode([7; 32]));
    if read_key {
        setup.read_credentials = Some(
            serde_json::from_value(json!({
                "key":(["organizations","provider-test","apiKeys","read"].join("/")),
                "secret":STANDARD.encode([8;32]),
            }))
            .unwrap(),
        );
    }
    let (local, ids) = safe_cex_venues::prepare_local_signer_for("coinbase", &mut setup).unwrap();
    let primary = setup
        .coinbase_signing
        .take()
        .map(|binding| serde_json::from_value(serde_json::to_value(binding).unwrap()).unwrap())
        .zip(ids.primary.clone());
    let secondary = setup
        .coinbase_read_signing
        .take()
        .map(|binding| serde_json::from_value(serde_json::to_value(binding).unwrap()).unwrap())
        .zip(ids.secondary.clone());
    let venue = Coinbase::with_signing(Options::default(), primary, secondary).unwrap();
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    let signer = Arc::new(Capture {
        local,
        ids: ids.clone(),
        requests: Mutex::new(vec![]),
        deferred: AtomicBool::new(false),
    });
    e.set_signer(Some(signer.clone())).unwrap();
    e.set_identities(ids).unwrap();
    let now = 1_700_000_000_000.;
    e.start(now);
    let mut r = Rig {
        e,
        now,
        http: Default::default(),
        timers: vec![],
        actions: vec![],
    };
    if read_key {
        for _ in 0..2 {
            let permissions = r.request();
            assert!(permissions.url.ends_with("/key_permissions"));
            r.reply(permissions, json!({"portfolio_uuid":"portfolio-one"}));
        }
    }
    (r, signer)
}

fn claims(packet: &Value) -> Value {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let token = packet["params"]["token"].as_str().unwrap();
    let parts: Vec<_> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap()
}

#[test]
fn provider_drb_http_auth_uses_the_primary_provider_and_egress() {
    for read_key in [false, true] {
        let (mut r, signed) = provider_rig(read_key);
        r.select_intx();
        r.collect();
        let auth = r
            .http
            .iter()
            .find(|request| request.url.ends_with("/api/v2/public/auth"))
            .expect("provider-backed gateway authentication must dispatch");
        let packet: Value = serde_json::from_str(auth.body.as_deref().unwrap()).unwrap();
        assert!(
            auth.headers
                .iter()
                .any(|(name, value)| { *name == "Content-Type" && value == "application/json" })
        );
        let claims = claims(&packet);
        assert_eq!(claims["iss"], "cdp");
        assert_eq!(
            claims["sub"],
            ["organizations", "provider-test", "apiKeys", "primary"].join("/")
        );
        assert!(claims.get("aud").is_none());
        assert_eq!(claims["uri"], "POST drb.coinbase.com/api/v2/public/auth");
        assert!(signed.requests.lock().unwrap().iter().any(|request| {
            request.context.host == "drb.coinbase.com"
                && request.context.path == "/api/v2/public/auth"
        }));
    }
}

#[test]
fn provider_drb_auth_route_is_primary_with_a_read_key() {
    let (mut r, _) = provider_rig(true);
    r.select_intx();
    r.e.prepare_dispatch(r.now);
    let intents: Vec<_> = r
        .e
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                safe_cex_engine::request_context::HttpAction::Prepare {
                    intent, request, ..
                } if request.url.ends_with("/api/v2/public/auth") => Some(intent),
                _ => None,
            },
            _ => None,
        })
        .collect();
    // Managed requests carry their authority in Prepare.intent, independently
    // of HttpRequest.route (the legacy transport override).
    assert_eq!(
        intents,
        vec![safe_cex_engine::request_context::RoutingIntent::Primary]
    );
}

#[test]
fn provider_drb_bearer_reads_keep_the_primary_session_egress() {
    let (r, _) = provider_rig(true);
    let mut r = Rig::ready_with(r);
    for _ in 0..30 {
        r.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, timer) = r.timers.remove(0);
        r.now = r.now.max(at);
        r.e.handle(r.now, Event::TimerFired { timer });
        r.e.prepare_dispatch(r.now);
        let mut intents = vec![];
        for action in r.e.drain_actions().collect::<Vec<_>>() {
            match action {
                Action::ManagedHttp(action) => {
                    if let safe_cex_engine::request_context::HttpAction::Prepare {
                        intent,
                        request,
                        ..
                    } = *action
                        && request.url.contains("/api/v2/private/")
                    {
                        intents.push(intent);
                    }
                }
                Action::SetTimer { at_ms, timer } => r.timers.push((at_ms, timer)),
                _ => {}
            }
        }
        if !intents.is_empty() {
            assert_eq!(
                intents,
                vec![safe_cex_engine::request_context::RoutingIntent::Primary; 3]
            );
            return;
        }
    }
    panic!("private gateway poll was not issued");
}

#[test]
fn provider_drb_websocket_auth_has_its_own_primary_provider_signature() {
    let (r, signed) = provider_rig(false);
    let mut r = Rig::ready_with(r);
    let before = signed.requests.lock().unwrap().len();
    let socket = r.private_socket();
    r.e.handle(r.now, Event::WsOpened { socket });
    r.collect();
    let packet = r
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::WsSend {
                socket: owner,
                text,
            } if *owner == socket => {
                let packet: Value = serde_json::from_str(text).unwrap();
                (packet["method"] == "public/auth").then_some(packet)
            }
            _ => None,
        })
        .expect("provider-backed websocket authentication must dispatch");
    let claims = claims(&packet);
    assert_eq!(claims["iss"], "cdp");
    assert!(claims.get("uri").is_none());
    assert!(claims.get("aud").is_none());
    assert_eq!(signed.requests.lock().unwrap().len(), before + 1);
}

#[test]
fn provider_drb_retired_socket_signature_cannot_authenticate_its_replacement() {
    let (r, signer) = provider_rig(false);
    let mut r = Rig::ready_with(r);
    let old = r.private_socket();
    signer.deferred.store(true, Ordering::Relaxed);
    r.e.handle(r.now, Event::WsOpened { socket: old });
    r.collect();
    let (attempt, work) = r
        .actions
        .iter()
        .find_map(|action| match action {
            Action::Sign { attempt, work, .. } if work.context.host == "drb.coinbase.com" => {
                Some((*attempt, work.clone()))
            }
            _ => None,
        })
        .expect("old socket's authentication is deferred");
    r.e.handle(
        r.now,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    r.collect();
    let mut replacement = None;
    for _ in 0..30 {
        r.timers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (at, timer) = r.timers.remove(0);
        r.now = r.now.max(at);
        r.e.handle(r.now, Event::TimerFired { timer });
        r.collect();
        replacement = r.actions.iter().rev().find_map(|action| match action {
            Action::WsConnect { socket, url, .. }
                if *socket != old && url == crate::drb::WS_URL =>
            {
                Some(*socket)
            }
            _ => None,
        });
        if replacement.is_some() {
            break;
        }
    }
    let replacement = replacement.expect("a new physical socket is connected");
    r.e.handle(
        r.now,
        Event::WsOpened {
            socket: replacement,
        },
    );
    r.collect();
    let (next, fresh_work) = r
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::Sign {
                attempt: next,
                work,
                ..
            } if *next != attempt && work.context.host == "drb.coinbase.com" => {
                Some((*next, work.clone()))
            }
            _ => None,
        })
        .expect("replacement owns a new signing attempt");
    r.e.handle(
        r.now,
        Event::Signed {
            attempt,
            result: signer.local.sign_ready(&work).unwrap(),
        },
    );
    r.collect();
    assert!(!r.actions.iter().any(
        |action| matches!(action,Action::WsSend {socket,..} if *socket==old || *socket==replacement)
    ));
    signer.deferred.store(false, Ordering::Relaxed);
    r.e.handle(
        r.now,
        Event::Signed {
            attempt: next,
            result: signer.run(&fresh_work),
        },
    );
    r.collect();
    assert!(
        !r.actions
            .iter()
            .any(|action| matches!(action,Action::WsSend {socket,..} if *socket==old))
    );
    assert!(r.actions.iter().any(|action| match action {
        Action::WsSend { socket, text } if *socket == replacement =>
            serde_json::from_str::<Value>(text).unwrap()["method"] == "public/auth",
        _ => false,
    }));
}

#[test]
fn provider_drb_delayed_user_a_signature_cannot_authenticate_user_b() {
    fn pending(r: &mut Rig, signer: &Capture) -> (safe_cex_engine::RequestId, SignRequest) {
        signer.deferred.store(true, Ordering::Relaxed);
        r.e.handle(
            r.now,
            Event::WsOpened {
                socket: r.private_socket(),
            },
        );
        r.collect();
        r.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::Sign { attempt, work, .. } if work.context.host == "drb.coinbase.com" => {
                    Some((*attempt, work.clone()))
                }
                _ => None,
            })
            .unwrap()
    }
    let (a, signer_a) = provider_rig_for(false, "user-a");
    let mut a = Rig::ready_with(a);
    let (_, work_a) = pending(&mut a, &signer_a);
    let socket_a = a.private_socket();
    a.e.handle(
        a.now,
        Event::WsClosed {
            socket: socket_a,
            code: 1006,
        },
    );
    a.collect();
    let (b, signer_b) = provider_rig_for(false, "user-b");
    let mut b = Rig::ready_with(b);
    let (attempt_b, _) = pending(&mut b, &signer_b);
    b.e.handle(
        b.now,
        Event::Signed {
            attempt: attempt_b,
            result: signer_a.run(&work_a),
        },
    );
    b.collect();
    assert!(
        !b.actions.iter().any(|a| matches!(a, Action::WsSend { .. })),
        "cross-user delayed signing must not authenticate the active user"
    );
    // A fresh B engine accepts only B's own signing result after the switch.
    let (fresh_b, fresh_signer) = provider_rig_for(false, "user-b");
    let mut fresh_b = Rig::ready_with(fresh_b);
    let (attempt, work) = pending(&mut fresh_b, &fresh_signer);
    fresh_b.e.handle(
        fresh_b.now,
        Event::Signed {
            attempt,
            result: fresh_signer.run(&work),
        },
    );
    fresh_b.collect();
    assert!(
        fresh_b
            .actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}
