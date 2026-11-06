//! Native subscribe/unsubscribe shapes emitted by the legacy venue encoders.
//! Request IDs, nonces and transport options are not topic identities.
use serde_json::{Value, json};

pub(super) fn topics(venue: &str, text: &str) -> Vec<(String, bool)> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return vec![];
    };
    let mut rows = Vec::new();
    let mut push = |topic: Value, added| rows.push((topic.to_string(), added));
    let op = v["method"]
        .as_str()
        .or(v["type"].as_str())
        .or(v["event"].as_str())
        .or(v["op"].as_str())
        .unwrap_or("");
    if venue == "mexc" {
        if let Some((added, topic)) = op
            .strip_prefix("sub.")
            .map(|t| (true, t))
            .or_else(|| op.strip_prefix("unsub.").map(|t| (false, t)))
        {
            push(
                json!([topic, v["param"]["symbol"], v["param"]["interval"]]),
                added,
            );
        }
        return rows;
    }
    if venue == "polymarket" {
        let operation = v["operation"].as_str().or(v["type"].as_str());
        if let (Some(operation @ ("market" | "subscribe" | "unsubscribe")), Some(ids)) =
            (operation, v["assets_ids"].as_array())
        {
            for id in ids {
                push(id.clone(), operation != "unsubscribe");
            }
        }
        return rows;
    }
    let added = match op {
        "subscribe" | "sub" => true,
        "unsubscribe" | "unsub" => false,
        _ => return rows,
    };
    match venue {
        "bitrue" | "bitunix" => {
            if let Some(channel) = v["params"]["channel"].as_str() {
                push(json!(channel), added);
            }
        }
        "grvt" => {
            if let (Some(stream), Some(selectors)) = (
                v["params"]["stream"].as_str(),
                v["params"]["selectors"].as_array(),
            ) {
                for selector in selectors {
                    push(json!([stream, selector]), added);
                }
            }
        }
        "kucoin" | "kucoinspot" => {
            if let Some(topic) = v["topic"].as_str() {
                if let Some((channel, selectors)) = topic.split_once(':') {
                    for selector in selectors.split(',') {
                        push(json!(format!("{channel}:{selector}")), added);
                    }
                } else {
                    push(json!(topic), added);
                }
            }
        }
        "orderly" => {
            if let Some(topic) = v["topic"].as_str() {
                push(json!(topic), added);
            }
        }
        "cryptocom" => {
            if let Some(channels) = v["params"]["channels"].as_array() {
                for channel in channels {
                    push(channel.clone(), added);
                }
            }
        }
        "lighter" | "lighter-robinhood" => {
            if let Some(channel) = v["channel"].as_str() {
                push(json!(channel), added);
            }
        }
        "bitget" | "bitgetinverse" | "bitgetusdc" => {
            if let Some(args) = v["args"].as_array() {
                for arg in args {
                    push(
                        json!([arg["instType"], arg["channel"], arg["instId"]]),
                        added,
                    );
                }
            }
        }
        "paradex" => {
            if let Some(channel) = v["params"]["channel"].as_str() {
                push(json!(channel), added);
            }
        }
        "coinbase" => {
            if let (Some(channel), Some(products)) =
                (v["channel"].as_str(), v["product_ids"].as_array())
            {
                for product in products {
                    push(json!([channel, product]), added);
                }
            }
        }
        _ => {}
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_shapes_count_each_topic_and_unsubscribe_the_same_identity() {
        for (venue, sub, unsub, count) in [
            (
                "bitrue",
                r#"{"event":"sub","params":{"channel":"depth_BTC","cb_id":"engine"}}"#,
                r#"{"event":"unsub","params":{"channel":"depth_BTC"}}"#,
                1,
            ),
            (
                "bitunix",
                r#"{"event":"sub","params":{"channel":"market_btcusdt_depth"}}"#,
                r#"{"event":"unsub","params":{"channel":"market_btcusdt_depth"}}"#,
                1,
            ),
            (
                "grvt",
                r#"{"id":1,"method":"subscribe","params":{"stream":"v1.book.s","selectors":["BTC_USDT_Perp@500"]}}"#,
                r#"{"id":2,"method":"unsubscribe","params":{"stream":"v1.book.s","selectors":["BTC_USDT_Perp@500"]}}"#,
                1,
            ),
            (
                "mexc",
                r#"{"method":"sub.depth","param":{"symbol":"BTC_USDT","compress":false}}"#,
                r#"{"method":"unsub.depth","param":{"symbol":"BTC_USDT"}}"#,
                1,
            ),
            (
                "kucoin",
                r#"{"type":"subscribe","topic":"/contractMarket/level2:BTCUSDTM","id":1}"#,
                r#"{"type":"unsubscribe","topic":"/contractMarket/level2:BTCUSDTM","id":2}"#,
                1,
            ),
            (
                "cryptocom",
                r#"{"method":"subscribe","params":{"channels":["ticker.BTC_USDT","book.BTC_USDT.50"]},"nonce":1}"#,
                r#"{"method":"unsubscribe","params":{"channels":["ticker.BTC_USDT","book.BTC_USDT.50"]},"nonce":2}"#,
                2,
            ),
            (
                "lighter",
                r#"{"type":"subscribe","channel":"order_book/1"}"#,
                r#"{"type":"unsubscribe","channel":"order_book/1"}"#,
                1,
            ),
            (
                "bitget",
                r#"{"op":"subscribe","args":[{"instType":"USDT-FUTURES","channel":"books","instId":"BTCUSDT"}]}"#,
                r#"{"op":"unsubscribe","args":[{"instType":"USDT-FUTURES","channel":"books","instId":"BTCUSDT"}]}"#,
                1,
            ),
            (
                "paradex",
                r#"{"method":"subscribe","params":{"channel":"bbo.BTC-USD-PERP"},"id":1}"#,
                r#"{"method":"unsubscribe","params":{"channel":"bbo.BTC-USD-PERP"},"id":2}"#,
                1,
            ),
            (
                "coinbase",
                r#"{"type":"subscribe","channel":"level2","product_ids":["BTC-USD","ETH-USD"]}"#,
                r#"{"type":"unsubscribe","channel":"level2","product_ids":["BTC-USD","ETH-USD"]}"#,
                2,
            ),
            (
                "orderly",
                r#"{"event":"subscribe","topic":"PERP_BTC_USDC@orderbook"}"#,
                r#"{"event":"unsubscribe","topic":"PERP_BTC_USDC@orderbook"}"#,
                1,
            ),
            (
                "polymarket",
                r#"{"type":"market","assets_ids":["asset1","asset2"],"custom_feature_enabled":true}"#,
                r#"{"operation":"unsubscribe","assets_ids":["asset1","asset2"],"custom_feature_enabled":true}"#,
                2,
            ),
        ] {
            let added = topics(venue, sub);
            let removed = topics(venue, unsub);
            assert_eq!(added.len(), count, "{venue}");
            assert_eq!(removed.len(), count, "{venue}");
            for ((a, subscribed), (b, unsubscribed)) in added.iter().zip(removed.iter()) {
                assert_eq!(a, b, "{venue}");
                assert!(*subscribed && !*unsubscribed, "{venue}");
            }
            for control in [
                "ping",
                r#"{"event":"pong"}"#,
                r#"{"type":"ping"}"#,
                r#"{"method":"public/heartbeat"}"#,
            ] {
                assert!(topics(venue, control).is_empty(), "{venue} {control}");
            }
        }
        assert!(
            topics("breakoutprop", "{}").is_empty(),
            "REST-only venue has no upstream socket topics"
        );
    }
}

#[cfg(test)]
#[test]
fn kucoin_batch_selectors_are_individual_topics() {
    let batch = topics(
        "kucoin",
        r#"{"type":"subscribe","topic":"/contractMarket/snapshot:BTCUSDTM,ETHUSDTM"}"#,
    );
    assert_eq!(batch.len(), 2);
    let single = topics(
        "kucoin",
        r#"{"type":"unsubscribe","topic":"/contractMarket/snapshot:ETHUSDTM"}"#,
    );
    assert_eq!(batch[1].0, single[0].0);
}
