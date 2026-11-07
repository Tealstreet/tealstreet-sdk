//! A partly filled order's edit re-places only its unfilled remainder.
mod support;
use safe_cex_engine::{Action, Command, Event};
use support::*;

fn amend_replacement(seed: &str, amount: Option<f64>) -> serde_json::Value {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(seed);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","msg":"success","data":[{"orderId":"12","clientOrderId":"","code":"0","msg":"Order canceled"}]}"#,
    );
    let (timer, at) = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > h.now => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("the verify delay");
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", DETAIL);
    let r = h.take("batch-orders");
    serde_json::from_str(r.body.as_ref().unwrap()).unwrap()
}

const PARTIAL: &str = r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"6","filledSize":"2","price":"60000","state":"partially_filled"}]}"#;

#[test]
fn a_price_edit_replaces_the_remainder() {
    assert_eq!(amend_replacement(PARTIAL, None)[0]["size"], "4");
}

#[test]
fn an_edit_keeping_the_total_replaces_the_remainder() {
    assert_eq!(amend_replacement(PARTIAL, Some(6.0))[0]["size"], "4");
}

#[test]
fn an_edit_growing_the_total_replaces_total_less_filled() {
    assert_eq!(amend_replacement(PARTIAL, Some(8.0))[0]["size"], "6");
}

const LIVE: &str = r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"6","filledSize":"0","price":"60000","state":"live"}]}"#;

fn after_live(amount: Option<f64>) -> Result<serde_json::Value, String> {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(LIVE);
    h.ws(PARTIAL);
    let o = h.e.store().order("12").unwrap();
    let extra = o.extra.as_ref().unwrap();
    eprintln!(
        "filled={} remaining={} remaining_native={:?} filled_units={:?}",
        o.filled, o.remaining, extra.remaining_native, extra.filled_in_placement_units
    );
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let (timer, at) = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > h.now => Some((*timer, *at_ms)),
            _ => None,
        })
        .ok_or("no verify delay")?;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", DETAIL);
    let i = h
        .pending
        .iter()
        .position(|r| r.url.contains("batch-orders"))
        .ok_or_else(|| {
            format!(
                "no replacement: {:?}",
                h.pending.iter().map(|r| &r.url).collect::<Vec<_>>()
            )
        })?;
    Ok(serde_json::from_str(h.pending.remove(i).body.as_ref().unwrap()).unwrap())
}

#[test]
fn a_fill_seen_after_placement_still_sizes_the_remainder() {
    assert_eq!(after_live(None).unwrap()[0]["size"], "4");
    assert_eq!(after_live(Some(6.0)).unwrap()[0]["size"], "4");
}

const DETAIL: &str = r#"{"code":"0","msg":"success","data":{"instId":"BTC-USDT","orderId":"12","state":"partially_canceled","size":"6","filledSize":"2"}}"#;
