//! Current TxFlow indexed history payloads and continuation semantics.
#[path = "support/account.rs"]
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query};
use safe_cex_venue_txflow::TxFlow;
use serde_json::{Value, json};
use support::*;
fn query(e: &mut Engine<TxFlow>, q: Query) -> safe_cex_engine::HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::Query {
            command: 91,
            query: q,
        }),
    );
    requests(e)
        .into_iter()
        .find(|r| {
            r.body
                .as_ref()
                .is_some_and(|b| b.contains("\"limit\":2000"))
        })
        .unwrap()
}
fn ready() -> Engine<TxFlow> {
    let (mut e, _, reads, _) = bootstrap();
    answer(&mut e, reads);
    e.drain_actions().for_each(drop);
    e
}
fn fills(from: Option<f64>, limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: Some("BTC-USDC".into()),
        order_id: None,
        from_ms: from,
        to_ms: Some(NOW + 10000.0),
        limit,
    }
}
fn execution(tid: u64, time: f64) -> Value {
    json!({"instrumentId":1,"tid":tid,"oid":42,"time":time as u64,"px":"101.5","sz":"0.5","side":"B","fee":"0.02","feeToken":"USDC","closedPnl":"3","crossed":true})
}
fn done(e: &mut Engine<TxFlow>) -> Result<Value, String> {
    e.drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 91,
                result,
            }) => Some(result.map(|s| serde_json::from_str(&s).unwrap())),
            _ => None,
        })
        .unwrap()
}
#[test]
fn exact_millisecond_filtering_precedes_seconds_output_and_native_tid_dedup() {
    let mut e = ready();
    let r = query(&mut e, fills(Some(NOW + 1.0), None));
    let body=json!({"success":true,"code":0,"data":{"data":[execution(7,NOW),execution(8,NOW+1.0),execution(8,NOW+1.0)],"hasMore":false}}).to_string();
    reply(&mut e, &r, &body);
    let rows = done(&mut e).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["id"], "8");
    assert_eq!(rows[0]["timestamp"], (NOW + 1.0) / 1000.0);
    assert_eq!(rows[0]["commission"], 0.02);
    assert_eq!(rows[0]["realizedPnl"], 3.0);
    assert_eq!(rows[0]["maker"], false);
}
#[test]
fn offset_continuation_uses_native_raw_pages_not_requested_output_limit() {
    let mut e = ready();
    let r = query(&mut e, fills(None, Some(1)));
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":true}})
            .to_string(),
    );
    let r = requests(&mut e).pop().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(r.body.as_ref().unwrap()).unwrap()["offset"],
        2000
    );
    reply(
        &mut e,
        &r,
        &json!({"success":true,"code":0,"data":{"data":[execution(2,NOW+1.0)],"hasMore":false}})
            .to_string(),
    );
    assert_eq!(done(&mut e).unwrap()[0]["id"], "2");
}
#[test]
fn failed_or_malformed_page_never_returns_successful_partial_history() {
    for body in [
        r#"{"success":false,"code":9,"message":"denied","data":{"data":[],"hasMore":false}}"#,
        r#"{"success":true,"code":0,"data":{"data":[{"px":"NaN"}],"hasMore":false}}"#,
        r#"{"success":true,"code":0,"data":{"data":[],"hasMore":true}}"#,
    ] {
        let mut e = ready();
        let r = query(&mut e, fills(None, None));
        reply(&mut e, &r, body);
        assert!(done(&mut e).is_err());
    }
}
#[test]
fn repeated_full_continuation_is_an_explicit_incomplete_read() {
    let mut e = ready();
    let r = query(&mut e, fills(None, None));
    let body = json!({"success":true,"code":0,"data":{"data":[execution(1,NOW)],"hasMore":true}})
        .to_string();
    reply(&mut e, &r, &body);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, &body);
    assert!(done(&mut e).unwrap_err().contains("Incomplete"));
}
#[test]
fn income_filter_preserves_signed_native_funding() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::Income {
            symbol: Some("BTC-USDC".into()),
            kind: Some("FUNDING_FEE".into()),
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    assert!(r.body.as_ref().unwrap().contains("userFundings"));
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"time":NOW as u64,"coin":"BTC","amount":"-0.12","szi":"2","fundingRate":"0.01","usdc":"-0.12"}],"hasMore":false}}).to_string());
    let rows = done(&mut e).unwrap();
    assert_eq!(rows[0]["income"], -0.12);
    assert_eq!(rows[0]["category"], "funding");
}
#[test]
fn order_history_keeps_native_total_and_remaining_distinct() {
    let mut e = ready();
    let r = query(
        &mut e,
        Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    );
    reply(&mut e,&r,&json!({"success":true,"code":0,"data":{"data":[{"order":{"instrumentId":1,"oid":42,"timestamp":NOW as u64,"side":"A","orderType":"Limit","limitPx":"101","origSz":"2","filledAmount":"0.5","reduceOnly":false},"status":"Canceled","statusTimestamp":(NOW+1000.0) as u64}],"hasMore":false}}).to_string());
    let rows = done(&mut e).unwrap();
    assert_eq!(rows[0]["amount"], 2.0);
    assert_eq!(rows[0]["filled"], 0.5);
    assert_eq!(rows[0]["status"], "canceled");
}
