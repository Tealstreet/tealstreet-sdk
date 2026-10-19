mod support;

use safe_cex_engine::{Command, Event, Query};
use support::*;

fn late_uta_query(h: &mut Harness) {
    h.command(Command::Query {
        command: 97,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    });
    h.respond(
        "/order/fills",
        200,
        r#"{"code":"40085","msg":"Classic Account API is not supported","data":null}"#,
    );
}

fn fire_next_timer(h: &mut Harness) {
    let (timer, at) = h
        .timers
        .iter()
        .copied()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("pending query has a timer");
    h.timers.retain(|(id, _)| *id != timer);
    h.now = at;
    h.event(Event::TimerFired { timer });
}

#[test]
fn confirmed_uta_survives_settings_transport_and_html_server_failures() {
    for transport_failure in [true, false] {
        let mut h = Harness::classic();
        late_uta_query(&mut h);
        let probe = h.take("/account/settings");
        let old_requests = h.requests.len();
        if transport_failure {
            h.event(Event::HttpFailed {
                request: probe.id,
                error: "temporary connection reset",
            });
        } else {
            h.reply(probe.id, 503, "<html>temporary gateway failure</html>");
        }
        assert!(
            h.requests[old_requests..]
                .iter()
                .all(|r| !r.url.contains("/api/v2/")),
            "confirmed UTA was downgraded after a transient settings failure: {:?}",
            h.requests
        );
        for _ in 0..20 {
            if h.requests
                .iter()
                .any(|r| r.url.contains("/account/settings"))
            {
                break;
            }
            fire_next_timer(&mut h);
        }
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
        );
        for _ in 0..20 {
            if h.requests.iter().any(|r| r.url.contains("/v3/trade/fills")) {
                break;
            }
            fire_next_timer(&mut h);
        }
        h.respond(
            "/v3/trade/fills",
            200,
            r#"{"code":"00000","data":{"list":[],"cursor":""}}"#,
        );
        assert_eq!(
            h.done.iter().find(|(id, _)| *id == 97).unwrap().1,
            Ok("[]".into())
        );
    }
}

#[test]
fn late_uta_detection_that_never_responds_completes_with_a_bounded_error() {
    let mut h = Harness::classic();
    late_uta_query(&mut h);
    for _ in 0..250 {
        if h.done.iter().any(|(id, _)| *id == 97) {
            break;
        }
        fire_next_timer(&mut h);
    }
    let result = &h.done.iter().find(|(id, _)| *id == 97).unwrap().1;
    assert!(result.as_ref().unwrap_err().contains("account detection"));
    assert!(h.now <= 1_010_100.0);
}

#[test]
fn confirmed_uta_probe_retry_exhaustion_cannot_restart_classic_detection() {
    let mut h = Harness::classic();
    late_uta_query(&mut h);
    for _ in 0..4 {
        for _ in 0..20 {
            if h.requests
                .iter()
                .any(|r| r.url.contains("/account/settings"))
            {
                break;
            }
            fire_next_timer(&mut h);
        }
        let probe = h.take("/account/settings");
        let old_requests = h.requests.len();
        h.event(Event::HttpFailed {
            request: probe.id,
            error: "settings temporarily unavailable",
        });
        assert!(
            h.requests[old_requests..]
                .iter()
                .all(|r| !r.url.contains("/api/v2/"))
        );
    }
    for _ in 0..250 {
        if h.done.iter().any(|(id, _)| *id == 97) {
            break;
        }
        fire_next_timer(&mut h);
    }
    let result = &h.done.iter().find(|(id, _)| *id == 97).unwrap().1;
    assert!(result.as_ref().unwrap_err().contains("account detection"));
    assert!(h.now <= 1_010_100.0);
}

#[test]
fn repeated_nonempty_account_order_cursor_preserves_the_previous_snapshot() {
    let mut h = Harness::classic();
    let first = h.take("/orders-pending");
    let page = OPEN_ORDER
        .replace("\"orderId\":\"1\"", "\"orderId\":\"2\"")
        .replace("\"endId\":\"\"", "\"endId\":\"repeated\"");
    h.reply(first.id, 200, &page);
    h.respond("idLessThan=repeated", 200, &page);
    for path in ["planType=profit_loss", "planType=track_plan"] {
        if h.requests.iter().any(|r| r.url.contains(path)) {
            h.respond(path, 200, r#"{"code":"00000","data":{"entrustedList":[]}}"#);
        }
    }
    assert!(h.engine.store().order("1").is_some());
    assert!(h.engine.store().order("2").is_none());
    assert!(h.logs.iter().any(|message| message.contains("cursor")));
}
