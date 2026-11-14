import type { DelegatedAlgoAction } from './delegatedAlgoActions';
import type { DelegatedOrderBinding } from './delegatedOrderFrames';
import type { DelegatedOrderParams } from './delegatedWriteParams';

import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';

import { isDelegatedAlgoAction } from './delegatedAlgoActions';
import { canonicalValue, DELEGATED_SUBMIT_MAX, delegatedIntentTime } from './delegatedOrderFrames';
import { delegatedNumber, isDelegatedOrderParams } from './delegatedWriteParams';

export type DelegatedAction =
  | DelegatedAlgoAction
  | { kind: 'amend'; order: { id: string; symbol: string }; update: { price?: number; amount?: number } }
  | { kind: 'cancel'; order: { id: string; symbol: string; type?: DelegatedOrderParams['type'] } }
  | { kind: 'cancelAll'; symbols?: string[] }
  | { kind: 'nuke' }
  | { kind: 'place'; orders: DelegatedOrderParams[] }
  | { kind: 'cancelOrders'; orders: { id: string; symbol: string; type?: DelegatedOrderParams['type'] }[] }
  | { kind: 'cancelSymbol'; symbol: string }
  | { kind: 'setLeverage'; symbol: string; leverage: number; side?: 'long' | 'short' }
  | { kind: 'setAllLeverage'; leverage: number }
  | { kind: 'setMarginType'; isolated: boolean; symbol?: string }
  | { kind: 'setPositionMode'; hedged: boolean; symbol?: string }
  | { kind: 'close'; order: DelegatedOrderParams & { reduceOnly: true } }
  | { kind: 'closeAll' | 'nukeOrders' | 'stopAlgos' | 'cancelAccountOrders' }
  | { kind: 'scheduleCancel'; time: number | null }
  | { kind: 'setStpMode'; mode: 'EXPIRE_TAKER' | 'EXPIRE_MAKER' | 'EXPIRE_BOTH' }
  | { kind: 'setAccountTier'; tier: 'standard' | 'premium' | 'plus' };
export type { DelegatedOrderParams } from './delegatedWriteParams';
export type DelegatedActionResult =
  | { kind: 'amend' | 'close'; orderIds: string[] }
  | { kind: 'place'; orderIds: string[]; protectionApplied?: true }
  | { kind: Exclude<DelegatedAction['kind'], 'amend' | 'place' | 'close' | 'algo'> };

/** Old action ledgers attest only the original four kinds. */
export const isDelegatedFullWriteAction = (action: DelegatedAction): boolean =>
  !['amend', 'cancel', 'cancelAll', 'nuke'].includes(action.kind) ||
  (action.kind === 'cancel' && action.order.type !== undefined) ||
  (action.kind === 'cancelAll' && action.symbols !== undefined) ||
  (action.kind === 'amend' && action.update.amount === 0);
export type DelegatedActionOutcome =
  | { outcome: 'applied'; result: DelegatedActionResult; error: null }
  | { outcome: 'unsupported'; result: null; error: { message: string; outcome: 'unsupported' } }
  | { outcome: 'refused'; result: null; error: { message: string; outcome: 'notSent' } }
  | { outcome: 'unknown'; result: null; error: { message: string; outcome: 'unknown' } };

const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value);
const keys = (value: Record<string, unknown>, allowed: string[]) =>
  Object.keys(value).every((k) => allowed.includes(k));
const target = (value: unknown): value is string =>
  typeof value === 'string' && value.length > 0 && utf8ToBytes(value).length <= 256;

export function isDelegatedAction(value: unknown): value is DelegatedAction {
  if (!record(value)) return false;
  if (value.kind === 'algo') return isDelegatedAlgoAction(value);
  if (value.kind === 'nuke') return keys(value, ['kind']);
  if (value.kind === 'cancelAll')
    return (
      keys(value, ['kind', 'symbols']) &&
      (!('symbols' in value) ||
        (Array.isArray(value.symbols) && value.symbols.length <= 32 && value.symbols.every(target)))
    );
  const optionalSymbol = () => !('symbol' in value) || target(value.symbol);
  switch (value.kind) {
    case 'place':
      return (
        keys(value, ['kind', 'orders']) &&
        Array.isArray(value.orders) &&
        value.orders.length > 0 &&
        value.orders.length <= 32 &&
        value.orders.every((v) => isDelegatedOrderParams(v))
      );
    case 'close':
      return keys(value, ['kind', 'order']) && isDelegatedOrderParams(value.order, true);
    case 'cancelOrders':
      return (
        keys(value, ['kind', 'orders']) &&
        Array.isArray(value.orders) &&
        value.orders.length > 0 &&
        value.orders.length <= 32 &&
        value.orders.every(
          (v) =>
            record(v) &&
            keys(v, ['id', 'symbol', 'type']) &&
            target(v.id) &&
            target(v.symbol) &&
            (!('type' in v) ||
              (typeof v.type === 'string' &&
                [
                  'market',
                  'limit',
                  'conditional',
                  'stop_market',
                  'take_profit_market',
                  'trailing_stop_market',
                ].includes(v.type))),
        )
      );
    case 'cancelSymbol':
      return keys(value, ['kind', 'symbol']) && target(value.symbol);
    case 'setLeverage':
      return (
        keys(value, ['kind', 'symbol', 'leverage', 'side']) &&
        target(value.symbol) &&
        delegatedNumber(value.leverage) &&
        value.leverage > 0 &&
        (!('side' in value) || value.side === 'long' || value.side === 'short')
      );
    case 'setAllLeverage':
      return keys(value, ['kind', 'leverage']) && delegatedNumber(value.leverage) && value.leverage > 0;
    case 'setMarginType':
      return keys(value, ['kind', 'isolated', 'symbol']) && typeof value.isolated === 'boolean' && optionalSymbol();
    case 'setPositionMode':
      return keys(value, ['kind', 'hedged', 'symbol']) && typeof value.hedged === 'boolean' && optionalSymbol();
    case 'closeAll':
    case 'nukeOrders':
    case 'stopAlgos':
    case 'cancelAccountOrders':
      return keys(value, ['kind']);
    case 'scheduleCancel':
      return (
        keys(value, ['kind', 'time']) &&
        (value.time === null || (delegatedNumber(value.time) && Number.isSafeInteger(value.time)))
      );
    case 'setStpMode':
      return (
        keys(value, ['kind', 'mode']) &&
        typeof value.mode === 'string' &&
        ['EXPIRE_TAKER', 'EXPIRE_MAKER', 'EXPIRE_BOTH'].includes(value.mode)
      );
    case 'setAccountTier':
      return (
        keys(value, ['kind', 'tier']) &&
        typeof value.tier === 'string' &&
        ['standard', 'premium', 'plus'].includes(value.tier)
      );
  }
  if (value.kind !== 'amend' && value.kind !== 'cancel') return false;
  if (!keys(value, value.kind === 'amend' ? ['kind', 'order', 'update'] : ['kind', 'order'])) return false;
  if (
    !record(value.order) ||
    !keys(value.order, value.kind === 'cancel' ? ['id', 'symbol', 'type'] : ['id', 'symbol']) ||
    !target(value.order.id) ||
    !target(value.order.symbol)
  )
    return false;
  if (value.kind === 'cancel')
    return (
      !('type' in value.order) ||
      (typeof value.order.type === 'string' &&
        ['market', 'limit', 'conditional', 'stop_market', 'take_profit_market', 'trailing_stop_market'].includes(
          value.order.type,
        ))
    );
  const update = value.update;
  return (
    record(update) &&
    keys(update, ['price', 'amount']) &&
    Object.keys(update).length > 0 &&
    Object.entries(update).every(
      ([key, n]) => typeof n === 'number' && Number.isFinite(n) && (key === 'amount' ? n >= 0 : n > 0),
    )
  );
}

export function isDelegatedActionResult(
  value: unknown,
  kind: DelegatedAction['kind'],
  action?: DelegatedAction,
): value is DelegatedActionResult {
  if (!record(value) || value.kind !== kind || kind === 'algo') return false;
  if (kind !== 'amend' && kind !== 'place' && kind !== 'close') return keys(value, ['kind']);
  if (kind === 'place' && value.protectionApplied === true)
    return (
      keys(value, ['kind', 'orderIds', 'protectionApplied']) &&
      Array.isArray(value.orderIds) &&
      value.orderIds.length === 0 &&
      action?.kind === 'place' &&
      action.orders.length > 0 &&
      action.orders.every(
        (order) =>
          order.reduceOnly === true &&
          ['stop_market', 'take_profit_market', 'trailing_stop_market'].includes(order.type),
      )
    );
  return (
    keys(value, ['kind', 'orderIds']) &&
    Array.isArray(value.orderIds) &&
    value.orderIds.length > 0 &&
    value.orderIds.length <= 32 &&
    value.orderIds.every(target)
  );
}

export function delegatedActionFingerprint(
  accountName: string,
  intentId: string,
  action: DelegatedAction,
  seq: number,
): string {
  delegatedIntentTime(intentId);
  if (!isDelegatedAction(action) || !Number.isSafeInteger(seq) || seq < 1) throw new Error('Invalid delegated action');
  const canonical = canonicalValue({ accountName, action, intentId, seq, version: 1 }, 0);
  const bytes = utf8ToBytes(`tealstreet/delegated-action/v1\0${canonical}`);
  if (bytes.length > DELEGATED_SUBMIT_MAX) throw new Error('Action is too large');
  return bytesToHex(sha256(bytes));
}

export interface DelegatedActionRequestFrame {
  t: 'order.action';
  d: DelegatedOrderBinding & { version: 1; ageMs: number; fingerprint: string; action: DelegatedAction };
}
export interface DelegatedActionAckFrame {
  t: 'order.actionAck';
  d: DelegatedOrderBinding & { version: 1; received: boolean };
}
export interface DelegatedActionOutcomeFrame {
  t: 'order.actionOutcome';
  d: DelegatedOrderBinding & { version: 1; outcomeSeq: number } & DelegatedActionOutcome;
}
export interface DelegatedActionCancelQueuedFrame {
  t: 'order.actionCancelQueued';
  d: DelegatedOrderBinding & { version: 1; fingerprint: string };
}
