import type { DelegatedOrderBinding } from './delegatedOrderFrames';

import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';

import { canonicalValue, DELEGATED_SUBMIT_MAX, delegatedIntentTime } from './delegatedOrderFrames';

export type DelegatedAction =
  | { kind: 'amend'; order: { id: string; symbol: string }; update: { price?: number; amount?: number } }
  | { kind: 'cancel'; order: { id: string; symbol: string } }
  | { kind: 'cancelAll' }
  | { kind: 'nuke' };
export type DelegatedActionResult = { kind: 'amend'; orderIds: string[] } | { kind: 'cancel' | 'cancelAll' | 'nuke' };
export type DelegatedActionOutcome =
  | { outcome: 'applied'; result: DelegatedActionResult; error: null }
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
  if (value.kind === 'cancelAll' || value.kind === 'nuke') return keys(value, ['kind']);
  if (value.kind !== 'amend' && value.kind !== 'cancel') return false;
  if (!keys(value, value.kind === 'amend' ? ['kind', 'order', 'update'] : ['kind', 'order'])) return false;
  if (
    !record(value.order) ||
    !keys(value.order, ['id', 'symbol']) ||
    !target(value.order.id) ||
    !target(value.order.symbol)
  )
    return false;
  if (value.kind === 'cancel') return true;
  const update = value.update;
  return (
    record(update) &&
    keys(update, ['price', 'amount']) &&
    Object.keys(update).length > 0 &&
    Object.values(update).every((n) => typeof n === 'number' && Number.isFinite(n) && n > 0)
  );
}

export function isDelegatedActionResult(value: unknown, kind: DelegatedAction['kind']): value is DelegatedActionResult {
  if (!record(value) || value.kind !== kind) return false;
  if (kind !== 'amend') return keys(value, ['kind']);
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
