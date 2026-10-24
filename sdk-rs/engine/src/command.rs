//! The shared JSON command boundary for engine hosts.
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
enum CommandIn {
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
            CommandIn::FetchLeverage { command, symbol } => Self::FetchLeverage { command, symbol },
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
    /// A command and the caller's wait for its reply, when the host sent one.
    Command {
        command: Command,
        timeout_ms: Option<u32>,
    },
    PublicFeedUpdate(crate::PublicFeedUpdate),
    /// Factor on each venue poll's own cadence; infinity pauses polling. The
    /// optional ceiling is the host's absolute bound on slowed polls.
    PollScaleChanged {
        scale: f64,
        ceiling_ms: Option<f64>,
    },
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
            SubscribeCandles { symbol: String, interval: Interval },
            UnsubscribeCandles { symbol: String, interval: Interval },
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

/// Decode trusted provider controls on every host and optional polling configuration.
// Keep the large command variant inlined through the host boundary as well.
#[inline(always)]
pub fn decode_host_input(json: &str) -> Result<HostInput, serde_json::Error> {
    // A present wait must hold an integer; `null` is not absence.
    fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
        u64::deserialize(deserializer).map(Some)
    }
    #[derive(Deserialize)]
    struct Header<'a> {
        #[serde(borrow)]
        op: std::borrow::Cow<'a, str>,
        #[serde(default, rename = "timeoutMs", deserialize_with = "present")]
        timeout_ms: Option<u64>,
    }
    let header: Header<'_> = serde_json::from_str(json)?;
    if header.op == "publicFeedUpdate" {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Control {
            op: String,
            #[serde(default)]
            tickers: Vec<crate::public_feed::PublicTicker>,
            #[serde(default)]
            books: Vec<crate::public_feed::PublicBook>,
            #[serde(default)]
            candles: Vec<crate::public_feed::PublicCandle>,
        }
        let control: Control = serde_json::from_str(json)?;
        debug_assert_eq!(control.op, "publicFeedUpdate");
        return Ok(HostInput::PublicFeedUpdate(crate::PublicFeedUpdate {
            tickers: control.tickers,
            books: control.books,
            candles: control.candles,
        }));
    }
    #[cfg(feature = "polling-control")]
    if header.op == "setPollScale" {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Scale {
            Factor(f64),
            Paused(()),
        }
        #[derive(Deserialize)]
        struct Control {
            scale: Scale,
            #[serde(default)]
            ceiling: Option<f64>,
        }
        let control: Control = serde_json::from_str(json)?;
        let scale = match control.scale {
            Scale::Factor(factor) => factor,
            Scale::Paused(()) => f64::INFINITY,
        };
        if scale.is_nan() || scale <= 0.0 {
            return Err(serde::de::Error::custom(
                "Polling scale must be positive or null to pause",
            ));
        }
        if control
            .ceiling
            .is_some_and(|ms| !ms.is_finite() || ms <= 0.0)
        {
            return Err(serde::de::Error::custom(
                "Polling ceiling must be a positive number of milliseconds",
            ));
        }
        return Ok(HostInput::PollScaleChanged {
            scale,
            ceiling_ms: control.ceiling,
        });
    }
    // The envelope's optional caller wait: absent is `None`; a value outside
    // 1..=MAX_COMMAND_WAIT_MS is an error.
    let timeout_ms = match header.timeout_ms {
        None => None,
        Some(ms) if (1..=u64::from(crate::MAX_COMMAND_WAIT_MS)).contains(&ms) => Some(ms as u32),
        Some(_) => {
            return Err(serde::de::Error::custom(
                "timeoutMs must be a positive integer no greater than 2147483647",
            ));
        }
    };
    decode_command(json).map(|command| HostInput::Command {
        command,
        timeout_ms,
    })
}

#[cfg(test)]
#[path = "command_wire_compat_tests.rs"]
mod wire_compat;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_tier_ops_decode_to_venue_actions() {
        assert_eq!(
            decode_command(r#"{"op":"setAccountTier","command":5,"tier":"plus"}"#).unwrap(),
            Command::Venue {
                command: 5,
                action: crate::VenueAction::SetAccountTier {
                    tier: "plus".into()
                },
            }
        );
        assert_eq!(
            decode_command(r#"{"op":"fetchAccountTier","command":6}"#).unwrap(),
            Command::Venue {
                command: 6,
                action: crate::VenueAction::FetchAccountTier,
            }
        );
    }
    #[test]
    fn balance_info_op_decodes_to_a_venue_action() {
        assert_eq!(
            decode_command(r#"{"op":"fetchBalanceInfo","command":8}"#).unwrap(),
            Command::Venue {
                command: 8,
                action: crate::VenueAction::FetchBalanceInfo,
            }
        );
    }
    #[test]
    fn closed_positions_op_decodes_to_a_venue_action() {
        assert_eq!(
            decode_command(r#"{"op":"fetchClosedPositions","command":7,"from":1,"to":2}"#).unwrap(),
            Command::Venue {
                command: 7,
                action: crate::VenueAction::FetchClosedPositions {
                    symbol: None,
                    from_ms: Some(1.0),
                    to_ms: Some(2.0),
                },
            }
        );
    }
    #[test]
    fn symbol_context_commands_follow_the_compiled_parser_features() {
        for op in ["getAccount", "fetchAccount"] {
            assert_eq!(
                decode_command(&format!(r#"{{"op":"{op}","command":77}}"#)).unwrap(),
                Command::FetchAccount { command: 77 }
            );
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

    #[test]
    fn maximum_leverage_read_is_distinct_from_current_account_leverage() {
        let json = r#"{"op":"fetchMaxLeverage","command":82,"symbol":"E-BTC-USDT"}"#;
        let expected = Command::FetchMaxLeverage {
            command: 82,
            symbol: "E-BTC-USDT".into(),
        };
        assert_eq!(decode_command(json).unwrap(), expected);
        assert_eq!(
            decode_host_input(json).unwrap(),
            HostInput::Command {
                command: expected,
                timeout_ms: None
            }
        );
        assert!(decode_command(r#"{"op":"fetchMaxLeverage","command":82}"#).is_err());
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
        assert!(decode_host_input(r#"{"op":"setPollScale","scale":4}"#).is_err());
        assert!(decode_host_input(r#"{"op":"setPollScale","scale":null}"#).is_err());
    }

    #[test]
    #[cfg(feature = "polling-control")]
    fn host_configuration_accepts_a_positive_scale_and_explicit_pause() {
        assert_eq!(
            decode_host_input(r#"{"op":"setPollScale","scale":1.5}"#).unwrap(),
            HostInput::PollScaleChanged {
                scale: 1.5,
                ceiling_ms: None
            }
        );
        assert_eq!(
            decode_host_input(r#"{"op":"setPoll\u0053cale","scale":null}"#).unwrap(),
            HostInput::PollScaleChanged {
                scale: f64::INFINITY,
                ceiling_ms: None,
            }
        );
    }

    #[test]
    #[cfg(feature = "polling-control")]
    fn host_configuration_carries_an_optional_positive_ceiling() {
        assert_eq!(
            decode_host_input(r#"{"op":"setPollScale","scale":60,"ceiling":30000}"#).unwrap(),
            HostInput::PollScaleChanged {
                scale: 60.,
                ceiling_ms: Some(30_000.)
            }
        );
        for json in [
            r#"{"op":"setPollScale","scale":2,"ceiling":0}"#,
            r#"{"op":"setPollScale","scale":2,"ceiling":-5}"#,
            r#"{"op":"setPollScale","scale":2,"ceiling":"1000"}"#,
        ] {
            assert!(decode_host_input(json).is_err(), "must reject {json}");
        }
    }

    /// A venue with one 2 s native poll, so a scale is the host interval / 2000.
    #[cfg(feature = "polling-control")]
    struct PollVenue;
    #[cfg(feature = "polling-control")]
    impl crate::Venue for PollVenue {
        type Pending = ();
        fn start(&mut self, cx: &mut crate::Cx<'_, ()>) {
            cx.set_poll_timer(2_000.0);
        }
        fn on_open(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::SocketId) {}
        fn on_message(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut crate::Cx<'_, ()>, _: (), _: crate::Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut crate::Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::TimerId) {}
        fn on_command(&mut self, _: &mut crate::Cx<'_, ()>, _: crate::Command) {}
        fn book(&self, _: &str) -> Option<&crate::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &crate::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    /// The effective poll deadline after a policy change on a fresh engine:
    /// the last one armed, counting the native arm at startup.
    #[cfg(feature = "polling-control")]
    fn armed_after(change: impl FnOnce(&mut crate::Engine<PollVenue>)) -> Vec<f64> {
        let mut engine = crate::test_support::seeded_engine(PollVenue);
        engine.start(0.0);
        change(&mut engine);
        let armed: Vec<_> = engine
            .drain_actions()
            .filter_map(|action| match action {
                crate::Action::SetTimer { at_ms, .. } => Some(at_ms),
                _ => None,
            })
            .collect();
        armed.last().copied().into_iter().collect()
    }

    // Port of #1351's decode bound: the host's absolute interval, sent as a scale
    // of the native cadence, still cannot flood a venue or overflow a timer.
    #[test]
    #[cfg(feature = "polling-control")]
    fn polling_intervals_cannot_create_request_floods_or_overflow_timers() {
        for (input, expected) in [
            (0.00001, 1_000.0),
            (1.0, 1_000.0),
            (2_000.0, 2_000.0),
            (1e300, 120_000.0),
        ] {
            let json = format!(r#"{{"op":"setPollScale","scale":{}}}"#, input / 2_000.0);
            let HostInput::PollScaleChanged { scale, ceiling_ms } =
                decode_host_input(&json).unwrap()
            else {
                panic!("{json} must decode as a polling control")
            };
            let armed = armed_after(|engine| {
                engine.handle(0.0, crate::Event::PollCeilingChanged { ceiling_ms });
                engine.handle(0.0, crate::Event::PollScaleChanged { scale });
            });
            assert_eq!(armed, [expected], "{json}");
        }
        // A finite scale whose product overflows is still capped; it never pauses.
        let json = r#"{"op":"setPollScale","scale":1e308}"#;
        let HostInput::PollScaleChanged { scale, .. } = decode_host_input(json).unwrap() else {
            panic!("{json} must decode as a polling control")
        };
        let armed = armed_after(|engine| {
            engine.handle(0.0, crate::Event::PollScaleChanged { scale });
        });
        assert_eq!(armed, [120_000.0], "overflowing product takes the cap");
    }

    #[test]
    fn host_configuration_requires_a_typed_positive_scale_or_null() {
        for json in [
            r#"{"op":"setPollScale"}"#,
            r#"{"op":"setPollScale","scale":0}"#,
            r#"{"op":"setPollScale","scale":-0.0}"#,
            r#"{"op":"setPollScale","scale":-1}"#,
            r#"{"op":"setPollScale","scale":"4"}"#,
            r#"{"op":"setPollScale","scale":false}"#,
            r#"{"op":"setPollScale","scale":{}}"#,
            r#"{"op":"setPollScale","scale":NaN}"#,
            r#"{"op":"setPollScale","scale":1e309}"#,
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
            HostInput::Command {
                command: Command::SetLeverage {
                    command: 37,
                    symbol: "BTCUSDT".into(),
                    leverage: 5.0,
                    side: None,
                },
                timeout_ms: None
            }
        );
        assert!(decode_host_input(r#"{"op":"setLeverage","command":"37"}"#).is_err());
        assert!(decode_host_input(r#"{"op":"unknown"}"#).is_err());
        assert!(decode_command(r#"{"op":"setPollScale","scale":4}"#).is_err());
    }

    #[test]
    fn leverage_side_is_optional_and_strictly_typed() {
        for (text, side) in [("long", PositionSide::Long), ("short", PositionSide::Short)] {
            let json = format!(
                r#"{{"op":"setLeverage","command":7,"symbol":"BTCUSDT","leverage":3,"side":"{text}"}}"#
            );
            assert_eq!(
                decode_command(&json).unwrap(),
                Command::SetLeverage {
                    command: 7,
                    symbol: "BTCUSDT".into(),
                    leverage: 3.0,
                    side: Some(side),
                }
            );
        }
        for side in [r#""both""#, r#""buy""#, r#""SHORT""#, "1", "{}"] {
            let json = format!(
                r#"{{"op":"setLeverage","command":7,"symbol":"BTCUSDT","leverage":3,"side":{side}}}"#
            );
            assert!(decode_command(&json).is_err(), "{json}");
        }
    }

    #[test]
    fn position_mode_keeps_optional_market_scope() {
        assert_eq!(
            decode_command(
                r#"{"op":"setPositionMode","command":9,"hedged":true,"symbol":"BTCUSDT"}"#
            )
            .unwrap(),
            Command::SetPositionMode {
                command: 9,
                hedged: true,
                symbol: Some("BTCUSDT".into()),
            }
        );
        assert_eq!(
            decode_command(r#"{"op":"setPositionMode","command":9,"hedged":false}"#).unwrap(),
            Command::SetPositionMode {
                command: 9,
                hedged: false,
                symbol: None,
            }
        );
        assert!(
            decode_command(r#"{"op":"setPositionMode","command":9,"hedged":true,"symbol":17}"#)
                .is_err()
        );
    }

    #[test]
    fn candle_intervals_parse_once_and_unknown_spellings_are_rejected() {
        for interval in Interval::ALL {
            assert_eq!(Interval::parse(interval.as_str()), Some(interval));
            let json = format!(
                r#"{{"op":"fetchCandles","command":3,"symbol":"BTC","interval":"{interval}"}}"#
            );
            assert!(matches!(
                decode_command(&json).unwrap(),
                Command::FetchCandles { interval: parsed, .. } if parsed == interval
            ));
        }
        assert_eq!(Interval::Mo1.ms(), 2_592_000_000.0);
        assert_eq!(Interval::H24.ms(), Interval::D1.ms());
        for bad in ["7m", "1H", "", "60", "1mo"] {
            let fetch =
                format!(r#"{{"op":"fetchCandles","command":3,"symbol":"BTC","interval":"{bad}"}}"#);
            let error = decode_command(&fetch).unwrap_err().to_string();
            assert!(
                error.contains("Unsupported candle interval"),
                "{bad}: {error}"
            );
            let subscribe =
                format!(r#"{{"op":"subscribeCandles","symbol":"BTC","interval":"{bad}"}}"#);
            assert!(decode_subscription(&subscribe).is_err(), "{bad}");
        }
    }
}

#[cfg(test)]
#[test]
fn market_refresh_json_is_an_explicit_read_venue_action() {
    assert!(matches!(
        decode_command(r#"{"op":"refreshMarkets","command":41}"#).unwrap(),
        Command::Venue {
            command: 41,
            action: crate::VenueAction::RefreshMarkets
        }
    ));
}
