//! AGT-115: a trailing or conditional entry returns before `format_order`'s leg loop,
//! so attached legs would be dropped (seen live on USD-M, 2026-11-21). Those brackets
//! refuse before any write; a stop-type entry still places its legs after the entry.
mod support;
use safe_cex_engine::{Command, Event, Method, Outcome};
use support::{Harness, Options};

const REFUSAL: &str = "Attaching a stop-loss or take-profit to a stop entry isn't supported on Binance. Place the stop-loss after the entry fills.";

fn ready(pm: bool) -> Harness {
    let mut h = Harness::started(Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-seed".into()),
        portfolio_margin: pm,
        ..Options::default()
    });
    h.load_public();
    h.requests.clear();
    h.done.clear();
    h
}

fn place(h: &mut Harness, orders: &str) {
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: serde_json::from_str(orders).unwrap(),
    }));
}

#[test]
fn trailing_and_conditional_entries_with_legs_refuse_before_any_write() {
    for pm in [false, true] {
        for entry in [
            r#""type":"conditional","triggerPrice":120"#,
            r#""type":"trailing_stop_market","price":90"#,
        ] {
            for legs in [
                r#""stopLoss":80"#,
                r#""takeProfit":150"#,
                r#""stopLoss":0,"takeProfit":150"#,
            ] {
                let mut h = ready(pm);
                place(
                    &mut h,
                    &format!(
                        r#"[{{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":99}},{{"symbol":"BTCUSDT","side":"buy","amount":0.01,{entry},{legs}}}]"#
                    ),
                );
                assert!(
                    h.requests.iter().all(|r| r.method == Method::Get),
                    "pm {pm} {entry} {legs}"
                );
                let (_, error) = h.failures.last().expect("refused");
                assert_eq!(error.legacy_text(), REFUSAL, "pm {pm} {entry} {legs}");
                assert_eq!(error.outcome, Some(Outcome::NotSent));
            }
        }
    }
}

// AGT-112 live order, pinned at the adapter: the stop entry first, then its leg.
#[test]
fn a_stop_market_entry_places_its_legs_after_the_entry() {
    let mut h = ready(false);
    place(
        &mut h,
        r#"[{"symbol":"BTCUSDT","type":"stop_market","side":"buy","amount":0.01,"price":120,"stopLoss":80}]"#,
    );
    let entry = h.take("/fapi/v1/algoOrder");
    assert!(entry.url.contains("side=BUY&"), "{}", entry.url);
    h.reply(entry.id, 200, "", r#"{"algoId":11}"#);
    let leg = h.take("/fapi/v1/algoOrder");
    assert!(leg.url.contains("side=SELL&"), "{}", leg.url);
    assert!(leg.url.contains("triggerPrice=80"), "{}", leg.url);
    h.reply(leg.id, 200, "", r#"{"algoId":12}"#);
    assert_eq!(h.result(77), Some(&Ok(r#"["11","12"]"#.to_owned())));
}
