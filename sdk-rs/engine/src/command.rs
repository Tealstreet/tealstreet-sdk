//! The shared JSON command boundary for engine hosts.
use crate::model::PlaceOrderOpts;
#[cfg(feature = "command-asterdex")]
use crate::model::{CancelOrderRef, StpMode};
use crate::{Command, Query};
use serde::Deserialize;

/// Commands shared by JavaScript and native WebSocket clients.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum CommandIn {
    #[cfg(feature = "command-apexomni")]
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
    #[cfg(feature = "command-asterdex")]
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
            #[cfg(feature = "command-apexomni")]
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
            #[cfg(feature = "command-asterdex")]
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

/// Decode the shared host protocol, including nullable polling configuration.
// Keep the large command variant inlined through the host boundary as well.
#[inline(always)]
pub fn decode_host_input(json: &str) -> Result<HostInput, serde_json::Error> {
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
    Ok(HostInput::TickIntervalChanged { interval_ms })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_configuration_accepts_positive_milliseconds_and_explicit_pause() {
        assert_eq!(
            decode_host_input(r#"{"op":"setTickInterval","interval":123.5}"#).unwrap(),
            HostInput::TickIntervalChanged { interval_ms: 123.5 }
        );
        assert_eq!(
            decode_host_input(r#"{"op":"setTick\u0049nterval","interval":null}"#).unwrap(),
            HostInput::TickIntervalChanged {
                interval_ms: f64::INFINITY
            }
        );
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
