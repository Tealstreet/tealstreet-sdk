//! Native Topic decoding and the production subscription-to-physical-wire boundary.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{
    classes, lifecycle,
    public_subscriptions::{PublicSubscription, PublicSubscriptionHost, Topic},
};
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_num::TableHit;
use safe_cex_types::ExchangeName;
use std::{process::Command, rc::Rc};

#[test]
fn topic_decoder_keeps_every_code_unit_and_rejects_non_u16_elements() {
    for units in [
        vec![],
        vec![0, 34, 92, 10],
        vec![0xd800, 65, 0xdc00],
        vec![0xd83d, 0xde00, 65535],
    ] {
        let value = serde_json::json!(units);
        let decoded: Topic = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decoded, units);
        let text = value.to_string();
        let mut input = serde_json::Deserializer::from_str(&text);
        let qualified = <Topic as serde::Deserialize>::deserialize(&mut input).unwrap();
        assert_eq!(qualified, units);
    }
    for value in [
        serde_json::json!(null),
        serde_json::json!("BTC/USD"),
        serde_json::json!({}),
        serde_json::json!([-1]),
        serde_json::json!([65536]),
        serde_json::json!([1.5]),
        serde_json::json!([true]),
        serde_json::json!([null]),
        serde_json::json!([[]]),
    ] {
        assert!(serde_json::from_value::<Topic>(value).is_err());
    }
}

#[test]
fn topic_code_units_reach_futures_subscription_wire_like_live_source() {
    check_subscription_wire(ExchangeName::Kraken);
}
#[test]
fn topic_code_units_reach_spot_subscription_wire_like_live_source() {
    check_subscription_wire(ExchangeName::Krakenspot);
}
fn check_subscription_wire(name: ExchangeName) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/topic-alias-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 6, "all maintained source cases executed");
    {
        let h = harness(name.clone(), options(), |_| {});
        let rt = &h.exchange.rt;
        let socket = if name == ExchangeName::Kraken {
            classes::futures_public(rt, Rc::new(rt.this.clone())).unwrap()
        } else {
            classes::spot_public(rt, Rc::new(rt.this.clone())).unwrap()
        };
        let host = rt.shared.extra.sockets.borrow().last().unwrap().clone();
        assert!(host.rt.this.strict_equals(&socket));
        if name == ExchangeName::Kraken {
            lifecycle::futures_public_connect(host.clone()).unwrap();
        } else {
            lifecycle::spot_public_connect(host.clone()).unwrap();
        }
        h.runtime.pump();
        h.runtime.advance(100.0);
        let physical = h.runtime.sockets.borrow()[0].1.clone();
        physical.sent.borrow_mut().clear();
        for case in &cases {
            let topic: Topic = serde_json::from_value(case["units"].clone()).unwrap();
            let frames = if name == ExchangeName::Kraken {
                vec![(
                    PublicSubscription::Futures {
                        event: "subscribe",
                        feed: "book",
                        product_ids: vec![topic.clone()],
                    },
                    "futures",
                )]
            } else {
                vec![
                    (
                        PublicSubscription::SpotTicker {
                            method: "subscribe",
                            channel: "ticker",
                            symbols: vec![topic.clone()],
                            event_trigger: "trades",
                        },
                        "ticker",
                    ),
                    (
                        PublicSubscription::SpotBook {
                            method: "subscribe",
                            channel: "book",
                            symbols: vec![topic.clone()],
                            depth: 100.0,
                        },
                        "book",
                    ),
                    (
                        PublicSubscription::SpotOhlc {
                            method: "subscribe",
                            channel: "ohlc",
                            symbols: vec![topic.clone()],
                            interval: TableHit::Mapped(1.0),
                        },
                        "ohlc",
                    ),
                ]
            };
            for (frame, key) in frames {
                host.stringify_and_send(frame).unwrap();
                let sent = physical.sent.borrow();
                assert_eq!(
                    sent.last().unwrap(),
                    case[key].as_str().unwrap(),
                    "{key} original native units {topic:?}"
                );
            }
            // The assembled listener body registers the original native handler key.
            let cleanup = socket
                .call_method(
                    "listenOrderBook",
                    vec![
                        Js::utf16(&topic),
                        Js::function("bookCallback", |_, _| Ok(Js::Undefined)),
                    ],
                )
                .unwrap();
            let key = if name == ExchangeName::Kraken {
                "futures"
            } else {
                "book"
            };
            assert_eq!(
                physical.sent.borrow().last().unwrap(),
                case[key].as_str().unwrap(),
                "live listener original topic"
            );
            // Real onOpen reads that map key back as Topic and resubscribes.
            socket.call_method("onOpen", vec![]).unwrap();
            assert_eq!(
                physical.sent.borrow().last().unwrap(),
                case[key].as_str().unwrap(),
                "reconnect original topic"
            );
            cleanup.call(Js::Undefined, vec![]).unwrap();
            if name == ExchangeName::Krakenspot {
                assert_eq!(
                    physical.sent.borrow().last().unwrap(),
                    &case["book"]
                        .as_str()
                        .unwrap()
                        .replacen("subscribe", "unsubscribe", 1)
                );
            }
        }
        assert_eq!(
            physical.sent.borrow().len(),
            if name == ExchangeName::Kraken { 18 } else { 36 }
        );
    }
}
