//! On-box user drain and its authenticated proof; no exchange I/O.
mod support;

use safe_cex_exchange_server::auth::{Verifier, unsigned_token};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use support::Page;

async fn json_post(address: &str, path: &str, body: Value, extra: &str) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let body = body.to_string();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("");
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

fn token(user: &str) -> String {
    unsigned_token(
        user,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600,
    )
}

#[tokio::test]
async fn idle_user_moves_without_draining_the_box_and_latch_survives_close() {
    let app = support::app(
        support::config(Duration::from_secs(60)),
        Verifier::unsigned(),
    );
    let address = support::serve(app).await;
    let mut target = Page::open(&address, &token("target"), None).await;
    let mut other = Page::open(&address, &token("other"), None).await;
    let (status, started) = json_post(
        &address,
        "/admin/user-drain/start",
        json!({"userId":"target"}),
        "",
    )
    .await;
    assert_eq!(status, 200, "{started}");
    let drain_id = started["drainId"].clone();
    assert!(drain_id.is_string());
    assert_eq!(
        target.ending(Duration::from_secs(2)).await,
        support::Ending::Closed(4100, "draining: reconnect".into())
    );
    let (status, proof) = json_post(
        &address,
        "/admin/user-drain/status",
        json!({"userId":"target"}),
        "",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(proof["drainId"], drain_id);
    assert_eq!(proof["released"], true, "{proof}");
    assert_eq!(proof["hasRunningAlgo"], false);
    assert_eq!(support::health(&address).await.0, 200);
    other.send(json!({"t":"ping","d":null})).await;
    other.until("pong", |_| true).await;
    let (_, repeated) = json_post(
        &address,
        "/admin/user-drain/start",
        json!({"userId":"target","capMs":1}),
        "",
    )
    .await;
    assert_eq!(repeated["drainId"], drain_id);
    let (status, _) = json_post(
        &address,
        "/admin/user-drain/cancel",
        json!({"userId":"target","drainId":"stale"}),
        "",
    )
    .await;
    assert_eq!(status, 409);
    let (status, cancelled) = json_post(
        &address,
        "/admin/user-drain/cancel",
        json!({"userId":"target","drainId":drain_id}),
        "",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(cancelled["state"], "off");
    assert_eq!(cancelled["released"], false);
}

#[tokio::test]
async fn user_admin_rejects_forwarded_requests_and_proof_fails_closed_without_access_config() {
    let app = support::app(
        support::config(Duration::from_secs(60)),
        Verifier::unsigned(),
    );
    let address = support::serve(app).await;
    for action in ["start", "status", "cancel", "force"] {
        let (status, _) = json_post(
            &address,
            &format!("/admin/user-drain/{action}"),
            json!({"userId":"u","drainId":"d"}),
            "cf-connecting-ip: 1.2.3.4\r\n",
        )
        .await;
        assert_eq!(status, 403, "{action}");
    }
    let (status, _) = json_post(
        &address,
        "/proof/user-drain",
        json!({"userId":"u","drainId":"d"}),
        "Cf-Access-Jwt-Assertion: forged\r\n",
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(
        json_post(
            &address,
            "/admin/users",
            json!({}),
            "forwarded: for=1.2.3.4\r\n"
        )
        .await
        .0,
        403
    );
}

#[tokio::test]
async fn capped_drain_lists_parked_jobs_blocks_starts_and_cancel_keeps_them_running() {
    tokio::task::LocalSet::new().run_until(async {
        use safe_cex_exchange_server::{accounts::testing, envelope::InitPayload};
        let app = support::app(support::config(Duration::from_secs(60)), Verifier::unsigned());
        let init = InitPayload {account_name:"acct".into(), exchange_name:"bybit".into(), key:"test".into(), ..Default::default()};
        let account = testing::trading_account(&app.registry, "target", &init);
        let address = support::serve(app.clone()).await;
        let mut page = Page::open(&address, &token("target"), None).await;
        page.init(&init).await;
        page.until("frame", |d| d["snapshot"] == true).await;
        let chase = json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"});
        page.op("acct", "chaser:start", chase.clone()).await;
        let store = page.until("store", |d| d["data"]["activeChasers"].as_array().is_some_and(|r| r.len()==1)).await;
        let id = store["data"]["activeChasers"][0]["id"].clone();
        let (_, started) = json_post(&address, "/admin/user-drain/start", json!({"userId":"target","capMs":1}), "").await;
        assert!(started["drainId"].is_string(), "{started}");
        tokio::time::sleep(Duration::from_millis(20)).await;
        for op in ["chaser:start", "twap:start", "twap:resume", "twap:state", "exec"] {
            let mut data = chase.clone();
            if op == "twap:start" {
                data["accountName"] = "acct".into();
            }
            page.op("acct", op, data).await;
            let refused = page.until("error", |_| true).await;
            assert_eq!(refused["data"][0]["name"], "ServerDraining", "{refused}");
            assert_eq!(refused["data"][0]["outcome"], "notSent", "{refused}");
        }
        let (_, capped) = json_post(&address, "/admin/user-drain/status", json!({"userId":"target"}), "").await;
        assert_eq!(capped["state"], "capped", "{capped}");
        assert_eq!(capped["released"], false);
        assert_eq!(capped["runningJobs"], json!([{"accountName":"acct","kind":"chase","id":id}]));
        page.close().await;
        let (_, users) = json_post(&address, "/admin/users", json!({}), "").await;
        let target = users["users"].as_array().unwrap().iter().find(|u| u["userId"]=="target").unwrap();
        assert_eq!(target["liveSockets"], 0);
        assert_eq!(target["hasRunningAlgo"], true);
        assert_eq!(target["drain"]["drainId"], started["drainId"]);
        let (status, cancelled) = json_post(&address, "/admin/user-drain/cancel", json!({"userId":"target","drainId":started["drainId"]}), "").await;
        assert_eq!(status, 200);
        assert_eq!(cancelled["released"], false);
        assert_eq!(cancelled["hasRunningAlgo"], true);
        assert!(account.running.any());
        testing::algo_op(&account, "chaser:terminateAll", r#"{"keep":true}"#);
    }).await;
}

#[tokio::test]
async fn natural_completion_waits_for_the_whole_user_and_force_is_user_scoped() {
    tokio::task::LocalSet::new().run_until(async {
        use safe_cex_exchange_server::{accounts::testing, envelope::InitPayload};
        let app = support::app(support::config(Duration::from_secs(60)), Verifier::unsigned());
        let init = InitPayload {account_name:"acct".into(),exchange_name:"bybit".into(),key:"test".into(),..Default::default()};
        let target = testing::trading_account(&app.registry,"target",&init);
        let untouched = testing::trading_account(&app.registry,"other",&init);
        let address = support::serve(app.clone()).await;
        let mut page = Page::open(&address,&token("target"),None).await;
        page.init(&init).await;
        page.until("frame",|d|d["snapshot"]==true).await;
        let mut idle_tab = Page::open(&address,&token("target"),None).await;
        let mut other = Page::open(&address,&token("other"),None).await;
        other.init(&init).await;
        other.until("frame",|d|d["snapshot"]==true).await;
        let chase = json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"});
        page.op("acct","chaser:start",chase.clone()).await;
        let active = page.until("store", |d|d["data"]["activeChasers"].as_array().is_some_and(|r|r.len()==1)).await;
        other.op("acct","chaser:start",chase).await;
        other.until("store", |d|d["data"]["activeChasers"].as_array().is_some_and(|r|r.len()==1)).await;
        let (_, drain) = json_post(&address,"/admin/user-drain/start",json!({"userId":"target"}),"").await;
        assert_eq!(idle_tab.ending(Duration::from_millis(30)).await,support::Ending::StillOpen,"an idle tab cannot leave while its user's other tab runs an algo");
        page.op("acct","chaser:terminate",active["data"]["activeChasers"][0]["id"].clone()).await;
        assert_eq!(page.ending(Duration::from_secs(3)).await,support::Ending::Closed(4100,"draining: reconnect".into()));
        assert_eq!(idle_tab.ending(Duration::from_secs(3)).await,support::Ending::Closed(4100,"draining: reconnect".into()));
        let (_, status) = json_post(&address,"/admin/user-drain/status",json!({"userId":"target"}),"").await;
        assert_eq!(status["released"],true,"{status}");
        assert!(untouched.running.any());
        assert!(!target.running.any());
        let survivor = testing::trading_account(&app.registry, "survivor", &init);
        let mut survivor_page = Page::open(&address, &token("survivor"), None).await;
        survivor_page.init(&init).await;
        survivor_page.until("frame", |d| d["snapshot"] == true).await;
        survivor_page.op("acct", "chaser:start", json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"})).await;
        survivor_page.until("store", |d| d["data"]["activeChasers"].as_array().is_some_and(|r| r.len()==1)).await;
        let (_, other_drain) = json_post(&address,"/admin/user-drain/start",json!({"userId":"other","capMs":1}),"").await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let (code, forced) = json_post(&address,"/admin/user-drain/force",json!({"userId":"other","drainId":other_drain["drainId"]}),"").await;
        assert_eq!(code,200);
        assert_eq!(forced["settled"],true,"{forced}");
        assert_eq!(other.ending(Duration::from_secs(3)).await,support::Ending::Closed(4100,"draining: reconnect".into()));
        assert!(other.ops("error").iter().any(|d|d["data"][1]["algo"]["kind"]=="chase" && d["data"][1]["forceToast"]==true),"stop notice flushed before4100: {:?}",other.seen);
        assert_eq!(app.registry.user_drains.proof("target",drain["drainId"].as_str().unwrap())["released"],true);
        assert_eq!(support::health(&address).await.0,200);
        assert!(survivor.running.any(), "force must not stop another user's chase");
        survivor_page.send(json!({"t":"ping","d":null})).await;
        survivor_page.until("pong", |_| true).await;
    }).await;
}

#[tokio::test]
async fn queued_starts_cannot_run_after_the_latch_and_paused_twaps_are_inventory() {
    tokio::task::LocalSet::new().run_until(async {
        use safe_cex_exchange_server::{accounts::testing, envelope::InitPayload};
        let app = support::app(support::config(Duration::from_secs(60)), Verifier::unsigned());
        let init = InitPayload {account_name:"acct".into(),exchange_name:"bybit".into(),key:"test".into(),..Default::default()};
        let queued = testing::trading_account(&app.registry,"queued",&init);
        let paused = testing::trading_account(&app.registry,"paused",&init);
        let address = support::serve(app.clone()).await;
        let mut page = Page::open(&address,&token("paused"),None).await;
        page.init(&init).await;
        let frame = page.until("frame",|d|d["snapshot"]==true).await;
        assert!(frame.is_object());
        let market = json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true});
        page.op("acct","twap:start",json!({"accountName":"acct","activeMarket":market,"size":0.1,"duration":600,"ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"})).await;
        let store = page.until("store",|d|d["data"]["activeTWAPs"].as_array().is_some_and(|r|r.len()==1)).await;
        let id = store["data"]["activeTWAPs"][0]["id"].clone();
        page.op("acct","twap:pause",id.clone()).await;
        page.until("store",|d|d["data"]["activeTWAPs"][0]["status"]=="paused").await;
        page.close().await;
        let (_, inventory) = json_post(&address,"/admin/users",json!({}),"").await;
        let entry = inventory["users"].as_array().unwrap().iter().find(|u|u["userId"]=="paused").unwrap();
        assert_eq!(entry["liveSockets"],0);
        assert_eq!(entry["hasRunningAlgo"],true);
        assert_eq!(entry["runningJobs"],json!([{"accountName":"acct","kind":"twap","id":id}]));
        assert_eq!(entry["drain"],Value::Null);

        // The actor cannot poll between this enqueue and the synchronous latch.
        testing::algo_op(&queued,"chaser:start",r#"{"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"}"#);
        let drain = app.registry.start_user_drain("queued",1_800_000);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!queued.running.any(),"the queued start was refused at dispatch");
        assert_eq!(app.registry.user_drains.proof("queued",drain["drainId"].as_str().unwrap())["released"],true);
        assert!(paused.running.any(),"another user's parked TWAP was untouched");
        let status = app.registry.start_user_drain("paused",1_800_000);
        assert_eq!(status["hasRunningAlgo"],true);
        assert_eq!(status["released"],false);
        testing::algo_op(&paused,"twap:stop",&id.to_string());
    }).await;
}

#[tokio::test]
async fn box_break_glass_surfaces_the_interruption_toast_before_4100() {
    tokio::task::LocalSet::new().run_until(async {
        use safe_cex_exchange_server::{accounts::testing, envelope::InitPayload};
        let app = support::app(support::config(Duration::from_secs(60)), Verifier::unsigned());
        let init: InitPayload = serde_json::from_value(json!({"accountName":"acct","exchangeName":"bybit","key":"k"})).unwrap();
        let _account = testing::trading_account(&app.registry, "target", &init);
        let address = support::serve(app).await;
        let mut page = Page::open(&address, &token("target"), None).await;
        page.init(&init).await;
        page.until("frame", |d| d["snapshot"] == true).await;
        page.op("acct", "chaser:start", json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"})).await;
        page.until("store", |d| d["data"]["activeChasers"].as_array().is_some_and(|rows| rows.len() == 1)).await;
        let (status, _) = json_post(&address, "/admin/drain?now=1", json!({}), "").await;
        assert_eq!(status, 200);
        assert_eq!(page.ending(Duration::from_secs(3)).await, support::Ending::Closed(4100, "draining: reconnect".into()));
        assert!(page.ops("error").iter().any(|d| d["data"][1]["forceToast"] == true
            && d["data"][1]["algo"]["kind"] == "chase"), "{:?}", page.seen);
    }).await;
}

#[tokio::test]
async fn self_fence_parks_and_same_box_reconnect_gets_the_retained_notice() {
    tokio::task::LocalSet::new().run_until(async {
        use safe_cex_exchange_server::{accounts::{testing, now_ms}, envelope::InitPayload, router_lease};
        let mut config = support::config(Duration::from_secs(60));
        config.router_lease = router_lease::Config { box_id: Some("exchange-01".into()), enforce: true, ..Default::default() };
        let app = support::app(config, Verifier::unsigned());
        let init: InitPayload = serde_json::from_value(json!({"accountName":"acct","exchangeName":"bybit","key":"k"})).unwrap();
        let now = now_ms();
        let beat = |sequence, issued| router_lease::Heartbeat { r#box:"exchange-01".into(), issued_at:issued,
            users:vec![router_lease::Assertion { user_id:"target".into(), owner_epoch:1, sequence, not_after:issued+1500 }] };
        app.registry.router_leases.accept(&beat(1, now)).unwrap();
        let account = testing::trading_account(&app.registry, "target", &init);
        let address = support::serve(app.clone()).await;
        let mut page = Page::open(&address, &token("target"), None).await;
        page.init(&init).await;
        page.until("frame", |d| d["snapshot"] == true).await;
        page.op("acct", "chaser:start", json!({"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"})).await;
        page.until("store", |d| d["data"]["activeChasers"].as_array().is_some_and(|rows| rows.len() == 1)).await;
        // Parked accounts count too: no live socket is needed to expire a lease.
        page.close().await;
        tokio::time::sleep(Duration::from_millis(1600)).await;
        app.registry.park_expired_router_leases();
        assert!(app.registry.user_lease_fenced("target"));
        tokio::time::timeout(Duration::from_secs(2), async {
            while account.running.any() { tokio::time::sleep(Duration::from_millis(1)).await; }
        }).await.unwrap();
        let mut renewed = beat(2, now_ms());
        renewed.users[0].not_after = renewed.issued_at + 45_000;
        app.registry.router_leases.accept(&renewed).unwrap();
        let _fresh = testing::trading_account(&app.registry, "target", &init);
        let holdings = testing::holdings(&app.registry);
        assert_eq!(holdings.len(), 1);
        let (owner_user, owner_id) = holdings.keys().next().unwrap();
        assert_eq!(owner_user, "target");
        assert_eq!(owner_id.len(), 64);
        assert_eq!(_fresh.twap_owner.snapshot().owner_id,*owner_id);
        let mut returned = Page::open(&address, &token("target"), None).await;
        returned.init(&init).await;
        returned.until("error", |d| d["data"][1]["forceToast"] == true).await;
        assert!(returned.ops("error").iter().any(|d| d["data"][0].as_str().is_some_and(|text| text.contains("Router ownership expired"))));
        let market = json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
            "precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true});
        let options = json!({"accountName":"acct","activeMarket":market,"size":0.1,"duration":10,
            "ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,
            "useLimitOrders":false,"useChaserOrders":false,"origin":"web"});
        let now = now_ms();
        let state = json!({"accountName":"acct","activeMarket":market,"size":0.1,"ordersSizes":[0.01,0.01,0.01],
            "side":"buy","status":"running","ordersCount":10,"ordersExecuted":7,"nextOrderAt":now+60_000,
            "origin":"web","createdAt":now-180_000,"startedAt":now-180_000});
        // A valid current-binding row still receives the router's pause protection.
        let owner = _fresh.twap_owner.snapshot();
        returned.op("acct", "twap:state", json!([{"id":"parked-twap","ownerId":owner.owner_id,
            "ownerGeneration":owner.generation,"options":options,"state":state}])).await;
        let store = returned.until("store", |d| d["data"]["activeTWAPs"].as_array().is_some_and(|rows| !rows.is_empty())).await;
        assert_eq!(store["data"]["activeTWAPs"][0]["status"], "paused");
        assert_eq!(store["data"]["activeTWAPs"][0]["ordersExecuted"], 7);
        returned.close().await;
    }).await;
}

#[tokio::test]
async fn idle_release_route_fails_closed_without_access_config() {
    let app = support::app(
        support::config(Duration::from_secs(60)),
        Verifier::unsigned(),
    );
    let address = support::serve(app.clone()).await;
    let before = app.registry.user_drains.status("u");
    for extra in ["", "Cf-Access-Jwt-Assertion: forged\r\n"] {
        let (status, _) = json_post(
            &address,
            "/proof/user-idle-release",
            json!({"userId":"u","requestId":"12345678-1234-4234-8234-123456789abc"}),
            extra,
        )
        .await;
        assert_eq!(
            status, 403,
            "idle release must use the same fail-closed Access gate"
        );
        assert_eq!(app.registry.user_drains.status("u"), before);
    }
}
