//! Paradex allows 200 subscriptions per connection (42900); 66 perps at three channels each hit it.
mod support;
use safe_cex_engine::{Action, Command, model::Interval};
use support::*;
const BTC: &str = "BTC-USD-PERP";

fn channels(h: &Harness, method: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Rpc {
        method: String,
        params: Channel,
    }
    #[derive(serde::Deserialize)]
    struct Channel {
        channel: String,
    }
    h.sent()
        .into_iter()
        .filter_map(|s| serde_json::from_str::<Rpc>(s).ok())
        .filter(|r| r.method == method)
        .map(|r| r.params.channel)
        .collect()
}

#[test]
fn trades_are_subscribed_only_for_charted_markets() {
    let mut h = Harness::new();
    assert!(
        !channels(&h, "subscribe")
            .iter()
            .any(|c| c.starts_with("trades."))
    );
    for interval in [Interval::M1, Interval::M5] {
        h.command(Command::SubscribeCandles {
            symbol: BTC.into(),
            interval,
        });
    }
    let trades: Vec<_> = channels(&h, "subscribe")
        .into_iter()
        .filter(|c| c.starts_with("trades."))
        .collect();
    assert_eq!(trades, [format!("trades.{BTC}")]);
    h.command(Command::UnsubscribeCandles {
        symbol: BTC.into(),
        interval: Interval::M1,
    });
    assert!(
        channels(&h, "unsubscribe").is_empty(),
        "M5 still charts BTC"
    );
    h.command(Command::UnsubscribeCandles {
        symbol: BTC.into(),
        interval: Interval::M5,
    });
    assert_eq!(channels(&h, "unsubscribe"), [format!("trades.{BTC}")]);
}

#[test]
fn a_subscription_cap_refusal_does_not_reconnect_the_socket() {
    let mut h = Harness::new();
    h.command(Command::SubscribeOrderBook { symbol: BTC.into() });
    for round in 1..=3 {
        let id = h.sent().iter().filter(|s| s.contains("subscribe")).count();
        h.ws(&format!(
            r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":42900,"message":"max subscriptions per connection reached","data":"limit: 200"}}}}"#
        ));
        h.collect();
        let retry = h.now + 1_000. * f64::from(round);
        if h.actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == retry))
        {
            h.timer(retry);
        }
    }
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { .. })),
        "the cap holds on every reconnect, so closing only loops"
    );
}
