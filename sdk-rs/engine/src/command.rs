//! The shared JSON command boundary for engine hosts.
#[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
use crate::model::CancelOrderRef;
#[cfg(feature = "command-asterdex")]
use crate::model::StpMode;
use crate::model::{Interval, PlaceOrderOpts, PositionSide};
use crate::{Command, Query};
use serde::Deserialize;

/// Commands shared by JavaScript and native WebSocket clients.
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
        price: Option<f64>,
        amount: Option<f64>,
    },
    #[cfg(feature = "command-kucoinspot")]
    AmendOrderWithSymbol {
        command: u32,
        id: String,
        symbol: String,
        price: Option<f64>,
        amount: Option<f64>,
    },
    CancelSymbolOrders {
        command: u32,
        symbol: String,
    },
    SetLeverage {
        command: u32,
        symbol: String,
        leverage: f64,
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
        symbol: Option<String>,
    },
    #[cfg(feature = "command-asterdex")]
    FetchStpMode {
        command: u32,
    },
    FetchAccount {
        command: u32,
    },
    #[cfg(feature = "command-order-book")]
    FetchOrderBook {
        command: u32,
        symbol: String,
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
        symbol: Option<String>,
    },
    FetchClosedPositions {
        command: u32,
        symbol: Option<String>,
        from: Option<f64>,
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
    FetchFills {
        command: u32,
        symbol: Option<String>,
        order_id: Option<String>,
        from: Option<f64>,
        to: Option<f64>,
        limit: Option<u32>,
    },
    FetchOrderHistory {
        command: u32,
        symbol: Option<String>,
        from: Option<f64>,
        to: Option<f64>,
        limit: Option<u32>,
    },
    FetchIncome {
        command: u32,
        symbol: Option<String>,
        income_type: Option<String>,
        from: Option<f64>,
        to: Option<f64>,
        limit: Option<u32>,
    },
    FindOrderByClientOrderId {
        command: u32,
        symbol: String,
        client_order_id: String,
    },
    FetchPublicTrades {
        command: u32,
        symbol: String,
        limit: Option<u32>,
    },
    FetchCandles {
        command: u32,
        symbol: String,
        interval: Interval,
        limit: Option<u32>,
        from: Option<f64>,
        to: Option<f64>,
    },
}

/// The command tag alone. A variant identifier, so a non-string or unknown
/// tag fails exactly as the derived internally tagged enum did.
#[derive(Clone, Copy, Deserialize)]
#[serde(variant_identifier, rename_all = "camelCase")]
enum Op {
    RefreshMarkets,
    #[cfg(feature = "command-options")]
    FetchOptionChain,
    #[cfg(feature = "command-options")]
    FetchOptionExpiries,
    #[cfg(feature = "command-options")]
    FetchAccountGreeks,
    SetTickerSymbols,
    SubscribeOrderBook,
    UnsubscribeOrderBook,
    SubscribeCandles,
    UnsubscribeCandles,
    PlaceOrders,
    CancelOrders,
    #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
    CancelOrdersWithSymbols,
    AmendOrder,
    #[cfg(feature = "command-kucoinspot")]
    AmendOrderWithSymbol,
    CancelSymbolOrders,
    SetLeverage,
    SetMarginType,
    SetPositionMode,
    #[cfg(feature = "command-asterdex")]
    FetchStpMode,
    #[serde(alias = "getAccount")]
    FetchAccount,
    #[cfg(feature = "command-order-book")]
    FetchOrderBook,
    #[cfg(feature = "command-asterdex")]
    SetStpMode,
    #[cfg(feature = "command-schedule-cancel")]
    ScheduleCancel,
    FetchAccountTier,
    SetAccountTier,
    FetchBalanceInfo,
    RefreshSettings,
    FetchClosedPositions,
    FetchLeverage,
    FetchMaxLeverage,
    FetchFills,
    FetchOrderHistory,
    FetchIncome,
    FindOrderByClientOrderId,
    FetchPublicTrades,
    FetchCandles,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::RefreshMarkets with 1 element")]
struct RefreshMarketsIn {
    command: u32,
}

#[cfg(feature = "command-options")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchOptionChain with 2 elements")]
struct FetchOptionChainIn {
    command: u32,
    query: crate::model::OptionChainQuery,
}

#[cfg(feature = "command-options")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchOptionExpiries with 2 elements")]
struct FetchOptionExpiriesIn {
    command: u32,
    underlying: String,
}

#[cfg(feature = "command-options")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchAccountGreeks with 1 element")]
struct FetchAccountGreeksIn {
    command: u32,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SetTickerSymbols with 1 element")]
struct SetTickerSymbolsIn {
    symbols: Vec<String>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SubscribeOrderBook with 1 element")]
struct SubscribeOrderBookIn {
    symbol: String,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::UnsubscribeOrderBook with 1 element")]
struct UnsubscribeOrderBookIn {
    symbol: String,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SubscribeCandles with 2 elements")]
struct SubscribeCandlesIn {
    symbol: String,
    interval: Interval,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::UnsubscribeCandles with 2 elements")]
struct UnsubscribeCandlesIn {
    symbol: String,
    interval: Interval,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::PlaceOrders with 2 elements")]
struct PlaceOrdersIn {
    command: u32,
    orders: Vec<PlaceOrderOpts>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::CancelOrders with 2 elements")]
struct CancelOrdersIn {
    command: u32,
    ids: Vec<String>,
}

#[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::CancelOrdersWithSymbols with 2 elements")]
struct CancelOrdersWithSymbolsIn {
    command: u32,
    orders: Vec<CancelOrderRef>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::AmendOrder with 4 elements")]
struct AmendOrderIn {
    command: u32,
    id: String,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    amount: Option<f64>,
}

#[cfg(feature = "command-kucoinspot")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::AmendOrderWithSymbol with 5 elements")]
struct AmendOrderWithSymbolIn {
    command: u32,
    id: String,
    symbol: String,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    amount: Option<f64>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::CancelSymbolOrders with 2 elements")]
struct CancelSymbolOrdersIn {
    command: u32,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    expecting = "struct variant CommandIn::SetLeverage with 4 elements"
)]
struct SetLeverageIn {
    command: u32,
    symbol: String,
    leverage: f64,
    #[serde(default, deserialize_with = "crate::model::replayed_enum::optional")]
    side: Option<PositionSide>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SetMarginType with 3 elements")]
struct SetMarginTypeIn {
    command: u32,
    symbol: String,
    isolated: bool,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SetPositionMode with 3 elements")]
struct SetPositionModeIn {
    command: u32,
    hedged: bool,
    #[serde(default)]
    symbol: Option<String>,
}

#[cfg(feature = "command-asterdex")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchStpMode with 1 element")]
struct FetchStpModeIn {
    command: u32,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchAccount with 1 element")]
struct FetchAccountIn {
    command: u32,
}

#[cfg(feature = "command-order-book")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchOrderBook with 3 elements")]
struct FetchOrderBookIn {
    command: u32,
    symbol: String,
    #[serde(default)]
    limit: Option<u32>,
}

#[cfg(feature = "command-asterdex")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SetStpMode with 2 elements")]
struct SetStpModeIn {
    command: u32,
    #[serde(deserialize_with = "crate::model::replayed_enum::required")]
    mode: StpMode,
}

#[cfg(feature = "command-schedule-cancel")]
#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::ScheduleCancel with 2 elements")]
struct ScheduleCancelIn {
    command: u32,
    #[serde(default)]
    time: Option<f64>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchAccountTier with 1 element")]
struct FetchAccountTierIn {
    command: u32,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::SetAccountTier with 2 elements")]
struct SetAccountTierIn {
    command: u32,
    tier: String,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchBalanceInfo with 1 element")]
struct FetchBalanceInfoIn {
    command: u32,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::RefreshSettings with 2 elements")]
struct RefreshSettingsIn {
    command: u32,
    #[serde(default)]
    symbol: Option<String>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchClosedPositions with 4 elements")]
struct FetchClosedPositionsIn {
    command: u32,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    from: Option<f64>,
    #[serde(default)]
    to: Option<f64>,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchLeverage with 2 elements")]
struct FetchLeverageIn {
    command: u32,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchMaxLeverage with 2 elements")]
struct FetchMaxLeverageIn {
    command: u32,
    symbol: String,
}

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    expecting = "struct variant CommandIn::FetchFills with 6 elements"
)]
struct FetchFillsIn {
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
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchOrderHistory with 5 elements")]
struct FetchOrderHistoryIn {
    command: u32,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    from: Option<f64>,
    #[serde(default)]
    to: Option<f64>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    expecting = "struct variant CommandIn::FetchIncome with 6 elements"
)]
struct FetchIncomeIn {
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
}

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    expecting = "struct variant CommandIn::FindOrderByClientOrderId with 3 elements"
)]
struct FindOrderByClientOrderIdIn {
    command: u32,
    symbol: String,
    client_order_id: String,
}

#[derive(Deserialize)]
#[serde(expecting = "struct variant CommandIn::FetchPublicTrades with 3 elements")]
struct FetchPublicTradesIn {
    command: u32,
    symbol: String,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(
    rename_all = "camelCase",
    expecting = "struct variant CommandIn::FetchCandles with 6 elements"
)]
struct FetchCandlesIn {
    command: u32,
    symbol: String,
    interval: Interval,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    from: Option<f64>,
    #[serde(default)]
    to: Option<f64>,
}

/// Pass 2: only the chosen variant's payload, straight from the text.
fn decode_payload(op: Op, json: &str) -> Result<CommandIn, serde_json::Error> {
    Ok(match op {
        Op::RefreshMarkets => {
            let RefreshMarketsIn { command } = payload(json)?;
            CommandIn::RefreshMarkets { command }
        }
        #[cfg(feature = "command-options")]
        Op::FetchOptionChain => {
            let FetchOptionChainIn { command, query } = payload(json)?;
            CommandIn::FetchOptionChain { command, query }
        }
        #[cfg(feature = "command-options")]
        Op::FetchOptionExpiries => {
            let FetchOptionExpiriesIn {
                command,
                underlying,
            } = payload(json)?;
            CommandIn::FetchOptionExpiries {
                command,
                underlying,
            }
        }
        #[cfg(feature = "command-options")]
        Op::FetchAccountGreeks => {
            let FetchAccountGreeksIn { command } = payload(json)?;
            CommandIn::FetchAccountGreeks { command }
        }
        Op::SetTickerSymbols => {
            let SetTickerSymbolsIn { symbols } = payload(json)?;
            CommandIn::SetTickerSymbols { symbols }
        }
        Op::SubscribeOrderBook => {
            let SubscribeOrderBookIn { symbol } = payload(json)?;
            CommandIn::SubscribeOrderBook { symbol }
        }
        Op::UnsubscribeOrderBook => {
            let UnsubscribeOrderBookIn { symbol } = payload(json)?;
            CommandIn::UnsubscribeOrderBook { symbol }
        }
        Op::SubscribeCandles => {
            let SubscribeCandlesIn { symbol, interval } = payload(json)?;
            CommandIn::SubscribeCandles { symbol, interval }
        }
        Op::UnsubscribeCandles => {
            let UnsubscribeCandlesIn { symbol, interval } = payload(json)?;
            CommandIn::UnsubscribeCandles { symbol, interval }
        }
        Op::PlaceOrders => {
            let PlaceOrdersIn { command, orders } = payload(json)?;
            CommandIn::PlaceOrders { command, orders }
        }
        Op::CancelOrders => {
            let CancelOrdersIn { command, ids } = payload(json)?;
            CommandIn::CancelOrders { command, ids }
        }
        #[cfg(any(feature = "command-asterdex", feature = "command-kucoinspot"))]
        Op::CancelOrdersWithSymbols => {
            let CancelOrdersWithSymbolsIn { command, orders } = payload(json)?;
            CommandIn::CancelOrdersWithSymbols { command, orders }
        }
        Op::AmendOrder => {
            let AmendOrderIn {
                command,
                id,
                price,
                amount,
            } = payload(json)?;
            CommandIn::AmendOrder {
                command,
                id,
                price,
                amount,
            }
        }
        #[cfg(feature = "command-kucoinspot")]
        Op::AmendOrderWithSymbol => {
            let AmendOrderWithSymbolIn {
                command,
                id,
                symbol,
                price,
                amount,
            } = payload(json)?;
            CommandIn::AmendOrderWithSymbol {
                command,
                id,
                symbol,
                price,
                amount,
            }
        }
        Op::CancelSymbolOrders => {
            let CancelSymbolOrdersIn { command, symbol } = payload(json)?;
            CommandIn::CancelSymbolOrders { command, symbol }
        }
        Op::SetLeverage => {
            let SetLeverageIn {
                command,
                symbol,
                leverage,
                side,
            } = payload(json)?;
            CommandIn::SetLeverage {
                command,
                symbol,
                leverage,
                side,
            }
        }
        Op::SetMarginType => {
            let SetMarginTypeIn {
                command,
                symbol,
                isolated,
            } = payload(json)?;
            CommandIn::SetMarginType {
                command,
                symbol,
                isolated,
            }
        }
        Op::SetPositionMode => {
            let SetPositionModeIn {
                command,
                hedged,
                symbol,
            } = payload(json)?;
            CommandIn::SetPositionMode {
                command,
                hedged,
                symbol,
            }
        }
        #[cfg(feature = "command-asterdex")]
        Op::FetchStpMode => {
            let FetchStpModeIn { command } = payload(json)?;
            CommandIn::FetchStpMode { command }
        }
        Op::FetchAccount => {
            let FetchAccountIn { command } = payload(json)?;
            CommandIn::FetchAccount { command }
        }
        #[cfg(feature = "command-order-book")]
        Op::FetchOrderBook => {
            let FetchOrderBookIn {
                command,
                symbol,
                limit,
            } = payload(json)?;
            CommandIn::FetchOrderBook {
                command,
                symbol,
                limit,
            }
        }
        #[cfg(feature = "command-asterdex")]
        Op::SetStpMode => {
            let SetStpModeIn { command, mode } = payload(json)?;
            CommandIn::SetStpMode { command, mode }
        }
        #[cfg(feature = "command-schedule-cancel")]
        Op::ScheduleCancel => {
            let ScheduleCancelIn { command, time } = payload(json)?;
            CommandIn::ScheduleCancel { command, time }
        }
        Op::FetchAccountTier => {
            let FetchAccountTierIn { command } = payload(json)?;
            CommandIn::FetchAccountTier { command }
        }
        Op::SetAccountTier => {
            let SetAccountTierIn { command, tier } = payload(json)?;
            CommandIn::SetAccountTier { command, tier }
        }
        Op::FetchBalanceInfo => {
            let FetchBalanceInfoIn { command } = payload(json)?;
            CommandIn::FetchBalanceInfo { command }
        }
        Op::RefreshSettings => {
            let RefreshSettingsIn { command, symbol } = payload(json)?;
            CommandIn::RefreshSettings { command, symbol }
        }
        Op::FetchClosedPositions => {
            let FetchClosedPositionsIn {
                command,
                symbol,
                from,
                to,
            } = payload(json)?;
            CommandIn::FetchClosedPositions {
                command,
                symbol,
                from,
                to,
            }
        }
        Op::FetchLeverage => {
            let FetchLeverageIn { command, symbol } = payload(json)?;
            CommandIn::FetchLeverage { command, symbol }
        }
        Op::FetchMaxLeverage => {
            let FetchMaxLeverageIn { command, symbol } = payload(json)?;
            CommandIn::FetchMaxLeverage { command, symbol }
        }
        Op::FetchFills => {
            let FetchFillsIn {
                command,
                symbol,
                order_id,
                from,
                to,
                limit,
            } = payload(json)?;
            CommandIn::FetchFills {
                command,
                symbol,
                order_id,
                from,
                to,
                limit,
            }
        }
        Op::FetchOrderHistory => {
            let FetchOrderHistoryIn {
                command,
                symbol,
                from,
                to,
                limit,
            } = payload(json)?;
            CommandIn::FetchOrderHistory {
                command,
                symbol,
                from,
                to,
                limit,
            }
        }
        Op::FetchIncome => {
            let FetchIncomeIn {
                command,
                symbol,
                income_type,
                from,
                to,
                limit,
            } = payload(json)?;
            CommandIn::FetchIncome {
                command,
                symbol,
                income_type,
                from,
                to,
                limit,
            }
        }
        Op::FindOrderByClientOrderId => {
            let FindOrderByClientOrderIdIn {
                command,
                symbol,
                client_order_id,
            } = payload(json)?;
            CommandIn::FindOrderByClientOrderId {
                command,
                symbol,
                client_order_id,
            }
        }
        Op::FetchPublicTrades => {
            let FetchPublicTradesIn {
                command,
                symbol,
                limit,
            } = payload(json)?;
            CommandIn::FetchPublicTrades {
                command,
                symbol,
                limit,
            }
        }
        Op::FetchCandles => {
            let FetchCandlesIn {
                command,
                symbol,
                interval,
                limit,
                from,
                to,
            } = payload(json)?;
            CommandIn::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from,
                to,
            }
        }
    })
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
    decode_command_in(json).map(Command::from)
}

/// Two passes over the same text replace serde's buffered internally tagged
/// enum. Pass 1 reads the tag and validates every other value exactly as the
/// buffer did; pass 2 parses only the chosen variant's payload.
#[inline(never)]
fn decode_command_in(json: &str) -> Result<CommandIn, serde_json::Error> {
    let Tag(op) = serde_json::from_str(json)?;
    decode_payload(op, json).map_err(|error| {
        // The buffered replay raised payload errors without a position.
        if error.classify() != serde_json::error::Category::Data || error.line() == 0 {
            return error;
        }
        let text = error.to_string();
        let message = text.rfind(" at line ").map_or(&text[..], |at| &text[..at]);
        <serde_json::Error as serde::de::Error>::custom(message)
    })
}

/// Pass 1: the tag, from a map key or a sequence's first element. Every other
/// value, at any depth, is parsed in full (not skimmed) so number range and
/// escape errors stay where the buffered decoder raised them.
struct Tag(Op);

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Tag;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("internally tagged enum CommandIn")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Tag, A::Error> {
                let op = seq
                    .next_element::<Op>()?
                    .ok_or_else(|| serde::de::Error::missing_field("op"))?;
                while seq.next_element::<Validated>()?.is_some() {}
                Ok(Tag(op))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Tag, A::Error> {
                let mut op = None;
                while let Some(key) = map.next_key::<std::borrow::Cow<'de, str>>()? {
                    if key == "op" {
                        if op.is_some() {
                            return Err(serde::de::Error::duplicate_field("op"));
                        }
                        op = Some(map.next_value::<Op>()?);
                    } else {
                        map.next_value::<Validated>()?;
                    }
                }
                op.map(Tag)
                    .ok_or_else(|| serde::de::Error::missing_field("op"))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Any JSON value, parsed in full and discarded.
struct Validated;

impl<'de> Deserialize<'de> for Validated {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Validated;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any value")
            }
            fn visit_bool<E>(self, _: bool) -> Result<Validated, E> {
                Ok(Validated)
            }
            fn visit_i64<E>(self, _: i64) -> Result<Validated, E> {
                Ok(Validated)
            }
            fn visit_u64<E>(self, _: u64) -> Result<Validated, E> {
                Ok(Validated)
            }
            fn visit_f64<E>(self, _: f64) -> Result<Validated, E> {
                Ok(Validated)
            }
            fn visit_str<E>(self, _: &str) -> Result<Validated, E> {
                Ok(Validated)
            }
            fn visit_unit<E>(self) -> Result<Validated, E> {
                Ok(Validated)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Validated, A::Error> {
                while seq.next_element::<Validated>()?.is_some() {}
                Ok(Validated)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Validated, A::Error> {
                while map.next_key::<Validated>()?.is_some() {
                    map.next_value::<Validated>()?;
                }
                Ok(Validated)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Pass 2 for one variant: its struct from the map (unknown keys, the tag
/// included, were validated in pass 1), or the sequence after its tag.
fn payload<P: serde::de::DeserializeOwned>(json: &str) -> Result<P, serde_json::Error> {
    struct Visitor<P>(std::marker::PhantomData<P>);
    impl<'de, P: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<P> {
        type Value = P;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("internally tagged enum CommandIn")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<P, A::Error> {
            P::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<P, A::Error> {
            seq.next_element::<serde::de::IgnoredAny>()?;
            let mut counted = Counted {
                seq: &mut seq,
                taken: 0,
            };
            let value = P::deserialize(serde::de::value::SeqAccessDeserializer::new(&mut counted))?;
            let taken = counted.taken;
            let mut extra = 0;
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
                extra += 1;
            }
            if extra > 0 {
                return Err(serde::de::Error::invalid_length(
                    taken + extra,
                    &InSequence(taken),
                ));
            }
            Ok(value)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(json);
    let value = serde::Deserializer::deserialize_any(
        &mut deserializer,
        Visitor::<P>(std::marker::PhantomData),
    )?;
    deserializer.end()?;
    Ok(value)
}

/// Counts the elements a payload consumed from the shared sequence.
struct Counted<'a, A> {
    seq: &'a mut A,
    taken: usize,
}

impl<'de, A: serde::de::SeqAccess<'de>> serde::de::SeqAccess<'de> for &mut Counted<'_, A> {
    type Error = A::Error;
    fn next_element_seed<T: serde::de::DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, A::Error> {
        let next = self.seq.next_element_seed(seed)?;
        self.taken += usize::from(next.is_some());
        Ok(next)
    }
}

/// serde's own wording for a sequence with leftover elements.
struct InSequence(usize);

impl serde::de::Expected for InSequence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 == 1 {
            f.write_str("1 element in sequence")
        } else {
            write!(f, "{} elements in sequence", self.0)
        }
    }
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
