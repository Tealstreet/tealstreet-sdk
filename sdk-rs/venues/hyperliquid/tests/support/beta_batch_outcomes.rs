use super::*;
use safe_cex_engine::{Outcome, VenueError};

fn failed(actions: &[Action]) -> &VenueError {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 81, error }) => Some(error),
            _ => None,
        })
        .expect("one failed batch")
}
fn pair(e: &mut Engine<Hyperliquid>) -> HttpRequest {
    let mut order =
        opts(r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#);
    order.stop_loss = Some(80.);
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 81,
            orders: vec![order],
        }),
    );
    let request = write(e);
    assert_eq!(
        body(&request)["action"]["orders"].as_array().unwrap().len(),
        2
    );
    request
}

#[test]
fn beta_mixed_place_preserves_every_accepted_oid_in_either_row_order() {
    for accepted in [r#"{"resting":{"oid":101}}"#, r#"{"filled":{"oid":101}}"#] {
        for accepted_first in [true, false] {
            let (mut e, _) = ready("disabled");
            let request = pair(&mut e);
            let error = r#"{"error":"second order rejected"}"#;
            let rows = if accepted_first {
                format!("{accepted},{error}")
            } else {
                format!("{error},{accepted}")
            };
            reply_at(
                &mut e,
                &request,
                2001.,
                &format!(
                    r#"{{"status":"ok","response":{{"type":"order","data":{{"statuses":[{rows}]}}}}}}"#
                ),
            );
            let actions = e.drain_plain().collect::<Vec<_>>();
            let error = failed(&actions);
            assert_eq!(
                error.partial.as_ref().map(|p| p.order_ids.as_slice()),
                Some(["101".to_owned()].as_slice())
            );
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(
                e.store().order("101").is_some(),
                accepted.contains("resting")
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
        }
    }
}

#[test]
fn beta_mixed_cancel_removes_only_successful_legs_in_either_row_order() {
    for success_first in [true, false] {
        let (mut e, socket) = ready("disabled");
        open_order(&mut e, socket, 1500.);
        message(
            &mut e,
            socket,
            1501.,
            r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":10,"side":"B","limitPx":"90","origSz":"1","sz":"1","orderType":"Limit","tif":"Gtc"},"status":"open"}]}"#,
        );
        e.drain_plain().for_each(drop);
        e.handle(
            2000.,
            Event::Command(Command::CancelOrders {
                command: 81,
                ids: vec!["9".into(), "10".into()],
            }),
        );
        let request = write(&mut e);
        let rows = if success_first {
            r#""success",{"error":"cancel refused"}"#
        } else {
            r#"{"error":"cancel refused"},"success""#
        };
        reply_at(
            &mut e,
            &request,
            2001.,
            &format!(
                r#"{{"status":"ok","response":{{"type":"cancel","data":{{"statuses":[{rows}]}}}}}}"#
            ),
        );
        let actions = e.drain_plain().collect::<Vec<_>>();
        assert_eq!(failed(&actions).outcome, Some(Outcome::Unknown));
        let (cancelled, refused) = if success_first {
            ("9", "10")
        } else {
            ("10", "9")
        };
        assert!(e.store().order(cancelled).is_none());
        assert_eq!(
            e.store().order(refused).unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
    }
}

#[test]
fn beta_all_refused_rows_have_no_partial_ids_or_published_orders() {
    let (mut e, _) = ready("disabled");
    let request = pair(&mut e);
    reply_at(
        &mut e,
        &request,
        2001.,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"first"},{"error":"second"}]}}}"#,
    );
    let actions = e.drain_plain().collect::<Vec<_>>();
    let error = failed(&actions);
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert!(error.partial.is_none());
    assert!(e.store().orders().is_empty());
}
