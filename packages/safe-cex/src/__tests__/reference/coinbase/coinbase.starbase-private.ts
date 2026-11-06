/**
 * Inert read-only private-state contracts for Coinbase's Starbase gateway.
 *
 * This module deliberately separates strict gateway DTO decoding from
 * safe-cex normalization. Coinbase's migration guide says `amount` is base
 * coin, while inherited OpenAPI descriptions still describe perpetual
 * `amount`/`filled_amount` as USD. Raw values therefore remain explicitly
 * gateway-labelled and unsafe conversions return a typed parity gate.
 */

import type { HistoricalTrade, Order } from '../../../types';

import {
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
} from '../../../types';
import { CoinbaseStarbaseProtocolError } from './coinbase.starbase';
import {
  toCoinbaseLegacyIntxSymbol,
  toCoinbaseStarbaseInstrumentName,
} from './coinbase.starbase-public';

const STARBASE_MARGIN_MODELS = ['cross_pm', 'cross_sm', 'segregated_pm', 'segregated_sm'] as const;
const STARBASE_ORDER_DIRECTIONS = ['buy', 'sell'] as const;
const STARBASE_ORDER_STATES = ['open', 'filled', 'rejected', 'cancelled', 'untriggered', 'triggered'] as const;
const STARBASE_ORDER_TYPES = [
  'market',
  'limit',
  'stop_market',
  'stop_limit',
  'take_market',
  'take_limit',
  'trailing_stop',
] as const;
const STARBASE_TIME_IN_FORCE = [
  'good_til_cancelled',
  'good_til_day',
  'fill_or_kill',
  'immediate_or_cancel',
] as const;
const STARBASE_TRADE_STATES = ['open', 'filled', 'rejected', 'cancelled', 'untriggered', 'archive'] as const;
const STARBASE_TRADE_TYPES = ['limit', 'market', 'liquidation'] as const;
const STARBASE_TRIGGER_TYPES = ['index_price', 'mark_price', 'last_price'] as const;
const STARBASE_FEE_CURRENCIES = ['BTC', 'ETH', 'USDC', 'USDT', 'EURR'] as const;

export type CoinbaseStarbaseMarginModel = (typeof STARBASE_MARGIN_MODELS)[number];
export type CoinbaseStarbaseOrderDirection = (typeof STARBASE_ORDER_DIRECTIONS)[number];
export type CoinbaseStarbaseOrderState = (typeof STARBASE_ORDER_STATES)[number];
export type CoinbaseStarbaseOrderType = (typeof STARBASE_ORDER_TYPES)[number];
export type CoinbaseStarbaseTimeInForce = (typeof STARBASE_TIME_IN_FORCE)[number];
export type CoinbaseStarbaseTradeState = (typeof STARBASE_TRADE_STATES)[number];
export type CoinbaseStarbaseTradeType = (typeof STARBASE_TRADE_TYPES)[number];
export type CoinbaseStarbaseTriggerType = (typeof STARBASE_TRIGGER_TYPES)[number];
export type CoinbaseStarbaseFeeCurrency = (typeof STARBASE_FEE_CURRENCIES)[number];

export type CoinbaseStarbasePrivateRequest<Method extends string, Params extends Record<string, unknown>> = {
  method: Method;
  params: Params;
};

export type CoinbaseStarbaseAccountSummary = {
  currency: 'USDC';
  balanceUsdc: number;
  equityUsdc: number;
  availableFundsUsdc: number;
  availableWithdrawalFundsUsdc: number;
  initialMarginUsdc: number;
  maintenanceMarginUsdc: number;
  marginBalanceUsdc?: number;
  totalEquityUsd?: number;
  totalInitialMarginUsd?: number;
  totalMaintenanceMarginUsd?: number;
  totalMarginBalanceUsd?: number;
  crossCollateralEnabled?: boolean;
  portfolioMarginingEnabled?: boolean;
  marginModel?: CoinbaseStarbaseMarginModel;
  gatewayFuturesPnl: number;
  gatewayFuturesSessionRealizedPnl: number;
  gatewayFuturesSessionUnrealizedPnl: number;
  gatewaySessionRealizedPnl: number;
  gatewaySessionUnrealizedPnl: number;
  gatewayTotalPnl: number;
};

export type CoinbaseStarbasePrivatePosition = {
  instrumentName: string;
  legacySavedSymbol: string;
  direction: 'buy' | 'sell' | 'zero';
  baseAmount: number;
  gatewaySizeUsd: number;
  entryPriceUsd: number;
  markPriceUsd: number;
  indexPriceUsd: number;
  currentNotionalUsd: number;
  leverage?: number;
  liquidationPriceUsd: number;
  gatewayInitialMargin: number;
  gatewayMaintenanceMargin: number;
  gatewayOpenOrdersMargin: number;
  gatewayFloatingPnl: number;
  gatewayRealizedPnl: number;
  gatewayTotalPnl: number;
};

export type CoinbaseStarbasePrivateOrder = {
  id: string;
  instrumentName: string;
  legacySavedSymbol: string;
  state: CoinbaseStarbaseOrderState;
  type: CoinbaseStarbaseOrderType;
  timeInForce: CoinbaseStarbaseTimeInForce;
  direction: CoinbaseStarbaseOrderDirection;
  price: number | 'market_price';
  averagePrice?: number;
  label: string;
  postOnly: boolean;
  reduceOnly?: boolean;
  creationTimestampMs: number;
  lastUpdateTimestampMs: number;
  gatewayAmount?: number;
  gatewayFilledAmount?: number;
  gatewayContracts?: number;
  trigger?: CoinbaseStarbaseTriggerType;
  triggerPrice?: number;
  triggerOrderId?: string;
};

export type CoinbaseStarbasePrivateTrade = {
  id: string;
  sequence: number;
  orderId: string;
  instrumentName: string;
  legacySavedSymbol: string;
  timestampMs: number;
  direction: CoinbaseStarbaseOrderDirection;
  type: CoinbaseStarbaseTradeType;
  state: CoinbaseStarbaseTradeState;
  priceUsd: number;
  markPriceUsd: number;
  indexPriceUsd: number;
  gatewayAmount: number;
  gatewayContracts?: number;
  gatewayProfitLoss?: number;
  fee: number;
  feeCurrency: CoinbaseStarbaseFeeCurrency;
  liquidity?: 'M' | 'T';
  label?: string;
};

export type CoinbaseStarbaseTradePage = {
  hasMore: boolean;
  trades: CoinbaseStarbasePrivateTrade[];
};

export type CoinbaseStarbaseOrderHistoryOptions = {
  count?: number;
  offset?: number;
  includeOld?: boolean;
  includeUnfilled?: boolean;
  historical?: boolean;
};

export type CoinbaseStarbaseOrderHistoryPageParams = {
  count: number;
  offset?: number;
  include_old?: boolean;
  include_unfilled?: boolean;
  historical?: boolean;
};

export type CoinbaseStarbaseParityBlocker =
  | 'account-pnl-unit-unpublished'
  | 'filled-amount-unit-conflict'
  | 'gateway-amount-missing'
  | 'gateway-filled-amount-missing'
  | 'good-til-day-unsupported'
  | 'position-margin-pnl-units-unpublished'
  | 'reduce-only-missing'
  | 'stop-limit-shape-unsupported'
  | 'take-limit-shape-unsupported'
  | 'trade-amount-unit-conflict'
  | 'trigger-missing';

export type CoinbaseStarbaseParityResult<T, Gateway> =
  | { status: 'ready'; value: T }
  | { status: 'blocked'; gateway: Gateway; blockers: CoinbaseStarbaseParityBlocker[] };

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const protocolError = (message: string): never => {
  throw new CoinbaseStarbaseProtocolError(message);
};

const requireRecord = (value: unknown, context: string): Record<string, unknown> => {
  if (!isRecord(value)) protocolError(`Invalid ${context}: expected an object`);
  return value as Record<string, unknown>;
};

const requireArray = (value: unknown, context: string): unknown[] => {
  if (!Array.isArray(value)) protocolError(`Invalid ${context}: expected an array`);
  return value as unknown[];
};

const requireString = (value: unknown, context: string, allowEmpty = false): string => {
  if (typeof value !== 'string' || (!allowEmpty && !value)) {
    protocolError(`Invalid ${context}: expected ${allowEmpty ? 'a string' : 'a non-empty string'}`);
  }
  return value as string;
};

const optionalString = (value: unknown, context: string): string | undefined =>
  value === undefined ? undefined : requireString(value, context, true);

const requireFinite = (value: unknown, context: string): number => {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    protocolError(`Invalid ${context}: expected a finite number`);
  }
  return value as number;
};

const requireNonNegative = (value: unknown, context: string): number => {
  const parsed = requireFinite(value, context);
  if (parsed < 0) protocolError(`Invalid ${context}: expected a non-negative number`);
  return parsed;
};

const requirePositive = (value: unknown, context: string): number => {
  const parsed = requireFinite(value, context);
  if (parsed <= 0) protocolError(`Invalid ${context}: expected a positive number`);
  return parsed;
};

const requireTimestamp = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || Number(value) < 0) {
    protocolError(`Invalid ${context}: expected a non-negative safe integer timestamp`);
  }
  return Number(value);
};

const requirePositiveInteger = (value: unknown, context: string): number => {
  if (!Number.isSafeInteger(value) || Number(value) <= 0) {
    protocolError(`Invalid ${context}: expected a positive safe integer`);
  }
  return Number(value);
};

const optionalBoolean = (value: unknown, context: string): boolean | undefined => {
  if (value === undefined) return undefined;
  if (typeof value !== 'boolean') protocolError(`Invalid ${context}: expected a boolean`);
  return value as boolean;
};

const parseEnum = <T extends string>(value: unknown, values: readonly T[], context: string): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) protocolError(`Invalid ${context}: unknown enum value`);
  return value as T;
};

const parseStarbaseInstrument = (value: unknown, context: string) => {
  const instrumentName = requireString(value, context);
  return {
    instrumentName,
    legacySavedSymbol: toCoinbaseLegacyIntxSymbol(instrumentName),
  };
};

const assertUnique = <T>(values: readonly T[], id: (value: T) => string, context: string): void => {
  const ids = new Set<string>();
  for (const value of values) {
    const key = id(value);
    if (ids.has(key)) protocolError(`Duplicate ${context}: ${key}`);
    ids.add(key);
  }
};

const validateCount = (count: number, context: string): number => {
  if (!Number.isSafeInteger(count) || count < 1 || count > 1000) {
    protocolError(`Starbase ${context} count must be an integer from 1 to 1000`);
  }
  return count;
};

const buildOrderHistoryPageParams = (
  options: CoinbaseStarbaseOrderHistoryOptions,
): CoinbaseStarbaseOrderHistoryPageParams => {
  const params: CoinbaseStarbaseOrderHistoryPageParams = {
    count: validateCount(options.count ?? 100, 'order-history'),
  };
  if (options.offset !== undefined) {
    if (!Number.isSafeInteger(options.offset) || options.offset < 0) {
      protocolError('Starbase order-history offset must be a non-negative safe integer');
    }
    params.offset = options.offset;
  }
  if (options.includeOld !== undefined) {
    params.include_old = optionalBoolean(options.includeOld, 'Starbase order-history include_old');
  }
  if (options.includeUnfilled !== undefined) {
    params.include_unfilled = optionalBoolean(options.includeUnfilled, 'Starbase order-history include_unfilled');
  }
  if (options.historical !== undefined) {
    params.historical = optionalBoolean(options.historical, 'Starbase order-history historical');
  }
  return params;
};

export const buildCoinbaseStarbaseAccountSummary = (): CoinbaseStarbasePrivateRequest<
  'private/get_account_summary',
  { currency: 'USDC'; extended: false }
> => ({
  method: 'private/get_account_summary',
  params: { currency: 'USDC', extended: false },
});

export const buildCoinbaseStarbasePositions = (): CoinbaseStarbasePrivateRequest<
  'private/get_positions',
  { currency: 'USDC'; kind: 'future' }
> => ({
  method: 'private/get_positions',
  params: { currency: 'USDC', kind: 'future' },
});

export const buildCoinbaseStarbaseOpenOrders = (): CoinbaseStarbasePrivateRequest<
  'private/get_open_orders_by_currency',
  { currency: 'USDC'; kind: 'future'; type: 'all' }
> => ({
  method: 'private/get_open_orders_by_currency',
  params: { currency: 'USDC', kind: 'future', type: 'all' },
});

export const buildCoinbaseStarbaseOrderState = (
  orderId: string,
): CoinbaseStarbasePrivateRequest<'private/get_order_state', { order_id: string }> => ({
  method: 'private/get_order_state',
  params: { order_id: requireString(orderId, 'Starbase order-state order_id') },
});

export const buildCoinbaseStarbaseOrderHistoryByCurrency = (
  options: CoinbaseStarbaseOrderHistoryOptions = {},
): CoinbaseStarbasePrivateRequest<
  'private/get_order_history_by_currency',
  CoinbaseStarbaseOrderHistoryPageParams & { currency: 'USDC'; kind: 'future' }
> => ({
  method: 'private/get_order_history_by_currency',
  params: {
    currency: 'USDC',
    kind: 'future',
    ...buildOrderHistoryPageParams(options),
  },
});

export const buildCoinbaseStarbaseOrderHistoryByInstrument = (
  symbol: string,
  options: CoinbaseStarbaseOrderHistoryOptions = {},
): CoinbaseStarbasePrivateRequest<
  'private/get_order_history_by_instrument',
  CoinbaseStarbaseOrderHistoryPageParams & { instrument_name: string }
> => ({
  method: 'private/get_order_history_by_instrument',
  params: {
    instrument_name: toCoinbaseStarbaseInstrumentName(symbol),
    ...buildOrderHistoryPageParams(options),
  },
});

export const buildCoinbaseStarbaseUserTrades = (
  count = 100,
): CoinbaseStarbasePrivateRequest<
  'private/get_user_trades_by_currency',
  { count: number; currency: 'USDC'; historical: false; kind: 'future'; sorting: 'desc' }
> => ({
  method: 'private/get_user_trades_by_currency',
  params: {
    count: validateCount(count, 'user-trade'),
    currency: 'USDC',
    historical: false,
    kind: 'future',
    sorting: 'desc',
  },
});

export const buildCoinbaseStarbaseUserTradesByTime = (
  startTimestampMs: number,
  endTimestampMs: number,
  count = 100,
): CoinbaseStarbasePrivateRequest<
  'private/get_user_trades_by_currency_and_time',
  {
    count: number;
    currency: 'USDC';
    end_timestamp: number;
    historical: false;
    kind: 'future';
    sorting: 'asc';
    start_timestamp: number;
  }
> => {
  const start = requireTimestamp(startTimestampMs, 'Starbase trades start_timestamp');
  const end = requireTimestamp(endTimestampMs, 'Starbase trades end_timestamp');
  if (end < start) protocolError('Starbase trades end_timestamp precedes start_timestamp');
  return {
    method: 'private/get_user_trades_by_currency_and_time',
    params: {
      count: validateCount(count, 'user-trade'),
      currency: 'USDC',
      end_timestamp: end,
      historical: false,
      kind: 'future',
      sorting: 'asc',
      start_timestamp: start,
    },
  };
};

export const decodeCoinbaseStarbaseAccountSummary = (value: unknown): CoinbaseStarbaseAccountSummary => {
  const raw = requireRecord(value, 'Starbase account summary');
  if (raw.currency !== 'USDC') protocolError('Unsupported Starbase account-summary currency');

  const marginModel =
    raw.margin_model === undefined
      ? undefined
      : parseEnum(raw.margin_model, STARBASE_MARGIN_MODELS, 'Starbase account margin_model');

  return {
    currency: 'USDC',
    balanceUsdc: requireFinite(raw.balance, 'Starbase account balance'),
    equityUsdc: requireFinite(raw.equity, 'Starbase account equity'),
    availableFundsUsdc: requireFinite(raw.available_funds, 'Starbase account available_funds'),
    availableWithdrawalFundsUsdc: requireFinite(
      raw.available_withdrawal_funds,
      'Starbase account available_withdrawal_funds',
    ),
    initialMarginUsdc: requireNonNegative(raw.initial_margin, 'Starbase account initial_margin'),
    maintenanceMarginUsdc: requireNonNegative(raw.maintenance_margin, 'Starbase account maintenance_margin'),
    ...(raw.margin_balance === undefined
      ? {}
      : { marginBalanceUsdc: requireFinite(raw.margin_balance, 'Starbase account margin_balance') }),
    ...(raw.total_equity_usd === undefined
      ? {}
      : { totalEquityUsd: requireFinite(raw.total_equity_usd, 'Starbase account total_equity_usd') }),
    ...(raw.total_initial_margin_usd === undefined
      ? {}
      : {
          totalInitialMarginUsd: requireNonNegative(
            raw.total_initial_margin_usd,
            'Starbase account total_initial_margin_usd',
          ),
        }),
    ...(raw.total_maintenance_margin_usd === undefined
      ? {}
      : {
          totalMaintenanceMarginUsd: requireNonNegative(
            raw.total_maintenance_margin_usd,
            'Starbase account total_maintenance_margin_usd',
          ),
        }),
    ...(raw.total_margin_balance_usd === undefined
      ? {}
      : {
          totalMarginBalanceUsd: requireFinite(
            raw.total_margin_balance_usd,
            'Starbase account total_margin_balance_usd',
          ),
        }),
    ...(raw.cross_collateral_enabled === undefined
      ? {}
      : {
          crossCollateralEnabled: optionalBoolean(
            raw.cross_collateral_enabled,
            'Starbase account cross_collateral_enabled',
          )!,
        }),
    ...(raw.portfolio_margining_enabled === undefined
      ? {}
      : {
          portfolioMarginingEnabled: optionalBoolean(
            raw.portfolio_margining_enabled,
            'Starbase account portfolio_margining_enabled',
          )!,
        }),
    ...(marginModel === undefined ? {} : { marginModel }),
    gatewayFuturesPnl: requireFinite(raw.futures_pl, 'Starbase account futures_pl'),
    gatewayFuturesSessionRealizedPnl: requireFinite(
      raw.futures_session_rpl,
      'Starbase account futures_session_rpl',
    ),
    gatewayFuturesSessionUnrealizedPnl: requireFinite(
      raw.futures_session_upl,
      'Starbase account futures_session_upl',
    ),
    gatewaySessionRealizedPnl: requireFinite(raw.session_rpl, 'Starbase account session_rpl'),
    gatewaySessionUnrealizedPnl: requireFinite(raw.session_upl, 'Starbase account session_upl'),
    gatewayTotalPnl: requireFinite(raw.total_pl, 'Starbase account total_pl'),
  };
};

export const assessCoinbaseStarbaseBalanceParity = (
  account: CoinbaseStarbaseAccountSummary,
): CoinbaseStarbaseParityResult<never, CoinbaseStarbaseAccountSummary> => ({
  status: 'blocked',
  gateway: account,
  blockers: ['account-pnl-unit-unpublished'],
});

export const decodeCoinbaseStarbasePosition = (value: unknown): CoinbaseStarbasePrivatePosition => {
  const raw = requireRecord(value, 'Starbase position');
  if (raw.kind !== 'future') protocolError('Unsupported Starbase position kind');
  const identity = parseStarbaseInstrument(raw.instrument_name, 'Starbase position instrument_name');
  const direction = parseEnum(raw.direction, ['buy', 'sell', 'zero'] as const, 'Starbase position direction');
  const baseAmount = requireFinite(raw.size_currency, 'Starbase position size_currency');
  const gatewaySizeUsd = requireFinite(raw.size, 'Starbase position size');
  if (direction === 'zero' && (baseAmount !== 0 || gatewaySizeUsd !== 0)) {
    protocolError('Invalid Starbase zero position with non-zero size');
  }
  if (direction !== 'zero' && (baseAmount === 0 || gatewaySizeUsd === 0)) {
    protocolError('Invalid Starbase non-zero position with zero size');
  }

  const markPriceUsd = requirePositive(raw.mark_price, 'Starbase position mark_price');
  const leverage =
    raw.leverage === undefined ? undefined : requirePositiveInteger(raw.leverage, 'Starbase position leverage');

  return {
    ...identity,
    direction,
    baseAmount: Math.abs(baseAmount),
    gatewaySizeUsd: Math.abs(gatewaySizeUsd),
    entryPriceUsd: requireNonNegative(raw.average_price, 'Starbase position average_price'),
    markPriceUsd,
    indexPriceUsd: requirePositive(raw.index_price, 'Starbase position index_price'),
    currentNotionalUsd: Math.abs(baseAmount) * markPriceUsd,
    ...(leverage === undefined ? {} : { leverage }),
    liquidationPriceUsd: requireNonNegative(
      raw.estimated_liquidation_price,
      'Starbase position estimated_liquidation_price',
    ),
    gatewayInitialMargin: requireNonNegative(raw.initial_margin, 'Starbase position initial_margin'),
    gatewayMaintenanceMargin: requireNonNegative(raw.maintenance_margin, 'Starbase position maintenance_margin'),
    gatewayOpenOrdersMargin: requireNonNegative(raw.open_orders_margin, 'Starbase position open_orders_margin'),
    gatewayFloatingPnl: requireFinite(raw.floating_profit_loss, 'Starbase position floating_profit_loss'),
    gatewayRealizedPnl: requireFinite(raw.realized_profit_loss, 'Starbase position realized_profit_loss'),
    gatewayTotalPnl: requireFinite(raw.total_profit_loss, 'Starbase position total_profit_loss'),
  };
};

export const decodeCoinbaseStarbasePositions = (value: unknown): CoinbaseStarbasePrivatePosition[] => {
  const positions = requireArray(value, 'private/get_positions result').map(decodeCoinbaseStarbasePosition);
  assertUnique(positions, (position) => position.instrumentName, 'Starbase position instrument_name');
  return positions;
};

export const assessCoinbaseStarbasePositionParity = (
  position: CoinbaseStarbasePrivatePosition,
): CoinbaseStarbaseParityResult<never, CoinbaseStarbasePrivatePosition> => ({
  status: 'blocked',
  gateway: position,
  blockers: ['position-margin-pnl-units-unpublished'],
});

const parseOrderPrice = (value: unknown): number | 'market_price' => {
  if (value === 'market_price') return value;
  return requireNonNegative(value, 'Starbase order price');
};

export const decodeCoinbaseStarbaseOrder = (value: unknown): CoinbaseStarbasePrivateOrder => {
  const raw = requireRecord(value, 'Starbase order');
  const identity = parseStarbaseInstrument(raw.instrument_name, 'Starbase order instrument_name');
  const trigger =
    raw.trigger === undefined
      ? undefined
      : parseEnum(raw.trigger, STARBASE_TRIGGER_TYPES, 'Starbase order trigger');

  if (typeof raw.post_only !== 'boolean') protocolError('Invalid Starbase order post_only: expected a boolean');

  return {
    id: requireString(raw.order_id, 'Starbase order order_id'),
    ...identity,
    state: parseEnum(raw.order_state, STARBASE_ORDER_STATES, 'Starbase order order_state'),
    type: parseEnum(raw.order_type, STARBASE_ORDER_TYPES, 'Starbase order order_type'),
    timeInForce: parseEnum(raw.time_in_force, STARBASE_TIME_IN_FORCE, 'Starbase order time_in_force'),
    direction: parseEnum(raw.direction, STARBASE_ORDER_DIRECTIONS, 'Starbase order direction'),
    price: parseOrderPrice(raw.price),
    ...(raw.average_price === undefined
      ? {}
      : { averagePrice: requireNonNegative(raw.average_price, 'Starbase order average_price') }),
    label: requireString(raw.label, 'Starbase order label', true),
    postOnly: optionalBoolean(raw.post_only, 'Starbase order post_only')!,
    ...(raw.reduce_only === undefined
      ? {}
      : { reduceOnly: optionalBoolean(raw.reduce_only, 'Starbase order reduce_only')! }),
    creationTimestampMs: requireTimestamp(raw.creation_timestamp, 'Starbase order creation_timestamp'),
    lastUpdateTimestampMs: requireTimestamp(raw.last_update_timestamp, 'Starbase order last_update_timestamp'),
    ...(raw.amount === undefined ? {} : { gatewayAmount: requireNonNegative(raw.amount, 'Starbase order amount') }),
    ...(raw.filled_amount === undefined
      ? {}
      : { gatewayFilledAmount: requireNonNegative(raw.filled_amount, 'Starbase order filled_amount') }),
    ...(raw.contracts === undefined
      ? {}
      : { gatewayContracts: requireNonNegative(raw.contracts, 'Starbase order contracts') }),
    ...(trigger === undefined ? {} : { trigger }),
    ...(raw.trigger_price === undefined
      ? {}
      : { triggerPrice: requirePositive(raw.trigger_price, 'Starbase order trigger_price') }),
    ...(raw.trigger_order_id === undefined
      ? {}
      : { triggerOrderId: requireString(raw.trigger_order_id, 'Starbase order trigger_order_id') }),
  };
};

export const decodeCoinbaseStarbaseOpenOrders = (value: unknown): CoinbaseStarbasePrivateOrder[] => {
  const orders = requireArray(value, 'private/get_open_orders result').map(decodeCoinbaseStarbaseOrder);
  assertUnique(orders, (order) => order.id, 'Starbase order_id');
  return orders;
};

export const decodeCoinbaseStarbaseOrderState = (
  value: unknown,
  expectedOrderId: string,
): CoinbaseStarbasePrivateOrder => {
  const expected = requireString(expectedOrderId, 'Starbase order-state expected order_id');
  const order = decodeCoinbaseStarbaseOrder(value);
  if (order.id !== expected) {
    protocolError(`Starbase order-state order_id mismatch: expected ${expected}, received ${order.id}`);
  }
  return order;
};

export const decodeCoinbaseStarbaseOrderHistory = (value: unknown): CoinbaseStarbasePrivateOrder[] => {
  const orders = requireArray(value, 'private/get_order_history result').map(decodeCoinbaseStarbaseOrder);
  assertUnique(orders, (order) => order.id, 'Starbase order-history order_id');
  return orders;
};

export const decodeCoinbaseStarbaseOrderHistoryByInstrument = (
  value: unknown,
  expectedSymbol: string,
): CoinbaseStarbasePrivateOrder[] => {
  const expectedInstrumentName = toCoinbaseStarbaseInstrumentName(expectedSymbol);
  const orders = decodeCoinbaseStarbaseOrderHistory(value);
  for (const order of orders) {
    if (order.instrumentName !== expectedInstrumentName) {
      protocolError(
        `Starbase order-history instrument mismatch: expected ${expectedInstrumentName}, received ${order.instrumentName}`,
      );
    }
  }
  return orders;
};

const mapOrderStatus = (state: CoinbaseStarbaseOrderState): OrderStatus => {
  if (state === 'filled') return OrderStatus.Closed;
  if (state === 'cancelled' || state === 'rejected') return OrderStatus.Canceled;
  return OrderStatus.Open;
};

const mapOrderSide = (direction: CoinbaseStarbaseOrderDirection): OrderSide =>
  direction === 'buy' ? OrderSide.Buy : OrderSide.Sell;

const mapOrderType = (type: CoinbaseStarbaseOrderType): OrderType | undefined => {
  if (type === 'market') return OrderType.Market;
  if (type === 'limit') return OrderType.Limit;
  if (type === 'stop_market') return OrderType.StopLoss;
  if (type === 'take_market') return OrderType.TakeProfit;
  if (type === 'trailing_stop') return OrderType.TrailingStopLoss;
  return undefined;
};

const mapTimeInForce = (value: CoinbaseStarbaseTimeInForce, postOnly: boolean): OrderTimeInForce | undefined => {
  if (postOnly) return OrderTimeInForce.PostOnly;
  if (value === 'good_til_cancelled') return OrderTimeInForce.GoodTillCancel;
  if (value === 'fill_or_kill') return OrderTimeInForce.FillOrKill;
  if (value === 'immediate_or_cancel') return OrderTimeInForce.ImmediateOrCancel;
  return undefined;
};

const mapTrigger = (value: CoinbaseStarbaseTriggerType): OrderStopTriggerType => {
  if (value === 'index_price') return OrderStopTriggerType.IndexPrice;
  if (value === 'mark_price') return OrderStopTriggerType.MarkPrice;
  return OrderStopTriggerType.LastPrice;
};

export const normalizeCoinbaseStarbaseOrder = (
  gateway: CoinbaseStarbasePrivateOrder,
): CoinbaseStarbaseParityResult<Order, CoinbaseStarbasePrivateOrder> => {
  const blockers: CoinbaseStarbaseParityBlocker[] = [];
  if (gateway.gatewayAmount === undefined) blockers.push('gateway-amount-missing');
  if (gateway.gatewayFilledAmount === undefined) blockers.push('gateway-filled-amount-missing');
  if (gateway.gatewayFilledAmount !== undefined && gateway.gatewayFilledAmount !== 0) {
    blockers.push('filled-amount-unit-conflict');
  }
  if (gateway.reduceOnly === undefined) blockers.push('reduce-only-missing');
  if (gateway.type === 'stop_limit') blockers.push('stop-limit-shape-unsupported');
  if (gateway.type === 'take_limit') blockers.push('take-limit-shape-unsupported');
  if (gateway.timeInForce === 'good_til_day') blockers.push('good-til-day-unsupported');
  if (
    ['stop_market', 'stop_limit', 'take_market', 'take_limit', 'trailing_stop'].includes(gateway.type) &&
    gateway.trigger === undefined
  ) {
    blockers.push('trigger-missing');
  }
  if (blockers.length > 0) return { status: 'blocked', gateway, blockers };

  const type = mapOrderType(gateway.type)!;
  const timeInForce = mapTimeInForce(gateway.timeInForce, gateway.postOnly)!;
  const triggerType = gateway.trigger === undefined ? undefined : mapTrigger(gateway.trigger);
  const amount = gateway.gatewayAmount!;
  const order: Order = {
    id: gateway.id,
    status: mapOrderStatus(gateway.state),
    symbol: gateway.legacySavedSymbol,
    type,
    side: mapOrderSide(gateway.direction),
    price: gateway.price === 'market_price' ? 0 : gateway.price,
    amount,
    filled: 0,
    remaining: amount,
    reduceOnly: gateway.reduceOnly!,
    timeInForce,
    lastUpdate: gateway.lastUpdateTimestampMs,
    ...(type === OrderType.TakeProfit && triggerType ? { tpTriggerType: triggerType } : {}),
    ...(type === OrderType.StopLoss && triggerType ? { slTriggerType: triggerType } : {}),
    ...(type === OrderType.TakeProfit && gateway.triggerPrice !== undefined
      ? { takeProfit: gateway.triggerPrice }
      : {}),
    ...(type === OrderType.StopLoss && gateway.triggerPrice !== undefined ? { stopLoss: gateway.triggerPrice } : {}),
    info: gateway,
  };
  return { status: 'ready', value: order };
};

export const decodeCoinbaseStarbaseTrade = (value: unknown): CoinbaseStarbasePrivateTrade => {
  const raw = requireRecord(value, 'Starbase user trade');
  const identity = parseStarbaseInstrument(raw.instrument_name, 'Starbase user trade instrument_name');
  if (raw.matching_id !== null) protocolError('Invalid Starbase user trade matching_id: expected null');

  const liquidity =
    raw.liquidity === undefined
      ? undefined
      : parseEnum(raw.liquidity, ['M', 'T'] as const, 'Starbase user trade liquidity');
  const type = parseEnum(raw.order_type, STARBASE_TRADE_TYPES, 'Starbase user trade order_type');
  const feeCurrency = parseEnum(raw.fee_currency, STARBASE_FEE_CURRENCIES, 'Starbase user trade fee_currency');

  return {
    id: requireString(raw.trade_id, 'Starbase user trade trade_id'),
    sequence: requirePositiveInteger(raw.trade_seq, 'Starbase user trade trade_seq'),
    orderId: requireString(raw.order_id, 'Starbase user trade order_id'),
    ...identity,
    timestampMs: requireTimestamp(raw.timestamp, 'Starbase user trade timestamp'),
    direction: parseEnum(raw.direction, STARBASE_ORDER_DIRECTIONS, 'Starbase user trade direction'),
    type,
    state: parseEnum(raw.state, STARBASE_TRADE_STATES, 'Starbase user trade state'),
    priceUsd: requirePositive(raw.price, 'Starbase user trade price'),
    markPriceUsd: requirePositive(raw.mark_price, 'Starbase user trade mark_price'),
    indexPriceUsd: requirePositive(raw.index_price, 'Starbase user trade index_price'),
    gatewayAmount: requirePositive(raw.amount, 'Starbase user trade amount'),
    ...(raw.contracts === undefined
      ? {}
      : { gatewayContracts: requirePositive(raw.contracts, 'Starbase user trade contracts') }),
    ...(raw.profit_loss === undefined
      ? {}
      : { gatewayProfitLoss: requireFinite(raw.profit_loss, 'Starbase user trade profit_loss') }),
    fee: requireFinite(raw.fee, 'Starbase user trade fee'),
    feeCurrency,
    ...(liquidity === undefined ? {} : { liquidity }),
    ...(raw.label === undefined ? {} : { label: optionalString(raw.label, 'Starbase user trade label')! }),
  };
};

export const decodeCoinbaseStarbaseTradePage = (value: unknown): CoinbaseStarbaseTradePage => {
  const raw = requireRecord(value, 'private/get_user_trades result');
  const hasMore = optionalBoolean(raw.has_more, 'Starbase user trades has_more');
  if (hasMore === undefined) protocolError('Invalid Starbase user trades has_more');
  const trades = requireArray(raw.trades, 'Starbase user trades trades').map(decodeCoinbaseStarbaseTrade);
  assertUnique(trades, (trade) => trade.id, 'Starbase trade_id');
  return { hasMore: hasMore!, trades };
};

export const normalizeCoinbaseStarbaseTrade = (
  gateway: CoinbaseStarbasePrivateTrade,
): CoinbaseStarbaseParityResult<HistoricalTrade, CoinbaseStarbasePrivateTrade> => ({
  status: 'blocked',
  gateway,
  blockers: ['trade-amount-unit-conflict'],
});

export class CoinbaseStarbaseReadOnlyState {
  account?: CoinbaseStarbaseAccountSummary;
  positions: CoinbaseStarbasePrivatePosition[] = [];
  orders: CoinbaseStarbasePrivateOrder[] = [];
  orderState?: CoinbaseStarbasePrivateOrder;
  orderHistory: CoinbaseStarbasePrivateOrder[] = [];
  tradePage: CoinbaseStarbaseTradePage = { hasMore: false, trades: [] };

  applyAccountSummary(value: unknown): CoinbaseStarbaseAccountSummary {
    const next = decodeCoinbaseStarbaseAccountSummary(value);
    this.account = next;
    return next;
  }

  applyPositions(value: unknown): CoinbaseStarbasePrivatePosition[] {
    const next = decodeCoinbaseStarbasePositions(value);
    this.positions = next;
    return next;
  }

  applyOpenOrders(value: unknown): CoinbaseStarbasePrivateOrder[] {
    const next = decodeCoinbaseStarbaseOpenOrders(value);
    this.orders = next;
    return next;
  }

  applyOrderState(orderId: string, value: unknown): CoinbaseStarbasePrivateOrder {
    const next = decodeCoinbaseStarbaseOrderState(value, orderId);
    this.orderState = next;
    return next;
  }

  applyOrderHistory(value: unknown): CoinbaseStarbasePrivateOrder[] {
    const next = decodeCoinbaseStarbaseOrderHistory(value);
    this.orderHistory = next;
    return next;
  }

  applyOrderHistoryByInstrument(symbol: string, value: unknown): CoinbaseStarbasePrivateOrder[] {
    const next = decodeCoinbaseStarbaseOrderHistoryByInstrument(value, symbol);
    this.orderHistory = next;
    return next;
  }

  applyTradePage(value: unknown): CoinbaseStarbaseTradePage {
    const next = decodeCoinbaseStarbaseTradePage(value);
    this.tradePage = next;
    return next;
  }
}
