//! Position ordering uses local receive/release cuts, never cross-stream native time.
mod support;
use safe_cex_engine::Event;
use serde_json::{json, Value};
use support::*;
fn ready() -> Harness {
    let mut h=Harness::classic();
    while h.requests.iter().any(|r|r.url.contains("/all-position")) { h.respond("/all-position",200,POSITION); }
    h
}
fn row(size: u32, time: &str) -> Value {
    json!({"symbol":"BTCUSDT","holdSide":"long","total":size.to_string(),"openPriceAvg":"90","markPrice":"100","leverage":"20","unrealizedPL":"10","liquidationPrice":"50","marginSize":"50","marginMode":"crossed","posMode":"one_way_mode","uTime":time})
}
fn ws(h:&mut Harness,size:u32,time:&str) {
    h.private_message(&json!({"arg":{"channel":"positions"},"action":"snapshot","data":[row(size,time)]}).to_string());
}
fn repair_at(h:&mut Harness,delay:f64) {
    let (timer,_) = h.timers.iter().find(|(_,at)| (*at-h.now-500.0).abs()<0.01).copied().unwrap();
    h.timers.retain(|(t,_)|*t!=timer); h.now+=delay;
    h.event(Event::TimerFired {timer});
}
fn rest(h:&mut Harness,rows:Value) { h.respond("/all-position",200,&json!({"code":"00000","data":rows}).to_string()); }
fn size(h:&Harness)->f64 { h.engine.store().positions().iter().map(|p|p.contracts).sum() }
#[test]
fn a_recent_pre_send_open_beats_stale_rest_even_with_a_later_native_time() {
    let mut h=ready(); ws(&mut h,2,"100"); repair_at(&mut h,1000.0);
    rest(&mut h,json!([row(1,"999999")]));
    assert_eq!(size(&h),2.0,"server uTime is not cross-stream causal evidence");
    assert!(h.timers.iter().any(|(_,at)| (*at-h.now-500.0).abs()<0.01),"conflicting REST owes a bounded re-read");
}
#[test]
fn a_recent_pre_send_zero_cannot_be_resurrected_by_stale_rest() {
    let mut h=ready(); ws(&mut h,0,"100"); repair_at(&mut h,1000.0);
    rest(&mut h,json!([row(1,"999999")]));
    assert_eq!(size(&h),0.0,"zero is a per-leg observation, not an old server timestamp");
    assert!(h.timers.iter().any(|(_,at)| (*at-h.now-500.0).abs()<0.01));
}
#[test]
fn a_recent_pre_send_open_survives_omission_and_owes_a_re_read() {
    let mut h=ready(); ws(&mut h,2,"100"); repair_at(&mut h,1000.0); rest(&mut h,json!([]));
    assert_eq!(size(&h),2.0);
    assert!(h.timers.iter().any(|(_,at)| (*at-h.now-500.0).abs()<0.01),"stale omission must schedule a re-read");
}
#[test]
fn same_connection_receive_order_and_a_post_send_observation_are_preserved() {
    let mut h=ready(); ws(&mut h,3,"9000"); ws(&mut h,2,"100"); assert_eq!(size(&h),2.0);
    repair_at(&mut h,500.0); h.now+=1.0; ws(&mut h,4,"1"); rest(&mut h,json!([])); assert_eq!(size(&h),4.0);
}

#[test]
fn one_cut_owes_one_coalesced_repair_without_a_500ms_feedback_loop() {
    let mut h=ready(); h.now += 1.0; ws(&mut h,2,"100"); repair_at(&mut h,1000.0);
    rest(&mut h,json!([row(1,"999999")])); assert_eq!(size(&h),2.0);
    repair_at(&mut h,500.0); rest(&mut h,json!([row(1,"999999")])); assert_eq!(size(&h),2.0);
    assert!(!h.timers.iter().any(|(_,at)| (*at-h.now-500.0).abs()<0.01),"a repair never recursively owes another repair");
    h.now+=5001.0;
    h.command(safe_cex_engine::Command::Venue {command:88,action:safe_cex_engine::VenueAction::ReadPositions});
    rest(&mut h,json!([row(1,"0")])); assert_eq!(size(&h),1.0,"expired local grace permits healing, regardless of native time");
}

#[test]
fn a_read_waiting_for_preparation_uses_its_physical_release_not_logical_start() {
    use safe_cex_engine::{Action,request_context::{HttpAction,Prepared}};
    let mut h=ready(); ws(&mut h,3,"100");
    let (timer,at)=h.timers.iter().copied().find(|(_,at)|(*at-h.now-500.0).abs()<0.01).unwrap();
    h.timers.retain(|(id,_)|*id!=timer); h.now=at;
    h.engine.handle(h.now,Event::TimerFired{timer});
    let mut held=None;
    for action in h.engine.drain_actions().collect::<Vec<_>>() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Prepare { preparation,request,.. } if request.url.contains("/all-position") => held=Some(preparation),
                _=>panic!("unexpected managed preparation"),
            },
            other=>h.absorb(vec![other]),
        }
    }
    let preparation=held.expect("real native positions preparation");
    h.now+=1.0; ws(&mut h,2,"900000"); h.now+=6000.0;
    h.engine.http_prepared(h.now,preparation,Prepared{id:preparation.attempt().request.0,expires_at:None});
    h.engine.prepare_dispatch(h.now);
    for action in h.engine.drain_actions().collect::<Vec<_>>() {
        match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind { permit, request, .. } => {
                    assert!(matches!(h.engine.claim_http(h.now,permit),Ok(safe_cex_engine::request_context::Claim::Released(_))));
                    h.requests.push(request);
                },
                HttpAction::Cancel { .. } => {},
                _=>panic!("expected the held preparation's exact Bind"),
            },
            other=>h.absorb(vec![other]),
        }
    }
    h.collect();
    rest(&mut h,json!([row(1,"0")]));
    assert_eq!(size(&h),1.0,"the pre-release observation is outside its bounded receipt window");
}

#[test]
fn reconnect_invalidates_the_old_cut_and_a_genuine_new_census_can_heal() {
    let mut h=ready(); ws(&mut h,2,"900000"); repair_at(&mut h,500.0);
    let old=h.take("/all-position");
    let socket=h.sockets.iter().rev().find(|(_,u)|u.ends_with("/private")).unwrap().0;
    let timers:Vec<_>=h.timers.iter().map(|(id,_)|*id).collect();
    h.event(Event::WsClosed{socket,code:1006});
    let (timer,at)=h.timers.iter().copied().filter(|(id,_)|!timers.contains(id)).min_by(|a,b|a.1.total_cmp(&b.1)).expect("native private reconnect timer");
    h.timers.retain(|(id,_)|*id!=timer);h.now=at;h.event(Event::TimerFired{timer});
    let next=h.sockets.iter().rev().find(|(_,u)|u.ends_with("/private")).unwrap().0;
    assert_ne!(socket,next); h.event(Event::WsOpened{socket:next});
    h.private_message(r#"{"event":"login","code":0,"msg":""}"#);
    h.reply(old.id,200,r#"{"code":"00000","data":[]}"#);
    assert_eq!(size(&h),2.0,"old connection census cannot pay the reconnect debt");
    repair_at(&mut h,500.0); rest(&mut h,json!([]));
    assert_eq!(size(&h),0.0,"post-reconnect REST heals without old connection marks");
}
