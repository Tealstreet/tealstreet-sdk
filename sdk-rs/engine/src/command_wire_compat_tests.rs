//! Wire compatibility for the host command decoder (RUST_COMMAND_DECODER.md).
//! `legacy` is a byte-for-byte copy of the derive-based decoder as it stood
//! before the borrowed-tag rewrite, kept only here as the oracle.
use super::*;

#[allow(clippy::large_enum_variant)]
mod legacy {
    #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
    use crate::model::CancelOrderRef;
    #[cfg(feature = "command-asterdex")]
    use crate::model::StpMode;
    use crate::model::{Interval, PlaceOrderOpts, PositionSide};
    use crate::{Command, Query};
    use serde::Deserialize;

    /// Commands shared by JavaScript and native WebSocket clients.
    #[derive(Deserialize)]
    #[serde(tag = "op", rename_all = "camelCase")]
    pub(super) enum CommandIn {
        RefreshMarkets {
            command: u32,
        },
        #[cfg(feature = "command-options")]
        FetchOptionChain {
            command: u32,
            query: crate::model::OptionChainQuery,
        },
        #[cfg(feature = "command-options")]
        FetchOptionExpiries {
            command: u32,
            underlying: String,
        },
        #[cfg(feature = "command-options")]
        FetchAccountGreeks {
            command: u32,
        },
        SetTickerSymbols {
            symbols: Vec<String>,
        },
        SubscribeOrderBook {
            symbol: String,
        },
        UnsubscribeOrderBook {
            symbol: String,
        },
        SubscribeCandles {
            symbol: String,
            interval: Interval,
        },
        UnsubscribeCandles {
            symbol: String,
            interval: Interval,
        },
        PlaceOrders {
            command: u32,
            orders: Vec<PlaceOrderOpts>,
        },
        CancelOrders {
            command: u32,
            ids: Vec<String>,
        },
        #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
        CancelOrdersWithSymbols {
            command: u32,
            orders: Vec<CancelOrderRef>,
        },
        AmendOrder {
            command: u32,
            id: String,
            #[serde(default)]
            price: Option<f64>,
            #[serde(default)]
            amount: Option<f64>,
        },
        #[cfg(feature = "command-kucoinspot")]
        AmendOrderWithSymbol {
            command: u32,
            id: String,
            symbol: String,
            #[serde(default)]
            price: Option<f64>,
            #[serde(default)]
            amount: Option<f64>,
        },
        CancelSymbolOrders {
            command: u32,
            symbol: String,
        },
        #[serde(rename_all = "camelCase")]
        SetLeverage {
            command: u32,
            symbol: String,
            leverage: f64,
            #[serde(default)]
            side: Option<PositionSide>,
        },
        SetMarginType {
            command: u32,
            symbol: String,
            isolated: bool,
        },
        SetPositionMode {
            command: u32,
            hedged: bool,
            #[serde(default)]
            symbol: Option<String>,
        },
        #[cfg(feature = "command-asterdex")]
        FetchStpMode {
            command: u32,
        },
        #[serde(alias = "getAccount")]
        FetchAccount {
            command: u32,
        },
        #[cfg(feature = "command-order-book")]
        FetchOrderBook {
            command: u32,
            symbol: String,
            #[serde(default)]
            limit: Option<u32>,
        },
        #[cfg(feature = "command-asterdex")]
        SetStpMode {
            command: u32,
            mode: StpMode,
        },
        #[cfg(feature = "command-schedule-cancel")]
        ScheduleCancel {
            command: u32,
            #[serde(default)]
            time: Option<f64>,
        },
        FetchAccountTier {
            command: u32,
        },
        SetAccountTier {
            command: u32,
            tier: String,
        },
        FetchBalanceInfo {
            command: u32,
        },
        RefreshSettings {
            command: u32,
            #[serde(default)]
            symbol: Option<String>,
        },
        FetchClosedPositions {
            command: u32,
            #[serde(default)]
            symbol: Option<String>,
            #[serde(default)]
            from: Option<f64>,
            #[serde(default)]
            to: Option<f64>,
        },
        FetchLeverage {
            command: u32,
            symbol: String,
        },
        FetchMaxLeverage {
            command: u32,
            symbol: String,
        },
        #[serde(rename_all = "camelCase")]
        FetchFills {
            command: u32,
            #[serde(default)]
            symbol: Option<String>,
            #[serde(default)]
            order_id: Option<String>,
            #[serde(default)]
            from: Option<f64>,
            #[serde(default)]
            to: Option<f64>,
            #[serde(default)]
            limit: Option<u32>,
        },
        FetchOrderHistory {
            command: u32,
            #[serde(default)]
            symbol: Option<String>,
            #[serde(default)]
            from: Option<f64>,
            #[serde(default)]
            to: Option<f64>,
            #[serde(default)]
            limit: Option<u32>,
        },
        #[serde(rename_all = "camelCase")]
        FetchIncome {
            command: u32,
            #[serde(default)]
            symbol: Option<String>,
            #[serde(default)]
            income_type: Option<String>,
            #[serde(default)]
            from: Option<f64>,
            #[serde(default)]
            to: Option<f64>,
            #[serde(default)]
            limit: Option<u32>,
        },
        #[serde(rename_all = "camelCase")]
        FindOrderByClientOrderId {
            command: u32,
            symbol: String,
            client_order_id: String,
        },
        FetchPublicTrades {
            command: u32,
            symbol: String,
            #[serde(default)]
            limit: Option<u32>,
        },
        #[serde(rename_all = "camelCase")]
        FetchCandles {
            command: u32,
            symbol: String,
            interval: Interval,
            #[serde(default)]
            limit: Option<u32>,
            #[serde(default)]
            from: Option<f64>,
            #[serde(default)]
            to: Option<f64>,
        },
    }

    impl From<CommandIn> for Command {
        #[inline(always)]
        fn from(command: CommandIn) -> Self {
            match command {
                #[cfg(feature = "command-options")]
                CommandIn::FetchOptionChain { command, query } => {
                    Self::FetchOptionChain { command, query }
                }
                #[cfg(feature = "command-options")]
                CommandIn::FetchOptionExpiries {
                    command,
                    underlying,
                } => Self::FetchOptionExpiries {
                    command,
                    underlying,
                },
                #[cfg(feature = "command-options")]
                CommandIn::FetchAccountGreeks { command } => Self::FetchAccountGreeks { command },
                CommandIn::FindOrderByClientOrderId {
                    command,
                    symbol,
                    client_order_id,
                } => Self::Query {
                    command,
                    query: Query::OrderByClientId {
                        symbol,
                        client_order_id,
                    },
                },
                CommandIn::SetTickerSymbols { symbols } => Self::SetTickerSymbols { symbols },
                CommandIn::SubscribeOrderBook { symbol } => Self::SubscribeOrderBook { symbol },
                CommandIn::UnsubscribeOrderBook { symbol } => Self::UnsubscribeOrderBook { symbol },
                CommandIn::SubscribeCandles { symbol, interval } => {
                    Self::SubscribeCandles { symbol, interval }
                }
                CommandIn::UnsubscribeCandles { symbol, interval } => {
                    Self::UnsubscribeCandles { symbol, interval }
                }
                CommandIn::PlaceOrders { command, orders } => Self::PlaceOrders { command, orders },
                CommandIn::CancelOrders { command, ids } => Self::CancelOrders { command, ids },
                #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
                CommandIn::CancelOrdersWithSymbols { command, orders } => {
                    Self::CancelOrdersWithSymbols { command, orders }
                }
                CommandIn::AmendOrder {
                    command,
                    id,
                    price,
                    amount,
                } => Self::AmendOrder {
                    command,
                    id,
                    price,
                    amount,
                },
                #[cfg(feature = "command-kucoinspot")]
                CommandIn::AmendOrderWithSymbol {
                    command,
                    id,
                    symbol,
                    price,
                    amount,
                } => Self::AmendOrderWithSymbol {
                    command,
                    id,
                    symbol,
                    price,
                    amount,
                },
                CommandIn::SetMarginType {
                    command,
                    symbol,
                    isolated,
                } => Self::SetMarginType {
                    command,
                    symbol,
                    isolated,
                },
                CommandIn::SetPositionMode {
                    command,
                    hedged,
                    symbol,
                } => Self::SetPositionMode {
                    command,
                    hedged,
                    symbol,
                },
                #[cfg(feature = "command-asterdex")]
                CommandIn::FetchStpMode { command } => Self::FetchStpMode { command },
                CommandIn::FetchAccount { command } => Self::FetchAccount { command },
                #[cfg(feature = "command-order-book")]
                CommandIn::FetchOrderBook {
                    command,
                    symbol,
                    limit,
                } => Self::FetchOrderBook {
                    command,
                    symbol,
                    limit,
                },
                #[cfg(feature = "command-asterdex")]
                CommandIn::SetStpMode { command, mode } => Self::SetStpMode { command, mode },
                #[cfg(feature = "command-schedule-cancel")]
                CommandIn::ScheduleCancel { command, time } => Self::Venue {
                    command,
                    action: crate::VenueAction::ScheduleCancel { time_ms: time },
                },
                CommandIn::RefreshMarkets { command } => Self::Venue {
                    command,
                    action: crate::VenueAction::RefreshMarkets,
                },
                CommandIn::FetchAccountTier { command } => Self::Venue {
                    command,
                    action: crate::VenueAction::FetchAccountTier,
                },
                CommandIn::SetAccountTier { command, tier } => Self::Venue {
                    command,
                    action: crate::VenueAction::SetAccountTier { tier },
                },
                CommandIn::FetchBalanceInfo { command } => Self::Venue {
                    command,
                    action: crate::VenueAction::FetchBalanceInfo,
                },
                CommandIn::RefreshSettings { command, symbol } => Self::Venue {
                    command,
                    action: crate::VenueAction::RefreshSettings { symbol },
                },
                CommandIn::FetchClosedPositions {
                    command,
                    symbol,
                    from,
                    to,
                } => Self::Venue {
                    command,
                    action: crate::VenueAction::FetchClosedPositions {
                        symbol,
                        from_ms: from,
                        to_ms: to,
                    },
                },
                CommandIn::FetchLeverage { command, symbol } => {
                    Self::FetchLeverage { command, symbol }
                }
                CommandIn::FetchMaxLeverage { command, symbol } => {
                    Self::FetchMaxLeverage { command, symbol }
                }
                CommandIn::FetchFills {
                    command,
                    symbol,
                    order_id,
                    from,
                    to,
                    limit,
                } => Self::Query {
                    command,
                    query: Query::Fills {
                        symbol,
                        order_id,
                        from_ms: from,
                        to_ms: to,
                        limit,
                    },
                },
                CommandIn::FetchOrderHistory {
                    command,
                    symbol,
                    from,
                    to,
                    limit,
                } => Self::Query {
                    command,
                    query: Query::OrderHistory {
                        symbol,
                        from_ms: from,
                        to_ms: to,
                        limit,
                    },
                },
                CommandIn::FetchIncome {
                    command,
                    symbol,
                    income_type,
                    from,
                    to,
                    limit,
                } => Self::Query {
                    command,
                    query: Query::Income {
                        symbol,
                        kind: income_type,
                        from_ms: from,
                        to_ms: to,
                        limit,
                    },
                },
                CommandIn::FetchPublicTrades {
                    command,
                    symbol,
                    limit,
                } => Self::Query {
                    command,
                    query: Query::PublicTrades { symbol, limit },
                },
                CommandIn::CancelSymbolOrders { command, symbol } => {
                    Self::CancelSymbolOrders { command, symbol }
                }
                CommandIn::SetLeverage {
                    command,
                    symbol,
                    leverage,
                    side,
                } => Self::SetLeverage {
                    command,
                    symbol,
                    leverage,
                    side,
                },
                CommandIn::FetchCandles {
                    command,
                    symbol,
                    interval,
                    limit,
                    from,
                    to,
                } => Self::FetchCandles {
                    command,
                    symbol,
                    interval,
                    limit,
                    from_ms: from,
                    to_ms: to,
                },
            }
        }
    }
}

/// One valid command per variant, as ordered (key, raw JSON value) pairs.
fn examples() -> Vec<Vec<(&'static str, &'static str)>> {
    let order = r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#;
    #[allow(unused_mut)]
    let mut all = vec![
        vec![("op", r#""refreshMarkets""#), ("command", "1")],
        vec![
            ("op", r#""setTickerSymbols""#),
            ("symbols", r#"["BTCUSDT","ETHUSDT"]"#),
        ],
        vec![
            ("op", r#""subscribeOrderBook""#),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![
            ("op", r#""unsubscribeOrderBook""#),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![
            ("op", r#""subscribeCandles""#),
            ("symbol", r#""BTCUSDT""#),
            ("interval", r#""1m""#),
        ],
        vec![
            ("op", r#""unsubscribeCandles""#),
            ("symbol", r#""BTCUSDT""#),
            ("interval", r#""1m""#),
        ],
        vec![
            ("op", r#""placeOrders""#),
            ("command", "2"),
            ("orders", Box::leak(format!("[{order}]").into_boxed_str())),
        ],
        vec![
            ("op", r#""cancelOrders""#),
            ("command", "3"),
            ("ids", r#"["a","b"]"#),
        ],
        vec![
            ("op", r#""amendOrder""#),
            ("command", "4"),
            ("id", r#""a""#),
            ("price", "101.5"),
            ("amount", "2"),
        ],
        vec![
            ("op", r#""cancelSymbolOrders""#),
            ("command", "5"),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![
            ("op", r#""setLeverage""#),
            ("command", "6"),
            ("symbol", r#""BTCUSDT""#),
            ("leverage", "10"),
            ("side", r#""long""#),
        ],
        vec![
            ("op", r#""setMarginType""#),
            ("command", "7"),
            ("symbol", r#""BTCUSDT""#),
            ("isolated", "true"),
        ],
        vec![
            ("op", r#""setPositionMode""#),
            ("command", "8"),
            ("hedged", "false"),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![("op", r#""fetchAccount""#), ("command", "9")],
        vec![("op", r#""getAccount""#), ("command", "9")],
        vec![("op", r#""fetchAccountTier""#), ("command", "10")],
        vec![
            ("op", r#""setAccountTier""#),
            ("command", "11"),
            ("tier", r#""plus""#),
        ],
        vec![("op", r#""fetchBalanceInfo""#), ("command", "12")],
        vec![
            ("op", r#""refreshSettings""#),
            ("command", "12"),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![
            ("op", r#""fetchClosedPositions""#),
            ("command", "13"),
            ("symbol", r#""BTCUSDT""#),
            ("from", "1"),
            ("to", "2"),
        ],
        vec![
            ("op", r#""fetchLeverage""#),
            ("command", "14"),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![
            ("op", r#""fetchMaxLeverage""#),
            ("command", "15"),
            ("symbol", r#""BTCUSDT""#),
        ],
        vec![
            ("op", r#""fetchFills""#),
            ("command", "16"),
            ("symbol", r#""BTCUSDT""#),
            ("orderId", r#""o""#),
            ("from", "1"),
            ("to", "2"),
            ("limit", "50"),
        ],
        vec![
            ("op", r#""fetchOrderHistory""#),
            ("command", "17"),
            ("symbol", r#""BTCUSDT""#),
            ("from", "1"),
            ("to", "2"),
            ("limit", "50"),
        ],
        vec![
            ("op", r#""fetchIncome""#),
            ("command", "18"),
            ("symbol", r#""BTCUSDT""#),
            ("incomeType", r#""funding""#),
            ("from", "1"),
            ("to", "2"),
            ("limit", "50"),
        ],
        vec![
            ("op", r#""findOrderByClientOrderId""#),
            ("command", "19"),
            ("symbol", r#""BTCUSDT""#),
            ("clientOrderId", r#""c""#),
        ],
        vec![
            ("op", r#""fetchPublicTrades""#),
            ("command", "20"),
            ("symbol", r#""BTCUSDT""#),
            ("limit", "5"),
        ],
        vec![
            ("op", r#""fetchCandles""#),
            ("command", "21"),
            ("symbol", r#""BTCUSDT""#),
            ("interval", r#""1m""#),
            ("limit", "2"),
            ("from", "1"),
            ("to", "2"),
        ],
    ];
    #[cfg(feature = "command-options")]
    all.extend([
        vec![("op", r#""fetchOptionChain""#), ("command", "22"), ("query", r#"{"underlying":"BTC","expiry":1800000000000,"right":"put","strikeMin":90000,"strikeMax":110000}"#)],
        vec![("op", r#""fetchOptionExpiries""#), ("command", "23"), ("underlying", r#""BTC""#)],
        vec![("op", r#""fetchAccountGreeks""#), ("command", "24")],
    ]);
    #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
    all.push(vec![
        ("op", r#""cancelOrdersWithSymbols""#),
        ("command", "25"),
        ("orders", r#"[{"id":"7","symbol":"BTCUSDT"}]"#),
    ]);
    #[cfg(feature = "command-kucoinspot")]
    all.push(vec![
        ("op", r#""amendOrderWithSymbol""#),
        ("command", "26"),
        ("id", r#""a""#),
        ("symbol", r#""BTC-USDT""#),
        ("price", "100"),
        ("amount", "2"),
    ]);
    #[cfg(feature = "command-asterdex")]
    all.extend([
        vec![("op", r#""fetchStpMode""#), ("command", "27")],
        vec![
            ("op", r#""setStpMode""#),
            ("command", "28"),
            ("mode", r#""EXPIRE_TAKER""#),
        ],
    ]);
    #[cfg(feature = "command-order-book")]
    all.push(vec![
        ("op", r#""fetchOrderBook""#),
        ("command", "29"),
        ("symbol", r#""BTCUSDT""#),
        ("limit", "50"),
    ]);
    #[cfg(feature = "command-schedule-cancel")]
    all.push(vec![
        ("op", r#""scheduleCancel""#),
        ("command", "30"),
        ("time", "1700000000000"),
    ]);
    all
}

fn render(fields: &[(&str, String)]) -> String {
    let body = fields
        .iter()
        .map(|(key, value)| format!("{key:?}:{value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

/// Every mutation the derive-based decoder distinguishes: placement of the
/// tag, absent, null, mistyped, duplicate and unknown fields, and tag errors.
fn corpus() -> Vec<String> {
    const VALUES: [&str; 23] = [
        "null",
        r#""x""#,
        "1",
        "1.5",
        "-1",
        "4294967296",
        "true",
        "[]",
        "{}",
        r#""1e3""#,
        "1e3",
        // Reader-level faults: number range, surrogate pairing, signed zero,
        // integral floats and 64-bit overflow.
        "1e999",
        "-1e999",
        r#""\ud800""#,
        r#""\udc00x""#,
        "-0",
        "1.0",
        "18446744073709551616",
        "-9223372036854775809",
        // Map forms an enum field accepts or refuses.
        r#"{"buy":null}"#,
        r#"{"buy":1}"#,
        r#"{"BUY":null}"#,
        r#"{"buy":null,"sell":null}"#,
    ];
    let mut out = vec![
        String::new(),
        " ".into(),
        "{}".into(),
        "[]".into(),
        "null".into(),
        "1".into(),
        r#""op""#.into(),
        r#"{"op":"#.into(),
        r#"{"op":"fetchAccount","command":1"#.into(),
        r#"{"op":"fetchAccount","command":1} x"#.into(),
        r#"{"op":"fetchAccount","command":1,}"#.into(),
        r#"{"command":1}"#.into(),
        r#"{"nested":{"op":"fetchAccount"},"command":1}"#.into(),
        r#"{"op":"\u0066etchAccount","command":1}"#.into(),
        r#"{"op":"fetch\u0041ccount","command":1}"#.into(),
        // Escaped keys: the tag itself and a payload field.
        r#"{"\u006fp":"fetchAccount","command":1}"#.into(),
        r#"{"op":"fetchAccount","\u0063ommand":1}"#.into(),
        // Nesting at and over the reader's recursion limit, in an ignored field.
        format!(
            r#"{{"op":"fetchAccount","command":1,"zz":{}{}}}"#,
            "[".repeat(126),
            "]".repeat(126)
        ),
        format!(
            r#"{{"op":"fetchAccount","command":1,"zz":{}{}}}"#,
            "[".repeat(127),
            "]".repeat(127)
        ),
        format!(
            r#"{{"op":"fetchAccount","command":1,"zz":{}{}}}"#,
            "[".repeat(128),
            "]".repeat(128)
        ),
        format!(
            r#"{{"op":"fetchAccount","command":1,"zz":{}{}}}"#,
            "[".repeat(200),
            "]".repeat(200)
        ),
        "{\n  \"op\" : \"fetchAccount\" ,\n  \"command\" : 1\n}".into(),
        r#"{"op":"fetchAccount","command":1,"timeoutMs":5000}"#.into(),
    ];
    for example in examples() {
        let owned: Vec<(&str, String)> =
            example.iter().map(|(k, v)| (*k, (*v).to_owned())).collect();
        out.push(render(&owned));
        // The tag last, and in the middle.
        let mut moved = owned.clone();
        let tag = moved.remove(0);
        moved.push(tag.clone());
        out.push(render(&moved));
        if moved.len() > 2 {
            let mut middle = owned[1..].to_vec();
            middle.insert(1, tag.clone());
            out.push(render(&middle));
        }
        // Unknown fields, including one shaped like a tag, and every value an
        // ignored field must still validate.
        let mut extra = owned.clone();
        extra.push(("zz", "1".into()));
        extra.push(("nested", r#"{"op":"refreshMarkets"}"#.into()));
        out.push(render(&extra));
        for value in VALUES {
            let mut ignored = owned.clone();
            ignored.insert(1, ("zz", value.into()));
            out.push(render(&ignored));
            let mut nested = owned.clone();
            nested.push(("zz", format!(r#"{{"a":[{value}]}}"#)));
            out.push(render(&nested));
        }
        let mut envelope = owned.clone();
        envelope.push(("timeoutMs", "5000".into()));
        out.push(render(&envelope));
        // Tag errors.
        for op in [r#""nope""#, r#""FetchAccount""#, "1", "null", r#""""#] {
            let mut bad = owned.clone();
            bad[0].1 = op.into();
            out.push(render(&bad));
        }
        let mut twice = owned.clone();
        twice.push(tag.clone());
        out.push(render(&twice));
        for index in 1..owned.len() {
            let mut missing = owned.clone();
            missing.remove(index);
            out.push(render(&missing));
            let mut duplicate = owned.clone();
            duplicate.push(owned[index].clone());
            out.push(render(&duplicate));
            for value in VALUES {
                let mut typed = owned.clone();
                typed[index].1 = value.into();
                out.push(render(&typed));
            }
        }
    }
    out.extend(nested_corpus(&VALUES));
    out.extend(precedence_corpus());
    out.extend(sequence_corpus());
    out
}

/// Error precedence when a tag fault coexists with a later fault: the tag
/// first, in the middle and last, against reader faults and payload faults,
/// plus decoded-equivalent duplicate tag keys (codex-uufub4).
fn precedence_corpus() -> Vec<String> {
    let tags = [r#""fetchAccount""#, r#""nope""#, "1", "null"];
    let faults = [
        ("command", r#""x""#),
        ("command", "1e999"),
        ("zz", "1e400"),
        ("zz", r#""\ud800""#),
        ("zz", r#"{"a":[1e999]}"#),
    ];
    let mut out = Vec::new();
    for tag in tags {
        for (key, value) in faults {
            let fields = [
                ("op", tag.to_owned()),
                (key, value.to_owned()),
                ("command", "1".to_owned()),
            ];
            for order in [[0, 1, 2], [1, 0, 2], [1, 2, 0]] {
                let ordered: Vec<(&str, String)> = order
                    .iter()
                    .map(|i| (fields[*i].0, fields[*i].1.clone()))
                    .collect();
                out.push(render(&ordered));
            }
        }
        // A duplicate tag, before and after a fault.
        out.push(format!(r#"{{"op":{tag},"op":"fetchAccount","command":1}}"#));
        out.push(format!(
            r#"{{"op":{tag},"zz":1e999,"op":"fetchAccount","command":1}}"#
        ));
        out.push(format!(
            r#"{{"op":{tag},"command":"x","op":"fetchAccount"}}"#
        ));
    }
    // Decoded-equivalent tag keys.
    out.push(r#"{"op":"fetchAccount","o\u0070":"fetchAccount","command":1}"#.to_owned());
    out.push(r#"{"\u006f\u0070":"fetchAccount","command":1}"#.to_owned());
    out.push(r#"{"o\u0070":"nope","op":"fetchAccount","command":1}"#.to_owned());
    // Bad payload in an order with a bad tag later or earlier.
    out.push(r#"{"orders":[{"side":1}],"op":"placeOrders","command":2}"#.to_owned());
    out.push(r#"{"orders":[{"side":1}],"op":"nope","command":2}"#.to_owned());
    out.push(r#"{"op":"nope","orders":[{"side":1e999}],"command":2}"#.to_owned());
    out
}

/// serde's internally tagged enums also accept a sequence whose first element
/// is the tag and whose remaining elements are the fields in order.
fn sequence_corpus() -> Vec<String> {
    let mut out = vec![
        r#"["fetchAccount"]"#.to_owned(),
        r#"["nope",1]"#.to_owned(),
        "[1,1]".to_owned(),
        "[null]".to_owned(),
    ];
    for example in examples() {
        let values: Vec<&str> = example.iter().map(|(_, v)| *v).collect();
        out.push(format!("[{}]", values.join(",")));
        if values.len() > 1 {
            out.push(format!("[{}]", values[..values.len() - 1].join(",")));
        }
        out.push(format!("[{},1]", values.join(",")));
        out.push(format!("[{},1e999]", values.join(",")));
        if values.len() > 1 {
            let mut typed = values.clone();
            typed[1] = r#""x""#;
            out.push(format!("[{}]", typed.join(",")));
        }
    }
    out
}

/// Outer fields, the nested payload key, its inner fields, and the payload
/// template the inner object replaces `@` in.
type Nest = (Fields, &'static str, Fields, &'static str);
type Fields = Vec<(&'static str, &'static str)>;

/// The same per-field mutations one level down: inside the first element of a
/// payload array, or inside a payload object.
fn nested_corpus(values: &[&str]) -> Vec<String> {
    let order: Vec<(&str, &str)> = vec![
        ("symbol", r#""BTCUSDT""#),
        ("type", r#""limit""#),
        ("side", r#""buy""#),
        ("amount", "1"),
        ("price", "100"),
        ("triggerPrice", "99"),
        ("maxSlippagePct", "0.5"),
        ("stopLoss", "90"),
        ("takeProfit", "110"),
        ("reduceOnly", "false"),
        ("timeInForce", r#""GoodTillCancel""#),
        ("triggerType", r#""mark_price""#),
        ("tpTriggerType", r#""last_price""#),
        ("slTriggerType", r#""index_price""#),
        ("clientOrderId", r#""client-1""#),
        ("close", "false"),
        ("stpMode", r#""EXPIRE_TAKER""#),
        ("bybitBbo", r#"{"sideType":"Queue","level":1}"#),
        ("bybitSlippageToleranceTicks", "2"),
        ("impliedVolatility", "0.5"),
    ];
    #[allow(unused_mut)]
    let mut nests: Vec<Nest> = vec![(
        vec![("op", r#""placeOrders""#), ("command", "2")],
        "orders",
        order,
        "[@]",
    )];
    #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
    nests.push((
        vec![("op", r#""cancelOrdersWithSymbols""#), ("command", "25")],
        "orders",
        vec![("id", r#""7""#), ("symbol", r#""BTCUSDT""#)],
        "[@]",
    ));
    #[cfg(feature = "command-options")]
    nests.push((
        vec![("op", r#""fetchOptionChain""#), ("command", "22")],
        "query",
        vec![
            ("underlying", r#""BTC""#),
            ("expiry", "1800000000000"),
            ("right", r#""put""#),
            ("strikeMin", "90000"),
        ],
        "@",
    ));
    // Third level: the Bybit BBO object inside an order.
    nests.push((
        vec![("op", r#""placeOrders""#), ("command", "2")],
        "orders",
        vec![("sideType", r#""Queue""#), ("level", "1")],
        r#"[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"bybitBbo":@}]"#,
    ));
    let mut out = Vec::new();
    for (outer, key, inner, template) in nests {
        let inner: Vec<(&str, String)> = inner.iter().map(|(k, v)| (*k, (*v).to_owned())).collect();
        let wrap = |fields: &[(&str, String)]| {
            let object = render(fields);
            let value = template.replacen('@', &object, 1);
            let mut whole: Vec<(&str, String)> =
                outer.iter().map(|(k, v)| (*k, (*v).to_owned())).collect();
            whole.push((key, value));
            render(&whole)
        };
        let base = wrap(&inner);
        assert!(
            serde_json::from_str::<legacy::CommandIn>(&base).is_ok(),
            "nested template must be valid: {base}"
        );
        out.push(base);
        for value in values {
            let mut unknown = inner.clone();
            unknown.push(("zz", (*value).to_owned()));
            out.push(wrap(&unknown));
        }
        for index in 0..inner.len() {
            let mut missing = inner.clone();
            missing.remove(index);
            out.push(wrap(&missing));
            let mut duplicate = inner.clone();
            duplicate.push(inner[index].clone());
            out.push(wrap(&duplicate));
            for value in values.iter().chain(&[r#""BUY""#, r#""LIMIT""#, r#""PUT""#]) {
                let mut typed = inner.clone();
                typed[index].1 = (*value).to_owned();
                out.push(wrap(&typed));
            }
        }
    }
    out
}

fn outcome(
    result: Result<Command, serde_json::Error>,
) -> Result<Command, (String, String, usize, usize)> {
    result.map_err(|error| {
        (
            error.to_string(),
            format!("{:?}", error.classify()),
            error.line(),
            error.column(),
        )
    })
}

#[test]
fn every_example_is_a_valid_command() {
    for example in examples() {
        let json = render(
            &example
                .iter()
                .map(|(k, v)| (*k, (*v).to_owned()))
                .collect::<Vec<_>>(),
        );
        assert!(
            serde_json::from_str::<legacy::CommandIn>(&json).is_ok(),
            "{json}"
        );
    }
}

/// Every corpus input decodes to the same command, or fails with the same
/// message, category and position, as the derive-based decoder.
#[test]
fn the_command_decoder_matches_the_derive_based_wire_exactly() {
    let corpus = corpus();
    assert!(corpus.len() > 1000, "corpus size {}", corpus.len());
    let mut mismatches = Vec::new();
    for json in &corpus {
        let expected = outcome(serde_json::from_str::<legacy::CommandIn>(json).map(Command::from));
        let actual = outcome(decode_command(json));
        if expected != actual {
            mismatches.push(format!(
                "{json}\n  legacy: {expected:?}\n  actual: {actual:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

/// F1 (claude-dazzy3): the derive decoder buffers every value, so the reader
/// validates fields it then ignores. A skim with `IgnoredAny` would accept these.
/// The decoder keeps today's behaviour: each is a positioned Syntax error.
#[test]
fn ignored_values_are_validated_like_every_other_value() {
    for value in ["1e999", "-1e999", r#""\ud800""#, r#""\udc00x""#] {
        for json in [
            format!(r#"{{"op":"fetchAccount","zz":{value},"command":1}}"#),
            format!(r#"{{"op":"fetchAccount","command":1,"zz":{{"a":[{value}]}}}}"#),
        ] {
            let error = decode_command(&json).expect_err(&json);
            assert_eq!(
                error.classify(),
                serde_json::error::Category::Syntax,
                "{json}"
            );
            assert!(error.line() > 0, "{json}");
            assert!(
                serde_json::from_str::<serde::de::IgnoredAny>(&json).is_ok(),
                "a skim alone accepts {json}"
            );
        }
    }
}
