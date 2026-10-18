//! Pure decisions for the AsterDEX websocket adapters.

use safe_cex_exchange::js_runtime::value::{Js, Thrown, to_typed};
use safe_cex_exchange::{
    runtime::{ExchangeError, LogSeverity},
    websocket::BaseWebSocket,
};
use safe_cex_num::{TableHit, js_table_lookup};
use std::collections::HashMap;
use std::rc::Rc;

const PUBLIC_STREAM_EVENT_TYPES: &[(&str, &str)] = &[
    ("ticker", "24hrTicker"),
    ("depth", "depthUpdate"),
    ("depth20", "depthUpdate"),
    ("depth10", "depthUpdate"),
    ("depth5", "depthUpdate"),
    ("aggTrade", "aggTrade"),
    ("markPrice", "markPriceUpdate"),
    ("kline", "kline"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicStreamDispatch {
    Ignore,
    DepthSnapshot {
        assigned_symbol: Option<String>,
    },
    Handler {
        key: &'static str,
        assigned_symbol: Option<String>,
    },
}

// ts: AsterDEXPublicWebsocket#handleDepthSnapshot
pub fn public_depth_snapshot(data: &Js) -> Option<safe_cex_types::OrderBook> {
    use safe_cex_num::{js_trim, number_from_string};
    use safe_cex_types::{OrderBook, OrderBookOrders};

    fn parse_levels(levels: &Js) -> Option<Vec<OrderBookOrders>> {
        let levels = levels.array_values()?;
        let mut parsed = Vec::with_capacity(levels.len());
        for level in levels {
            let pair = level.array_values()?;
            if pair.len() != 2 {
                return None;
            }
            let Js::String(raw_price) = &pair[0] else {
                return None;
            };
            let raw_price = String::from_utf16_lossy(raw_price);
            let Js::String(raw_amount) = &pair[1] else {
                return None;
            };
            let raw_amount = String::from_utf16_lossy(raw_amount);
            if js_trim(&raw_price).is_empty() || js_trim(&raw_amount).is_empty() {
                return None;
            }
            let price = number_from_string(&raw_price);
            let amount = number_from_string(&raw_amount);
            if !price.is_finite() || !amount.is_finite() || price <= 0.0 || amount <= 0.0 {
                return None;
            }
            parsed.push(OrderBookOrders {
                price,
                amount,
                total: 0.0,
            });
        }
        Some(parsed)
    }

    let levels_or_empty = |key: &str| {
        let value = data.get(key).ok()?;
        Some(if value.truthy() {
            value
        } else {
            Js::array(vec![])
        })
    };
    let mut bids = parse_levels(&levels_or_empty("b")?)?;
    let mut asks = parse_levels(&levels_or_empty("a")?)?;
    if bids.is_empty() || asks.is_empty() {
        return None;
    }
    bids.sort_by(|left, right| {
        right
            .price
            .partial_cmp(&left.price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    asks.sort_by(|left, right| {
        left.price
            .partial_cmp(&right.price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if bids[0].price >= asks[0].price {
        return None;
    }
    for levels in [&mut bids, &mut asks] {
        let mut total = 0.0;
        for level in levels.iter_mut() {
            total += level.amount;
            level.total = total;
        }
    }
    Some(OrderBook { bids, asks })
}

#[derive(Debug, Clone)]
pub enum PublicEventDataDispatch {
    Ignore,
    Handlers {
        event_type: String,
        data: Js,
    },
    InheritedHandlerFailure {
        event_type: String,
        member: safe_cex_num::ProtoMember,
    },
}

// ts: AsterDEXPublicWebsocket#handleEventData
pub fn public_event_data_dispatch(
    data: &Js,
    handler_keys: &[(&str, ())],
) -> Result<PublicEventDataDispatch, Thrown> {
    let event_type = data.get("e")?;
    if !event_type.truthy() {
        return Ok(PublicEventDataDispatch::Ignore);
    }
    let event_type = String::from_utf16_lossy(&event_type.to_js_string()?);
    match js_table_lookup(handler_keys, &event_type) {
        TableHit::Mapped(_) => Ok(PublicEventDataDispatch::Handlers {
            event_type,
            data: data.clone(),
        }),
        TableHit::Missing => Ok(PublicEventDataDispatch::Ignore),
        TableHit::Inherited(member) => {
            Ok(PublicEventDataDispatch::InheritedHandlerFailure { event_type, member })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicKlineCandle {
    pub timestamp: Option<serde_json::Value>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicKlineUpdate {
    pub stream: String,
    pub candle: PublicKlineCandle,
}

// ts: AsterDEXPublicWebsocket#handleKlineUpdate
pub fn public_kline_update(
    data: &Js,
    callback_streams: &[&str],
) -> Result<Option<PublicKlineUpdate>, Thrown> {
    let kline = data.get("k")?;
    if !kline.truthy() {
        return Ok(None);
    }
    let mut symbol = data.get("s")?;
    if !symbol.truthy() {
        symbol = kline.get("s")?;
    }
    if !symbol.truthy() {
        symbol = Js::str("");
    }
    let Js::String(symbol) = symbol else {
        return Err(Thrown::type_error(
            "(data.s || kline.s || \"\").toLowerCase is not a function",
        ));
    };
    let symbol = String::from_utf16_lossy(&symbol).to_lowercase();
    let interval = String::from_utf16_lossy(&kline.get("i")?.to_js_string()?);
    let stream = format!("{symbol}@kline_{interval}");
    if !callback_streams.contains(&stream.as_str()) {
        return Ok(None);
    }
    let parse = |key: &str| -> Result<f64, Thrown> { kline.get(key)?.parse_float() };
    Ok(Some(PublicKlineUpdate {
        stream,
        candle: PublicKlineCandle {
            timestamp: {
                let value = kline.get("t")?;
                if matches!(value, Js::Undefined) {
                    None
                } else {
                    Some(to_typed(&value)?)
                }
            },
            open: parse("o")?,
            high: parse("h")?,
            low: parse("l")?,
            close: parse("c")?,
            volume: parse("v")?,
        },
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicStreamTradeEvent {
    pub symbol: String,
    pub price: f64,
    pub qty: f64,
    pub timestamp: f64,
    pub side: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicStreamTradeUpdate<'a> {
    pub event: PublicStreamTradeEvent,
    pub ticker: TableHit<&'a safe_cex_types::Ticker>,
    pub ticker_last: f64,
}

// ts: AsterDEXPublicWebsocket#handleTradeUpdate
pub fn public_stream_trade_update<'a>(
    trade: &crate::types::AsterDEXStreamTrade,
    ticker_entries: &[(&str, &'a safe_cex_types::Ticker)],
) -> PublicStreamTradeUpdate<'a> {
    use safe_cex_num::parse_float;

    let price = parse_float(&trade.p);
    let ticker = match js_table_lookup(ticker_entries, &trade.s) {
        TableHit::Mapped(ticker) => TableHit::Mapped(*ticker),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    };
    PublicStreamTradeUpdate {
        event: PublicStreamTradeEvent {
            symbol: trade.s.clone(),
            price,
            qty: parse_float(&trade.q),
            timestamp: trade.capital_t,
            side: if trade.m { "SELL" } else { "BUY" }.to_owned(),
        },
        ticker,
        ticker_last: price,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicMarkPricePatch {
    pub mark: Option<f64>,
    pub index: Option<f64>,
    pub funding_rate: Option<f64>,
    pub funding_time: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct PublicMarkPriceUpdate<'a> {
    pub symbol: Js,
    pub ticker: TableHit<&'a safe_cex_types::Ticker>,
    pub patch: PublicMarkPricePatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicTickerPatch {
    pub last: Option<f64>,
    pub percentage: Option<f64>,
    pub volume: Option<f64>,
    pub quote_volume: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct PublicTickerUpdate<'a> {
    pub symbol: Js,
    pub ticker: TableHit<&'a safe_cex_types::Ticker>,
    pub patch: PublicTickerPatch,
}

// ts: AsterDEXPublicWebsocket#handleTickerUpdate
pub fn public_ticker_update<'a>(
    data: &Js,
    ticker_entries: &[(&str, &'a safe_cex_types::Ticker)],
) -> Result<Option<PublicTickerUpdate<'a>>, Thrown> {
    let symbol = data.get("s")?;
    let ticker_key = String::from_utf16_lossy(&symbol.to_js_string()?);
    let ticker = match js_table_lookup(ticker_entries, &ticker_key) {
        TableHit::Mapped(ticker) => TableHit::Mapped(*ticker),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    };
    if matches!(ticker, TableHit::Missing) {
        return Ok(None);
    }
    let parse_truthy = |key: &str| -> Result<Option<f64>, Thrown> {
        let value = data.get(key)?;
        if !value.truthy() {
            return Ok(None);
        }
        Ok(Some(data.get(key)?.parse_float()?))
    };
    Ok(Some(PublicTickerUpdate {
        symbol,
        ticker,
        patch: PublicTickerPatch {
            last: parse_truthy("c")?,
            percentage: parse_truthy("P")?,
            volume: parse_truthy("v")?,
            quote_volume: parse_truthy("q")?,
        },
    }))
}

// ts: AsterDEXPublicWebsocket#handleMarkPriceUpdate
pub fn public_mark_price_update<'a>(
    data: &Js,
    ticker_entries: &[(&str, &'a safe_cex_types::Ticker)],
) -> Result<Option<PublicMarkPriceUpdate<'a>>, Thrown> {
    let symbol = data.get("s")?;
    let ticker_key = String::from_utf16_lossy(&symbol.to_js_string()?);
    let ticker = match js_table_lookup(ticker_entries, &ticker_key) {
        TableHit::Mapped(ticker) => TableHit::Mapped(*ticker),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Missing,
    };
    if matches!(ticker, TableHit::Missing) {
        return Ok(None);
    }

    let parse_truthy = |key: &str| -> Result<Option<f64>, Thrown> {
        let value = data.get(key)?;
        if !value.truthy() {
            return Ok(None);
        }
        Ok(Some(data.get(key)?.parse_float()?))
    };
    let rate = data.get("r")?;
    let funding_rate = if rate.is_nullish() || rate.strict_equals(&Js::str("")) {
        None
    } else {
        Some(rate.parse_float()?)
    };
    let time = data.get("T")?;
    let funding_time = if time.truthy() {
        Some(to_typed(&time)?)
    } else {
        None
    };

    Ok(Some(PublicMarkPriceUpdate {
        symbol,
        ticker,
        patch: PublicMarkPricePatch {
            mark: parse_truthy("p")?,
            index: parse_truthy("i")?,
            funding_rate,
            funding_time,
        },
    }))
}

// ts: AsterDEXPublicWebsocket#handleStreamData
pub fn public_stream_dispatch(
    stream: &str,
    data_symbol_truthy: bool,
) -> Result<PublicStreamDispatch, &'static str> {
    let mut parts = stream.split('@');
    let symbol = parts.next().unwrap_or_default();
    let stream_type = parts.next().ok_or("streamType.split is not a function")?;
    let event_type = stream_type.split('_').next().unwrap_or_default();
    let mapped_type = match js_table_lookup(PUBLIC_STREAM_EVENT_TYPES, event_type) {
        TableHit::Mapped(mapped) => *mapped,
        // The inherited value is truthy, but its function/object string does not
        // name an own handler. This preserves the source's silent frame drop.
        TableHit::Inherited(_) | TableHit::Missing => return Ok(PublicStreamDispatch::Ignore),
    };

    if mapped_type == "depthUpdate" {
        return Ok(if is_partial_depth_stream(stream) {
            PublicStreamDispatch::DepthSnapshot {
                assigned_symbol: (!data_symbol_truthy).then(|| symbol.to_uppercase()),
            }
        } else {
            PublicStreamDispatch::Ignore
        });
    }

    let handler = match mapped_type {
        "24hrTicker" | "aggTrade" | "markPriceUpdate" | "kline" => mapped_type,
        _ => return Ok(PublicStreamDispatch::Ignore),
    };
    Ok(PublicStreamDispatch::Handler {
        key: handler,
        assigned_symbol: (!data_symbol_truthy).then(|| symbol.to_uppercase()),
    })
}

fn is_partial_depth_stream(stream: &str) -> bool {
    [
        "@depth5",
        "@depth10",
        "@depth20",
        "@depth5@100ms",
        "@depth5@250ms",
        "@depth5@500ms",
        "@depth10@100ms",
        "@depth10@250ms",
        "@depth10@500ms",
        "@depth20@100ms",
        "@depth20@250ms",
        "@depth20@500ms",
    ]
    .iter()
    .any(|suffix| stream.ends_with(suffix))
}

// ts: AsterDEXPublicWebsocket.get connectionURL
pub fn public_connection_url(testnet: bool, request_id: &str) -> String {
    let endpoint_name = if testnet {
        "WS.PUBLIC.TESTNET"
    } else {
        "WS.PUBLIC.PRODUCTION"
    };
    let endpoint = crate::tables::ASTERDEX_ENDPOINTS
        .iter()
        .find_map(|(name, url)| (*name == endpoint_name).then_some(*url))
        .expect("AsterDEX public websocket endpoint table is complete");
    format!("{endpoint}?requestId={request_id}")
}

// ts: AsterDEXPublicWebsocket.ping
pub fn public_ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    public_ping_with(
        socket.is_disposed(),
        socket.is_connected(),
        || socket.mark_ping_sent(),
        |message| socket.send(message),
    )
}

// ts: AsterDEXPrivateWebsocket.ping
pub fn private_ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    private_ping_with(
        socket.is_disposed(),
        socket.is_connected(),
        || socket.mark_ping_sent(),
        |message| socket.send(message),
    )
}

// ts: AsterDEXPrivateWebsocket.onOpen
pub fn private_on_open(
    is_disposed: bool,
    reconnecting_websocket_enabled: bool,
    reset_retry_count: impl FnOnce(u32),
    log: impl FnOnce(&str, LogSeverity),
    ping: impl FnOnce() -> Result<(), ExchangeError>,
    fetch_snapshot: impl FnOnce(u32),
) -> Result<(), ExchangeError> {
    if is_disposed {
        return Ok(());
    }
    reset_retry_count(0);
    log("AsterDEX private WebSocket connected", LogSeverity::Info);
    ping()?;
    if reconnecting_websocket_enabled {
        fetch_snapshot(10);
    }
    Ok(())
}

// ts: AsterDEXPrivateWebsocket#handleListenKeyExpired
pub fn private_listen_key_expired(
    mut log: impl FnMut(&str, LogSeverity),
    clear_refresh: impl FnOnce(),
    clear_watchdog: impl FnOnce(),
    reconnect: impl FnOnce(),
) {
    log(
        "AsterDEX private listen key expired event received, reconnecting private stream",
        LogSeverity::Warning,
    );
    clear_refresh();
    clear_watchdog();
    reconnect();
}

fn private_ping_with(
    is_disposed: bool,
    is_connected: bool,
    mark_ping_sent: impl FnOnce(),
    mut send: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if !is_disposed && is_connected {
        mark_ping_sent();
        send(r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#)?;
    }
    Ok(())
}

fn public_ping_with(
    is_disposed: bool,
    is_connected: bool,
    mark_ping_sent: impl FnOnce(),
    mut send: impl FnMut(&str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if !is_disposed && is_connected {
        mark_ping_sent();
        send(r#"{"id":-1,"method":"LIST_SUBSCRIPTIONS"}"#)?;
    }
    Ok(())
}

type PublicSubscribePlan = Option<(String, u64, Vec<(String, Vec<String>)>)>;

// ts: AsterDEXPublicWebsocket#unsubscribe
pub fn public_unsubscribe_plan(
    connected: bool,
    subscription_id: u64,
    streams: &[String],
) -> Result<Option<(String, u64)>, serde_json::Error> {
    if !connected {
        return Ok(None);
    }
    let params = serde_json::to_string(streams)?;
    Ok(Some((
        format!(r#"{{"method":"UNSUBSCRIBE","params":{params},"id":{subscription_id}}}"#),
        subscription_id + 1,
    )))
}

// ts: AsterDEXPublicWebsocket#subscribe
pub fn public_subscribe_plan(
    connected: bool,
    subscription_id: u64,
    streams: &[String],
    topics: &HashMap<String, Vec<String>>,
) -> Result<PublicSubscribePlan, serde_json::Error> {
    if !connected {
        return Ok(None);
    }
    let params = serde_json::to_string(streams)?;
    let message = format!(r#"{{"method":"SUBSCRIBE","params":{params},"id":{subscription_id}}}"#);

    let mut virtual_topics = topics.clone();
    let mut topic_mutations = Vec::new();
    for stream in streams {
        if stream.contains("@depth") || stream.contains("@kline") {
            let values = vec![stream.clone()];
            virtual_topics.insert(stream.clone(), values.clone());
            topic_mutations.push((stream.clone(), values));
            continue;
        }
        let mut parts = stream.split('@');
        let symbol = parts.next().unwrap_or("").to_owned();
        let topic = parts.next().unwrap_or("undefined").to_owned();
        let own_entries: Vec<_> = virtual_topics
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect();
        // ts-bug: inherited truthy topic-map members suppress topic allocation.
        match js_table_lookup(&own_entries, &topic) {
            TableHit::Inherited(_) => continue,
            TableHit::Mapped(values) => {
                let mut values = (*values).clone();
                values.push(symbol);
                virtual_topics.insert(topic.clone(), values.clone());
                topic_mutations.push((topic, values));
            }
            TableHit::Missing => {
                let values = vec![symbol];
                virtual_topics.insert(topic.clone(), values.clone());
                topic_mutations.push((topic, values));
            }
        }
    }
    Ok(Some((message, subscription_id + 1, topic_mutations)))
}

#[cfg(test)]
mod tests {
    use super::{private_on_open, private_ping_with, public_ping_with};
    use serde_json::Value;

    #[test]
    #[ignore = "requires live TypeScript oracle"]
    fn public_ping_matches_live_typescript() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/public-ws-ping-oracle.cjs"
            ))
            .output()
            .expect("run live TypeScript ping oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).expect("oracle JSON");
        for case in cases {
            let calls = std::cell::RefCell::new(Vec::new());
            let result = public_ping_with(
                case["disposed"].as_bool().unwrap(),
                case["connected"].as_bool().unwrap(),
                || calls.borrow_mut().push("mark".to_owned()),
                |message| {
                    calls.borrow_mut().push(message.to_owned());
                    Ok(())
                },
            );
            result.unwrap();
            let expected = if case["sent"].as_bool().unwrap() {
                vec![
                    "mark".to_owned(),
                    case["message"].as_str().unwrap().to_owned(),
                ]
            } else {
                Vec::new()
            };
            assert_eq!(
                *calls.borrow(),
                expected,
                "public ping source parity {case}"
            );
        }
    }

    #[test]
    #[ignore = "requires live TypeScript oracle"]
    fn public_ping_source_mutation_fails_by_row_name() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ws_helpers::tests::public_ping_matches_live_typescript",
                "--include-ignored",
                "--nocapture",
            ])
            .env("ASTERDEX_PUBLIC_PING_CONTROL", "wrong-message")
            .output()
            .unwrap();
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{diagnostics}");
        assert!(
            diagnostics.contains("public ping source parity"),
            "{diagnostics}"
        );
    }

    #[test]
    #[ignore = "requires live TypeScript oracle"]
    fn private_ping_matches_live_typescript() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/private-ws-ping-oracle.cjs"
            ))
            .output()
            .expect("run live TypeScript private ping oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).expect("oracle JSON");
        for case in cases {
            let calls = std::cell::RefCell::new(Vec::new());
            private_ping_with(
                case["disposed"].as_bool().unwrap(),
                case["connected"].as_bool().unwrap(),
                || calls.borrow_mut().push("mark".to_owned()),
                |message| {
                    calls.borrow_mut().push(message.to_owned());
                    Ok(())
                },
            )
            .unwrap();
            let expected = if case["sent"].as_bool().unwrap() {
                vec![
                    "mark".to_owned(),
                    case["message"].as_str().unwrap().to_owned(),
                ]
            } else {
                Vec::new()
            };
            assert_eq!(
                *calls.borrow(),
                expected,
                "private ping source parity {case}"
            );
        }
    }

    #[test]
    #[ignore = "requires live TypeScript oracle"]
    fn private_ping_oracle_rejects_source_mutation() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ws_helpers::tests::private_ping_matches_live_typescript",
                "--include-ignored",
                "--nocapture",
            ])
            .env("ASTERDEX_PRIVATE_PING_CONTROL", "wrong-message")
            .output()
            .unwrap();
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{diagnostics}");
        assert!(
            diagnostics.contains("private ping source parity"),
            "{diagnostics}"
        );
    }

    #[test]
    #[ignore = "requires live TypeScript oracle"]
    fn private_on_open_matches_live_typescript() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/private-ws-open-oracle.cjs"
            ))
            .output()
            .expect("run live TypeScript private onOpen oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<Value> = serde_json::from_slice(&output.stdout).expect("oracle JSON");
        for case in cases {
            let calls = std::cell::RefCell::new(Vec::new());
            let mut retry_count = 7;
            private_on_open(
                case["disposed"].as_bool().unwrap(),
                case["enabled"].as_bool().unwrap(),
                |value| {
                    retry_count = value;
                    calls.borrow_mut().push(serde_json::json!(["retry", value]));
                },
                |message, severity| {
                    calls
                        .borrow_mut()
                        .push(serde_json::json!(["log", message, severity.as_str()]));
                },
                || {
                    calls.borrow_mut().push(serde_json::json!(["ping"]));
                    Ok(())
                },
                |retries| {
                    calls
                        .borrow_mut()
                        .push(serde_json::json!(["snapshot", retries]));
                },
            )
            .unwrap();
            assert_eq!(
                retry_count,
                if case["disposed"].as_bool().unwrap() {
                    7
                } else {
                    0
                },
                "private onOpen retry reset {case}"
            );
            assert_eq!(
                *calls.borrow(),
                case["trace"].as_array().unwrap().to_vec(),
                "private onOpen source parity {case}"
            );
        }
    }

    #[test]
    #[ignore = "requires live TypeScript oracle"]
    fn private_on_open_oracle_rejects_source_mutation() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ws_helpers::tests::private_on_open_matches_live_typescript",
                "--include-ignored",
                "--nocapture",
            ])
            .env("ASTERDEX_PRIVATE_OPEN_CONTROL", "wrong-retry-reset")
            .output()
            .unwrap();
        let diagnostics = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{diagnostics}");
        assert!(
            diagnostics.contains("private onOpen source parity"),
            "{diagnostics}"
        );
    }
}
