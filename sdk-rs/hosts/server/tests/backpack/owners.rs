use super::{native, sessions, support::*};
use safe_cex_engine::{Engine, Method, test_support::ProbeVenue};
use safe_cex_exchange_server::{
    accounts::{Account, Registry, testing},
    algos::Running,
    auth::{Verifier, unsigned_token},
};
use safe_cex_native::{Host, ServerConfig, spawn_account};
use safe_cex_venue_backpack::{Backpack, Options, Pending, sign};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

struct Owned {
    account: Arc<Account>,
    device: Device,
    wire: mpsc::UnboundedReceiver<native::Observed>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

fn owned(registry: &Registry, user: &str, client: u32) -> Result<Owned> {
    let device = Device::new()?;
    device.link.suspend();
    let (remote, options) = device.remote()?;
    let options: safe_cex_venues::EngineOptions = serde_json::from_str(&options)?;
    let ids = options.signing.clone().ok_or("missing identities")?;
    let identity = ids.primary.clone().ok_or("missing primary")?;
    let body = json!({"symbol":SYMBOL,"side":"Bid","orderType":"Limit",
        "quantity":"1","price":"140","clientId":client})
    .to_string();
    let rows = super::rows(&super::Golden {
        method: "POST".into(),
        path: "/api/v1/order".into(),
        body: Some(body.clone()),
        canonical: String::new(),
        signatures: vec![],
    })?;
    let venue = ProbeVenue::new(Backpack::new(Options::default()), move |_, cx| {
        let request = cx.request(
            Method::Post,
            "https://api.backpack.exchange/api/v1/order".into(),
            vec![],
            Some(body.clone()),
            Pending::SocketSnapshot(String::new()),
        );
        cx.sign_http(
            request,
            Box::new(sign::Http::new(
                identity.clone(),
                sign::Request {
                    method: Method::Post,
                    path: "/api/v1/order".into(),
                    rows: rows.clone(),
                    body: Some(body.clone()),
                    window: 60_000,
                },
            )),
        );
    });
    let mut engine = Engine::new(
        venue,
        Some(safe_cex_engine::test_support::seeded_entropy(5)),
    )?;
    engine.set_signer(Some(remote.provider))?;
    engine.set_identities(ids)?;
    let (out, wire) = mpsc::unbounded_channel();
    let (handle, task) = spawn_account(
        Host::new(engine, native::Recorder(out)),
        ServerConfig::default(),
    )?;
    let init = init("fixture-key");
    let (mut account, _) = testing::account_on(
        testing::HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        },
        (user, &init.account_name, "backpack"),
        Some(device.link.clone()),
        init.fingerprint(),
        Arc::new(Running::new(Arc::new(tokio::sync::watch::channel(0).0))),
    );
    let slot = Arc::get_mut(&mut account).ok_or("account already shared")?;
    // Keep the registry's test account shell, with the real signed native actor.
    slot.handle.dispose();
    slot.handle = handle;
    testing::register(registry, &account);
    Ok(Owned {
        account,
        device,
        wire,
        task,
    })
}

fn token(user: &str) -> String {
    unsigned_token(
        user,
        safe_cex_exchange_server::accounts::now_ms() / 1000 + 60,
    )
}

async fn signature(page: &mut sessions::Page, owner: &mut Owned, client: u32) -> Result<Value> {
    let request = match page.seen.iter().find(|frame| frame["t"] == "sign-req") {
        Some(frame) => frame["d"].clone(),
        None => page.until("sign-req", |_| true).await,
    };
    let descriptor: Value = serde_json::from_str(
        request["wsAuth"]["toSign"]
            .as_str()
            .ok_or("no descriptor")?,
    )?;
    assert!(
        descriptor["payload"]
            .as_str()
            .ok_or("no payload")?
            .contains(&format!("clientId={client}&"))
    );
    owner
        .device
        .response(&json!({"t":"sign-req","d":request}).to_string())
}

async fn dispatched(owner: &mut Owned, client: u32) -> Result {
    let observed = tokio::time::timeout(Duration::from_secs(5), owner.wire.recv())
        .await?
        .ok_or("transport closed")?;
    let native::Observed::Http(packet, _) = observed else {
        return Err("expected HTTP".into());
    };
    assert_eq!(packet.method, Method::Post);
    assert_eq!(
        serde_json::from_str::<Value>(packet.body.as_deref().ok_or("no body")?)?["clientId"],
        client
    );
    assert_signature_shape(&packet)?;
    Ok(())
}

async fn proof(same_owner: bool) -> Result {
    let app = sessions::app(
        sessions::config(Duration::from_secs(30)),
        Verifier::unsigned(),
    );
    let mut a = owned(&app.registry, "user-a", 101)?;
    let mut b = owned(&app.registry, "user-b", 202)?;
    let address = sessions::serve(app.clone()).await;
    let mut page = sessions::Page::open(&address, &token("user-a"), None).await;
    page.init(&init("fixture-key")).await;
    let late_a = signature(&mut page, &mut a, 101).await?;
    assert!(a.wire.try_recv().is_err(), "A is awaiting its signature");
    let resume = page.resume_id.clone().ok_or("missing resume")?;
    page.close().await;

    // Authenticate the replacement while A's signing future remains pending.
    let user = if same_owner { "user-a" } else { "user-b" };
    let mut replacement = sessions::Page::open(&address, &token(user), Some(&resume)).await;
    let session = replacement
        .seen
        .iter()
        .find(|frame| frame["t"] == "session")
        .ok_or("missing session")?;
    assert_eq!(session["d"]["resumed"], same_owner);
    replacement.init(&init("fixture-key")).await;
    if same_owner {
        let replay = signature(&mut replacement, &mut a, 101).await?;
        assert_eq!(
            replay, late_a,
            "same-owner replay keeps the original request"
        );
        replacement.send(json!({"t":"sign-res","d":replay})).await;
        dispatched(&mut a, 101).await?;
    } else {
        let fresh_b = signature(&mut replacement, &mut b, 202).await?;
        assert_ne!(fresh_b["id"], late_a["id"]);
        replacement.send(json!({"t":"sign-res","d":late_a})).await;
        replacement.send(json!({"t":"ping"})).await;
        replacement.until("pong", |_| true).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(200), a.wire.recv())
                .await
                .is_err(),
            "A's late signature must not dispatch A's write"
        );
        assert!(
            b.wire.try_recv().is_err(),
            "A's result must not dispatch B's write"
        );
        assert!(
            replacement
                .seen
                .iter()
                .filter(|frame| frame["t"] == "sign-req")
                .all(|frame| frame["d"]["id"] == fresh_b["id"]),
            "A's request must not reach B"
        );
        replacement.send(json!({"t":"sign-res","d":fresh_b})).await;
        dispatched(&mut b, 202).await?;
        assert!(a.wire.try_recv().is_err());
    }
    replacement.close().await;
    a.account.handle.dispose();
    b.account.handle.dispose();
    a.task.await??;
    b.task.await??;
    assert!(a.wire.try_recv().is_err());
    assert!(b.wire.try_recv().is_err());
    Ok(())
}

#[tokio::test]
async fn backpack_deferred_signature_cannot_follow_a_different_authenticated_user() -> Result {
    tokio::task::LocalSet::new().run_until(proof(false)).await
}

#[tokio::test]
async fn backpack_deferred_signature_resumes_for_the_same_authenticated_user() -> Result {
    tokio::task::LocalSet::new().run_until(proof(true)).await
}
