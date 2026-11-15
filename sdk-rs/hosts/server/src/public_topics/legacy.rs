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
            if venue == "bitunix"
                && let Some(args) = v["args"].as_array()
            {
                for arg in args {
                    if let (Some(symbol), Some(channel)) =
                        (arg["symbol"].as_str(), arg["ch"].as_str())
                    {
                        push(json!([symbol, channel]), added);
                    }
                }
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
#[cfg(test)]
#[test]
fn review_bitunix_public_batch_topics_are_counted() {
    let sub = r#"{"op":"subscribe","args":[{"symbol":"BTCUSDT","ch":"ticker"},{"symbol":"ETHUSDT","ch":"ticker"}]}"#;
    let unsub = r#"{"op":"unsubscribe","args":[{"symbol":"BTCUSDT","ch":"ticker"},{"symbol":"ETHUSDT","ch":"ticker"}]}"#;
    let add = topics("bitunix", sub);
    let remove = topics("bitunix", unsub);
    assert_eq!(add.len(), 2);
    assert_eq!(remove.len(), 2);
    assert_ne!(add[0].0, add[1].0);
    for ((a, subscribed), (b, unsubscribed)) in add.iter().zip(&remove) {
        assert_eq!(a, b);
        assert!(*subscribed && !*unsubscribed);
    }
}

#[cfg(test)]
#[test]
fn bitunix_real_public_encoder_preserves_symbol_and_channel_identity() {
    use safe_cex_engine::{Action, Engine, Event, Response, Venue};
    struct PublicBatches;
    impl safe_cex_venues::Visit for PublicBatches {
        type Output = Vec<String>;
        fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
            let pairs = r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"},{"symbol":"ETHUSDT","base":"ETH","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}]}"#;
            let tickers = r#"{"code":0,"data":[{"symbol":"BTCUSDT","last":"96000","open":"95000","markPrice":"96001","baseVol":"1000","quoteVol":"96000000"},{"symbol":"ETHUSDT","last":"3000","open":"2900","markPrice":"3001","baseVol":"1000","quoteVol":"3000000"}]}"#;
            let mut batches = Vec::new();
            engine.start(0.);
            for _ in 0..16 {
                let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.);
                if actions.is_empty() {
                    break;
                }
                for action in actions {
                    match action {
                        Action::Http(request) => {
                            let body = if request.url.ends_with("/trading_pairs") {
                                pairs
                            } else if request.url.ends_with("/tickers") {
                                tickers
                            } else {
                                panic!("unexpected public discovery request");
                            };
                            engine.handle(
                                0.,
                                Event::HttpResponse {
                                    request: request.id,
                                    response: Response {
                                        status: 200,
                                        headers: "",
                                        body,
                                    },
                                },
                            );
                        }
                        Action::WsConnect { socket, .. } => {
                            engine.handle(0., Event::WsOpened { socket })
                        }
                        Action::WsSend { text, .. }
                            if serde_json::from_str::<Value>(&text)
                                .is_ok_and(|v| v["op"] == "subscribe" && v["args"].is_array()) =>
                        {
                            batches.push(text);
                        }
                        _ => {}
                    }
                }
            }
            batches
        }
    }
    let batches = safe_cex_venues::build(
        "bitunix",
        serde_json::from_value(json!({"publicDataOnly":true})).unwrap(),
        Some(safe_cex_engine::test_support::seeded_entropy(1)),
        PublicBatches,
    )
    .unwrap();
    assert_eq!(
        batches.len(),
        2,
        "ticker and price each emit a two-symbol batch"
    );
    let mut identities = std::collections::HashSet::new();
    for frame in batches {
        let added = topics("bitunix", &frame);
        assert_eq!(added.len(), 2);
        for (key, subscribed) in &added {
            assert!(*subscribed);
            assert!(identities.insert(key.clone()));
        }
        let mut unsubscribe: Value = serde_json::from_str(&frame).unwrap();
        unsubscribe["op"] = json!("unsubscribe");
        let removed = topics("bitunix", &unsubscribe.to_string());
        assert_eq!(
            removed,
            added
                .into_iter()
                .map(|(key, _)| (key, false))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(
        identities.len(),
        4,
        "both symbol and channel are part of the identity"
    );
}
