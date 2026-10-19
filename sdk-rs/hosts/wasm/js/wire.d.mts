/** Reader for the engine's flush frame. See wire.mjs and docs/plans/RUST_REWRITE.md. */
export const SECTION: {
  STRINGS: 1;
  MARKETS: 2;
  TICKERS: 3;
  TICKER_DELTAS: 4;
  BOOK: 5;
  CANDLES: 6;
  ORDERS: 7;
  POSITIONS: 8;
  BALANCE: 9;
  STATUS: 10;
  FILLS: 11;
  SETTINGS: 12;
};
export const NONE: number;
export const TICKER_FIELDS: readonly [
  'bid', 'ask', 'last', 'mark', 'index', 'percentage',
  'openInterest', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume',
];
export type TickerField = (typeof TICKER_FIELDS)[number];
export const MARKET_COLUMNS: readonly [
  'precisionAmount', 'precisionPrice', 'minAmount', 'maxAmount', 'minLeverage',
  'maxLeverage', 'minPrice', 'contractSize', 'takerFee', 'makerFee',
];
export type MarketColumn = (typeof MARKET_COLUMNS)[number];
export const MARKET_FLAG: { ACTIVE: 1; INVERSE: 2; SPOT: 4; PERPETUAL: 8; EXPIRING: 16; READ_ONLY: 32 };
export const LOADED_FLAG: { BALANCE: 1; ORDERS: 2; MARKETS: 4; TICKERS: 8; POSITIONS: 16 };
export const STATUS_FLAG: { HEDGED: 1 };
export const ORDER_COLUMNS: readonly ['price', 'amount', 'filled', 'remaining', 'lastUpdate', 'takeProfit', 'stopLoss'];
export type OrderColumn = (typeof ORDER_COLUMNS)[number];
export const POSITION_COLUMNS: readonly [
  'entryPrice', 'notional', 'leverage', 'unrealizedPnl', 'realizedPnl',
  'contracts', 'liquidationPrice', 'marginSize', 'lastUpdate',
];
export type PositionColumn = (typeof POSITION_COLUMNS)[number];

export interface OrderEnums {
  status: 'open' | 'closed' | 'canceled' | 'creating' | 'updating' | 'canceling';
  type: 'market' | 'limit' | 'conditional' | 'stop_market' | 'take_profit_market' | 'trailing_stop_market';
  side: 'buy' | 'sell';
  reduceOnly: boolean;
  close: boolean | undefined;
  rejectReason: 'PostOnly' | 'InsufficientBalance' | 'ReduceOnly' | undefined;
  timeInForce: 'GoodTillCancel' | 'ImmediateOrCancel' | 'FillOrKill' | 'PostOnly' | undefined;
  triggerType: 'mark_price' | 'last_price' | 'index_price' | undefined;
}
export function orderEnums(word: number): OrderEnums;

export interface PositionEnums {
  side: 'long' | 'short';
  marginType: 'isolated' | 'cross' | undefined;
  positionMode: 'hedged' | 'oneway' | undefined;
}
export function positionEnums(word: number): PositionEnums;

export type Columns<K extends string> = { [column in K]: Float64Array };

export type MarketTable = { count: number; flags: Uint32Array } & Columns<MarketColumn> & {
  id(i: number): string;
  symbol(i: number): string;
  base(i: number): string;
  quote(i: number): string;
};
export type TickerTable = { count: number; market: Uint32Array } & Columns<TickerField>;
export interface BookRows {
  market: number;
  /** `[price, amount, total]` per level, best first. */
  bids: Float64Array;
  asks: Float64Array;
}
export interface CandleRows {
  count: number;
  market: Uint32Array;
  interval(i: number): string;
  /** `[time, open, high, low, close, volume]` per candle. */
  rows: Float64Array;
}
export type OrderTable = { count: number; market: Uint32Array; enums: Uint32Array } & Columns<OrderColumn> & {
  id(i: number): string;
  clientOrderId(i: number): string | undefined;
  parentId(i: number): string | undefined;
};
export type PositionTable = { count: number; market: Uint32Array; enums: Uint32Array } & Columns<PositionColumn>;
export interface Balance {
  used: number;
  free: number;
  total: number;
  upnl: number;
}
export interface Status {
  loaded: number;
  hedged: boolean;
  latency: number;
}
export interface SettingTable {
  count: number;
  market: Uint32Array;
  leverage: Float64Array;
  buyLeverage: Float64Array;
  sellLeverage: Float64Array;
  marginType(i: number): 'isolated' | 'cross' | undefined;
}
export interface FillRows {
  count: number;
  market: Uint32Array;
  side: Uint32Array;
  /** `[price, amount]` per execution. */
  rows: Float64Array;
  orderId(i: number): string;
}

export class Frame {
  constructor(buffer: ArrayBuffer);
  readonly buffer: ArrayBuffer;
  /** Engine time of the flush, ms. */
  readonly time: number;
  string(index: number): string | undefined;
  markets(): MarketTable | undefined;
  tickers(): TickerTable | undefined;
  tickerDeltas(): Float64Array | undefined;
  books(): Generator<BookRows>;
  candles(): CandleRows | undefined;
  orders(): OrderTable | undefined;
  positions(): PositionTable | undefined;
  balance(): Balance | undefined;
  status(): Status | undefined;
  settings(): SettingTable | undefined;
  fills(): FillRows | undefined;
}

/** Live ticker columns: adopts each replacement table and applies deltas in place. */
export class TickerState {
  count: number;
  market: Uint32Array;
  columns: Partial<Columns<TickerField>>;
  apply(frame: Frame): void;
}
