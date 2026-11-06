/**
 * Inert order-write contracts for Coinbase's Deribit-powered Starbase gateway.
 *
 * The active Coinbase exchange does not import this module. It owns no
 * transport, credentials, routing, feature flag, or side effect. Builders
 * consume explicit total-base-amount intent because Coinbase's technical
 * migration guide says `amount` is base coin while the inherited OpenAPI text
 * still describes perpetual `amount` as USD. Contract sizing and size edits
 * stay typed parity blockers until Coinbase publishes a coherent conversion.
 */

import { OrderTimeInForce } from '../../../types';
import { CoinbaseStarbaseProtocolError } from './coinbase.starbase';
import {
  decodeCoinbaseStarbaseOrder,
  decodeCoinbaseStarbaseTrade,
  type CoinbaseStarbasePrivateOrder,
  type CoinbaseStarbasePrivateRequest,
  type CoinbaseStarbasePrivateTrade,
  type CoinbaseStarbaseTimeInForce as CoinbaseStarbaseGatewayTimeInForce,
  type CoinbaseStarbaseTriggerType,
} from './coinbase.starbase-private';
import { toCoinbaseStarbaseInstrumentName } from './coinbase.starbase-public';

export const COINBASE_STARBASE_BASE_AMOUNT_SOURCE = 'coinbase-technical-migration-guide' as const;

const STARBASE_WRITE_ORDER_TYPES = [
  'limit',
  'stop_limit',
  'take_limit',
  'market',
  'stop_market',
  'take_market',
  'market_limit',
  'trailing_stop',
] as const;
const STARBASE_TRIGGER_FILL_CONDITIONS = ['first_hit', 'complete_fill', 'incremental'] as const;

export type CoinbaseStarbaseWriteOrderType = (typeof STARBASE_WRITE_ORDER_TYPES)[number];
export type CoinbaseStarbaseLinkedOrderType = 'one_triggers_one_cancels_other';
export type CoinbaseStarbaseTriggerFillCondition = (typeof STARBASE_TRIGGER_FILL_CONDITIONS)[number];
export type CoinbaseStarbaseWriteSide = 'buy' | 'sell';

export type CoinbaseStarbaseBaseAmountIntent = {
  /** Total requested order size, never remaining or filled size. */
  totalBaseAmount: number;
  source: typeof COINBASE_STARBASE_BASE_AMOUNT_SOURCE;
  unit: 'base';
};

export type CoinbaseStarbaseContractAmountIntent = {
  /** Total requested contract count; conversion is intentionally not implemented. */
  totalContracts: number;
  source: 'gateway-contract-count';
  unit: 'contracts';
};

export type CoinbaseStarbaseWriteAmountIntent = CoinbaseStarbaseBaseAmountIntent | CoinbaseStarbaseContractAmountIntent;

export type CoinbaseStarbaseWriteParityBlocker =
  | 'contract-unit-conversion-unverified'
  | 'edit-total-vs-remaining-amount-unresolved'
  | 'market-limit-response-schema-missing'
  | 'response-filled-amount-unit-conflict'
  | 'response-trade-amount-unit-conflict'
  | 'valid-until-unit-unpublished';

export type CoinbaseStarbaseWriteGate<T, Intent> =
  | { status: 'ready'; value: T }
  | {
      status: 'blocked';
      blocker: CoinbaseStarbaseWriteParityBlocker;
      intent: Intent;
    };

export const coinbaseStarbaseTotalBaseAmount = (totalBaseAmount: number): CoinbaseStarbaseBaseAmountIntent => {
  requirePositive(totalBaseAmount, 'Starbase total base amount');
  return {
    totalBaseAmount,
    source: COINBASE_STARBASE_BASE_AMOUNT_SOURCE,
    unit: 'base',
  };
};

export const coinbaseStarbaseTotalContracts = (totalContracts: number): CoinbaseStarbaseContractAmountIntent => {
  requirePositive(totalContracts, 'Starbase total contract count');
  return {
    totalContracts,
    source: 'gateway-contract-count',
    unit: 'contracts',
  };
};

export type CoinbaseStarbaseUnresolvedValidUntilIntent = {
  gatewayIntegerTimestamp: number;
  unit: 'upstream-unspecified';
};

export const coinbaseStarbaseUnresolvedValidUntil = (
  gatewayIntegerTimestamp: number,
): CoinbaseStarbaseUnresolvedValidUntilIntent => {
  if (!Number.isSafeInteger(gatewayIntegerTimestamp)) {
    return protocolError('Invalid Starbase valid_until: expected a safe integer');
  }
  return { gatewayIntegerTimestamp, unit: 'upstream-unspecified' };
};

export type CoinbaseStarbaseTifParams = {
  post_only: boolean;
  time_in_force: CoinbaseStarbaseGatewayTimeInForce;
};

export const mapCoinbaseStarbaseTimeInForce = (timeInForce: OrderTimeInForce): CoinbaseStarbaseTifParams => {
  if (timeInForce === OrderTimeInForce.GoodTillCancel) {
    return { post_only: false, time_in_force: 'good_til_cancelled' };
  }
  if (timeInForce === OrderTimeInForce.ImmediateOrCancel) {
    return { post_only: false, time_in_force: 'immediate_or_cancel' };
  }
  if (timeInForce === OrderTimeInForce.FillOrKill) {
    return { post_only: false, time_in_force: 'fill_or_kill' };
  }
  if (timeInForce === OrderTimeInForce.PostOnly) {
    return { post_only: true, time_in_force: 'good_til_cancelled' };
  }
  return protocolError('Unsupported safe-cex time in force for Starbase');
};

type CoinbaseStarbaseOtocoLegIntent = {
  direction: CoinbaseStarbaseWriteSide;
  label?: string;
  postOnly?: boolean;
  price?: number;
  reduceOnly: true;
  rejectPostOnly?: boolean;
  size: CoinbaseStarbaseBaseAmountIntent;
  timeInForce?: OrderTimeInForce;
  trigger: CoinbaseStarbaseTriggerType;
  triggerPrice?: number;
  type: 'take_limit' | 'take_market' | 'stop_limit' | 'stop_market';
};

export type CoinbaseStarbaseOtocoIntent = {
  legs: readonly [CoinbaseStarbaseOtocoLegIntent, CoinbaseStarbaseOtocoLegIntent];
  linkedOrderType: 'one_triggers_one_cancels_other';
  triggerFillCondition?: CoinbaseStarbaseTriggerFillCondition;
};

export type CoinbaseStarbasePlaceOrderIntent = {
  label?: string;
  otoco?: CoinbaseStarbaseOtocoIntent;
  postOnly?: boolean;
  price?: number;
  reduceOnly?: boolean;
  rejectPostOnly?: boolean;
  savedSymbol: string;
  side: CoinbaseStarbaseWriteSide;
  size: CoinbaseStarbaseWriteAmountIntent;
  timeInForce?: OrderTimeInForce;
  trigger?: CoinbaseStarbaseTriggerType;
  triggerOffset?: number;
  triggerPrice?: number;
  type: CoinbaseStarbaseWriteOrderType;
  validUntil?: CoinbaseStarbaseUnresolvedValidUntilIntent;
};

type CoinbaseStarbaseOtocoParams = {
  amount: number;
  direction: CoinbaseStarbaseWriteSide;
  label?: string;
  post_only?: boolean;
  price?: number;
  reduce_only: true;
  reject_post_only?: boolean;
  time_in_force?: CoinbaseStarbaseGatewayTimeInForce;
  trigger: CoinbaseStarbaseTriggerType;
  trigger_price?: number;
  type: CoinbaseStarbaseOtocoLegIntent['type'];
};

export type CoinbaseStarbasePlaceOrderParams = {
  amount: number;
  instrument_name: string;
  label?: string;
  linked_order_type?: CoinbaseStarbaseLinkedOrderType;
  otoco_config?: readonly [CoinbaseStarbaseOtocoParams, CoinbaseStarbaseOtocoParams];
  post_only?: boolean;
  price?: number;
  reduce_only?: boolean;
  reject_post_only?: boolean;
  time_in_force?: CoinbaseStarbaseGatewayTimeInForce;
  trigger?: CoinbaseStarbaseTriggerType;
  trigger_fill_condition?: CoinbaseStarbaseTriggerFillCondition;
  trigger_offset?: number;
  trigger_price?: number;
  type: CoinbaseStarbaseWriteOrderType;
};

export type CoinbaseStarbasePlaceOrderRequest = CoinbaseStarbasePrivateRequest<
  `private/${CoinbaseStarbaseWriteSide}`,
  CoinbaseStarbasePlaceOrderParams
>;

export type CoinbaseStarbaseSideOrderIntent = Omit<CoinbaseStarbasePlaceOrderIntent, 'side'>;

export const buildCoinbaseStarbaseBuy = (
  intent: CoinbaseStarbaseSideOrderIntent,
): CoinbaseStarbaseWriteGate<CoinbaseStarbasePlaceOrderRequest, CoinbaseStarbasePlaceOrderIntent> =>
  buildCoinbaseStarbasePlaceOrder({ ...intent, side: 'buy' });

export const buildCoinbaseStarbaseSell = (
  intent: CoinbaseStarbaseSideOrderIntent,
): CoinbaseStarbaseWriteGate<CoinbaseStarbasePlaceOrderRequest, CoinbaseStarbasePlaceOrderIntent> =>
  buildCoinbaseStarbasePlaceOrder({ ...intent, side: 'sell' });

export const buildCoinbaseStarbasePlaceOrder = (
  intent: CoinbaseStarbasePlaceOrderIntent,
): CoinbaseStarbaseWriteGate<CoinbaseStarbasePlaceOrderRequest, CoinbaseStarbasePlaceOrderIntent> => {
  if (intent.size.unit === 'contracts') {
    return {
      status: 'blocked',
      blocker: 'contract-unit-conversion-unverified',
      intent,
    };
  }
  if (intent.type === 'market_limit') {
    return {
      status: 'blocked',
      blocker: 'market-limit-response-schema-missing',
      intent,
    };
  }
  if (intent.validUntil !== undefined) {
    return {
      status: 'blocked',
      blocker: 'valid-until-unit-unpublished',
      intent,
    };
  }
  if (intent.size.source !== COINBASE_STARBASE_BASE_AMOUNT_SOURCE) {
    return protocolError('Invalid Starbase base amount source');
  }
  requirePositive(intent.size.totalBaseAmount, 'Starbase total base amount');
  parseEnum(intent.side, ['buy', 'sell'] as const, 'Starbase order side');
  parseEnum(intent.type, STARBASE_WRITE_ORDER_TYPES, 'Starbase order type');
  validateLabel(intent.label);
  validateOrderShape(intent);

  const tif = intent.timeInForce === undefined ? undefined : mapCoinbaseStarbaseTimeInForce(intent.timeInForce);
  const postOnly = intent.postOnly ?? tif?.post_only;
  if (tif?.post_only && intent.postOnly === false) {
    return protocolError('Starbase PostOnly time in force conflicts with post_only false');
  }
  if (postOnly && tif && tif.time_in_force !== 'good_til_cancelled') {
    return protocolError('Starbase post-only requires good_til_cancelled');
  }
  if (intent.rejectPostOnly && !postOnly) {
    return protocolError('Starbase reject_post_only requires post_only');
  }

  const otoco = intent.otoco === undefined ? undefined : buildOtoco(intent.otoco);
  return {
    status: 'ready',
    value: {
      method: `private/${intent.side}`,
      params: {
        amount: intent.size.totalBaseAmount,
        instrument_name: toCoinbaseStarbaseInstrumentName(intent.savedSymbol),
        type: intent.type,
        ...(intent.label === undefined ? {} : { label: intent.label }),
        ...(intent.price === undefined ? {} : { price: requirePositive(intent.price, 'Starbase order price') }),
        ...(tif === undefined ? {} : { time_in_force: tif.time_in_force }),
        ...(postOnly === undefined ? {} : { post_only: postOnly }),
        ...(intent.rejectPostOnly === undefined ? {} : { reject_post_only: intent.rejectPostOnly }),
        ...(intent.reduceOnly === undefined ? {} : { reduce_only: intent.reduceOnly }),
        ...(intent.trigger === undefined ? {} : { trigger: intent.trigger }),
        ...(intent.triggerPrice === undefined
          ? {}
          : {
              trigger_price: requirePositive(intent.triggerPrice, 'Starbase order trigger_price'),
            }),
        ...(intent.triggerOffset === undefined
          ? {}
          : {
              trigger_offset: requirePositive(intent.triggerOffset, 'Starbase order trigger_offset'),
            }),
        ...(otoco === undefined
          ? {}
          : {
              linked_order_type: intent.otoco!.linkedOrderType,
              otoco_config: otoco,
              ...(intent.otoco!.triggerFillCondition === undefined
                ? {}
                : {
                    trigger_fill_condition: intent.otoco!.triggerFillCondition,
                  }),
            }),
      },
    },
  };
};

export type CoinbaseStarbaseEditIntent = {
  orderId: string;
  price?: number;
  postOnly?: boolean;
  reduceOnly?: boolean;
  rejectPostOnly?: boolean;
  size?: CoinbaseStarbaseWriteAmountIntent;
  triggerOffset?: number;
  triggerPrice?: number;
  validUntil?: CoinbaseStarbaseUnresolvedValidUntilIntent;
};

export type CoinbaseStarbaseEditParams = {
  order_id: string;
  post_only?: boolean;
  price?: number;
  reduce_only?: boolean;
  reject_post_only?: boolean;
  trigger_offset?: number;
  trigger_price?: number;
};

export type CoinbaseStarbaseEditRequest = CoinbaseStarbasePrivateRequest<'private/edit', CoinbaseStarbaseEditParams>;

export const buildCoinbaseStarbaseEdit = (
  intent: CoinbaseStarbaseEditIntent,
): CoinbaseStarbaseWriteGate<CoinbaseStarbaseEditRequest, CoinbaseStarbaseEditIntent> => {
  const orderId = requireString(intent.orderId, 'Starbase edit order_id');
  if (intent.size?.unit === 'contracts') {
    return {
      status: 'blocked',
      blocker: 'contract-unit-conversion-unverified',
      intent,
    };
  }
  if (intent.size) {
    return {
      status: 'blocked',
      blocker: 'edit-total-vs-remaining-amount-unresolved',
      intent,
    };
  }
  if (intent.validUntil !== undefined) {
    return {
      status: 'blocked',
      blocker: 'valid-until-unit-unpublished',
      intent,
    };
  }
  if (intent.rejectPostOnly && intent.postOnly !== true) {
    return protocolError('Starbase edit reject_post_only requires post_only');
  }

  const params: CoinbaseStarbaseEditParams = {
    order_id: orderId,
    ...(intent.price === undefined ? {} : { price: requirePositive(intent.price, 'Starbase edit price') }),
    ...(intent.postOnly === undefined ? {} : { post_only: intent.postOnly }),
    ...(intent.reduceOnly === undefined ? {} : { reduce_only: intent.reduceOnly }),
    ...(intent.rejectPostOnly === undefined ? {} : { reject_post_only: intent.rejectPostOnly }),
    ...(intent.triggerPrice === undefined
      ? {}
      : {
          trigger_price: requirePositive(intent.triggerPrice, 'Starbase edit trigger_price'),
        }),
    ...(intent.triggerOffset === undefined
      ? {}
      : {
          trigger_offset: requirePositive(intent.triggerOffset, 'Starbase edit trigger_offset'),
        }),
  };
  if (Object.keys(params).length === 1) return protocolError('Starbase edit requires at least one change');

  return { status: 'ready', value: { method: 'private/edit', params } };
};

export const buildCoinbaseStarbaseCancel = (
  orderId: string,
): CoinbaseStarbasePrivateRequest<'private/cancel', { order_id: string }> => ({
  method: 'private/cancel',
  params: { order_id: requireString(orderId, 'Starbase cancel order_id') },
});

export type CoinbaseStarbaseClosePositionIntent = {
  price?: number;
  savedSymbol: string;
  type: 'limit' | 'market';
};

export const buildCoinbaseStarbaseClosePosition = (
  intent: CoinbaseStarbaseClosePositionIntent,
): CoinbaseStarbasePrivateRequest<
  'private/close_position',
  { instrument_name: string; price?: number; type: 'limit' | 'market' }
> => {
  if (intent.type === 'limit' && intent.price === undefined) {
    return protocolError('Starbase limit close requires price');
  }
  if (intent.type === 'market' && intent.price !== undefined) {
    return protocolError('Starbase market close must not include price');
  }
  return {
    method: 'private/close_position',
    params: {
      instrument_name: toCoinbaseStarbaseInstrumentName(intent.savedSymbol),
      type: intent.type,
      ...(intent.price === undefined ? {} : { price: requirePositive(intent.price, 'Starbase close price') }),
    },
  };
};

export type CoinbaseStarbaseOrderIdentity = {
  /** Exchange-issued order_id is the only canonical write identity. */
  canonicalOrderId: string;
  /** Display/correlation metadata only; Coinbase explicitly says labels are non-unique. */
  nonCanonicalLabel: string;
  /** Present on a triggered order and points to its pre-trigger order ID. */
  preTriggerOrderId?: string;
  /** Published IDs for the OTOCO children attached to this primary order. */
  otocoChildOrderIds: string[];
  /** Present on an OTOCO child and points to its primary order. */
  otocoPrimaryOrderId?: string;
};

export type CoinbaseStarbaseWriteResult = {
  identity: CoinbaseStarbaseOrderIdentity;
  order: CoinbaseStarbasePrivateOrder;
  trades: CoinbaseStarbasePrivateTrade[];
};

export const decodeCoinbaseStarbaseWriteResult = (value: unknown): CoinbaseStarbaseWriteResult => {
  const raw = requireRecord(value, 'Starbase write result');
  const orderRaw = requireRecord(raw.order, 'Starbase write result order');
  const tradesRaw = requireArray(raw.trades, 'Starbase write result trades');
  const order = decodeCoinbaseStarbaseOrder(orderRaw);
  const trades = tradesRaw.map(decodeCoinbaseStarbaseTrade);
  assertUnique(
    trades.map((trade) => trade.id),
    'Starbase write trade_id',
  );
  return { order, trades, identity: decodeOrderIdentity(orderRaw, order) };
};

export const decodeCoinbaseStarbaseCancelResult = (value: unknown): CoinbaseStarbaseWriteResult => {
  const orderRaw = requireRecord(value, 'Starbase cancel result');
  const order = decodeCoinbaseStarbaseOrder(orderRaw);
  return { order, trades: [], identity: decodeOrderIdentity(orderRaw, order) };
};

export const assessCoinbaseStarbaseWriteResultParity = (
  result: CoinbaseStarbaseWriteResult,
): CoinbaseStarbaseWriteGate<CoinbaseStarbaseWriteResult, CoinbaseStarbaseWriteResult> => {
  if (result.trades.length > 0) {
    return {
      status: 'blocked',
      blocker: 'response-trade-amount-unit-conflict',
      intent: result,
    };
  }
  if ((result.order.gatewayFilledAmount ?? 0) > 0) {
    return {
      status: 'blocked',
      blocker: 'response-filled-amount-unit-conflict',
      intent: result,
    };
  }
  return { status: 'ready', value: result };
};

const buildOtoco = (
  intent: CoinbaseStarbaseOtocoIntent,
): readonly [CoinbaseStarbaseOtocoParams, CoinbaseStarbaseOtocoParams] => {
  if (intent.linkedOrderType !== 'one_triggers_one_cancels_other') {
    return protocolError('Unsupported Starbase linked order type');
  }
  if (
    intent.triggerFillCondition !== undefined &&
    !STARBASE_TRIGGER_FILL_CONDITIONS.includes(intent.triggerFillCondition)
  ) {
    return protocolError('Unsupported Starbase trigger fill condition');
  }
  const kinds = new Set(intent.legs.map((leg) => (leg.type.startsWith('take_') ? 'take' : 'stop')));
  if (kinds.size !== 2) return protocolError('Starbase OTOCO requires one take-profit and one stop-loss leg');
  return [buildOtocoLeg(intent.legs[0]), buildOtocoLeg(intent.legs[1])];
};

const buildOtocoLeg = (leg: CoinbaseStarbaseOtocoLegIntent): CoinbaseStarbaseOtocoParams => {
  validateLabel(leg.label);
  if (leg.size.unit !== 'base' || leg.size.source !== COINBASE_STARBASE_BASE_AMOUNT_SOURCE) {
    return protocolError('Invalid Starbase OTOCO base amount source');
  }
  requirePositive(leg.size.totalBaseAmount, 'Starbase OTOCO total base amount');
  validateOtocoLegShape(leg);
  const tif = leg.timeInForce === undefined ? undefined : mapCoinbaseStarbaseTimeInForce(leg.timeInForce);
  const postOnly = leg.postOnly ?? tif?.post_only;
  if (tif?.post_only && leg.postOnly === false) {
    return protocolError('Starbase OTOCO PostOnly time in force conflicts with post_only false');
  }
  if (leg.rejectPostOnly && !postOnly) return protocolError('Starbase OTOCO reject_post_only requires post_only');
  return {
    amount: leg.size.totalBaseAmount,
    direction: leg.direction,
    reduce_only: true,
    trigger: leg.trigger,
    type: leg.type,
    ...(leg.label === undefined ? {} : { label: leg.label }),
    ...(leg.price === undefined ? {} : { price: requirePositive(leg.price, 'Starbase OTOCO price') }),
    ...(tif === undefined ? {} : { time_in_force: tif.time_in_force }),
    ...(postOnly === undefined ? {} : { post_only: postOnly }),
    ...(leg.rejectPostOnly === undefined ? {} : { reject_post_only: leg.rejectPostOnly }),
    ...(leg.triggerPrice === undefined
      ? {}
      : {
          trigger_price: requirePositive(leg.triggerPrice, 'Starbase OTOCO trigger_price'),
        }),
  };
};

const validateOtocoLegShape = (leg: CoinbaseStarbaseOtocoLegIntent): void => {
  if (leg.trigger === undefined) protocolError(`Starbase OTOCO ${leg.type} requires trigger`);
  if (leg.type.endsWith('_limit') && leg.price === undefined) {
    protocolError(`Starbase OTOCO ${leg.type} requires price`);
  }
  if (leg.type.endsWith('_market') && leg.price !== undefined) {
    protocolError(`Starbase OTOCO ${leg.type} must not include price`);
  }
  // Coinbase's technical migration guide publishes a take_limit OTOCO leg
  // with price + trigger but no trigger_price. Stop legs and take_market still
  // need the OpenAPI trigger_price field; do not manufacture one for take_limit.
  if (leg.type !== 'take_limit' && leg.triggerPrice === undefined) {
    protocolError(`Starbase OTOCO ${leg.type} requires trigger_price`);
  }
};

const validateOrderShape = (intent: {
  price?: number;
  trigger?: CoinbaseStarbaseTriggerType;
  triggerOffset?: number;
  triggerPrice?: number;
  type: CoinbaseStarbaseWriteOrderType;
}): void => {
  const requiresPrice = ['limit', 'stop_limit', 'take_limit'].includes(intent.type);
  const forbidsPrice = ['market', 'stop_market', 'take_market', 'trailing_stop'].includes(intent.type);
  if (requiresPrice && intent.price === undefined) protocolError(`Starbase ${intent.type} requires price`);
  if (forbidsPrice && intent.price !== undefined) protocolError(`Starbase ${intent.type} must not include price`);
  const triggerOrder = ['stop_limit', 'take_limit', 'stop_market', 'take_market', 'trailing_stop'].includes(
    intent.type,
  );
  if (triggerOrder && intent.trigger === undefined) protocolError(`Starbase ${intent.type} requires trigger`);
  if (!triggerOrder && intent.trigger !== undefined) protocolError(`Starbase ${intent.type} must not include trigger`);
  if (intent.type === 'trailing_stop') {
    if (intent.triggerOffset === undefined) protocolError('Starbase trailing_stop requires trigger_offset');
    if (intent.triggerPrice !== undefined) protocolError('Starbase trailing_stop must not include trigger_price');
  } else if (triggerOrder && intent.triggerPrice === undefined) {
    protocolError(`Starbase ${intent.type} requires trigger_price`);
  }
};

const decodeOrderIdentity = (
  raw: Record<string, unknown>,
  order: CoinbaseStarbasePrivateOrder,
): CoinbaseStarbaseOrderIdentity => ({
  canonicalOrderId: order.id,
  nonCanonicalLabel: order.label,
  ...(raw.trigger_order_id === undefined
    ? {}
    : {
        preTriggerOrderId: requireString(raw.trigger_order_id, 'Starbase order trigger_order_id'),
      }),
  otocoChildOrderIds: decodeStringArray(raw.oto_order_ids, 'Starbase order oto_order_ids'),
  ...(raw.primary_order_id === undefined
    ? {}
    : {
        otocoPrimaryOrderId: requireString(raw.primary_order_id, 'Starbase order primary_order_id'),
      }),
});

const decodeStringArray = (value: unknown, context: string): string[] => {
  if (value === undefined) return [];
  const result = requireArray(value, context).map((entry) => requireString(entry, context));
  assertUnique(result, context);
  return result;
};

const validateLabel = (label: string | undefined): void => {
  if (label === undefined) return;
  if (typeof label !== 'string' || label.length > 64) protocolError('Starbase label must be at most 64 characters');
};

const parseEnum = <T extends string>(value: unknown, values: readonly T[], context: string): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) protocolError(`Invalid ${context}`);
  return value as T;
};

const requireRecord = (value: unknown, context: string): Record<string, unknown> => {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    protocolError(`Invalid ${context}: expected an object`);
  }
  return value as Record<string, unknown>;
};

const requireArray = (value: unknown, context: string): unknown[] => {
  if (!Array.isArray(value)) protocolError(`Invalid ${context}: expected an array`);
  return value as unknown[];
};

const requireString = (value: unknown, context: string): string => {
  if (typeof value !== 'string' || !value) protocolError(`Invalid ${context}: expected a non-empty string`);
  return value as string;
};

const requirePositive = (value: unknown, context: string): number => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value <= 0) {
    protocolError(`Invalid ${context}: expected a positive finite number`);
  }
  return value as number;
};

const assertUnique = (values: readonly string[], context: string): void => {
  if (new Set(values).size !== values.length) protocolError(`Duplicate ${context}`);
};

const protocolError = (message: string): never => {
  throw new CoinbaseStarbaseProtocolError(message);
};
