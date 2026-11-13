//! Reviewer reproductions through account startup and real hosted algo dispatch.
mod support;
use safe_cex_exchange_server::{
    accounts::{now_ms, testing},
    auth::{Verifier, unsigned_token},
    envelope::InitPayload,
    interruption, router_lease,
};
use serde_json::json;
use std::time::Duration;

// Independent valid rows for each trusted binding; never production migration logic.
fn bound_row(
    account: &safe_cex_exchange_server::accounts::Account,
    id: &str,
    options: &serde_json::Value,
    state: &serde_json::Value,
) -> serde_json::Value {
    let owner = account.twap_owner.snapshot();
    json!({"id":id,"ownerId":owner.owner_id,"ownerGeneration":owner.generation,"options":options,"state":state})
}

fn account_identity(registry: &safe_cex_exchange_server::accounts::Registry) -> String {
    let holdings = testing::holdings(registry);
    assert_eq!(holdings.len(), 1);
    let (user, identity) = holdings.keys().next().unwrap();
    assert_eq!(user, "target");
    assert_eq!(identity.len(), 64);
    assert!(
        identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    identity.clone()
}

async fn restored_state(reason: Option<interruption::Reason>) {
    tokio::task::LocalSet::new().run_until(async {
    let mut config=support::config(Duration::from_secs(60));
    config.router_lease=router_lease::Config{box_id:Some("02".into()),enforce:true,..Default::default()};
    let app=support::app(config,Verifier::unsigned());
    let now=now_ms();
    app.registry.router_leases.accept(&router_lease::Heartbeat{r#box:"02".into(),issued_at:now,
      users:vec![router_lease::Assertion{user_id:"target".into(),owner_epoch:2,sequence:1,not_after:now+45000}]}).unwrap();
    let lease=app.registry.router_leases.get("target").unwrap();
    assert_eq!(lease.status().generation,0,"fresh destination working control");
    assert!(lease.permit().allowed());
    if let Some(reason) = reason {
      app.registry.interruptions.record("target",interruption::Marker{reason,from_box:"01".into(),at:now}).unwrap();
    }
    let init:InitPayload=serde_json::from_value(json!({"accountName":"acct","exchangeName":"bybit","key":"k"})).unwrap();
    let _account=testing::trading_account(&app.registry,"target",&init);
    let owner_id=account_identity(&app.registry);
    assert_eq!(_account.twap_owner.snapshot().owner_id,owner_id,"TWAP restore tags follow registry identity");
    let address=support::serve(app).await;
    let mut page=support::Page::open(&address,&unsigned_token("target",now/1000+3600),None).await;
    page.init(&init).await;
    if reason.is_some() {
      let marker=page.until("error",|d|d["data"][1]["forceToast"]==true).await;
      assert!(marker["data"][0].as_str().unwrap().contains("previous server"),"working destination warning");
    } else {
      page.until("frame", |d| d["snapshot"] == true).await;
    }
    let market=json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
      "precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true});
    let options=json!({"accountName":"acct","activeMarket":market,"size":0.1,"duration":10,"ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"});
    let state=json!({"accountName":"acct","activeMarket":market,"size":0.1,"ordersSizes":[0.01,0.01,0.01],"side":"buy","status":"running","ordersCount":10,"ordersExecuted":7,"nextOrderAt":now+60000,"origin":"web","createdAt":now-180000,"startedAt":now-180000});
    page.op("acct","twap:state",json!([bound_row(&_account,"transferred-twap",&options,&state)])).await;
    let store=page.until("store",|d|d["data"]["activeTWAPs"].as_array().is_some_and(|r|!r.is_empty())).await;
    assert_eq!(store["data"]["activeTWAPs"][0]["ordersExecuted"],7,"progress preserved");
    assert_eq!(store["data"]["activeTWAPs"][0]["status"], if reason.is_some() { "paused" } else { "running" }, "interrupted cross-box restore must not auto-resume merely because the destination never fenced");
    if reason.is_some() {
      page.op("acct", "twap:resume", json!("transferred-twap")).await;
      let resumed = page.until("store", |d| d["data"]["activeTWAPs"].as_array().is_some_and(|rows| rows.first().is_some_and(|row| row["status"] == "running"))).await;
      assert!(resumed["data"]["activeTWAPs"][0]["ordersExecuted"].as_u64().unwrap() >= 7);
    } else {
      assert!(!page.ops("error").iter().any(|d| d["data"][1]["forceToast"] == true));
    }
    page.close().await;
 }).await;
}

#[tokio::test]
async fn lease_expired_marker_pauses_a_fresh_destinations_restored_twap() {
    restored_state(Some(interruption::Reason::LeaseExpired)).await;
}
#[tokio::test]
async fn owner_released_marker_pauses_a_fresh_destinations_restored_twap() {
    restored_state(Some(interruption::Reason::OwnerReleased)).await;
}
#[tokio::test]
async fn delegated_restart_marker_restores_client_twap_paused_then_explicitly_resumes() {
    let reason = serde_json::from_value(json!("waiting-for-key")).expect("typed restart reason");
    restored_state(Some(reason)).await;
}
#[tokio::test]
async fn normal_restore_without_a_marker_keeps_its_running_state() {
    restored_state(None).await;
}

async fn resume_survives_rebuild(same_box: bool) {
    tokio::task::LocalSet::new().run_until(async {
    let mut config=support::config(Duration::from_secs(60));
    config.router_lease=router_lease::Config{box_id:Some("02".into()),enforce:true,..Default::default()};
    let app=support::app(config,Verifier::unsigned());
    let now=now_ms();
    app.registry.router_leases.accept(&router_lease::Heartbeat{r#box:"02".into(),issued_at:now,
      users:vec![router_lease::Assertion{user_id:"target".into(),owner_epoch:2,sequence:1,not_after:now+45000}]}).unwrap();
    let lease=app.registry.router_leases.get("target").unwrap();
    assert_eq!(lease.status().generation,0,"fresh destination working control");
    assert!(lease.permit().allowed());
    if same_box {
      let old = lease.permit();
      app.registry.router_leases.accept(&router_lease::Heartbeat { r#box:"02".into(), issued_at:now,
        users:vec![router_lease::Assertion { user_id:"target".into(), owner_epoch:3, sequence:1, not_after:now+45000 }] }).unwrap();
      assert!(!old.allowed());
      assert_eq!(lease.status().generation, 1);
    } else {
      app.registry.interruptions.record("target",interruption::Marker{reason:interruption::Reason::LeaseExpired,from_box:"01".into(),at:now}).unwrap();
    }
    let init:InitPayload=serde_json::from_value(json!({"accountName":"acct","exchangeName":"bybit","key":"k"})).unwrap();
    let _account=testing::trading_account(&app.registry,"target",&init);
    let owner_id=account_identity(&app.registry);
    assert_eq!(_account.twap_owner.snapshot().owner_id,owner_id,"TWAP restore tags follow registry identity");
    let address=support::serve(app.clone()).await;
    let mut page=support::Page::open(&address,&unsigned_token("target",now/1000+3600),None).await;
    page.init(&init).await;
    if same_box {
      page.until("frame", |d| d["snapshot"]==true).await;
    } else {
      let marker=page.until("error",|d|d["data"][1]["forceToast"]==true).await;
      assert!(marker["data"][0].as_str().unwrap().contains("previous server"),"working destination warning");
    }
    let market=json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
      "precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true});
    let options=json!({"accountName":"acct","activeMarket":market,"size":0.1,"duration":10,"ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"});
    let state=json!({"accountName":"acct","activeMarket":market,"size":0.1,"ordersSizes":[0.01,0.01,0.01],"side":"buy","status":"running","ordersCount":10,"ordersExecuted":7,"nextOrderAt":now+60000,"origin":"web","createdAt":now-180000,"startedAt":now-180000});
    let original_row=bound_row(&_account,"transferred-twap",&options,&state);
    page.op("acct","twap:state",json!([original_row])).await;
    let store=page.until("store",|d|d["data"]["activeTWAPs"].as_array().is_some_and(|r|!r.is_empty())).await;
    assert_eq!(store["data"]["activeTWAPs"][0]["ordersExecuted"],7,"progress preserved");
    assert_eq!(store["data"]["activeTWAPs"][0]["status"],"paused","interrupted cross-box restore must not auto-resume merely because the destination never fenced");
    page.op("acct","twap:resume",json!("transferred-twap")).await;
    page.until("store",|d|d["data"]["activeTWAPs"].as_array().is_some_and(|r|r.first().is_some_and(|r|r["status"]=="running"))).await;
    page.close().await;
    _account.handle.dispose();
    let _replacement=testing::trading_account(&app.registry,"target",&init);
    let mut returned=support::Page::open(&address,&unsigned_token("target",now/1000+3600),None).await;
    returned.init(&init).await;
    returned.until("frame",|d|d["snapshot"]==true).await;
    // The same owner keeps its generation across the rebuild, so its saved row stays restorable.
    assert_eq!(_account.twap_owner.snapshot(),_replacement.twap_owner.snapshot(),"rebuild keeps the owner generation");
    // Current-binding controls still exercise per-job acknowledgement, not row migration.
    returned.op("acct","twap:state",json!([
      bound_row(&_replacement,"transferred-twap",&options,&state),
      bound_row(&_replacement,"unacknowledged-twap",&options,&state)
    ])).await;
    let restored=returned.until("store",|d|d["data"]["activeTWAPs"].as_array().is_some_and(|r|!r.is_empty())).await;
    assert_eq!(restored["data"]["activeTWAPs"][0]["status"],"running","explicit resume must acknowledge the old interruption for later starts, or have a bounded expiry rule");
    let jobs=restored["data"]["activeTWAPs"].as_array().unwrap();
    assert_eq!(jobs.iter().find(|job|job["id"]=="unacknowledged-twap").unwrap()["status"], "paused");
    app.registry.interruptions.record("target",interruption::Marker { reason:interruption::Reason::OwnerReleased,from_box:"01".into(),at:now_ms().max(now+1) }).unwrap();
    returned.close().await;
    _replacement.handle.dispose();
    let _third=testing::trading_account(&app.registry,"target",&init);
    let mut newer=support::Page::open(&address,&unsigned_token("target",now/1000+3600),None).await;
    newer.init(&init).await;
    newer.until("error",|d|d["data"][1]["forceToast"]==true).await;
    newer.op("acct","twap:state",json!([bound_row(&_third,"transferred-twap",&options,&state)])).await;
    let restored=newer.until("store",|d|d["data"]["activeTWAPs"].as_array().is_some_and(|rows|!rows.is_empty())).await;
    assert_eq!(restored["data"]["activeTWAPs"][0]["status"],"paused","newer marker must rearm an acknowledged job");
    newer.close().await;


 }).await;
}

#[tokio::test]
async fn explicit_resume_survives_same_grant_rebuild_and_new_marker_rearms_pause() {
    resume_survives_rebuild(false).await;
}
#[tokio::test]
async fn explicit_resume_survives_a_nonzero_lease_generation_rebuild() {
    resume_survives_rebuild(true).await;
}
