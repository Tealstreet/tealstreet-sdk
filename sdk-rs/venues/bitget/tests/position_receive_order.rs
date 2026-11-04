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
