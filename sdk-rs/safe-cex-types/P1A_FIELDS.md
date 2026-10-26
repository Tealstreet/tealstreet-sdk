# P1a field and alias triage

The classifications and finite JSON/capability limits are defined in
[TYPE_PARITY.md](TYPE_PARITY.md). Behavioral guidance remains in the linked TS
CLAUDE.md files. This table covers only new P1a declarations; P0 rows are unchanged.
Nested object types abbreviate `{ … }` and list every child below. Union branch
indices follow source order. PublicExchangeOptions shows the effective Omit/intersection
shape; the compiler also checks the original intersection. Index parameter names,
quote spelling and Array<T>/T[] are cosmetic. Mutable properties remain mutable.

## CoinBalance

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `CoinBalance.base` | `base: string` | `base: string` | `base: string` | `String` | mutable parity |
| `CoinBalance.amount` | `amount: number` | `amount: number` | `amount: number` | `f64` | mutable parity, C1 |
| `CoinBalance.notional` | `notional: number` | `notional: number` | `notional: number` | `f64` | mutable parity, C1 |

## Balance

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Balance.used` | `readonly used: number` | `used: number` | `readonly used: number` | `f64` | B1, C1 |
| `Balance.free` | `readonly free: number` | `free: number` | `readonly free: number` | `f64` | B1, C1 |
| `Balance.total` | `readonly total: number` | `total: number` | `readonly total: number` | `f64` | B1, C1 |
| `Balance.upnl` | `readonly upnl: number` | `upnl: number` | `readonly upnl: number` | `f64` | B1, C1 |
| `Balance.info` | `readonly info?: any` | `info?: any` | `readonly info?: any` | `Option<serde_json::Value>` | B1, B2, B3/C2 |
| `Balance.currencies` | `readonly currencies?: { … }` | `currencies?: { … }` | `readonly currencies?: { … }` | `Option<std::collections::BTreeMap<String, Balance>>` | B1, B2, C3 |
| `Balance.currencies.[key]` | `[key: string]: Balance` | `[key: string]: Balance` | `[key: string]: Balance` | `Balance` | mutable parity |
| `Balance.coins` | `readonly coins?: { … }` | `coins?: { … }` | `readonly coins?: { … }` | `Option<std::collections::BTreeMap<String, CoinBalance>>` | B1, B2, C3 |
| `Balance.coins.[key]` | `[key: string]: CoinBalance` | `[key: string]: CoinBalance` | `[key: string]: CoinBalance` | `CoinBalance` | mutable parity |

## Ticker

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Ticker.id` | `readonly id: string` | `id: string` | `readonly id: string` | `String` | B1 |
| `Ticker.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `Ticker.bid` | `readonly bid: number` | `bid: number` | `readonly bid: number` | `f64` | B1, C1 |
| `Ticker.ask` | `readonly ask: number` | `ask: number` | `readonly ask: number` | `f64` | B1, C1 |
| `Ticker.last` | `readonly last: number` | `last: number` | `readonly last: number` | `f64` | B1, C1 |
| `Ticker.mark` | `readonly mark: number` | `mark: number` | `readonly mark: number` | `f64` | B1, C1 |
| `Ticker.index` | `readonly index: number` | `index: number` | `readonly index: number` | `Option<f64>` | B1, C1; live WOO runtime null retained, static declaration unchanged |
| `Ticker.midPrice` | `readonly midPrice?: number` | `midPrice?: number` | `readonly midPrice?: number` | `Option<f64>` | B1, B2, C1 |
| `Ticker.percentage` | `readonly percentage: number` | `percentage: number` | `readonly percentage: number` | `f64` | B1, C1 |
| `Ticker.openInterest` | `readonly openInterest: number` | `openInterest: number` | `readonly openInterest: number` | `f64` | B1, C1 |
| `Ticker.fundingRate` | `readonly fundingRate: number` | `fundingRate: number` | `readonly fundingRate: number` | `f64` | B1, C1 |
| `Ticker.fundingTime` | `readonly fundingTime: number` | `fundingTime: number` | `readonly fundingTime: number` | `f64` | B1, C1 |
| `Ticker.volume` | `readonly volume: number` | `volume: number` | `readonly volume: number` | `f64` | B1, C1 |
| `Ticker.quoteVolume` | `readonly quoteVolume: number` | `quoteVolume: number` | `readonly quoteVolume: number` | `f64` | B1, C1 |
| `Ticker.info` | `readonly info?: any` | `info?: any` | `readonly info?: any` | `Option<serde_json::Value>` | B1, B2, B3/C2 |

## PublicTrade

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `PublicTrade.id` | `readonly id: string` | `id: string` | `readonly id: string` | `String` | B1 |
| `PublicTrade.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `PublicTrade.price` | `readonly price: number` | `price: number` | `readonly price: number` | `f64` | B1, C1 |
| `PublicTrade.qty` | `readonly qty: number` | `qty: number` | `readonly qty: number` | `f64` | B1, C1 |
| `PublicTrade.quoteQty` | `readonly quoteQty?: number` | `quoteQty?: number` | `readonly quoteQty?: number` | `Option<f64>` | B1, B2, C1 |
| `PublicTrade.timestamp` | `readonly timestamp: number` | `timestamp: number` | `readonly timestamp: number` | `f64` | B1, C1 |
| `PublicTrade.side` | `readonly side: OrderSide` | `side: OrderSide` | `readonly side: OrderSide` | `OrderSide` | B1 |

## Candle

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Candle.timestamp` | `readonly timestamp: number` | `timestamp: number` | `readonly timestamp: number` | `f64` | B1, C1 |
| `Candle.open` | `readonly open: number` | `open: number` | `readonly open: number` | `f64` | B1, C1 |
| `Candle.high` | `readonly high: number` | `high: number` | `readonly high: number` | `f64` | B1, C1 |
| `Candle.low` | `readonly low: number` | `low: number` | `readonly low: number` | `f64` | B1, C1 |
| `Candle.close` | `readonly close: number` | `close: number` | `readonly close: number` | `f64` | B1, C1 |
| `Candle.volume` | `readonly volume: number` | `volume: number` | `readonly volume: number` | `f64` | B1, C1 |
| `Candle.lastSide` | `readonly lastSide?: OrderSide` | `lastSide?: OrderSide` | `readonly lastSide?: OrderSide` | `Option<OrderSide>` | B1, B2 |

## Execution

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `Execution.id` | `readonly id?: string` | `id?: string` | `readonly id?: string` | `Option<String>` | B1, B2 |
| `Execution.timestamp` | `readonly timestamp: number` | `timestamp: number` | `readonly timestamp: number` | `f64` | B1, C1 |
| `Execution.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `Execution.orderId` | `readonly orderId?: string` | `orderId?: string` | `readonly orderId?: string` | `Option<String>` | B1, B2 |
| `Execution.price` | `readonly price: number` | `price: number` | `readonly price: number` | `f64` | B1, C1 |
| `Execution.qty` | `readonly qty: number` | `qty: number` | `readonly qty: number` | `f64` | B1, C1 |
| `Execution.side` | `readonly side: OrderSide` | `side: OrderSide` | `readonly side: OrderSide` | `OrderSide` | B1 |
| `Execution.commission` | `readonly commission?: number` | `commission?: number` | `readonly commission?: number` | `Option<f64>` | B1, B2, C1 |
| `Execution.commissionAsset` | `readonly commissionAsset?: string` | `commissionAsset?: string` | `readonly commissionAsset?: string` | `Option<String>` | B1, B2 |
| `Execution.positionSide` | `readonly positionSide?: PositionSide \| 'both'` | `positionSide?: PositionSide \| 'both'` | `readonly positionSide?: PositionSide \| 'both'` | `Option<ExecutionPositionSide>` | B1, B2 |
| `Execution.realizedPnl` | `readonly realizedPnl?: number` | `realizedPnl?: number` | `readonly realizedPnl?: number` | `Option<f64>` | B1, B2, C1 |
| `Execution.maker` | `readonly maker?: boolean` | `maker?: boolean` | `readonly maker?: boolean` | `Option<bool>` | B1, B2 |
| `Execution.venueReportedRevenue` | `readonly venueReportedRevenue?: number` | `venueReportedRevenue?: number` | `readonly venueReportedRevenue?: number` | `Option<f64>` | B1, B2, C1 |
| `Execution.venueReportedRevenueAsset` | `readonly venueReportedRevenueAsset?: string` | `venueReportedRevenueAsset?: string` | `readonly venueReportedRevenueAsset?: string` | `Option<String>` | B1, B2 |

## OrderBookOrders

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `OrderBookOrders.price` | `price: number` | `price: number` | `price: number` | `f64` | mutable parity, C1 |
| `OrderBookOrders.amount` | `amount: number` | `amount: number` | `amount: number` | `f64` | mutable parity, C1 |
| `OrderBookOrders.total` | `total: number` | `total: number` | `total: number` | `f64` | mutable parity, C1 |

## OrderBook

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `OrderBook.bids` | `bids: OrderBookOrders[]` | `bids: Array<OrderBookOrders>` | `bids: Array<OrderBookOrders>` | `Vec<OrderBookOrders>` | mutable parity |
| `OrderBook.asks` | `asks: OrderBookOrders[]` | `asks: Array<OrderBookOrders>` | `asks: Array<OrderBookOrders>` | `Vec<OrderBookOrders>` | mutable parity |

## PlaceOrderOpts

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `PlaceOrderOpts.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `PlaceOrderOpts.type` | `readonly type: OrderType` | `type: OrderType` | `readonly type: OrderType` | `OrderType` | B1 |
| `PlaceOrderOpts.side` | `readonly side: OrderSide` | `side: OrderSide` | `readonly side: OrderSide` | `OrderSide` | B1 |
| `PlaceOrderOpts.amount` | `readonly amount: number` | `amount: number` | `readonly amount: number` | `f64` | B1, C1 |
| `PlaceOrderOpts.price` | `readonly price?: number` | `price?: number` | `readonly price?: number` | `Option<f64>` | B1, B2, C1 |
| `PlaceOrderOpts.triggerPrice` | `readonly triggerPrice?: number` | `triggerPrice?: number` | `readonly triggerPrice?: number` | `Option<f64>` | B1, B2, C1 |
| `PlaceOrderOpts.stopLoss` | `readonly stopLoss?: number` | `stopLoss?: number` | `readonly stopLoss?: number` | `Option<f64>` | B1, B2, C1 |
| `PlaceOrderOpts.takeProfit` | `readonly takeProfit?: number` | `takeProfit?: number` | `readonly takeProfit?: number` | `Option<f64>` | B1, B2, C1 |
| `PlaceOrderOpts.reduceOnly` | `readonly reduceOnly?: boolean` | `reduceOnly?: boolean` | `readonly reduceOnly?: boolean` | `Option<bool>` | B1, B2 |
| `PlaceOrderOpts.timeInForce` | `readonly timeInForce?: OrderTimeInForce` | `timeInForce?: OrderTimeInForce` | `readonly timeInForce?: OrderTimeInForce` | `Option<OrderTimeInForce>` | B1, B2 |
| `PlaceOrderOpts.triggerType` | `readonly triggerType?: OrderStopTriggerType` | `triggerType?: OrderStopTriggerType` | `readonly triggerType?: OrderStopTriggerType` | `Option<OrderStopTriggerType>` | B1, B2 |
| `PlaceOrderOpts.tpTriggerType` | `readonly tpTriggerType?: OrderStopTriggerType` | `tpTriggerType?: OrderStopTriggerType` | `readonly tpTriggerType?: OrderStopTriggerType` | `Option<OrderStopTriggerType>` | B1, B2 |
| `PlaceOrderOpts.slTriggerType` | `readonly slTriggerType?: OrderStopTriggerType` | `slTriggerType?: OrderStopTriggerType` | `readonly slTriggerType?: OrderStopTriggerType` | `Option<OrderStopTriggerType>` | B1, B2 |
| `PlaceOrderOpts.clientOrderId` | `readonly clientOrderId?: string` | `clientOrderId?: string` | `readonly clientOrderId?: string` | `Option<String>` | B1, B2 |
| `PlaceOrderOpts.maxSlippagePct` | `readonly maxSlippagePct?: number` | `maxSlippagePct?: number` | `readonly maxSlippagePct?: number` | `Option<f64>` | B1, B2, C1 |
| `PlaceOrderOpts.close` | `readonly close?: boolean` | `close?: boolean` | `readonly close?: boolean` | `Option<bool>` | B1, B2 |

## HistoricalTrade

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `HistoricalTrade.id` | `readonly id?: string` | `id?: string` | `readonly id?: string` | `Option<String>` | B1, B2 |
| `HistoricalTrade.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `HistoricalTrade.orderId` | `readonly orderId: string` | `orderId: string` | `readonly orderId: string` | `String` | B1 |
| `HistoricalTrade.price` | `readonly price: number` | `price: number` | `readonly price: number` | `f64` | B1, C1 |
| `HistoricalTrade.qty` | `readonly qty: number` | `qty: number` | `readonly qty: number` | `f64` | B1, C1 |
| `HistoricalTrade.quoteQty` | `readonly quoteQty?: number` | `quoteQty?: number` | `readonly quoteQty?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTrade.realizedPnl` | `readonly realizedPnl?: number` | `realizedPnl?: number` | `readonly realizedPnl?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTrade.positionSide` | `readonly positionSide: PositionSide \| 'both'` | `positionSide: PositionSide \| 'both'` | `readonly positionSide: PositionSide \| 'both'` | `ExecutionPositionSide` | B1 |
| `HistoricalTrade.side` | `readonly side: OrderSide` | `side: OrderSide` | `readonly side: OrderSide` | `OrderSide` | B1 |
| `HistoricalTrade.buyer` | `readonly buyer?: boolean` | `buyer?: boolean` | `readonly buyer?: boolean` | `Option<bool>` | B1, B2 |
| `HistoricalTrade.maker` | `readonly maker?: boolean` | `maker?: boolean` | `readonly maker?: boolean` | `Option<bool>` | B1, B2 |
| `HistoricalTrade.commission` | `readonly commission?: number` | `commission?: number` | `readonly commission?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTrade.commissionAsset` | `readonly commissionAsset?: string` | `commissionAsset?: string` | `readonly commissionAsset?: string` | `Option<String>` | B1, B2 |
| `HistoricalTrade.time` | `readonly time: number` | `time: number` | `readonly time: number` | `f64` | B1, C1 |

## HistoricalTradeOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `HistoricalTradeOptions.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `HistoricalTradeOptions.orderId` | `readonly orderId?: string` | `orderId?: string` | `readonly orderId?: string` | `Option<String>` | B1, B2 |
| `HistoricalTradeOptions.from` | `readonly from?: number` | `from?: number` | `readonly from?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTradeOptions.to` | `readonly to?: number` | `to?: number` | `readonly to?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTradeOptions.limit` | `readonly limit?: number` | `limit?: number` | `readonly limit?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTradeOptions.fromId` | `readonly fromId?: number` | `fromId?: number` | `readonly fromId?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalTradeOptions.accountName` | `readonly accountName?: string` | `accountName?: string` | `readonly accountName?: string` | `Option<String>` | B1, B2 |

## OHLCVOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `OHLCVOptions.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `OHLCVOptions.interval` | `readonly interval: Timeframe` | `interval: Timeframe` | `readonly interval: Timeframe` | `Timeframe` | B1 |
| `OHLCVOptions.from` | `readonly from?: number` | `from?: number` | `readonly from?: number` | `Option<f64>` | B1, B2, C1 |
| `OHLCVOptions.to` | `readonly to?: number` | `to?: number` | `readonly to?: number` | `Option<f64>` | B1, B2, C1 |
| `OHLCVOptions.limit` | `readonly limit?: number` | `limit?: number` | `readonly limit?: number` | `Option<f64>` | B1, B2, C1 |

## OHLCVSOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `OHLCVSOptions.symbols` | `readonly symbols: string[]` | `symbols: Array<string>` | `readonly symbols: Array<string>` | `Vec<String>` | B1 |
| `OHLCVSOptions.interval` | `readonly interval: Timeframe` | `interval: Timeframe` | `readonly interval: Timeframe` | `Timeframe` | B1 |
| `OHLCVSOptions.from` | `readonly from?: number` | `from?: number` | `readonly from?: number` | `Option<f64>` | B1, B2, C1 |
| `OHLCVSOptions.to` | `readonly to?: number` | `to?: number` | `readonly to?: number` | `Option<f64>` | B1, B2, C1 |
| `OHLCVSOptions.limit` | `readonly limit?: number` | `limit?: number` | `readonly limit?: number` | `Option<f64>` | B1, B2, C1 |

## ExecutionsOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `ExecutionsOptions.symbol` | `readonly symbol?: string` | `symbol?: string` | `readonly symbol?: string` | `Option<String>` | B1, B2 |
| `ExecutionsOptions.orderId` | `readonly orderId?: string` | `orderId?: string` | `readonly orderId?: string` | `Option<String>` | B1, B2 |
| `ExecutionsOptions.from` | `readonly from?: number` | `from?: number` | `readonly from?: number` | `Option<f64>` | B1, B2, C1 |
| `ExecutionsOptions.to` | `readonly to?: number` | `to?: number` | `readonly to?: number` | `Option<f64>` | B1, B2, C1 |
| `ExecutionsOptions.limit` | `readonly limit?: number` | `limit?: number` | `readonly limit?: number` | `Option<f64>` | B1, B2, C1 |

## OrderHistoryOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `OrderHistoryOptions.symbol` | `readonly symbol?: string` | `symbol?: string` | `readonly symbol?: string` | `Option<String>` | B1, B2 |
| `OrderHistoryOptions.orderId` | `readonly orderId?: string` | `orderId?: string` | `readonly orderId?: string` | `Option<String>` | B1, B2 |
| `OrderHistoryOptions.from` | `readonly from?: number` | `from?: number` | `readonly from?: number` | `Option<f64>` | B1, B2, C1 |
| `OrderHistoryOptions.to` | `readonly to?: number` | `to?: number` | `readonly to?: number` | `Option<f64>` | B1, B2, C1 |
| `OrderHistoryOptions.limit` | `readonly limit?: number` | `limit?: number` | `readonly limit?: number` | `Option<f64>` | B1, B2, C1 |
| `OrderHistoryOptions.status` | `readonly status?: OrderStatus[]` | `status?: Array<OrderStatus>` | `readonly status?: Array<OrderStatus>` | `Option<Vec<OrderStatus>>` | B1, B2 |

## HistoricalOrder

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `HistoricalOrder.id` | `readonly id: string` | `id: string` | `readonly id: string` | `String` | B1 |
| `HistoricalOrder.symbol` | `readonly symbol: string` | `symbol: string` | `readonly symbol: string` | `String` | B1 |
| `HistoricalOrder.side` | `readonly side: OrderSide` | `side: OrderSide` | `readonly side: OrderSide` | `OrderSide` | B1 |
| `HistoricalOrder.type` | `readonly type: OrderType` | `type: OrderType` | `readonly type: OrderType` | `OrderType` | B1 |
| `HistoricalOrder.status` | `readonly status: OrderStatus` | `status: OrderStatus` | `readonly status: OrderStatus` | `OrderStatus` | B1 |
| `HistoricalOrder.price` | `readonly price?: number` | `price?: number` | `readonly price?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalOrder.averagePrice` | `readonly averagePrice?: number` | `averagePrice?: number` | `readonly averagePrice?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalOrder.amount` | `readonly amount: number` | `amount: number` | `readonly amount: number` | `f64` | B1, C1 |
| `HistoricalOrder.filled` | `readonly filled: number` | `filled: number` | `readonly filled: number` | `f64` | B1, C1 |
| `HistoricalOrder.reduceOnly` | `readonly reduceOnly?: boolean` | `reduceOnly?: boolean` | `readonly reduceOnly?: boolean` | `Option<bool>` | B1, B2 |
| `HistoricalOrder.positionSide` | `readonly positionSide?: PositionSide \| 'both'` | `positionSide?: PositionSide \| 'both'` | `readonly positionSide?: PositionSide \| 'both'` | `Option<ExecutionPositionSide>` | B1, B2 |
| `HistoricalOrder.createdTime` | `readonly createdTime: number` | `createdTime: number` | `readonly createdTime: number` | `f64` | B1, C1 |
| `HistoricalOrder.updatedTime` | `readonly updatedTime?: number` | `updatedTime?: number` | `readonly updatedTime?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalOrder.realizedPnl` | `readonly realizedPnl?: number` | `realizedPnl?: number` | `readonly realizedPnl?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalOrder.fees` | `readonly fees?: number` | `fees?: number` | `readonly fees?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalOrder.feeAsset` | `readonly feeAsset?: string` | `feeAsset?: string` | `readonly feeAsset?: string` | `Option<String>` | B1, B2 |
| `HistoricalOrder.rebate` | `readonly rebate?: number` | `rebate?: number` | `readonly rebate?: number` | `Option<f64>` | B1, B2, C1 |
| `HistoricalOrder.rebateAsset` | `readonly rebateAsset?: string` | `rebateAsset?: string` | `readonly rebateAsset?: string` | `Option<String>` | B1, B2 |

## StoreDataLoaded

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `StoreDataLoaded.balance` | `readonly balance: boolean` | `balance: boolean` | `readonly balance: boolean` | `bool` | B1 |
| `StoreDataLoaded.orders` | `readonly orders: boolean` | `orders: boolean` | `readonly orders: boolean` | `bool` | B1 |
| `StoreDataLoaded.markets` | `readonly markets: boolean` | `markets: boolean` | `readonly markets: boolean` | `bool` | B1 |
| `StoreDataLoaded.tickers` | `readonly tickers: boolean` | `tickers: boolean` | `readonly tickers: boolean` | `bool` | B1 |
| `StoreDataLoaded.positions` | `readonly positions: boolean` | `positions: boolean` | `readonly positions: boolean` | `bool` | B1 |

## StoreOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `StoreOptions.isHedged` | `readonly isHedged: boolean` | `isHedged: boolean` | `readonly isHedged: boolean` | `bool` | B1 |

## FetchedLeverage

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `FetchedLeverage.buyLeverage` | `buyLeverage: number` | `buyLeverage: number` | `buyLeverage: number` | `f64` | mutable parity, C1 |
| `FetchedLeverage.sellLeverage` | `sellLeverage: number` | `sellLeverage: number` | `sellLeverage: number` | `f64` | mutable parity, C1 |
| `FetchedLeverage.marginType` | `marginType?: MarginType` | `marginType?: MarginType` | `marginType?: MarginType` | `Option<MarginType>` | mutable parity, B2 |
| `FetchedLeverage.positionMode` | `positionMode?: PositionMode` | `positionMode?: PositionMode` | `positionMode?: PositionMode` | `Option<PositionMode>` | mutable parity, B2 |
| `FetchedLeverage.isFetching` | `isFetching?: boolean` | `isFetching?: boolean` | `isFetching?: boolean` | `Option<bool>` | mutable parity, B2 |

## DisplayedMargin

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `DisplayedMargin.longExposure` | `readonly longExposure: number` | `longExposure: number` | `readonly longExposure: number` | `f64` | B1, C1 |
| `DisplayedMargin.shortExposure` | `readonly shortExposure: number` | `shortExposure: number` | `readonly shortExposure: number` | `f64` | B1, C1 |
| `DisplayedMargin.totalNotional` | `readonly totalNotional: number` | `totalNotional: number` | `readonly totalNotional: number` | `f64` | B1, C1 |

## IncomeHistoryOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `IncomeHistoryOptions.symbol` | `symbol?: string` | `symbol?: string` | `symbol?: string` | `Option<String>` | mutable parity, B2 |
| `IncomeHistoryOptions.incomeType` | `incomeType?: string` | `incomeType?: string` | `incomeType?: string` | `Option<String>` | mutable parity, B2 |
| `IncomeHistoryOptions.startTime` | `startTime?: number` | `startTime?: number` | `startTime?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `IncomeHistoryOptions.endTime` | `endTime?: number` | `endTime?: number` | `endTime?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `IncomeHistoryOptions.limit` | `limit?: number` | `limit?: number` | `limit?: number` | `Option<f64>` | mutable parity, B2, C1 |

## ExchangeName

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'apexomni' \| 'asterdex' \| 'backpack' \| 'binance' \| 'binancespot' \| 'binancepm' \| 'bingx' \| 'bitget' \| 'bitgetinverse' \| 'bitgetusdc' \| 'bitrue' \| 'bitunix' \| 'blofin' \| 'breakoutprop' \| 'bybit' \| 'bybitdemo' \| 'bybitinverse' \| 'bybitspot' \| 'hyrotrader' \| 'coinbase' \| 'coinbasespot' \| 'cryptocom' \| 'grvt' \| 'hyperliquid' \| 'hyperliquidspot' \| 'lighter' \| 'lighter-robinhood' \| 'mexc' \| 'okx' \| 'okxdemo' \| 'okxeu' \| 'okxeudemo' \| 'paradex' \| 'phemex' \| 'polymarket' \| 'kodiakfi' \| 'mode' \| 'salsadex' \| 'tealstreet-dex' \| 'what' \| 'weex' \| 'woo' \| 'woofi' \| 'kucoin' \| 'kucoinspot' \| 'kraken' \| 'krakenspot'` | `"apexomni" \| "asterdex" \| "backpack" \| "binance" \| "binancespot" \| "binancepm" \| "bingx" \| "bitget" \| "bitgetinverse" \| "bitgetusdc" \| "bitrue" \| "bitunix" \| "blofin" \| "breakoutprop" \| "bybit" \| "bybitdemo" \| "bybitinverse" \| "bybitspot" \| "hyrotrader" \| "coinbase" \| "coinbasespot" \| "cryptocom" \| "grvt" \| "hyperliquid" \| "hyperliquidspot" \| "lighter" \| "lighter-robinhood" \| "mexc" \| "okx" \| "okxdemo" \| "okxeu" \| "okxeudemo" \| "paradex" \| "phemex" \| "polymarket" \| "kodiakfi" \| "mode" \| "salsadex" \| "tealstreet-dex" \| "what" \| "weex" \| "woo" \| "woofi" \| "kucoin" \| "kucoinspot" \| "kraken" \| "krakenspot"` | `"apexomni" \| "asterdex" \| "backpack" \| "binance" \| "binancespot" \| "binancepm" \| "bingx" \| "bitget" \| "bitgetinverse" \| "bitgetusdc" \| "bitrue" \| "bitunix" \| "blofin" \| "breakoutprop" \| "bybit" \| "bybitdemo" \| "bybitinverse" \| "bybitspot" \| "hyrotrader" \| "coinbase" \| "coinbasespot" \| "cryptocom" \| "grvt" \| "hyperliquid" \| "hyperliquidspot" \| "lighter" \| "lighter-robinhood" \| "mexc" \| "okx" \| "okxdemo" \| "okxeu" \| "okxeudemo" \| "paradex" \| "phemex" \| "polymarket" \| "kodiakfi" \| "mode" \| "salsadex" \| "tealstreet-dex" \| "what" \| "weex" \| "woo" \| "woofi" \| "kucoin" \| "kucoinspot" \| "kraken" \| "krakenspot"` | `ExchangeName` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## ExchangeAccountConfig

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `ExchangeAccountConfig.exchange` | `exchange: ExchangeName` | `exchange: ExchangeName` | `exchange: ExchangeName` | `ExchangeName` | mutable parity |
| `ExchangeAccountConfig.name` | `name: string` | `name: string` | `name: string` | `String` | mutable parity |
| `ExchangeAccountConfig.testnet` | `testnet?: boolean` | `testnet?: boolean` | `testnet?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.apiKey` | `apiKey: string` | `apiKey: string` | `apiKey: string` | `String` | mutable parity |
| `ExchangeAccountConfig.apiSecret` | `apiSecret: string` | `apiSecret: string` | `apiSecret: string` | `String` | mutable parity |
| `ExchangeAccountConfig.passphrase` | `passphrase?: string` | `passphrase?: string` | `passphrase?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.secondary_apiKey` | `secondary_apiKey?: string` | `secondary_apiKey?: string` | `secondary_apiKey?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.secondary_apiSecret` | `secondary_apiSecret?: string` | `secondary_apiSecret?: string` | `secondary_apiSecret?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.secondary_passphrase` | `secondary_passphrase?: string` | `secondary_passphrase?: string` | `secondary_passphrase?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.enableSpotMarkets` | `enableSpotMarkets?: boolean` | `enableSpotMarkets?: boolean` | `enableSpotMarkets?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.bybitSiteId` | `bybitSiteId?: 'BRA_BTL'` | `bybitSiteId?: "BRA_BTL"` | `bybitSiteId?: "BRA_BTL"` | `Option<ExchangeAccountConfigBybitSiteId>` | mutable parity, B2 |
| `ExchangeAccountConfig.applicationId` | `applicationId?: string` | `applicationId?: string` | `applicationId?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.apiKeyIndex` | `apiKeyIndex?: string` | `apiKeyIndex?: string` | `apiKeyIndex?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.vault` | `vault?: string` | `vault?: string` | `vault?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.omniKeySeed` | `omniKeySeed?: string` | `omniKeySeed?: string` | `omniKeySeed?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.subaccount` | `subaccount?: string` | `subaccount?: string` | `subaccount?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.orderlyExchangeId` | `orderlyExchangeId?: string` | `orderlyExchangeId?: string` | `orderlyExchangeId?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.paradexAddress` | `paradexAddress?: string` | `paradexAddress?: string` | `paradexAddress?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.subkeyAddress` | `subkeyAddress?: string` | `subkeyAddress?: string` | `subkeyAddress?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.grvtSignerAddress` | `grvtSignerAddress?: string` | `grvtSignerAddress?: string` | `grvtSignerAddress?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.grvtTradingAccountId` | `grvtTradingAccountId?: string` | `grvtTradingAccountId?: string` | `grvtTradingAccountId?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.tealstreetProxy` | `tealstreetProxy?: boolean` | `tealstreetProxy?: boolean` | `tealstreetProxy?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.tealstreetTunnel` | `tealstreetTunnel?: boolean` | `tealstreetTunnel?: boolean` | `tealstreetTunnel?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.tealstreetWsRoute` | `tealstreetWsRoute?: boolean` | `tealstreetWsRoute?: boolean` | `tealstreetWsRoute?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.proxyRegion` | `proxyRegion?: string` | `proxyRegion?: string` | `proxyRegion?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.useExtensionDirect` | `useExtensionDirect?: boolean` | `useExtensionDirect?: boolean` | `useExtensionDirect?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.user` | `user?: string` | `user?: string` | `user?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.color` | `color?: string` | `color?: string` | `color?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.active` | `active?: boolean` | `active?: boolean` | `active?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.pinned` | `pinned?: boolean` | `pinned?: boolean` | `pinned?: boolean` | `Option<bool>` | mutable parity, B2 |
| `ExchangeAccountConfig.telegramBotToken` | `telegramBotToken?: string` | `telegramBotToken?: string` | `telegramBotToken?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.lastSymbol` | `lastSymbol?: string` | `lastSymbol?: string` | `lastSymbol?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeAccountConfig.healthStatus` | `healthStatus?: 'ok' \| 'auth_error' \| 'auth_expired' \| 'permissions_error' \| 'startup_failed'` | `healthStatus?: "ok" \| "auth_error" \| "auth_expired" \| "permissions_error" \| "startup_failed"` | `healthStatus?: "ok" \| "auth_error" \| "auth_expired" \| "permissions_error" \| "startup_failed"` | `Option<ExchangeAccountConfigHealthStatus>` | mutable parity, B2 |
| `ExchangeAccountConfig.updatedAt` | `updatedAt?: number` | `updatedAt?: number` | `updatedAt?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `ExchangeAccountConfig.deletedAt` | `deletedAt?: number` | `deletedAt?: number` | `deletedAt?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `ExchangeAccountConfig.syncId` | `syncId?: string` | `syncId?: string` | `syncId?: string` | `Option<String>` | mutable parity, B2 |

## ExchangeOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `ExchangeOptions.key` | `readonly key: string` | `key: string` | `readonly key: string` | `String` | B1 |
| `ExchangeOptions.secret` | `readonly secret: string` | `secret: string` | `readonly secret: string` | `String` | B1 |
| `ExchangeOptions.passphrase` | `readonly passphrase?: string` | `passphrase?: string` | `readonly passphrase?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.secondary_key` | `readonly secondary_key?: string` | `secondary_key?: string` | `readonly secondary_key?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.secondary_secret` | `readonly secondary_secret?: string` | `secondary_secret?: string` | `readonly secondary_secret?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.secondary_passphrase` | `readonly secondary_passphrase?: string` | `secondary_passphrase?: string` | `readonly secondary_passphrase?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.secondaryProxyUrl` | `readonly secondaryProxyUrl?: string` | `secondaryProxyUrl?: string` | `readonly secondaryProxyUrl?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.auth` | `readonly auth?: { … }` | `auth?: { … }` | `readonly auth?: { … }` | `Option<ExchangeOptionsAuth>` | B1, B2, C3 |
| `ExchangeOptions.auth.userAddress` | `userAddress?: string` | `userAddress?: string` | `userAddress?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.auth.signerAddress` | `signerAddress?: string` | `signerAddress?: string` | `signerAddress?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.auth.privateKey` | `privateKey?: string` | `privateKey?: string` | `privateKey?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.paradexAddress` | `readonly paradexAddress?: string` | `paradexAddress?: string` | `readonly paradexAddress?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.subkeyAddress` | `readonly subkeyAddress?: string` | `subkeyAddress?: string` | `readonly subkeyAddress?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.grvtSignerAddress` | `readonly grvtSignerAddress?: string` | `grvtSignerAddress?: string` | `readonly grvtSignerAddress?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.grvtTradingAccountId` | `readonly grvtTradingAccountId?: string` | `grvtTradingAccountId?: string` | `readonly grvtTradingAccountId?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.starkPrivateKey` | `readonly starkPrivateKey?: string` | `starkPrivateKey?: string` | `readonly starkPrivateKey?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.vault` | `readonly vault?: string` | `vault?: string` | `readonly vault?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.applicationId` | `readonly applicationId?: string` | `applicationId?: string` | `readonly applicationId?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.apiKeyIndex` | `readonly apiKeyIndex?: string` | `apiKeyIndex?: string` | `readonly apiKeyIndex?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.hyperliquidBuilder` | `readonly hyperliquidBuilder?: { … }` | `hyperliquidBuilder?: { … }` | `readonly hyperliquidBuilder?: { … }` | `Option<ExchangeOptionsHyperliquidBuilder>` | B1, B2, C3 |
| `ExchangeOptions.hyperliquidBuilder.address` | `readonly address: string` | `address: string` | `readonly address: string` | `String` | B1 |
| `ExchangeOptions.hyperliquidBuilder.feeTenthsBps` | `readonly feeTenthsBps: number` | `feeTenthsBps: number` | `readonly feeTenthsBps: number` | `f64` | B1, C1 |
| `ExchangeOptions.testnet` | `readonly testnet?: boolean` | `testnet?: boolean` | `readonly testnet?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.enableSpotMarkets` | `readonly enableSpotMarkets?: boolean` | `enableSpotMarkets?: boolean` | `readonly enableSpotMarkets?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.bybitSiteId` | `readonly bybitSiteId?: 'BRA_BTL'` | `bybitSiteId?: "BRA_BTL"` | `readonly bybitSiteId?: "BRA_BTL"` | `Option<ExchangeOptionsBybitSiteId>` | B1, B2 |
| `ExchangeOptions.publicDataOnly` | `readonly publicDataOnly?: boolean` | `publicDataOnly?: boolean` | `readonly publicDataOnly?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.privateOnly` | `readonly privateOnly?: boolean` | `privateOnly?: boolean` | `readonly privateOnly?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.tealstreetProxy` | `readonly tealstreetProxy?: boolean` | `tealstreetProxy?: boolean` | `readonly tealstreetProxy?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.tealstreetTunnel` | `readonly tealstreetTunnel?: boolean` | `tealstreetTunnel?: boolean` | `readonly tealstreetTunnel?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.tealstreetWsRoute` | `readonly tealstreetWsRoute?: boolean` | `tealstreetWsRoute?: boolean` | `readonly tealstreetWsRoute?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.overrideProxyUrl` | `readonly overrideProxyUrl?: string` | `overrideProxyUrl?: string` | `readonly overrideProxyUrl?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.proxyRegion` | `readonly proxyRegion?: string` | `proxyRegion?: string` | `readonly proxyRegion?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.omniKeySeed` | `readonly omniKeySeed?: string` | `omniKeySeed?: string` | `readonly omniKeySeed?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.subaccount` | `readonly subaccount?: string` | `subaccount?: string` | `readonly subaccount?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.orderlyExchangeId` | `readonly orderlyExchangeId?: string` | `orderlyExchangeId?: string` | `readonly orderlyExchangeId?: string` | `Option<String>` | B1, B2 |
| `ExchangeOptions.tuning` | `readonly tuning?: { … }` | `tuning?: { … }` | `readonly tuning?: { … }` | `Option<ExchangeOptionsTuning>` | B1, B2, C3 |
| `ExchangeOptions.tuning.activeTickerSymbols` | `readonly activeTickerSymbols?: string[]` | `activeTickerSymbols?: Array<string>` | `readonly activeTickerSymbols?: Array<string>` | `Option<Vec<String>>` | B1, B2 |
| `ExchangeOptions.tuning.calculateUpnlFromRealtimeLastPrice` | `readonly calculateUpnlFromRealtimeLastPrice?: boolean` | `calculateUpnlFromRealtimeLastPrice?: boolean` | `readonly calculateUpnlFromRealtimeLastPrice?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.tuning.dataUpdateThrottleMultiplier` | `readonly dataUpdateThrottleMultiplier?: number` | `dataUpdateThrottleMultiplier?: number` | `readonly dataUpdateThrottleMultiplier?: number` | `Option<f64>` | B1, B2, C1 |
| `ExchangeOptions.tuning.skipRendersWhenWindowIsHidden` | `readonly skipRendersWhenWindowIsHidden?: boolean` | `skipRendersWhenWindowIsHidden?: boolean` | `readonly skipRendersWhenWindowIsHidden?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.tuning.consoleLogging` | `readonly consoleLogging?: boolean` | `consoleLogging?: boolean` | `readonly consoleLogging?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.extra` | `readonly extra?: { … }` | `extra?: { … }` | `readonly extra?: { … }` | `Option<ExchangeOptionsExtra>` | B1, B2, C3 |
| `ExchangeOptions.extra.tickInterval` | `tickInterval?: number` | `tickInterval?: number` | `tickInterval?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `ExchangeOptions.extra.recvWindow` | `recvWindow?: number` | `recvWindow?: number` | `recvWindow?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `ExchangeOptions.extra.asterdexApiVersion` | `asterdexApiVersion?: 'v1' \| 'v3'` | `asterdexApiVersion?: "v1" \| "v3"` | `asterdexApiVersion?: "v1" \| "v3"` | `Option<ExchangeOptionsExtraAsterdexApiVersion>` | mutable parity, B2 |
| `ExchangeOptions.extra.asterBuilderAddress` | `asterBuilderAddress?: string` | `asterBuilderAddress?: string` | `asterBuilderAddress?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.extra.asterBuilderFeeRateMaker` | `asterBuilderFeeRateMaker?: string` | `asterBuilderFeeRateMaker?: string` | `asterBuilderFeeRateMaker?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.extra.asterBuilderFeeRateTaker` | `asterBuilderFeeRateTaker?: string` | `asterBuilderFeeRateTaker?: string` | `asterBuilderFeeRateTaker?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.extra.lighterBuilderFeeMode` | `lighterBuilderFeeMode?: 'dynamic' \| 'static'` | `lighterBuilderFeeMode?: "dynamic" \| "static"` | `lighterBuilderFeeMode?: "dynamic" \| "static"` | `Option<ExchangeOptionsExtraLighterBuilderFeeMode>` | mutable parity, B2 |
| `ExchangeOptions.extra.lighterIntegratorAccountIndex` | `lighterIntegratorAccountIndex?: string` | `lighterIntegratorAccountIndex?: string` | `lighterIntegratorAccountIndex?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.extra.lighterStopSlippagePct` | `lighterStopSlippagePct?: number` | `lighterStopSlippagePct?: number` | `lighterStopSlippagePct?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `ExchangeOptions.extra.kucoinBroker` | `kucoinBroker?: { … }` | `kucoinBroker?: { … }` | `kucoinBroker?: { … }` | `Option<ExchangeOptionsExtraKucoinBroker>` | mutable parity, B2, C3 |
| `ExchangeOptions.extra.kucoinBroker.partner` | `readonly partner: string` | `partner: string` | `readonly partner: string` | `String` | B1 |
| `ExchangeOptions.extra.kucoinBroker.brokerKey` | `readonly brokerKey: string` | `brokerKey: string` | `readonly brokerKey: string` | `String` | B1 |
| `ExchangeOptions.extra.kucoinBroker.brokerName` | `readonly brokerName: string` | `brokerName: string` | `readonly brokerName: string` | `String` | B1 |
| `ExchangeOptions.extra.kucoinBroker.verify` | `readonly verify?: boolean` | `verify?: boolean` | `readonly verify?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.remoteSigner` | `readonly remoteSigner?: RemoteSigner` | `remoteSigner?: RemoteSigner` | `readonly remoteSigner?: RemoteSigner` | `Option<RemoteSignerCapability>` | B1, B2, C4 |
| `ExchangeOptions.transport` | `readonly transport?: TransportProvider` | `transport?: TransportProvider` | `readonly transport?: TransportProvider` | `Option<TransportCapability>` | B1, B2, C4 |
| `ExchangeOptions.payloadSigner` | `readonly payloadSigner?: PayloadSigner` | `payloadSigner?: PayloadSigner` | `readonly payloadSigner?: PayloadSigner` | `Option<PayloadSignerCapability>` | B1, B2, C4 |
| `ExchangeOptions.hostedMode` | `readonly hostedMode?: boolean` | `hostedMode?: boolean` | `readonly hostedMode?: boolean` | `Option<bool>` | B1, B2 |
| `ExchangeOptions.externalPublicData` | `readonly externalPublicData?: { … } \| { … }` | `externalPublicData?: { … } \| { … }` | `readonly externalPublicData?: { … } \| { … }` | `Option<ExchangeOptionsExternalPublicData>` | B1, B2, C3 |
| `ExchangeOptions.externalPublicData[0].type` | `type: 'websocket'` | `type: "websocket"` | `type: "websocket"` | `ExchangeOptionsExternalPublicDataVariant0Type` | mutable parity |
| `ExchangeOptions.externalPublicData[0].url` | `url: string` | `url: string` | `url: string` | `String` | mutable parity |
| `ExchangeOptions.externalPublicData[0].exchangeId` | `exchangeId: string` | `exchangeId: string` | `exchangeId: string` | `String` | mutable parity |
| `ExchangeOptions.externalPublicData[0].authToken` | `authToken?: string` | `authToken?: string` | `authToken?: string` | `Option<String>` | mutable parity, B2 |
| `ExchangeOptions.externalPublicData[1].type` | `type: 'in-process'` | `type: "in-process"` | `type: "in-process"` | `ExchangeOptionsExternalPublicDataVariant1Type` | mutable parity |
| `ExchangeOptions.externalPublicData[1].source` | `source: PublicDataSource` | `source: PublicDataSource` | `source: PublicDataSource` | `PublicDataSourceCapability` | mutable parity, C4 |

## ValidateAccountResult

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `ValidateAccountResult.error` | `error: string` | `error: string` | `error: string` | `String` | mutable parity |
| `ValidateAccountResult.balance` | `balance?: Balance` | `balance?: Balance` | `balance?: Balance` | `Option<Balance>` | mutable parity, B2 |

## UpdateOrderOpts

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `UpdateOrderOpts.order` | `readonly order: Order` | `order: Order` | `readonly order: Order` | `Order` | B1 |
| `UpdateOrderOpts.update` | `readonly update: { … } \| { … }` | `update: { … } \| { … }` | `readonly update: { … } \| { … }` | `UpdateOrderOptsUpdate` | B1, C3 |
| `UpdateOrderOpts.update[0].amount` | `readonly amount: number` | `amount: number` | `readonly amount: number` | `f64` | B1, C1 |
| `UpdateOrderOpts.update[1].price` | `readonly price: number` | `price: number` | `readonly price: number` | `f64` | B1, C1 |

## ExchangeAccount

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `ExchangeAccount.userId` | `readonly userId: string` | `userId: string` | `readonly userId: string` | `String` | B1 |
| `ExchangeAccount.subId` | `readonly subId?: string \| null` | `subId?: string \| null` | `readonly subId?: string \| null` | `Option<Option<String>>` | B1, B2 |
| `ExchangeAccount.affiliateId` | `readonly affiliateId?: string \| null` | `affiliateId?: string \| null` | `readonly affiliateId?: string \| null` | `Option<Option<String>>` | B1, B2 |

## DataProvenance

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'venue' \| 'derived'` | `"venue" \| "derived"` | `"venue" \| "derived"` | `DataProvenance` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## ClosedPnlEntry

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `ClosedPnlEntry.provenance` | `provenance: DataProvenance` | `provenance: DataProvenance` | `provenance: DataProvenance` | `DataProvenance` | mutable parity |
| `ClosedPnlEntry.symbol` | `symbol: string` | `symbol: string` | `symbol: string` | `String` | mutable parity |
| `ClosedPnlEntry.notional` | `notional: string \| number` | `notional: string \| number` | `notional: string \| number` | `ClosedPnlEntryNotional` | mutable parity |
| `ClosedPnlEntry.entryPrice` | `entryPrice: string \| number` | `entryPrice: string \| number` | `entryPrice: string \| number` | `ClosedPnlEntryEntryPrice` | mutable parity |
| `ClosedPnlEntry.exitPrice` | `exitPrice: string \| number` | `exitPrice: string \| number` | `exitPrice: string \| number` | `ClosedPnlEntryExitPrice` | mutable parity |
| `ClosedPnlEntry.unrealizedPnl` | `unrealizedPnl: string \| number` | `unrealizedPnl: string \| number` | `unrealizedPnl: string \| number` | `ClosedPnlEntryUnrealizedPnl` | mutable parity |
| `ClosedPnlEntry.side` | `side: string` | `side: string` | `side: string` | `String` | mutable parity |
| `ClosedPnlEntry.fees` | `fees: string \| number` | `fees: string \| number` | `fees: string \| number` | `ClosedPnlEntryFees` | mutable parity |
| `ClosedPnlEntry.time` | `time: number` | `time: number` | `time: number` | `f64` | mutable parity, C1 |
| `ClosedPnlEntry.closedTime` | `closedTime: number` | `closedTime: number` | `closedTime: number` | `f64` | mutable parity, C1 |
| `ClosedPnlEntry.account` | `account: string` | `account: string` | `account: string` | `String` | mutable parity |
| `ClosedPnlEntry.contracts` | `contracts: string \| number` | `contracts: string \| number` | `contracts: string \| number` | `ClosedPnlEntryContracts` | mutable parity |
| `ClosedPnlEntry.leverage` | `leverage: number \| string` | `leverage: number \| string` | `leverage: number \| string` | `ClosedPnlEntryLeverage` | mutable parity |

## IncomeCategory

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'realized_pnl' \| 'funding' \| 'commission' \| 'rebate' \| 'transfer' \| 'liquidation' \| 'other'` | `"realized_pnl" \| "funding" \| "commission" \| "rebate" \| "transfer" \| "liquidation" \| "other"` | `"realized_pnl" \| "funding" \| "commission" \| "rebate" \| "transfer" \| "liquidation" \| "other"` | `IncomeCategory` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## IncomeRecord

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `IncomeRecord.symbol` | `symbol: string` | `symbol: string` | `symbol: string` | `String` | mutable parity |
| `IncomeRecord.incomeType` | `incomeType: string` | `incomeType: string` | `incomeType: string` | `String` | mutable parity |
| `IncomeRecord.income` | `income: number` | `income: number` | `income: number` | `f64` | mutable parity, C1 |
| `IncomeRecord.asset` | `asset: string` | `asset: string` | `asset: string` | `String` | mutable parity |
| `IncomeRecord.timestamp` | `timestamp: number` | `timestamp: number` | `timestamp: number` | `f64` | mutable parity, C1 |
| `IncomeRecord.tranId` | `tranId: string` | `tranId: string` | `tranId: string` | `String` | mutable parity |
| `IncomeRecord.tradeId` | `tradeId?: string` | `tradeId?: string` | `tradeId?: string` | `Option<String>` | mutable parity, B2 |
| `IncomeRecord.category` | `category?: IncomeCategory` | `category?: IncomeCategory` | `category?: IncomeCategory` | `Option<IncomeCategory>` | mutable parity, B2 |

## RequestOutcome

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'ok' \| 'rateLimit' \| 'auth' \| 'network' \| 'other'` | `"ok" \| "rateLimit" \| "auth" \| "network" \| "other"` | `"ok" \| "rateLimit" \| "auth" \| "network" \| "other"` | `RequestOutcome` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## RequestOutcomeCounts

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `Record<RequestOutcome, number>` | `Record<RequestOutcome, number>` | `Record<RequestOutcome, number>` | `RequestOutcomeCounts` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## MessageHandlerStatsType

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'public' \| 'private' \| 'futures' \| 'business'` | `"public" \| "private" \| "futures" \| "business"` | `"public" \| "private" \| "futures" \| "business"` | `MessageHandlerStatsType` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## SecondsTimeframe

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'1s' \| '5s' \| '15s' \| '30s'` | `"1s" \| "5s" \| "15s" \| "30s"` | `"1s" \| "5s" \| "15s" \| "30s"` | `SecondsTimeframe` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## ExtendedTimeframe

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `'3h' \| '5h' \| '7h' \| '8h' \| '9h' \| '10h' \| '11h' \| '24h' \| '2d' \| '3d' \| '1M'` | `"3h" \| "5h" \| "7h" \| "8h" \| "9h" \| "10h" \| "11h" \| "24h" \| "2d" \| "3d" \| "1M"` | `"3h" \| "5h" \| "7h" \| "8h" \| "9h" \| "10h" \| "11h" \| "24h" \| "2d" \| "3d" \| "1M"` | `ExtendedTimeframe` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## AnyTimeframe

| Oracle type | Raw type | Final type | Rust storage | Triage |
| --- | --- | --- | --- | --- |
| `SecondsTimeframe \| Timeframe \| ExtendedTimeframe` | `SecondsTimeframe \| Timeframe \| ExtendedTimeframe` | `SecondsTimeframe \| Timeframe \| ExtendedTimeframe` | `AnyTimeframe` | Literal aliases stay aliases; scalar representation C1 where numeric. |

## StoreUpdateIds

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `StoreUpdateIds.positions` | `positions: number` | `positions: number` | `positions: number` | `f64` | mutable parity, C1 |
| `StoreUpdateIds.orders` | `orders: number` | `orders: number` | `orders: number` | `f64` | mutable parity, C1 |

## PublicExchangeOptions

| Field | Oracle | Raw | Final | Rust storage | Triage |
| --- | --- | --- | --- | --- | --- |
| `PublicExchangeOptions.passphrase` | `readonly passphrase?: string` | `passphrase?: string` | `readonly passphrase?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.secondary_key` | `readonly secondary_key?: string` | `secondary_key?: string` | `readonly secondary_key?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.secondary_secret` | `readonly secondary_secret?: string` | `secondary_secret?: string` | `readonly secondary_secret?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.secondary_passphrase` | `readonly secondary_passphrase?: string` | `secondary_passphrase?: string` | `readonly secondary_passphrase?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.secondaryProxyUrl` | `readonly secondaryProxyUrl?: string` | `secondaryProxyUrl?: string` | `readonly secondaryProxyUrl?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.auth` | `readonly auth?: { … }` | `auth?: { … }` | `readonly auth?: { … }` | `Option<ExchangeOptionsAuth>` | B1, B2, C3 |
| `PublicExchangeOptions.auth.userAddress` | `userAddress?: string` | `userAddress?: string` | `userAddress?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.auth.signerAddress` | `signerAddress?: string` | `signerAddress?: string` | `signerAddress?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.auth.privateKey` | `privateKey?: string` | `privateKey?: string` | `privateKey?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.paradexAddress` | `readonly paradexAddress?: string` | `paradexAddress?: string` | `readonly paradexAddress?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.subkeyAddress` | `readonly subkeyAddress?: string` | `subkeyAddress?: string` | `readonly subkeyAddress?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.grvtSignerAddress` | `readonly grvtSignerAddress?: string` | `grvtSignerAddress?: string` | `readonly grvtSignerAddress?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.grvtTradingAccountId` | `readonly grvtTradingAccountId?: string` | `grvtTradingAccountId?: string` | `readonly grvtTradingAccountId?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.starkPrivateKey` | `readonly starkPrivateKey?: string` | `starkPrivateKey?: string` | `readonly starkPrivateKey?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.vault` | `readonly vault?: string` | `vault?: string` | `readonly vault?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.applicationId` | `readonly applicationId?: string` | `applicationId?: string` | `readonly applicationId?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.apiKeyIndex` | `readonly apiKeyIndex?: string` | `apiKeyIndex?: string` | `readonly apiKeyIndex?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.hyperliquidBuilder` | `readonly hyperliquidBuilder?: { … }` | `hyperliquidBuilder?: { … }` | `readonly hyperliquidBuilder?: { … }` | `Option<ExchangeOptionsHyperliquidBuilder>` | B1, B2, C3 |
| `PublicExchangeOptions.hyperliquidBuilder.address` | `readonly address: string` | `address: string` | `readonly address: string` | `String` | B1 |
| `PublicExchangeOptions.hyperliquidBuilder.feeTenthsBps` | `readonly feeTenthsBps: number` | `feeTenthsBps: number` | `readonly feeTenthsBps: number` | `f64` | B1, C1 |
| `PublicExchangeOptions.testnet` | `readonly testnet?: boolean` | `testnet?: boolean` | `readonly testnet?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.enableSpotMarkets` | `readonly enableSpotMarkets?: boolean` | `enableSpotMarkets?: boolean` | `readonly enableSpotMarkets?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.bybitSiteId` | `readonly bybitSiteId?: 'BRA_BTL'` | `bybitSiteId?: "BRA_BTL"` | `readonly bybitSiteId?: "BRA_BTL"` | `Option<ExchangeOptionsBybitSiteId>` | B1, B2 |
| `PublicExchangeOptions.privateOnly` | `readonly privateOnly?: boolean` | `privateOnly?: boolean` | `readonly privateOnly?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.tealstreetProxy` | `readonly tealstreetProxy?: boolean` | `tealstreetProxy?: boolean` | `readonly tealstreetProxy?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.tealstreetTunnel` | `readonly tealstreetTunnel?: boolean` | `tealstreetTunnel?: boolean` | `readonly tealstreetTunnel?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.tealstreetWsRoute` | `readonly tealstreetWsRoute?: boolean` | `tealstreetWsRoute?: boolean` | `readonly tealstreetWsRoute?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.overrideProxyUrl` | `readonly overrideProxyUrl?: string` | `overrideProxyUrl?: string` | `readonly overrideProxyUrl?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.proxyRegion` | `readonly proxyRegion?: string` | `proxyRegion?: string` | `readonly proxyRegion?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.omniKeySeed` | `readonly omniKeySeed?: string` | `omniKeySeed?: string` | `readonly omniKeySeed?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.subaccount` | `readonly subaccount?: string` | `subaccount?: string` | `readonly subaccount?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.orderlyExchangeId` | `readonly orderlyExchangeId?: string` | `orderlyExchangeId?: string` | `readonly orderlyExchangeId?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.tuning` | `readonly tuning?: { … }` | `tuning?: { … }` | `readonly tuning?: { … }` | `Option<ExchangeOptionsTuning>` | B1, B2, C3 |
| `PublicExchangeOptions.tuning.activeTickerSymbols` | `readonly activeTickerSymbols?: string[]` | `activeTickerSymbols?: Array<string>` | `readonly activeTickerSymbols?: Array<string>` | `Option<Vec<String>>` | B1, B2 |
| `PublicExchangeOptions.tuning.calculateUpnlFromRealtimeLastPrice` | `readonly calculateUpnlFromRealtimeLastPrice?: boolean` | `calculateUpnlFromRealtimeLastPrice?: boolean` | `readonly calculateUpnlFromRealtimeLastPrice?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.tuning.dataUpdateThrottleMultiplier` | `readonly dataUpdateThrottleMultiplier?: number` | `dataUpdateThrottleMultiplier?: number` | `readonly dataUpdateThrottleMultiplier?: number` | `Option<f64>` | B1, B2, C1 |
| `PublicExchangeOptions.tuning.skipRendersWhenWindowIsHidden` | `readonly skipRendersWhenWindowIsHidden?: boolean` | `skipRendersWhenWindowIsHidden?: boolean` | `readonly skipRendersWhenWindowIsHidden?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.tuning.consoleLogging` | `readonly consoleLogging?: boolean` | `consoleLogging?: boolean` | `readonly consoleLogging?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.extra` | `readonly extra?: { … }` | `extra?: { … }` | `readonly extra?: { … }` | `Option<ExchangeOptionsExtra>` | B1, B2, C3 |
| `PublicExchangeOptions.extra.tickInterval` | `tickInterval?: number` | `tickInterval?: number` | `tickInterval?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `PublicExchangeOptions.extra.recvWindow` | `recvWindow?: number` | `recvWindow?: number` | `recvWindow?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `PublicExchangeOptions.extra.asterdexApiVersion` | `asterdexApiVersion?: 'v1' \| 'v3'` | `asterdexApiVersion?: "v1" \| "v3"` | `asterdexApiVersion?: "v1" \| "v3"` | `Option<ExchangeOptionsExtraAsterdexApiVersion>` | mutable parity, B2 |
| `PublicExchangeOptions.extra.asterBuilderAddress` | `asterBuilderAddress?: string` | `asterBuilderAddress?: string` | `asterBuilderAddress?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.extra.asterBuilderFeeRateMaker` | `asterBuilderFeeRateMaker?: string` | `asterBuilderFeeRateMaker?: string` | `asterBuilderFeeRateMaker?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.extra.asterBuilderFeeRateTaker` | `asterBuilderFeeRateTaker?: string` | `asterBuilderFeeRateTaker?: string` | `asterBuilderFeeRateTaker?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.extra.lighterBuilderFeeMode` | `lighterBuilderFeeMode?: 'dynamic' \| 'static'` | `lighterBuilderFeeMode?: "dynamic" \| "static"` | `lighterBuilderFeeMode?: "dynamic" \| "static"` | `Option<ExchangeOptionsExtraLighterBuilderFeeMode>` | mutable parity, B2 |
| `PublicExchangeOptions.extra.lighterIntegratorAccountIndex` | `lighterIntegratorAccountIndex?: string` | `lighterIntegratorAccountIndex?: string` | `lighterIntegratorAccountIndex?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.extra.lighterStopSlippagePct` | `lighterStopSlippagePct?: number` | `lighterStopSlippagePct?: number` | `lighterStopSlippagePct?: number` | `Option<f64>` | mutable parity, B2, C1 |
| `PublicExchangeOptions.extra.kucoinBroker` | `kucoinBroker?: { … }` | `kucoinBroker?: { … }` | `kucoinBroker?: { … }` | `Option<ExchangeOptionsExtraKucoinBroker>` | mutable parity, B2, C3 |
| `PublicExchangeOptions.extra.kucoinBroker.partner` | `readonly partner: string` | `partner: string` | `readonly partner: string` | `String` | B1 |
| `PublicExchangeOptions.extra.kucoinBroker.brokerKey` | `readonly brokerKey: string` | `brokerKey: string` | `readonly brokerKey: string` | `String` | B1 |
| `PublicExchangeOptions.extra.kucoinBroker.brokerName` | `readonly brokerName: string` | `brokerName: string` | `readonly brokerName: string` | `String` | B1 |
| `PublicExchangeOptions.extra.kucoinBroker.verify` | `readonly verify?: boolean` | `verify?: boolean` | `readonly verify?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.remoteSigner` | `readonly remoteSigner?: RemoteSigner` | `remoteSigner?: RemoteSigner` | `readonly remoteSigner?: RemoteSigner` | `Option<RemoteSignerCapability>` | B1, B2, C4 |
| `PublicExchangeOptions.transport` | `readonly transport?: TransportProvider` | `transport?: TransportProvider` | `readonly transport?: TransportProvider` | `Option<TransportCapability>` | B1, B2, C4 |
| `PublicExchangeOptions.payloadSigner` | `readonly payloadSigner?: PayloadSigner` | `payloadSigner?: PayloadSigner` | `readonly payloadSigner?: PayloadSigner` | `Option<PayloadSignerCapability>` | B1, B2, C4 |
| `PublicExchangeOptions.hostedMode` | `readonly hostedMode?: boolean` | `hostedMode?: boolean` | `readonly hostedMode?: boolean` | `Option<bool>` | B1, B2 |
| `PublicExchangeOptions.externalPublicData` | `readonly externalPublicData?: { … } \| { … }` | `externalPublicData?: { … } \| { … }` | `readonly externalPublicData?: { … } \| { … }` | `Option<ExchangeOptionsExternalPublicData>` | B1, B2, C3 |
| `PublicExchangeOptions.externalPublicData[0].type` | `type: 'websocket'` | `type: "websocket"` | `type: "websocket"` | `ExchangeOptionsExternalPublicDataVariant0Type` | mutable parity |
| `PublicExchangeOptions.externalPublicData[0].url` | `url: string` | `url: string` | `url: string` | `String` | mutable parity |
| `PublicExchangeOptions.externalPublicData[0].exchangeId` | `exchangeId: string` | `exchangeId: string` | `exchangeId: string` | `String` | mutable parity |
| `PublicExchangeOptions.externalPublicData[0].authToken` | `authToken?: string` | `authToken?: string` | `authToken?: string` | `Option<String>` | mutable parity, B2 |
| `PublicExchangeOptions.externalPublicData[1].type` | `type: 'in-process'` | `type: "in-process"` | `type: "in-process"` | `ExchangeOptionsExternalPublicDataVariant1Type` | mutable parity |
| `PublicExchangeOptions.externalPublicData[1].source` | `source: PublicDataSource` | `source: PublicDataSource` | `source: PublicDataSource` | `PublicDataSourceCapability` | mutable parity, C4 |
| `PublicExchangeOptions.key` | `readonly key?: string` | `key?: string` | `readonly key?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.secret` | `readonly secret?: string` | `secret?: string` | `readonly secret?: string` | `Option<String>` | B1, B2 |
| `PublicExchangeOptions.publicDataOnly` | `readonly publicDataOnly: true` | `publicDataOnly: true` | `readonly publicDataOnly: true` | `True` | B1 |
| `PublicExchangeOptions.forceAllProxiesOff` | `readonly forceAllProxiesOff?: boolean` | `forceAllProxiesOff?: boolean` | `readonly forceAllProxiesOff?: boolean` | `Option<bool>` | B1, B2 |

P1a adds 358 checked property/index declarations to the 72 P0 declarations.
