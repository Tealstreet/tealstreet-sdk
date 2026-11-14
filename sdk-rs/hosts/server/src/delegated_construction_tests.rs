use super::*;
use crate::auth::{Verifier, unsigned_token};
use crate::out::Out;
use crate::router_lease::{Assertion, Config, Heartbeat};
use crate::session::{Identity, Session};
use axum::http::{HeaderMap, Uri};
use safe_cex_native::{HttpRoute, WsRoute, WsRoutes};
use std::future::Future;
use std::task::Poll;

const A: &str = "synthetic-tenant-a/same-user";
const B: &str = "synthetic-tenant-b/same-user";

struct Received {
    transport_user: &'static str,
    user: String,
    key: Option<String>,
}

struct Loopback {
    plan: RoutingPlan,
    received: Arc<Mutex<Vec<Received>>>,
    server: tokio::task::JoinHandle<()>,
}

/// The real NetworkTransport and venue actor run against this loopback recorder.
/// Only verified synthetic owner names and synthetic public identifiers are kept.
async fn loopback() -> Loopback {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let record = received.clone();
    let app = axum::Router::new().fallback(move |uri: Uri, headers: HeaderMap| {
        let record = record.clone();
        async move {
            // WebSocket attempts also stay on loopback, without opening a feed.
            if uri.path() == "/ws" {
                return axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response();
            }
            let transport_user = if uri.path().starts_with("/a/") { A } else {
                assert!(uri.path().starts_with("/b/"), "account-specific loopback route");
                B
            };
            let token = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .expect("construction transport supplies proxy authorization");
            let verified = Verifier::unsigned().verify(token, now_ms()).expect("synthetic identity");
            let key = headers.get("x-bapi-api-key").map(|v| v.to_str().unwrap().to_owned());
            record.lock().unwrap().push(Received { transport_user, user: verified.user_id, key });
            axum::Json(json!({"retCode":0,"retMsg":"OK","result":{"category":"linear","list":[],"nextPageCursor":""}})).into_response()
        }
    });
    use axum::response::IntoResponse;
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let http = HttpRoute {
        prefix: format!("http://{address}"),
        authorize: true,
        ..Default::default()
    };
    let ws = WsRoute {
        host: Some(format!("ws://{address}/ws")),
        ..Default::default()
    };
    let plan = RoutingPlan {
        http: http.clone(),
        public_http: Some(http.clone()),
        read_http: Some(http.clone()),
        read_public_http: Some(http.clone()),
        public_read_http: Some(http),
        ws: WsRoutes {
            public: ws.clone(),
            private: ws.clone(),
            business: ws,
        },
        ..Default::default()
    };
    Loopback {
        plan,
        received,
        server,
    }
}

fn account_plan(mut plan: RoutingPlan, suffix: &str) -> RoutingPlan {
    for route in [
        Some(&mut plan.http),
        plan.public_http.as_mut(),
        plan.read_http.as_mut(),
        plan.read_public_http.as_mut(),
        plan.public_read_http.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        route.prefix.push_str(suffix);
    }
    plan
}

fn authenticated(registry: &Arc<Registry>, user: &str) -> (Session, crate::out::Receiver) {
    // Same verifier/Identity boundary as an authenticated upgrade, using the
    // existing unsigned fixture verifier rather than live credentials.
    let token = unsigned_token(user, now_ms() / 1000 + 300);
    let verified = Verifier::unsigned().verify(&token, now_ms()).unwrap();
    let identity = Identity {
        user: Some(verified.user_id),
        chat_handle: verified.chat_handle,
        token: Some((token, verified.expires_at_ms)),
        resume_from: None,
        resumable: false,
    };
    let (out, received, _) = Out::channel(32);
    (
        Session::new(registry.clone(), Arc::default(), identity, out),
        received,
    )
}

fn setup(key: &str) -> (InitPayload, DelegatedPair) {
    (
        InitPayload {
            account_name: "synthetic-main".into(),
            exchange_name: "bybit".into(),
            key: key.into(),
            ..Default::default()
        },
        DelegatedPair::new(key.into(), "synthetic-construction-material".into(), None),
    )
}

fn grant(registry: &Registry, a_epoch: u64) {
    let now = now_ms();
    registry
        .router_leases
        .accept(&Heartbeat {
            r#box: "synthetic-box".into(),
            issued_at: now,
            users: [A, B]
                .into_iter()
                .map(|user| Assertion {
                    user_id: user.into(),
                    owner_epoch: if user == A { a_epoch } else { 1 },
                    sequence: a_epoch,
                    not_after: now + 45_000,
                })
                .collect(),
        })
        .unwrap();
}

async fn queued<F: Future>(future: std::pin::Pin<&mut F>, registry: &Registry, count: usize) {
    let mut future = future;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx)))
                    .await
                    .is_pending()
            );
            if registry.threads.accounts() == count {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("real construction is queued behind the held engine lane");
}

fn startup_store() -> String {
    crate::envelope::msg(
        "synthetic-main",
        "store",
        json!({"activeChasers":[], "activeTWAPs":[]}),
    )
}

fn assert_construction_replies(
    received: &mut crate::out::Receiver,
    expected: Option<&str>,
    admitted: bool,
) {
    let mut found = false;
    while let Ok(text) = received.try_recv() {
        if admitted && text == startup_store() {
            continue;
        }
        assert!(!found, "construction replies contain no extra notification");
        assert_eq!(Some(text.as_str()), expected, "construction reply owner");
        found = true;
    }
    assert_eq!(found, expected.is_some(), "construction reply is delivered");
}

#[test]
fn construction_reply_checks_still_reject_foreign_notifications() {
    for foreign in ["synthetic-A-result", "synthetic-B-result", "unexpected"] {
        let (sender, mut received, _) = Out::channel(4);
        assert!(sender.send(startup_store()));
        assert!(sender.send(foreign.into()));
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                assert_construction_replies(&mut received, None, true);
            }))
            .is_err()
        );
    }
}

#[test]
fn construction_reply_checks_allow_startup_for_admitted_owner() {
    for expected in [None, Some("synthetic-A-result")] {
        let (sender, mut received, _) = Out::channel(4);
        assert!(sender.send(startup_store()));
        if let Some(reply) = expected {
            assert!(sender.send(reply.into()));
        }
        assert_construction_replies(&mut received, expected, true);
    }
}

#[test]
fn construction_reply_checks_reject_startup_for_never_admitted_owner() {
    let (sender, mut received, _) = Out::channel(4);
    assert!(sender.send(startup_store()));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_construction_replies(&mut received, None, false);
        }))
        .is_err(),
        "a never-admitted owner's outbox must reject even the startup snapshot"
    );
}

#[test]
fn construction_reply_checks_allow_empty_outbox_for_never_admitted_owner() {
    let (_sender, mut received, _) = Out::channel(4);
    assert_construction_replies(&mut received, None, false);
}

async fn construction_case(switch_user: bool, revoke_a: bool, apply_settings: bool) {
    let Loopback {
        plan,
        received,
        server,
    } = loopback().await;
    let mut registry = Registry::new(
        EngineThreads::start(1).unwrap(),
        Duration::from_millis(1),
        Duration::from_secs(60),
    )
    .with_router_lease(Config {
        box_id: Some("synthetic-box".into()),
        enforce: true,
        ..Default::default()
    });
    registry
        .construction_test_plans
        .insert(A.into(), account_plan(plan.clone(), "/a"));
    registry
        .construction_test_plans
        .insert(B.into(), account_plan(plan, "/b"));
    let registry = Arc::new(registry);
    grant(&registry, 1);
    let original_authority = registry.router_leases.get(A).unwrap().permit();
    let (mut caller, mut a_out) = authenticated(&registry, A);
    caller.open().await;
    let a_socket = caller.socket;

    let mut resident = None;
    if apply_settings {
        let (init, pair) = setup("synthetic-A-key");
        let account = caller.delegated_test_build(init, pair).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while !received
                .lock()
                .unwrap()
                .iter()
                .any(|event| event.key.as_deref() == Some("synthetic-A-key"))
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("resident A positive control signs against loopback");
        resident = Some(account);
    }

    // Hold the actual engine placement lane, after preparation and before spawn.
    let (release, blocked) = std::sync::mpsc::channel();
    let (entered, waiting) = tokio::sync::oneshot::channel();
    assert!(registry.threads.place(move |_| {
        entered.send(()).unwrap();
        blocked.recv_timeout(Duration::from_secs(20)).unwrap();
    }));
    waiting.await.unwrap();
    let (mut init, pair) = setup("synthetic-A-key");
    let a: std::pin::Pin<Box<dyn Future<Output = Result<Arc<Account>, String>>>> =
        if let Some(account) = &resident {
            init.signing_mode = Some("delegated".into());
            init.enable_spot_markets = Some(true);
            Box::pin(caller.settings_change(init, account.clone()))
        } else {
            Box::pin(caller.delegated_test_build(init, pair))
        };
    tokio::pin!(a);
    queued(a.as_mut(), &registry, 2 + usize::from(apply_settings)).await;
    if let Some(account) = &resident {
        assert!(account.handle.disposal_cause().is_none());
        received.lock().unwrap().clear();
        assert_construction_replies(&mut a_out, None, true);
    } else {
        assert!(received.lock().unwrap().is_empty());
    }

    let mut b_out = None;
    let mut b = None;
    if switch_user {
        let (next, out) = authenticated(&registry, B);
        caller = next;
        caller.open().await;
        b_out = Some(out);
        let (init, pair) = setup("synthetic-B-key");
        b = Some(Box::pin(caller.delegated_test_build(init, pair)));
        queued(
            b.as_mut().unwrap().as_mut(),
            &registry,
            3 + usize::from(apply_settings),
        )
        .await;
    }
    if revoke_a {
        grant(&registry, 2);
        assert!(!original_authority.allowed());
        assert!(registry.router_leases.get(A).unwrap().permit().allowed());
    }
    release.send(()).unwrap();
    let a = tokio::time::timeout(Duration::from_secs(10), a)
        .await
        .unwrap();
    let a = if revoke_a {
        assert!(matches!(a, Err(ref e) if e.contains("Router lease expired")));
        if let Some(resident) = &resident {
            assert!(registry.held(A, "synthetic-main", a_socket).is_some());
            assert!(!resident.running.io_allowed());
        } else {
            assert!(registry.held(A, "synthetic-main", a_socket).is_none());
        }
        None
    } else {
        let account = a.unwrap_or_else(|_| panic!("same-owner construction completes"));
        assert_eq!(account.user, A);
        if let Some(before) = &resident {
            assert!(!Arc::ptr_eq(before, &account));
            assert_eq!(before.registry_name(), account.registry_name());
            assert_ne!(before.fingerprint, account.fingerprint);
            assert!(Arc::ptr_eq(
                &before.delegated.as_ref().unwrap().provider(),
                &account.delegated.as_ref().unwrap().provider()
            ));
        }
        assert!(registry.held(A, &account.name, a_socket).is_some());
        assert!(!registry.holds(&account, caller.socket) || !switch_user);
        account.notify("synthetic-A-result".into());
        assert_construction_replies(&mut a_out, Some("synthetic-A-result"), true);
        Some(account)
    };
    let b = if let Some(b) = b {
        let account = tokio::time::timeout(Duration::from_secs(10), b)
            .await
            .unwrap()
            .unwrap_or_else(|_| panic!("fresh B construction completes"));
        assert_eq!(account.user, B);
        assert!(registry.held(B, &account.name, caller.socket).is_some());
        assert!(!registry.holds(&account, a_socket));
        account.notify(startup_store());
        assert_construction_replies(b_out.as_mut().unwrap(), None, true);
        account.notify("synthetic-B-result".into());
        assert_construction_replies(b_out.as_mut().unwrap(), Some("synthetic-B-result"), true);
        Some(account)
    } else {
        None
    };
    if let Some(account) = &a {
        account.notify(startup_store());
    }
    assert_construction_replies(&mut a_out, None, a.is_some());

    // Startup's signed private fee read exercises the actual injected provider
    // and the actual transport's per-operation proxy token, not a label-only mock.
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let ready = {
                let events = received.lock().unwrap();
                (revoke_a
                    || events.iter().any(|e| {
                        e.transport_user == A && e.key.as_deref() == Some("synthetic-A-key")
                    }))
                    && (!switch_user
                        || events.iter().any(|e| {
                            e.transport_user == B && e.key.as_deref() == Some("synthetic-B-key")
                        }))
            };
            if ready {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("positive controls reach signed loopback HTTP");
    {
        let events = received.lock().unwrap();
        for event in events.iter() {
            assert!(event.user == A || event.user == B);
            assert_eq!(
                event.user, event.transport_user,
                "proxy identity matches the account's transport"
            );
            if revoke_a {
                assert_ne!(event.user, A, "retired A work never reaches transport");
            }
            if let Some(key) = &event.key {
                assert!(
                    key == if event.user == A {
                        "synthetic-A-key"
                    } else {
                        "synthetic-B-key"
                    },
                    "provider and transport owners match"
                );
            }
        }
    }
    assert_eq!(
        registry.account_count(),
        usize::from(!revoke_a || apply_settings) + usize::from(switch_user)
    );
    if let Some(resident) = resident.filter(|_| revoke_a) {
        registry.dispose(&resident, Disposal::PageDisposed);
    }
    for account in [a, b].into_iter().flatten() {
        assert!(!account.is_public() && account.link.is_none() && account.delegated.is_some());
        registry.dispose(&account, Disposal::PageDisposed);
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while registry.threads.accounts() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("construction actors finish");
    assert_eq!(registry.threads.panics(), 0);
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn delegated_construction_same_authenticated_owner_control() {
    construction_case(false, false, false).await;
}

#[tokio::test]
async fn delegated_construction_authenticated_switch_keeps_original_attribution() {
    construction_case(true, false, false).await;
}

#[tokio::test]
async fn delegated_construction_authenticated_switch_drops_retired_work_and_builds_fresh_b() {
    construction_case(true, true, false).await;
}

#[tokio::test]
async fn delegated_settings_same_authenticated_owner_control() {
    construction_case(false, false, true).await;
}

#[tokio::test]
async fn delegated_settings_authenticated_switch_keeps_original_attribution() {
    construction_case(true, false, true).await;
}

#[tokio::test]
async fn delegated_settings_authenticated_switch_drops_retired_work_and_builds_fresh_b() {
    construction_case(true, true, true).await;
}
