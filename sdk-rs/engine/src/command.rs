//! The shared JSON command boundary for engine hosts.
#[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
use crate::model::CancelOrderRef;
use crate::model::PlaceOrderOpts;
#[cfg(feature = "command-asterdex")]
use crate::model::StpMode;
use crate::{Command, Query};
use serde::Deserialize;

/// Commands shared by JavaScript and native WebSocket clients.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CommandIn {
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
    #[cfg(any(feature = "command-apexomni", feature = "command-kucoinspot"))]
    GetAccount {
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
        interval: String,
    },
    UnsubscribeCandles {
        symbol: String,
        interval: String,
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
    },
    SetMarginType {
        command: u32,
        symbol: String,
        isolated: bool,
    },
    SetPositionMode {
        command: u32,
        hedged: bool,
    },
    #[cfg(feature = "command-asterdex")]
    FetchStpMode {
        command: u32,
    },
    #[cfg(any(feature = "command-asterdex", feature = "command-polymarket"))]
    FetchAccount {
        command: u32,
    },
    #[cfg(any(feature = "command-asterdex", feature = "command-polymarket"))]
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
    FetchLeverage {
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
        interval: String,
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
            #[cfg(any(feature = "command-apexomni", feature = "command-kucoinspot"))]
            CommandIn::GetAccount { command } => Self::FetchAccount { command },
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
            CommandIn::SetPositionMode { command, hedged } => {
                Self::SetPositionMode { command, hedged }
            }
            #[cfg(feature = "command-asterdex")]
            CommandIn::FetchStpMode { command } => Self::FetchStpMode { command },
            #[cfg(any(feature = "command-asterdex", feature = "command-polymarket"))]
            CommandIn::FetchAccount { command } => Self::FetchAccount { command },
            #[cfg(any(feature = "command-asterdex", feature = "command-polymarket"))]
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
            CommandIn::FetchLeverage { command, symbol } => Self::FetchLeverage { command, symbol },
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
            } => Self::SetLeverage {
                command,
                symbol,
                leverage,
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

/// Decode the same typed command protocol on every host. Native hosts enable
/// all command features; WASM retains the venue-specific parser size gates.
// Inline both boundaries to avoid copying a large command enum across crates.
#[inline(always)]
pub fn decode_command(json: &str) -> Result<Command, serde_json::Error> {
    serde_json::from_str::<CommandIn>(json).map(Command::from)
}

/// Host configuration is outside the command/result ID domain.
#[derive(Debug, PartialEq)]
pub enum HostInput {
    Command(Command),
    TickIntervalChanged { interval_ms: f64 },
}

/// Decode startup subscriptions without compiling the trading/query parser.
#[inline(always)]
pub fn decode_subscription(json: &str) -> Result<Option<Command>, serde_json::Error> {
    #[derive(Deserialize)]
    struct Header<'a> {
        #[serde(borrow)]
        op: std::borrow::Cow<'a, str>,
    }
    let header: Header<'_> = serde_json::from_str(json)?;
    if matches!(
        header.op.as_ref(),
        "subscribeOrderBook" | "unsubscribeOrderBook" | "subscribeCandles" | "unsubscribeCandles"
    ) {
        // Keep the internally tagged representation: unknown values and duplicate
        // fields must validate exactly as they do in the complete command enum.
        #[derive(Deserialize)]
        #[serde(tag = "op", rename_all = "camelCase")]
        enum Subscription {
            SubscribeOrderBook { symbol: String },
            UnsubscribeOrderBook { symbol: String },
            SubscribeCandles { symbol: String, interval: String },
            UnsubscribeCandles { symbol: String, interval: String },
        }
        let command = match serde_json::from_str::<Subscription>(json)? {
            Subscription::SubscribeOrderBook { symbol } => Command::SubscribeOrderBook { symbol },
            Subscription::UnsubscribeOrderBook { symbol } => {
                Command::UnsubscribeOrderBook { symbol }
            }
            Subscription::SubscribeCandles { symbol, interval } => {
                Command::SubscribeCandles { symbol, interval }
            }
            Subscription::UnsubscribeCandles { symbol, interval } => {
                Command::UnsubscribeCandles { symbol, interval }
            }
        };
        return Ok(Some(command));
    }
    Ok(None)
}

/// Decode the shared host protocol, including nullable polling configuration.
/// Hosts opt into polling controls; other standalone builds parse commands once.
// Keep the large command variant inlined through the host boundary as well.
#[inline(always)]
pub fn decode_host_input(json: &str) -> Result<HostInput, serde_json::Error> {
    #[cfg(not(feature = "polling-control"))]
    {
        decode_command(json).map(HostInput::Command)
    }
    #[cfg(feature = "polling-control")]
    {
        #[derive(Deserialize)]
        struct Header<'a> {
            #[serde(borrow)]
            op: std::borrow::Cow<'a, str>,
        }
        let header: Header<'_> = serde_json::from_str(json)?;
        if header.op != "setTickInterval" {
            return decode_command(json).map(HostInput::Command);
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Interval {
            Millis(f64),
            Paused(()),
        }
        #[derive(Deserialize)]
        struct Control {
            interval: Interval,
        }
        let control: Control = serde_json::from_str(json)?;
        let interval_ms = match control.interval {
            Interval::Millis(ms) => ms,
            Interval::Paused(()) => f64::INFINITY,
        };
        if interval_ms.is_nan() || interval_ms <= 0.0 {
            return Err(serde::de::Error::custom(
                "Polling interval must be positive or null to pause",
            ));
        }
        let interval_ms = crate::engine::bounded_tick_interval(interval_ms);
        Ok(HostInput::TickIntervalChanged { interval_ms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_context_commands_follow_the_compiled_parser_features() {
        let account = decode_command(r#"{"op":"getAccount","command":77}"#);
        assert_eq!(
            account.is_ok(),
            cfg!(any(
                feature = "command-apexomni",
                feature = "command-kucoinspot"
            ))
        );
        if let Ok(command) = account {
            assert_eq!(command, Command::FetchAccount { command: 77 });
        }
        let cancel = decode_command(
            r#"{"op":"cancelOrdersWithSymbols","command":78,"orders":[{"id":"unseen","symbol":"BTC-USDT"}]}"#,
        );
        assert_eq!(
            cancel.is_ok(),
            cfg!(any(
                feature = "command-asterdex",
                feature = "command-kucoinspot"
            ))
        );
        if let Ok(cancel) = cancel {
            let Command::CancelOrdersWithSymbols { command, orders } = cancel else {
                panic!("symbol-scoped cancellation changed its command mapping");
            };
            assert_eq!(command, 78);
            assert_eq!(
                orders,
                vec![crate::model::CancelOrderRef {
                    id: "unseen".into(),
                    symbol: "BTC-USDT".into(),
                }]
            );
        }
        let amend = decode_command(
            r#"{"op":"amendOrderWithSymbol","command":79,"id":"unseen","symbol":"BTC-USDT","price":100,"amount":2}"#,
        );
        #[cfg(feature = "command-kucoinspot")]
        assert_eq!(
            amend.unwrap(),
            Command::AmendOrderWithSymbol {
                command: 79,
                id: "unseen".into(),
                symbol: "BTC-USDT".into(),
                price: Some(100.0),
                amount: Some(2.0),
            }
        );
        #[cfg(not(feature = "command-kucoinspot"))]
        assert!(amend.is_err());
        assert!(
            decode_command(
                r#"{"op":"amendOrderWithSymbol","command":79,"id":"unseen","price":100}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn subscription_boundary_matches_the_complete_command_decoder() {
        for op in [
            "subscribeOrderBook",
            "unsubscribeOrderBook",
            "subscribeCandles",
            "unsubscribeCandles",
        ] {
            for template in [
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":"1m"}"#,
                r#"{"interval":"1m","symbol":"BTCUSDT","op":"@OP@"}"#,
                r#"{"op":"@OP@","symbol":"BTC\u0055SDT","interval":"1\u006d"}"#,
                r#"{"op":"@OP@","symbol":"","interval":""}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT"}"#,
                r#"{"op":"@OP@","interval":"1m"}"#,
                r#"{"op":"@OP@","symbol":null,"interval":"1m"}"#,
                r#"{"op":"@OP@","symbol":123,"interval":"1m"}"#,
                r#"{"op":"@OP@","symbol":[],"interval":"1m"}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":null}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":2}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":{}}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","symbol":"ETHUSDT","interval":"1m"}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":"1m","interval":"5m"}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":"1m","unknown":{"rows":[true,null,1.5,"x"]}}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":"1m","unknown":1e400}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":"1m","unknown":{"n":-1e400}}"#,
                r#"{"op":"@OP@","symbol":"BTCUSDT","interval":"1m"}null"#,
            ] {
                let json = template.replace("@OP@", op);
                assert_subscription_decoders_match(&json);
                assert_subscription_decoders_match(&json.replace("\"op\"", r#""\u006fp""#));
                assert_subscription_decoders_match(
                    &json.replace(op, &op.replacen('s', r#"\u0073"#, 1)),
                );
            }
            let nested = format!(
                r#"{{"op":"{op}","symbol":"BTCUSDT","interval":"1m","unknown":{}0{}}}"#,
                "[".repeat(130),
                "]".repeat(130)
            );
            assert_subscription_decoders_match(&nested);
        }
        // The host header already rejects duplicate operations before dispatch.
        for json in [
            r#"{"op":"subscribeOrderBook","op":"subscribeOrderBook","symbol":"BTCUSDT"}"#,
            r#"{"op":"subscribeCandles","op":"placeOrders","symbol":"BTCUSDT","interval":"1m"}"#,
        ] {
            assert!(decode_subscription(json).is_err());
            assert!(decode_host_input(json).is_err());
            assert!(decode_command(json).is_err());
        }
    }

    fn assert_subscription_decoders_match(json: &str) {
        let host = decode_subscription(json).map(|selected| selected.expect("subscription input"));
        assert_eq!(
            host.map_err(|error| error.to_string()),
            decode_command(json).map_err(|error| error.to_string()),
            "{json}"
        );
    }

    #[test]
    fn iv_survives_the_command_boundary_with_or_without_option_queries() {
        let command = decode_command(r#"{"op":"placeOrders","command":41,"orders":[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100},{"symbol":"BTC-option","type":"limit","side":"buy","amount":1,"impliedVolatility":0.5}]}"#).unwrap();
        let Command::PlaceOrders { command, orders } = command else {
            panic!("place orders")
        };
        assert_eq!(command, 41);
        assert_eq!(orders[0].implied_volatility, None);
        assert_eq!(orders[1].implied_volatility, Some(0.5));
    }

    #[test]
    #[cfg(not(feature = "command-options"))]
    fn option_json_queries_are_absent_from_unrelated_venue_parsers() {
        for json in [
            r#"{"op":"fetchOptionChain","command":1,"query":{"underlying":"BTC"}}"#,
            r#"{"op":"fetchOptionExpiries","command":2,"underlying":"BTC"}"#,
            r#"{"op":"fetchAccountGreeks","command":3}"#,
        ] {
            assert!(
                decode_command(json).is_err(),
                "option parser must be explicitly enabled: {json}"
            );
        }
    }

    #[test]
    #[cfg(feature = "command-options")]
    fn option_json_queries_preserve_ids_and_all_native_filters_when_enabled() {
        assert_eq!(decode_command(r#"{"op":"fetchOptionChain","command":1,"query":{"underlying":"BTC","expiry":1800000000000,"right":"put","strikeMin":90000,"strikeMax":110000}}"#).unwrap(),
            Command::FetchOptionChain { command: 1, query: crate::model::OptionChainQuery {
                underlying: "BTC".into(), expiry: Some(1_800_000_000_000.0), right: Some(crate::model::OptionRight::Put), strike_min: Some(90_000.0), strike_max: Some(110_000.0)
            }});
        assert_eq!(
            decode_command(r#"{"op":"fetchOptionExpiries","command":2,"underlying":"ETH"}"#)
                .unwrap(),
            Command::FetchOptionExpiries {
                command: 2,
                underlying: "ETH".into()
            }
        );
        assert_eq!(
            decode_command(r#"{"op":"fetchAccountGreeks","command":3}"#).unwrap(),
            Command::FetchAccountGreeks { command: 3 }
        );
    }

    #[test]
    #[cfg(not(feature = "polling-control"))]
    fn polling_controls_are_excluded_from_ordinary_command_parsers() {
        assert!(decode_host_input(r#"{"op":"setTickInterval","interval":1000}"#).is_err());
        assert!(decode_host_input(r#"{"op":"setTickInterval","interval":null}"#).is_err());
    }

    #[test]
    #[cfg(feature = "polling-control")]
    fn host_configuration_accepts_positive_milliseconds_and_explicit_pause() {
        assert_eq!(
            decode_host_input(r#"{"op":"setTickInterval","interval":123.5}"#).unwrap(),
            HostInput::TickIntervalChanged {
                interval_ms: 1_000.0
            }
        );
        assert_eq!(
            decode_host_input(r#"{"op":"setTick\u0049nterval","interval":null}"#).unwrap(),
            HostInput::TickIntervalChanged {
                interval_ms: f64::INFINITY
            }
        );
    }

    #[test]
    #[cfg(feature = "polling-control")]
    fn polling_intervals_cannot_create_request_floods_or_overflow_timers() {
        for (input, expected) in [
            (0.00001, 1_000.0),
            (1.0, 1_000.0),
            (2_000.0, 2_000.0),
            (1e300, 120_000.0),
        ] {
            let json = format!(r#"{{"op":"setTickInterval","interval":{input}}}"#);
            assert_eq!(
                decode_host_input(&json).unwrap(),
                HostInput::TickIntervalChanged {
                    interval_ms: expected
                }
            );
        }
    }

    #[test]
    fn host_configuration_requires_a_typed_positive_interval_or_null() {
        for json in [
            r#"{"op":"setTickInterval"}"#,
            r#"{"op":"setTickInterval","interval":0}"#,
            r#"{"op":"setTickInterval","interval":-0.0}"#,
            r#"{"op":"setTickInterval","interval":-1}"#,
            r#"{"op":"setTickInterval","interval":"1000"}"#,
            r#"{"op":"setTickInterval","interval":false}"#,
            r#"{"op":"setTickInterval","interval":{}}"#,
            r#"{"op":"setTickInterval","interval":NaN}"#,
            r#"{"op":"setTickInterval","interval":1e309}"#,
        ] {
            assert!(decode_host_input(json).is_err(), "must reject {json}");
        }
    }

    #[test]
    fn host_input_preserves_command_ids_and_the_typed_command_boundary() {
        assert_eq!(
            decode_host_input(
                r#"{"op":"setLeverage","command":37,"symbol":"BTCUSDT","leverage":5}"#
            )
            .unwrap(),
            HostInput::Command(Command::SetLeverage {
                command: 37,
                symbol: "BTCUSDT".into(),
                leverage: 5.0,
            })
        );
        assert!(decode_host_input(r#"{"op":"setLeverage","command":"37"}"#).is_err());
        assert!(decode_host_input(r#"{"op":"unknown"}"#).is_err());
        assert!(decode_command(r#"{"op":"setTickInterval","interval":1000}"#).is_err());
    }
}
