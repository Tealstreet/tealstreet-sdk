import type { Balance, Execution, HistoricalTrade, Order } from '../../../types';
import type { WooV3Decimal } from './woo.v3.contracts';

import {
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../../types';
import { subtract } from '../../../utils/safe-math';

/**
 * Strict, transport-free decoders and mappers for authenticated namespaced
 * WOO V3 reads. The live Woo exchange intentionally does not import this file.
 *
 * A future runtime adapter must decode and map a complete response before it
 * replaces any store slice. Decode/mapping failure means retain the previous
 * known-good slice; partial rows must never be committed.
 */

type UnknownRecord = Record<string, unknown>;

export type WooV3AccountMode = 'MARGIN' | 'SPOT';
export type WooV3AccountPositionMode = 'ONE_WAY' | 'HEDGE_MODE';
export type WooV3PositionSide = 'BOTH' | 'LONG' | 'SHORT';
export type WooV3OrderSide = 'BUY' | 'SELL';
export type WooV3OrderStatus = 'NEW' | 'CANCELLED' | 'PARTIAL_FILLED' | 'FILLED' | 'REJECTED';
export type WooV3RegularOrderType =
  | 'LIMIT'
  | 'MARKET'
  | 'IOC'
  | 'POST_ONLY'
  | 'AC'
  | 'FOK'
  | 'LIQUIDATE'
  | 'BID'
  | 'ASK'
  | 'RPI'
  | 'LIQUIDATE_BLP'
  | 'ADL';
export type WooV3AlgoType =
  | 'STOP'
  | 'OCO'
  | 'TRAILING_STOP'
  | 'POSITIONAL_TP_SL'
  | 'TP_SL'
  | 'BRACKET'
  | 'STOP_BRACKET'
  // These two are absent from the schema enum but present in its official response example.
  | 'TAKE_PROFIT'
  | 'STOP_LOSS';
export type WooV3TriggerPriceType = 'MARKET_PRICE' | 'INDEX_PRICE' | 'LAST_PRICE';
export type WooV3TriggerStatus = 'USELESS' | 'TRIGGERED' | 'CANCELLED' | 'SUCCESS';
export type WooV3MarginMode = 'ISOLATED' | 'CROSSED';

export type WooV3PageMeta = Readonly<{
  total: number;
  recordsPerPage: number;
  currentPage: number;
}>;

export type WooV3AccountInfo = Readonly<{
  applicationId: string;
  account: string;
  alias: string;
  otpauth: boolean;
  accountMode: WooV3AccountMode;
  positionMode: WooV3AccountPositionMode;
  leverage: number;
  marginRatio: WooV3Decimal;
  openMarginRatio: WooV3Decimal;
  initialMarginRatio: WooV3Decimal;
  maintenanceMarginRatio: WooV3Decimal;
  totalCollateral: WooV3Decimal;
  freeCollateral: WooV3Decimal;
  totalAccountValue: WooV3Decimal;
  totalTradingValue: WooV3Decimal;
  totalVaultValue: WooV3Decimal;
  totalStakingValue: WooV3Decimal;
  totalEarnValue: WooV3Decimal;
  totalLaunchpadValue: WooV3Decimal;
  referrerID: string;
  accountType: 'Main' | 'Sub';
}>;

export type WooV3AssetBalance = Readonly<{
  token: string;
  holding: WooV3Decimal;
  frozen: WooV3Decimal;
  staked: WooV3Decimal;
  unbonding: WooV3Decimal;
  vault: WooV3Decimal;
  interest: WooV3Decimal;
  earn: WooV3Decimal;
  pendingShortQty: WooV3Decimal;
  pendingLongQty: WooV3Decimal;
  availableBalance: WooV3Decimal;
  averageOpenPrice: WooV3Decimal;
  markPrice: WooV3Decimal;
  pnl24H: WooV3Decimal;
  fee24H: WooV3Decimal;
  timestamp: number;
}>;

export type WooV3AssetBalanceReadModel = Readonly<{
  token: string;
  tradingTotal: number;
  tradingFree: number;
  tradingFrozen: number;
  staked: number;
  unbonding: number;
  vault: number;
  interest: number;
  earn: number;
  pendingShortQuantity: number;
  pendingLongQuantity: number;
  averageOpenPrice: number;
  markPrice: number;
  pnl24H: number;
  fee24H: number;
  timestamp: number;
}>;

export type WooV3FuturesPosition = Readonly<{
  symbol: string;
  holding: WooV3Decimal;
  positionSide: WooV3PositionSide;
  pendingLongQty: WooV3Decimal;
  pendingShortQty: WooV3Decimal;
  settlePrice: WooV3Decimal;
  averageOpenPrice: WooV3Decimal;
  pnl24H: WooV3Decimal;
  fee24H: WooV3Decimal;
  markPrice: WooV3Decimal;
  estLiqPrice: WooV3Decimal;
  adlQuantile: number;
  timestamp: number;
}>;

/**
 * V3 positions do not contain marginMode or leverage. This read model therefore
 * stops before safe-cex Position; a future adapter must join separately decoded
 * account/leverage configuration rather than invent either field.
 */
export type WooV3PositionReadModel = Readonly<{
  symbol: string;
  side: PositionSide;
  positionMode: PositionMode;
  gatewayPositionSide: WooV3PositionSide;
  signedHolding: number;
  contracts: number;
  entryPrice: number;
  markPrice: number;
  notional: number;
  estimatedLiquidationPrice: number;
  pendingLongQuantity: number;
  pendingShortQuantity: number;
  pnl24H: number;
  fee24H: number;
  adlQuantile: number;
  timestamp: number;
}>;

export type WooV3RegularOrder = Readonly<{
  symbol: string;
  status: WooV3OrderStatus;
  side: WooV3OrderSide;
  positionSide: WooV3PositionSide;
  createdTime: number;
  orderId: number;
  orderTag: string;
  price: WooV3Decimal;
  type: WooV3RegularOrderType;
  quantity: WooV3Decimal;
  amount: WooV3Decimal | null | 'null';
  visible: WooV3Decimal;
  executed: WooV3Decimal;
  totalFee: WooV3Decimal;
  feeAsset: string;
  totalRebate: number;
  rebateCurrency: string;
  clientOrderId: number;
  reduceOnly: boolean;
  realizedPnl?: WooV3Decimal;
  averageExecutedPrice: WooV3Decimal;
  leverage?: number;
  marginMode?: WooV3MarginMode;
  bidAskLevel?: number;
}>;

/**
 * WOO's algo schema says quantity is an integer, while its own response example
 * alternates between JSON numbers and decimal strings. Keep that representation
 * drift gateway-labelled; safe-cex mapping only consumes decimal strings.
 */
export type WooV3GatewayAlgoQuantity = WooV3Decimal | number;

export type WooV3AlgoOrder = Readonly<{
  algoOrderId: number;
  clientAlgoOrderId: number;
  rootAlgoOrderId: number;
  parentAlgoOrderId: number;
  symbol: string;
  algoOrderTag: string;
  algoType: WooV3AlgoType;
  side: WooV3OrderSide;
  positionSide: WooV3PositionSide;
  quantity: WooV3GatewayAlgoQuantity;
  isTriggered: boolean;
  triggerStatus: WooV3TriggerStatus;
  type?: WooV3RegularOrderType;
  rootAlgoStatus: WooV3OrderStatus;
  algoStatus: WooV3OrderStatus;
  triggerPriceType: WooV3TriggerPriceType;
  triggerTime: WooV3Decimal;
  triggerPrice?: WooV3Decimal;
  price?: WooV3Decimal;
  visibleQuantity?: WooV3Decimal;
  totalExecutedQuantity: WooV3Decimal;
  averageExecutedPrice: WooV3Decimal;
  totalFee: WooV3Decimal;
  feeAsset: string;
  totalRebate: number;
  rebateCurrency: string;
  reduceOnly: boolean;
  createdTime: number;
  updatedTime: number;
  isActivated: boolean;
  callbackRate?: WooV3Decimal;
  activatedPrice?: WooV3Decimal;
  leverage?: number;
  marginMode?: WooV3MarginMode;
  childOrders: readonly WooV3AlgoOrder[];
}>;

export type WooV3Transaction = Readonly<{
  id: number;
  symbol: string;
  orderId: number;
  orderTag: string;
  executedPrice: WooV3Decimal;
  executedQuantity: WooV3Decimal;
  isMaker: 0 | 1;
  isMatchRpi: boolean;
  side: WooV3OrderSide;
  // The official OpenAPI deliberately models fee as JSON number, unlike price/quantity.
  fee: number;
  feeAsset: string;
  realizedPnl: WooV3Decimal | null;
  executedTimestamp: number;
}>;

export type WooV3MappedTransaction = Readonly<{
  execution: Execution;
  historicalTrade?: HistoricalTrade;
  deferredHistoricalTradeReason?: string;
  maker: boolean;
  isMatchRpi: boolean;
}>;

export type WooV3DeferredAlgoOrder = Readonly<{
  order: WooV3AlgoOrder;
  reason: string;
}>;

export type WooV3AlgoOrderMapping = Readonly<{
  orders: readonly Order[];
  deferred: readonly WooV3DeferredAlgoOrder[];
}>;

const DECIMAL_PATTERN = /^-?(?:\d+(?:\.\d*)?|\.\d+)$/;

const isRecord = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (!isRecord(value)) throw new Error(`${path} must be an object`);
  return value;
};

const requireArray = (value: unknown, path: string): readonly unknown[] => {
  if (!Array.isArray(value)) throw new Error(`${path} must be an array`);
  return value;
};

const requireString = (value: unknown, path: string, allowEmpty = false): string => {
  if (typeof value !== 'string' || (!allowEmpty && value.length === 0)) {
    throw new Error(`${path} must be ${allowEmpty ? 'a string' : 'a non-empty string'}`);
  }
  return value;
};

const requireBoolean = (value: unknown, path: string): boolean => {
  if (typeof value !== 'boolean') throw new Error(`${path} must be a boolean`);
  return value;
};

const requireInteger = (value: unknown, path: string, minimum = 0): number => {
  if (!Number.isSafeInteger(value) || (value as number) < minimum) {
    throw new Error(`${path} must be a safe integer of at least ${minimum}`);
  }
  return value as number;
};

const requireFiniteNumber = (value: unknown, path: string): number => {
  if (typeof value !== 'number' || !Number.isFinite(value)) throw new Error(`${path} must be a finite number`);
  return value;
};

const requireDecimalString = (value: unknown, path: string): WooV3Decimal => {
  if (typeof value !== 'string' || !DECIMAL_PATTERN.test(value) || !Number.isFinite(Number(value))) {
    throw new Error(`${path} must be a finite decimal string`);
  }
  return value;
};

const optionalDecimalString = (value: unknown, path: string): WooV3Decimal | undefined =>
  value === undefined ? undefined : requireDecimalString(value, path);

const requireEnum = <T extends string>(value: unknown, values: readonly T[], path: string): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) {
    throw new Error(`${path} must be one of ${values.join(', ')}`);
  }
  return value as T;
};

const decodeEnvelope = (input: unknown, name: string): UnknownRecord => {
  const response = requireRecord(input, `WOO V3 ${name} response`);
  if (requireBoolean(response.success, `WOO V3 ${name}.success`) !== true) {
    throw new Error(`WOO V3 ${name} response reports success=false`);
  }
  requireInteger(response.timestamp, `WOO V3 ${name}.timestamp`);
  return requireRecord(response.data, `WOO V3 ${name}.data`);
};

const decodeMeta = (input: unknown, path: string): WooV3PageMeta => {
  const meta = requireRecord(input, path);
  return Object.freeze({
    total: requireInteger(meta.total, `${path}.total`),
    recordsPerPage: requireInteger(meta.recordsPerPage, `${path}.recordsPerPage`, 1),
    currentPage: requireInteger(meta.currentPage, `${path}.currentPage`, 1),
  });
};

const decodePaginatedRows = <T>(
  input: unknown,
  name: string,
  decoder: (row: unknown, path: string) => T,
): Readonly<{ rows: readonly T[]; meta: WooV3PageMeta }> => {
  const data = decodeEnvelope(input, name);
  const meta = decodeMeta(data.meta, `WOO V3 ${name}.data.meta`);
  const rows = requireArray(data.rows, `WOO V3 ${name}.data.rows`).map((row, index) =>
    decoder(row, `WOO V3 ${name}.data.rows[${index}]`),
  );
  if (rows.length > meta.recordsPerPage) throw new Error(`WOO V3 ${name} page exceeds recordsPerPage`);
  return Object.freeze({ rows: Object.freeze(rows), meta });
};

const decimalToNumber = (value: WooV3Decimal, path: string, minimum?: number): number => {
  const number = Number(value);
  if (!Number.isFinite(number) || (minimum !== undefined && number < minimum)) {
    throw new Error(`${path} must be${minimum === undefined ? '' : ` at least ${minimum}`}`);
  }
  return number;
};

const normalizePerpetualSymbol = (value: string, path: string): string => {
  const match = /^PERP_([A-Z0-9]+)_([A-Z0-9]+)$/.exec(value);
  if (!match) throw new Error(`${path} must use the PERP_BASE_QUOTE form`);
  return `${match[1]}${match[2]}`;
};

const mapSide = (side: WooV3OrderSide): OrderSide => (side === 'BUY' ? OrderSide.Buy : OrderSide.Sell);

const mapPositionMode = (mode: WooV3AccountPositionMode): PositionMode =>
  mode === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;

const assertPositionSideMatchesMode = (
  side: WooV3PositionSide,
  mode: WooV3AccountPositionMode,
  path: string,
): void => {
  if ((mode === 'ONE_WAY' && side !== 'BOTH') || (mode === 'HEDGE_MODE' && side === 'BOTH')) {
    throw new Error(`${path}=${side} is incompatible with ${mode}`);
  }
};

const mapPendingStatus = (status: WooV3OrderStatus, path: string): OrderStatus => {
  if (status !== 'NEW' && status !== 'PARTIAL_FILLED') {
    throw new Error(`${path}=${status} is not an incomplete/pending order`);
  }
  return OrderStatus.Open;
};

const mapRegularOrderType = (
  type: WooV3RegularOrderType,
  path: string,
): Readonly<{ type: OrderType; timeInForce?: OrderTimeInForce }> => {
  switch (type) {
    case 'LIMIT':
      return { type: OrderType.Limit, timeInForce: OrderTimeInForce.GoodTillCancel };
    case 'MARKET':
      return { type: OrderType.Market };
    case 'POST_ONLY':
      return { type: OrderType.Limit, timeInForce: OrderTimeInForce.PostOnly };
    case 'IOC':
      return { type: OrderType.Limit, timeInForce: OrderTimeInForce.ImmediateOrCancel };
    case 'FOK':
      return { type: OrderType.Limit, timeInForce: OrderTimeInForce.FillOrKill };
    default:
      throw new Error(`${path}=${type} has no lossless safe-cex order mapping`);
  }
};

const mapTriggerPriceType = (type: WooV3TriggerPriceType): OrderStopTriggerType =>
  type === 'INDEX_PRICE' ? OrderStopTriggerType.IndexPrice : OrderStopTriggerType.LastPrice;

const decodeRegularOrder = (input: unknown, path: string): WooV3RegularOrder => {
  const row = requireRecord(input, path);
  const amount =
    row.amount === null || row.amount === 'null' ? row.amount : requireDecimalString(row.amount, `${path}.amount`);
  const realizedPnl = optionalDecimalString(row.realizedPnl, `${path}.realizedPnl`);
  const marginMode =
    row.marginMode === undefined
      ? undefined
      : requireEnum(row.marginMode, ['ISOLATED', 'CROSSED'] as const, `${path}.marginMode`);
  return Object.freeze({
    symbol: requireString(row.symbol, `${path}.symbol`),
    status: requireEnum(
      row.status,
      ['NEW', 'CANCELLED', 'PARTIAL_FILLED', 'FILLED', 'REJECTED'] as const,
      `${path}.status`,
    ),
    side: requireEnum(row.side, ['BUY', 'SELL'] as const, `${path}.side`),
    positionSide: requireEnum(row.positionSide, ['BOTH', 'LONG', 'SHORT'] as const, `${path}.positionSide`),
    createdTime: requireInteger(row.createdTime, `${path}.createdTime`),
    orderId: requireInteger(row.orderId, `${path}.orderId`, 1),
    orderTag: requireString(row.orderTag, `${path}.orderTag`),
    price: requireDecimalString(row.price, `${path}.price`),
    type: requireEnum(
      row.type,
      [
        'LIMIT',
        'MARKET',
        'IOC',
        'POST_ONLY',
        'AC',
        'FOK',
        'LIQUIDATE',
        'BID',
        'ASK',
        'RPI',
        'LIQUIDATE_BLP',
        'ADL',
      ] as const,
      `${path}.type`,
    ),
    quantity: requireDecimalString(row.quantity, `${path}.quantity`),
    amount,
    visible: requireDecimalString(row.visible, `${path}.visible`),
    executed: requireDecimalString(row.executed, `${path}.executed`),
    totalFee: requireDecimalString(row.totalFee, `${path}.totalFee`),
    feeAsset: requireString(row.feeAsset, `${path}.feeAsset`, true),
    totalRebate: requireFiniteNumber(row.totalRebate, `${path}.totalRebate`),
    rebateCurrency: requireString(row.rebateCurrency, `${path}.rebateCurrency`, true),
    clientOrderId: requireInteger(row.clientOrderId, `${path}.clientOrderId`),
    reduceOnly: requireBoolean(row.reduceOnly, `${path}.reduceOnly`),
    ...(realizedPnl === undefined ? {} : { realizedPnl }),
    averageExecutedPrice: requireDecimalString(row.averageExecutedPrice, `${path}.averageExecutedPrice`),
    ...(row.leverage === undefined ? {} : { leverage: requireInteger(row.leverage, `${path}.leverage`, 1) }),
    ...(marginMode === undefined ? {} : { marginMode }),
    ...(row.bidAskLevel === undefined ? {} : { bidAskLevel: requireInteger(row.bidAskLevel, `${path}.bidAskLevel`) }),
  });
};

const decodeAlgoQuantity = (value: unknown, path: string): WooV3GatewayAlgoQuantity => {
  if (typeof value === 'string') return requireDecimalString(value, path);
  const number = requireFiniteNumber(value, path);
  if (number < 0) throw new Error(`${path} must not be negative`);
  return number;
};

const decodeAlgoOrder = (input: unknown, path: string, depth = 0): WooV3AlgoOrder => {
  if (depth > 8) throw new Error(`${path} exceeds the supported child-order nesting depth`);
  const row = requireRecord(input, path);
  const type =
    row.type === undefined
      ? undefined
      : requireEnum(
          row.type,
          ['LIMIT', 'MARKET', 'IOC', 'POST_ONLY', 'AC', 'FOK', 'LIQUIDATE', 'BID', 'ASK', 'RPI'] as const,
          `${path}.type`,
        );
  const marginMode =
    row.marginMode === undefined
      ? undefined
      : requireEnum(row.marginMode, ['ISOLATED', 'CROSSED'] as const, `${path}.marginMode`);
  const children =
    row.childOrders === undefined
      ? []
      : requireArray(row.childOrders, `${path}.childOrders`).map((child, index) =>
          decodeAlgoOrder(child, `${path}.childOrders[${index}]`, depth + 1),
        );
  return Object.freeze({
    algoOrderId: requireInteger(row.algoOrderId, `${path}.algoOrderId`, 1),
    clientAlgoOrderId: requireInteger(row.clientAlgoOrderId, `${path}.clientAlgoOrderId`),
    rootAlgoOrderId: requireInteger(row.rootAlgoOrderId, `${path}.rootAlgoOrderId`, 1),
    parentAlgoOrderId: requireInteger(row.parentAlgoOrderId, `${path}.parentAlgoOrderId`),
    symbol: requireString(row.symbol, `${path}.symbol`),
    algoOrderTag: requireString(row.algoOrderTag, `${path}.algoOrderTag`),
    algoType: requireEnum(
      row.algoType,
      [
        'STOP',
        'OCO',
        'TRAILING_STOP',
        'POSITIONAL_TP_SL',
        'TP_SL',
        'BRACKET',
        'STOP_BRACKET',
        'TAKE_PROFIT',
        'STOP_LOSS',
      ] as const,
      `${path}.algoType`,
    ),
    side: requireEnum(row.side, ['BUY', 'SELL'] as const, `${path}.side`),
    positionSide: requireEnum(row.positionSide, ['BOTH', 'LONG', 'SHORT'] as const, `${path}.positionSide`),
    quantity: decodeAlgoQuantity(row.quantity, `${path}.quantity`),
    isTriggered: requireBoolean(row.isTriggered, `${path}.isTriggered`),
    triggerStatus: requireEnum(
      row.triggerStatus,
      ['USELESS', 'TRIGGERED', 'CANCELLED', 'SUCCESS'] as const,
      `${path}.triggerStatus`,
    ),
    ...(type === undefined ? {} : { type }),
    rootAlgoStatus: requireEnum(
      row.rootAlgoStatus,
      ['NEW', 'CANCELLED', 'PARTIAL_FILLED', 'FILLED', 'REJECTED'] as const,
      `${path}.rootAlgoStatus`,
    ),
    algoStatus: requireEnum(
      row.algoStatus,
      ['NEW', 'CANCELLED', 'PARTIAL_FILLED', 'FILLED', 'REJECTED'] as const,
      `${path}.algoStatus`,
    ),
    triggerPriceType: requireEnum(
      row.triggerPriceType,
      ['MARKET_PRICE', 'INDEX_PRICE', 'LAST_PRICE'] as const,
      `${path}.triggerPriceType`,
    ),
    triggerTime: requireDecimalString(row.triggerTime, `${path}.triggerTime`),
    ...(row.triggerPrice === undefined
      ? {}
      : { triggerPrice: requireDecimalString(row.triggerPrice, `${path}.triggerPrice`) }),
    ...(row.price === undefined ? {} : { price: requireDecimalString(row.price, `${path}.price`) }),
    ...(row.visibleQuantity === undefined
      ? {}
      : { visibleQuantity: requireDecimalString(row.visibleQuantity, `${path}.visibleQuantity`) }),
    totalExecutedQuantity: requireDecimalString(row.totalExecutedQuantity, `${path}.totalExecutedQuantity`),
    averageExecutedPrice: requireDecimalString(row.averageExecutedPrice, `${path}.averageExecutedPrice`),
    totalFee: requireDecimalString(row.totalFee, `${path}.totalFee`),
    feeAsset: requireString(row.feeAsset, `${path}.feeAsset`, true),
    totalRebate: requireFiniteNumber(row.totalRebate, `${path}.totalRebate`),
    rebateCurrency: requireString(row.rebateCurrency, `${path}.rebateCurrency`, true),
    reduceOnly: requireBoolean(row.reduceOnly, `${path}.reduceOnly`),
    createdTime: requireInteger(row.createdTime, `${path}.createdTime`),
    updatedTime: requireInteger(row.updatedTime, `${path}.updatedTime`),
    isActivated: requireBoolean(row.isActivated, `${path}.isActivated`),
    ...(row.callbackRate === undefined
      ? {}
      : { callbackRate: requireDecimalString(row.callbackRate, `${path}.callbackRate`) }),
    ...(row.activatedPrice === undefined
      ? {}
      : { activatedPrice: requireDecimalString(row.activatedPrice, `${path}.activatedPrice`) }),
    ...(row.leverage === undefined ? {} : { leverage: requireInteger(row.leverage, `${path}.leverage`, 1) }),
    ...(marginMode === undefined ? {} : { marginMode }),
    childOrders: Object.freeze(children),
  });
};

const decodeTransaction = (input: unknown, path: string): WooV3Transaction => {
  const row = requireRecord(input, path);
  const isMaker = requireInteger(row.isMaker, `${path}.isMaker`);
  if (isMaker !== 0 && isMaker !== 1) throw new Error(`${path}.isMaker must be 0 or 1`);
  const realizedPnl = row.realizedPnl === null ? null : requireDecimalString(row.realizedPnl, `${path}.realizedPnl`);
  return Object.freeze({
    id: requireInteger(row.id, `${path}.id`, 1),
    symbol: requireString(row.symbol, `${path}.symbol`),
    orderId: requireInteger(row.orderId, `${path}.orderId`, 1),
    orderTag: requireString(row.orderTag, `${path}.orderTag`),
    executedPrice: requireDecimalString(row.executedPrice, `${path}.executedPrice`),
    executedQuantity: requireDecimalString(row.executedQuantity, `${path}.executedQuantity`),
    isMaker,
    isMatchRpi: requireBoolean(row.isMatchRpi, `${path}.isMatchRpi`),
    side: requireEnum(row.side, ['BUY', 'SELL'] as const, `${path}.side`),
    fee: requireFiniteNumber(row.fee, `${path}.fee`),
    feeAsset: requireString(row.feeAsset, `${path}.feeAsset`),
    realizedPnl,
    executedTimestamp: requireInteger(row.executedTimestamp, `${path}.executedTimestamp`),
  });
};

export const decodeWooV3AccountInfo = (input: unknown): WooV3AccountInfo => {
  const data = decodeEnvelope(input, 'account info');
  return Object.freeze({
    applicationId: requireString(data.applicationId, 'WOO V3 account info.data.applicationId'),
    account: requireString(data.account, 'WOO V3 account info.data.account'),
    alias: requireString(data.alias, 'WOO V3 account info.data.alias'),
    otpauth: requireBoolean(data.otpauth, 'WOO V3 account info.data.otpauth'),
    accountMode: requireEnum(data.accountMode, ['MARGIN', 'SPOT'] as const, 'WOO V3 account info.data.accountMode'),
    positionMode: requireEnum(
      data.positionMode,
      ['ONE_WAY', 'HEDGE_MODE'] as const,
      'WOO V3 account info.data.positionMode',
    ),
    leverage: requireInteger(data.leverage, 'WOO V3 account info.data.leverage', 1),
    marginRatio: requireDecimalString(data.marginRatio, 'WOO V3 account info.data.marginRatio'),
    openMarginRatio: requireDecimalString(data.openMarginRatio, 'WOO V3 account info.data.openMarginRatio'),
    initialMarginRatio: requireDecimalString(
      data.initialMarginRatio,
      'WOO V3 account info.data.initialMarginRatio',
    ),
    maintenanceMarginRatio: requireDecimalString(
      data.maintenanceMarginRatio,
      'WOO V3 account info.data.maintenanceMarginRatio',
    ),
    totalCollateral: requireDecimalString(data.totalCollateral, 'WOO V3 account info.data.totalCollateral'),
    freeCollateral: requireDecimalString(data.freeCollateral, 'WOO V3 account info.data.freeCollateral'),
    totalAccountValue: requireDecimalString(data.totalAccountValue, 'WOO V3 account info.data.totalAccountValue'),
    totalTradingValue: requireDecimalString(data.totalTradingValue, 'WOO V3 account info.data.totalTradingValue'),
    totalVaultValue: requireDecimalString(data.totalVaultValue, 'WOO V3 account info.data.totalVaultValue'),
    totalStakingValue: requireDecimalString(data.totalStakingValue, 'WOO V3 account info.data.totalStakingValue'),
    totalEarnValue: requireDecimalString(data.totalEarnValue, 'WOO V3 account info.data.totalEarnValue'),
    totalLaunchpadValue: requireDecimalString(
      data.totalLaunchpadValue,
      'WOO V3 account info.data.totalLaunchpadValue',
    ),
    referrerID: requireString(data.referrerID, 'WOO V3 account info.data.referrerID'),
    accountType: requireEnum(data.accountType, ['Main', 'Sub'] as const, 'WOO V3 account info.data.accountType'),
  });
};

export const mapWooV3AccountBalance = (info: WooV3AccountInfo, previousUpnl: number): Balance => {
  if (info.accountMode !== 'MARGIN') throw new Error(`WOO V3 account mode ${info.accountMode} is not futures-capable`);
  if (!Number.isFinite(previousUpnl)) throw new Error('WOO V3 previous uPnL must be finite');
  const totalCollateral = decimalToNumber(info.totalCollateral, 'WOO V3 totalCollateral');
  const free = decimalToNumber(info.freeCollateral, 'WOO V3 freeCollateral');
  const total = decimalToNumber(info.totalAccountValue, 'WOO V3 totalAccountValue');
  const used = totalCollateral - free;
  if (used < 0) throw new Error('WOO V3 freeCollateral must not exceed totalCollateral');
  return Object.freeze({ free, used, total, upnl: previousUpnl, info });
};

export const decodeWooV3AssetBalances = (input: unknown): readonly WooV3AssetBalance[] => {
  const data = decodeEnvelope(input, 'asset balances');
  return Object.freeze(
    requireArray(data.holding, 'WOO V3 asset balances.data.holding').map((input, index) => {
      const path = `WOO V3 asset balances.data.holding[${index}]`;
      const row = requireRecord(input, path);
      return Object.freeze({
        token: requireString(row.token, `${path}.token`),
        holding: requireDecimalString(row.holding, `${path}.holding`),
        frozen: requireDecimalString(row.frozen, `${path}.frozen`),
        staked: requireDecimalString(row.staked, `${path}.staked`),
        unbonding: requireDecimalString(row.unbonding, `${path}.unbonding`),
        vault: requireDecimalString(row.vault, `${path}.vault`),
        interest: requireDecimalString(row.interest, `${path}.interest`),
        earn: requireDecimalString(row.earn, `${path}.earn`),
        pendingShortQty: requireDecimalString(row.pendingShortQty, `${path}.pendingShortQty`),
        pendingLongQty: requireDecimalString(row.pendingLongQty, `${path}.pendingLongQty`),
        availableBalance: requireDecimalString(row.availableBalance, `${path}.availableBalance`),
        averageOpenPrice: requireDecimalString(row.averageOpenPrice, `${path}.averageOpenPrice`),
        markPrice: requireDecimalString(row.markPrice, `${path}.markPrice`),
        pnl24H: requireDecimalString(row.pnl24H, `${path}.pnl24H`),
        fee24H: requireDecimalString(row.fee24H, `${path}.fee24H`),
        timestamp: requireInteger(row.timestamp, `${path}.timestamp`),
      });
    }),
  );
};

export const mapWooV3AssetBalances = (rows: readonly WooV3AssetBalance[]): readonly WooV3AssetBalanceReadModel[] =>
  Object.freeze(
    rows.map((row, index) => {
      const path = `WOO V3 asset balance[${index}]`;
      return Object.freeze({
        token: row.token,
        tradingTotal: decimalToNumber(row.holding, `${path}.holding`, 0),
        tradingFree: decimalToNumber(row.availableBalance, `${path}.availableBalance`, 0),
        tradingFrozen: decimalToNumber(row.frozen, `${path}.frozen`, 0),
        staked: decimalToNumber(row.staked, `${path}.staked`, 0),
        unbonding: decimalToNumber(row.unbonding, `${path}.unbonding`, 0),
        vault: decimalToNumber(row.vault, `${path}.vault`, 0),
        interest: decimalToNumber(row.interest, `${path}.interest`),
        earn: decimalToNumber(row.earn, `${path}.earn`, 0),
        pendingShortQuantity: decimalToNumber(row.pendingShortQty, `${path}.pendingShortQty`, 0),
        pendingLongQuantity: decimalToNumber(row.pendingLongQty, `${path}.pendingLongQty`, 0),
        averageOpenPrice: decimalToNumber(row.averageOpenPrice, `${path}.averageOpenPrice`, 0),
        markPrice: decimalToNumber(row.markPrice, `${path}.markPrice`, 0),
        pnl24H: decimalToNumber(row.pnl24H, `${path}.pnl24H`),
        fee24H: decimalToNumber(row.fee24H, `${path}.fee24H`),
        timestamp: row.timestamp,
      });
    }),
  );

export const decodeWooV3FuturesPositions = (input: unknown): readonly WooV3FuturesPosition[] => {
  const data = decodeEnvelope(input, 'futures positions');
  const hasRows = data.rows !== undefined;
  const hasPositions = data.positions !== undefined;
  if (hasRows === hasPositions) {
    throw new Error('WOO V3 futures positions must contain exactly one of data.rows or data.positions');
  }
  const inputRows = hasRows ? data.rows : data.positions;
  return Object.freeze(
    requireArray(inputRows, `WOO V3 futures positions.data.${hasRows ? 'rows' : 'positions'}`).map((input, index) => {
      const path = `WOO V3 futures positions.data.${hasRows ? 'rows' : 'positions'}[${index}]`;
      const row = requireRecord(input, path);
      return Object.freeze({
        symbol: requireString(row.symbol, `${path}.symbol`),
        holding: requireDecimalString(row.holding, `${path}.holding`),
        positionSide: requireEnum(row.positionSide, ['BOTH', 'LONG', 'SHORT'] as const, `${path}.positionSide`),
        pendingLongQty: requireDecimalString(row.pendingLongQty, `${path}.pendingLongQty`),
        pendingShortQty: requireDecimalString(row.pendingShortQty, `${path}.pendingShortQty`),
        settlePrice: requireDecimalString(row.settlePrice, `${path}.settlePrice`),
        averageOpenPrice: requireDecimalString(row.averageOpenPrice, `${path}.averageOpenPrice`),
        pnl24H: requireDecimalString(row.pnl24H, `${path}.pnl24H`),
        fee24H: requireDecimalString(row.fee24H, `${path}.fee24H`),
        markPrice: requireDecimalString(row.markPrice, `${path}.markPrice`),
        estLiqPrice: requireDecimalString(row.estLiqPrice, `${path}.estLiqPrice`),
        adlQuantile: requireInteger(row.adlQuantile, `${path}.adlQuantile`),
        timestamp: requireInteger(row.timestamp, `${path}.timestamp`),
      });
    }),
  );
};

export const mapWooV3FuturesPositions = (
  rows: readonly WooV3FuturesPosition[],
  accountPositionMode: WooV3AccountPositionMode,
): readonly WooV3PositionReadModel[] =>
  Object.freeze(
    rows
      .map((row, index): WooV3PositionReadModel | null => {
        const path = `WOO V3 futures position[${index}]`;
        assertPositionSideMatchesMode(row.positionSide, accountPositionMode, `${path}.positionSide`);
        const signedHolding = decimalToNumber(row.holding, `${path}.holding`);
        if (signedHolding === 0) return null;
        const side =
          accountPositionMode === 'HEDGE_MODE'
            ? row.positionSide === 'LONG'
              ? PositionSide.Long
              : PositionSide.Short
            : signedHolding > 0
              ? PositionSide.Long
              : PositionSide.Short;
        if (
          (accountPositionMode === 'HEDGE_MODE' && side === PositionSide.Long && signedHolding < 0) ||
          (accountPositionMode === 'HEDGE_MODE' && side === PositionSide.Short && signedHolding > 0)
        ) {
          throw new Error(`${path}.holding sign conflicts with positionSide`);
        }
        const contracts = Math.abs(signedHolding);
        const markPrice = decimalToNumber(row.markPrice, `${path}.markPrice`, Number.MIN_VALUE);
        return Object.freeze({
          symbol: normalizePerpetualSymbol(row.symbol, `${path}.symbol`),
          side,
          positionMode: mapPositionMode(accountPositionMode),
          gatewayPositionSide: row.positionSide,
          signedHolding,
          contracts,
          entryPrice: decimalToNumber(row.averageOpenPrice, `${path}.averageOpenPrice`, 0),
          markPrice,
          notional: contracts * markPrice,
          estimatedLiquidationPrice: decimalToNumber(row.estLiqPrice, `${path}.estLiqPrice`, 0),
          pendingLongQuantity: decimalToNumber(row.pendingLongQty, `${path}.pendingLongQty`, 0),
          pendingShortQuantity: decimalToNumber(row.pendingShortQty, `${path}.pendingShortQty`, 0),
          pnl24H: decimalToNumber(row.pnl24H, `${path}.pnl24H`),
          fee24H: decimalToNumber(row.fee24H, `${path}.fee24H`),
          adlQuantile: row.adlQuantile,
          timestamp: row.timestamp,
        });
      })
      .filter((row): row is WooV3PositionReadModel => row !== null),
  );

export const decodeWooV3PendingOrders = (input: unknown) =>
  decodePaginatedRows(input, 'pending orders', decodeRegularOrder);

export const mapWooV3PendingOrders = (
  rows: readonly WooV3RegularOrder[],
  accountPositionMode: WooV3AccountPositionMode,
): readonly Order[] =>
  Object.freeze(
    rows.map((row, index) => {
      const path = `WOO V3 pending order[${index}]`;
      assertPositionSideMatchesMode(row.positionSide, accountPositionMode, `${path}.positionSide`);
      const mappedType = mapRegularOrderType(row.type, `${path}.type`);
      const amount = decimalToNumber(row.quantity, `${path}.quantity`, Number.MIN_VALUE);
      const filled = decimalToNumber(row.executed, `${path}.executed`, 0);
      const remaining = subtract(amount, filled);
      if (remaining < 0) throw new Error(`${path}.executed must not exceed quantity`);
      return Object.freeze({
        id: String(row.orderId),
        status: mapPendingStatus(row.status, `${path}.status`),
        symbol: normalizePerpetualSymbol(row.symbol, `${path}.symbol`),
        type: mappedType.type,
        side: mapSide(row.side),
        price: decimalToNumber(row.price, `${path}.price`, 0),
        amount,
        filled,
        remaining,
        reduceOnly: row.reduceOnly,
        ...(mappedType.timeInForce === undefined ? {} : { timeInForce: mappedType.timeInForce }),
        lastUpdate: row.createdTime,
        info: row,
      });
    }),
  );

export const decodeWooV3PendingAlgoOrders = (input: unknown) =>
  decodePaginatedRows(input, 'pending algo orders', decodeAlgoOrder);

export const mapWooV3PendingAlgoOrders = (
  rows: readonly WooV3AlgoOrder[],
  accountPositionMode: WooV3AccountPositionMode,
): WooV3AlgoOrderMapping => {
  const orders: Order[] = [];
  const deferred: WooV3DeferredAlgoOrder[] = [];
  const visit = (row: WooV3AlgoOrder, rootId: number, path: string): void => {
    assertPositionSideMatchesMode(row.positionSide, accountPositionMode, `${path}.positionSide`);
    mapPendingStatus(row.algoStatus, `${path}.algoStatus`);
    if (row.childOrders.length > 0) {
      row.childOrders.forEach((child, index) => visit(child, rootId, `${path}.childOrders[${index}]`));
      return;
    }
    if (row.algoType !== 'TAKE_PROFIT' && row.algoType !== 'STOP_LOSS') {
      deferred.push({ order: row, reason: `${row.algoType} has no lossless absolute-trigger safe-cex mapping` });
      return;
    }
    if (typeof row.quantity !== 'string') {
      deferred.push({ order: row, reason: 'official algo quantity number/string drift requires runtime evidence' });
      return;
    }
    if (row.triggerPrice === undefined) {
      deferred.push({ order: row, reason: `${row.algoType} is missing an absolute triggerPrice` });
      return;
    }
    const amount = decimalToNumber(row.quantity, `${path}.quantity`, Number.MIN_VALUE);
    const filled = decimalToNumber(row.totalExecutedQuantity, `${path}.totalExecutedQuantity`, 0);
    const remaining = subtract(amount, filled);
    if (remaining < 0) throw new Error(`${path}.totalExecutedQuantity must not exceed quantity`);
    const isStopLoss = row.algoType === 'STOP_LOSS';
    const triggerType = mapTriggerPriceType(row.triggerPriceType);
    orders.push(
      Object.freeze({
        id: String(row.algoOrderId),
        parentId: String(rootId),
        status: OrderStatus.Open,
        symbol: normalizePerpetualSymbol(row.symbol, `${path}.symbol`),
        type: isStopLoss ? OrderType.StopLoss : OrderType.TakeProfit,
        side: mapSide(row.side),
        price: decimalToNumber(row.triggerPrice, `${path}.triggerPrice`, Number.MIN_VALUE),
        amount,
        filled,
        remaining,
        reduceOnly: row.reduceOnly,
        ...(isStopLoss ? { slTriggerType: triggerType } : { tpTriggerType: triggerType }),
        lastUpdate: row.updatedTime,
        info: row,
      }),
    );
  };
  rows.forEach((row, index) => visit(row, row.rootAlgoOrderId, `WOO V3 pending algo order[${index}]`));
  return Object.freeze({ orders: Object.freeze(orders), deferred: Object.freeze(deferred) });
};

export const decodeWooV3Transactions = (input: unknown) =>
  decodePaginatedRows(input, 'transaction history', decodeTransaction);

export const mapWooV3Transactions = (
  rows: readonly WooV3Transaction[],
  accountPositionMode: WooV3AccountPositionMode,
): readonly WooV3MappedTransaction[] =>
  Object.freeze(
    rows.map((row, index) => {
      const path = `WOO V3 transaction[${index}]`;
      const symbol = normalizePerpetualSymbol(row.symbol, `${path}.symbol`);
      const side = mapSide(row.side);
      const price = decimalToNumber(row.executedPrice, `${path}.executedPrice`, Number.MIN_VALUE);
      const qty = decimalToNumber(row.executedQuantity, `${path}.executedQuantity`, Number.MIN_VALUE);
      const realizedPnl = row.realizedPnl === null ? undefined : decimalToNumber(row.realizedPnl, `${path}.realizedPnl`);
      const historicalTrade: HistoricalTrade | undefined =
        accountPositionMode === 'ONE_WAY'
          ? Object.freeze({
              id: String(row.id),
              symbol,
              orderId: String(row.orderId),
              price,
              qty,
              ...(realizedPnl === undefined ? {} : { realizedPnl }),
              positionSide: 'both' as const,
              side,
              maker: row.isMaker === 1,
              commission: row.fee,
              commissionAsset: row.feeAsset,
              time: row.executedTimestamp,
            })
          : undefined;
      return Object.freeze({
        execution: Object.freeze({
          id: String(row.id),
          timestamp: Math.floor(row.executedTimestamp / 1000),
          symbol,
          orderId: String(row.orderId),
          price,
          qty,
          side,
          commission: row.fee,
          commissionAsset: row.feeAsset,
        }),
        ...(historicalTrade === undefined
          ? {
              deferredHistoricalTradeReason:
                'WOO V3 transaction history omits positionSide required by hedge-mode HistoricalTrade',
            }
          : { historicalTrade }),
        maker: row.isMaker === 1,
        isMatchRpi: row.isMatchRpi,
      });
    }),
  );
