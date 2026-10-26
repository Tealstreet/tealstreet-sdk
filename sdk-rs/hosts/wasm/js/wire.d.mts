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
  PREDICTION_MARKETS: 13;
  PREDICTION_HOLDINGS: 14;
  BOOK_DELTA: 15;
  INSTRUMENT_CLASSIFICATIONS: 16;
  DERIVATIVE_PRODUCTS: 17;
  POSITION_GREEKS: 18;
  CAPABILITIES: 19;
  UNDERLYING_TOKENS: 20;
  ACCOUNT_READ: 21;
};
export const NONE: number;
export const TICKER_FIELDS: readonly [
  'bid', 'ask', 'last', 'mark', 'index', 'percentage',
  'openInterest', 'fundingRate', 'fundingTime', 'volume', 'quoteVolume', 'midPrice',
];
export type TickerField = (typeof TICKER_FIELDS)[number];
export const MARKET_COLUMNS: readonly [
  'precisionAmount', 'precisionPrice', 'minAmount', 'maxAmount', 'minLeverage',
  'maxLeverage', 'minPrice', 'contractSize', 'takerFee', 'makerFee',
];
export type MarketColumn = (typeof MARKET_COLUMNS)[number];
export const MARKET_FLAG: { ACTIVE: 1; INVERSE: 2; SPOT: 4; PERPETUAL: 8; EXPIRING: 16; READ_ONLY: 32 };
export type QuantityUnits = 'base' | 'nativePlacement' | 'inverseNative' | 'contractCount' | 'inverseContractSized';
export const QUANTITY_UNITS: readonly QuantityUnits[];
/** A market's quantity convention, from its MARKETS flags. */
export function marketQuantityUnits(flags: number): QuantityUnits;
/** What a venue answers beyond the common commands (`safe_cex_engine::Capabilities`). */
export interface EngineCapabilities {
  account: 'unsupported' | 'query' | 'afterLoad';
  clientOrderLookup: boolean;
  fillHistory: boolean;
  orderHistory: boolean;
  incomeHistory: boolean;
  fillsByOrderId: boolean;
  cancelAll: 'eachOpenSymbol' | 'eachPlacedSymbol' | 'native';
  leverageQuery: 'settings' | 'onRead' | 'onMetadata' | 'onReadAndMetadata';
  maxLeverage: 'market' | 'query' | 'rails' | 'unsupported';
  accountLeverage: boolean;
  accountMarginType: boolean;
  cancelWithSymbols: boolean;
  amendWithSymbol: boolean;
  orderBookQuery: boolean;
  options: boolean;
  quantityUnits: QuantityUnits;
  accountTier: boolean;
  closedPositions: boolean;
  balanceInfo: boolean;
  marketRefresh: boolean;
  positionHistory: 'unsupported' | 'fillsAccount' | 'fillsSymbol' | 'bybit' | 'okx' | 'blofin' | 'wooOrders' | 'bitunix';
  closedPositionFormat: 'unsupported' | 'bybit' | 'okx' | 'blofin' | 'bitunix';
  fillHistoryTime: 'asProvided' | 'secondsOrMilliseconds' | 'nonnegativeSecondsOrMilliseconds';
  stpControls: boolean;
  bybitExecutionControls: boolean;
  rejectClientSlippage: boolean;
  finiteCommandNumbers: boolean;
  finiteHistoryBounds: boolean;
  rejectOrderHistoryFilters: boolean;
  publicCatalogueValidation: boolean;
  isolatedBookSubscriptions: boolean;
  inverseClosedPositions: boolean;
  scheduleCancel: boolean;
}
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
  status: 'open' | 'closed' | 'canceled' | 'creating' | 'updating' | 'canceling' | undefined;
  type: 'market' | 'limit' | 'conditional' | 'stop_market' | 'take_profit_market' | 'trailing_stop_market' | undefined;
  side: 'buy' | 'sell' | undefined;
  reduceOnly: boolean;
  close: boolean | undefined;
  rejectReason: 'PostOnly' | 'InsufficientBalance' | 'ReduceOnly' | undefined;
  timeInForce: 'GoodTillCancel' | 'ImmediateOrCancel' | 'FillOrKill' | 'PostOnly' | undefined;
  triggerType: 'mark_price' | 'last_price' | 'index_price' | undefined;
}
export function orderEnums(word: number): OrderEnums;

export interface PositionEnums {
  side: 'long' | 'short' | undefined;
  marginType: 'isolated' | 'cross' | undefined;
  positionMode: 'hedged' | 'oneway' | undefined;
}
export function positionEnums(word: number): PositionEnums;
export const ORDER_STATUS: ReadonlyArray<OrderEnums['status']>;
export const ORDER_TYPE: ReadonlyArray<OrderEnums['type']>;
export const ORDER_SIDE: ReadonlyArray<OrderEnums['side']>;
export const TIME_IN_FORCE: ReadonlyArray<OrderEnums['timeInForce']>;
export const TRIGGER_TYPE: ReadonlyArray<OrderEnums['triggerType']>;
export const REJECT_REASON: ReadonlyArray<OrderEnums['rejectReason']>;
export const POSITION_SIDE: ReadonlyArray<PositionEnums['side']>;
export const MARGIN_TYPE: ReadonlyArray<PositionEnums['marginType']>;
export const POSITION_MODE: ReadonlyArray<PositionEnums['positionMode']>;

export type Columns<K extends string> = { [column in K]: Float64Array };

export type MarketTable = { count: number; flags: Uint32Array } & Columns<MarketColumn> & {
  id(i: number): string;
  symbol(i: number): string;
  base(i: number): string;
  quote(i: number): string;
  prettySymbol?(i: number): string | undefined;
};
export interface InstrumentClassificationTable {
  count: number;
  market: Uint32Array;
  symbolType(i: number): string | undefined;
  marketRegion(i: number): string | undefined;
  underlyingTicker(i: number): string | undefined;
}
export interface UnderlyingTokenTable {
  count: number;
  market: Uint32Array;
  name(i: number): string;
  fullName(i: number): string | undefined;
}
export interface DerivativeProductTable {
  count: number;
  market: Uint32Array;
  kind: Uint32Array;
  expiry: Float64Array;
  strike: Float64Array;
  underlying(i: number): string;
  settleAsset(i: number): string | undefined;
  premiumAsset(i: number): string | undefined;
  style(i: number): string | undefined;
  settlement(i: number): string | undefined;
}
export interface PositionGreekTable {
  count: number;
  delta: Float64Array;
  gamma: Float64Array;
  vega: Float64Array;
  theta: Float64Array;
  positionId(i: number): string;
  units(i: number): string;
}
export interface PredictionMarketTable {
  count: number;
  market: Uint32Array;
  flags: Uint32Array;
  tickSize: Float64Array;
  conditionId(i: number): string;
  outcome(i: number): string;
  question(i: number): string;
  description(i: number): string;
  endDate(i: number): string;
  slug(i: number): string;
}
export interface PredictionHoldingTable {
  count: number;
  market: Uint32Array;
  flags: Uint32Array;
}
export interface TickerIdentity { id: string; symbol: string }
export type TickerTable = {
  count: number;
  market: Uint32Array;
  identity?(i: number): TickerIdentity;
  midPrice?: Float64Array;
} & Columns<Exclude<TickerField, 'midPrice'>>;
export interface BookRows {
  market: number;
  /** `[price, amount, total]` per level, best first. */
  bids: Float64Array;
  asks: Float64Array;
}
export interface BookDeltaRows {
  market: number;
  /** Changed `[price, amount]` pairs; zero amount deletes a price. */
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
  /** Optional trailing column; absent in legacy frames, NaN for rows without native units. */
  remainingNative?: Float64Array;
  filledInPlacementUnits?: Float64Array;
  remainingNotional?: Float64Array;
  conditionalOrderPrice?: Float64Array;
  tpTriggerType?(i: number): OrderEnums['triggerType'];
  slTriggerType?(i: number): OrderEnums['triggerType'];
  symbol?(i: number): string | undefined;
  id(i: number): string;
  clientOrderId(i: number): string | undefined;
  parentId(i: number): string | undefined;
};
export type PositionTable = { count: number; market: Uint32Array; enums: Uint32Array; id(i: number): string | undefined; symbol?(i: number): string | undefined } & Columns<PositionColumn>;
export interface CoinBalance { base: string; amount: number; notional: number }
export interface CurrencyBalance {
  used: number;
  free: number;
  total: number;
  upnl: number;
}
export interface Balance extends CurrencyBalance {
  coins?: Record<string, CoinBalance>;
  currencies?: Record<string, CurrencyBalance>;
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
  buyLeverage?: Float64Array;
  sellLeverage?: Float64Array;
  symbol?(i: number): string | undefined;
  marginType(i: number): 'isolated' | 'cross' | undefined;
}
export interface FillRows {
  count: number;
  market: Uint32Array;
  side: Uint32Array;
  /** `[price, amount]` per execution. */
  rows: Float64Array;
  symbol?(i: number): string | undefined;
  /** Native execution identity; absent in legacy frames or when unreported. */
  fillId?(i: number): string | undefined;
  /** Optional financial tails; NaN means unreported, including legacy rows. */
  realizedPnl?: Float64Array;
  fee?: Float64Array;
  orderId(i: number): string;
}

export class Frame {
  constructor(buffer: ArrayBuffer);
  readonly buffer: ArrayBuffer;
  /** Engine time of the flush, ms. */
  readonly time: number;
  /** NONE is absent; other references require a valid STRINGS entry. */
  string(index: number): string | undefined;
  accountRead(): boolean;
  markets(): MarketTable | undefined;
  predictionMarkets(): PredictionMarketTable | undefined;
  instrumentClassifications(): InstrumentClassificationTable | undefined;
  underlyingTokens(): UnderlyingTokenTable | undefined;
  derivativeProducts(): DerivativeProductTable | undefined;
  positionGreeks(): PositionGreekTable | undefined;
  predictionHoldings(): PredictionHoldingTable | undefined;
  /** Owned copies survive source transfer; identities remain lazily decoded. */
  tickers(owned?: boolean): TickerTable | undefined;
  tickerDeltas(): Float64Array | undefined;
  books(): Generator<BookRows>;
  bookDeltas(): Generator<BookDeltaRows>;
  candles(): CandleRows | undefined;
  orders(): OrderTable | undefined;
  positions(): PositionTable | undefined;
  balance(): Balance | undefined;
  status(): Status | undefined;
  capabilities(): EngineCapabilities | undefined;
  settings(): SettingTable | undefined;
  fills(): FillRows | undefined;
}

/** Owned ticker columns and identities remain valid after transferring source frames. */
export class TickerState {
  count: number;
  market: Uint32Array;
  columns: Partial<Columns<TickerField>>;
  identity?: (i: number) => TickerIdentity;
  apply(frame: Frame): void;
}
