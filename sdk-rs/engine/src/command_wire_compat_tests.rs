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
    const VALUES: [&str; 11] = [
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
        r#"{"op":"fetchAccount","command":1}"#.into(),
        r#"{"op":"fetchAccount","command":1}"#.into(),
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
        // Unknown fields, including one shaped like a tag.
        let mut extra = owned.clone();
        extra.push(("zz", "1".into()));
        extra.push(("nested", r#"{"op":"refreshMarkets"}"#.into()));
        out.push(render(&extra));
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
