import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';

export interface DelegatedOrderBinding {
  accountName: string;
  instanceId?: string;
  intentId: string;
  seq: number;
}
export interface DelegatedOrderSubmitFrame {
  t: 'order.submit';
  d: DelegatedOrderBinding & { params: Record<string, unknown>; ageMs: number; fingerprint: string };
}
export interface DelegatedOrderAckFrame {
  t: 'order.ack';
  d: DelegatedOrderBinding & { received: boolean };
}
export interface DelegatedOrderOutcomeFrame {
  t: 'order.outcome';
  d: DelegatedOrderBinding & {
    outcomeSeq: number;
    outcome: 'placed' | 'refused' | 'unknown';
    venueOrderId: string | null;
    clientOrderId: string | null;
    /** `rejected`: the venue refused the whole order. Not notSent proof; the outbox keeps it unknown. */
    error: { message: string; outcome?: 'notSent' | 'rejected' | 'unknown'; elements?: DelegatedLegResult[] } | null;
  };
}
/**
 * One leg of an unknown multi-leg placement, as the box reports it (AGT-185). It only lets the
 * client say which legs landed; it never proves the intent's outcome, which stays unknown.
 */
export type DelegatedLegResult = { index: number; clientOrderId?: string } & (
  | { status: 'placed'; orderIds: string[] }
  | { status: 'applied' }
  | { status: 'notSent'; error?: { message: string } }
  | { status: 'unknown'; error: { message: string } }
);
const LEGS_MAX = 32;
const LEG_ID_MAX = 256;
const LEG_REASON_MAX = 160;
const LEGS_BYTES_MAX = 4 * 1024;

/** The box's per-leg results, or undefined when any leg is unreadable or out of bounds. */
export function delegatedLegResults(value: unknown): DelegatedLegResult[] | undefined {
  if (!Array.isArray(value) || value.length === 0 || value.length > LEGS_MAX) return undefined;
  const boundedId = (id: unknown): id is string =>
    typeof id === 'string' && id.length > 0 && utf8ToBytes(id).length <= LEG_ID_MAX;
  const reason = (error: unknown): { message: string } | undefined | null => {
    if (error === undefined) return undefined;
    if (typeof error !== 'object' || error === null || Array.isArray(error)) return null;
    const message = (error as Record<string, unknown>).message;
    return typeof message === 'string' && [...message].length <= LEG_REASON_MAX ? { message } : null;
  };
  const legs: DelegatedLegResult[] = [];
  for (const leg of value) {
    if (typeof leg !== 'object' || leg === null || Array.isArray(leg)) return undefined;
    const { index, clientOrderId, status, orderIds, error } = leg as Record<string, unknown>;
    if (typeof index !== 'number' || !Number.isSafeInteger(index) || index < 0) return undefined;
    if (clientOrderId !== undefined && !boundedId(clientOrderId)) return undefined;
    const identity = { index, ...(clientOrderId !== undefined ? { clientOrderId } : {}) };
    const why = reason(error);
    if (why === null) return undefined;
    if (status === 'placed') {
      if (!Array.isArray(orderIds) || orderIds.length > LEGS_MAX || !orderIds.every(boundedId)) return undefined;
      legs.push({ ...identity, status, orderIds: [...orderIds] });
    } else if (status === 'applied') legs.push({ ...identity, status });
    else if (status === 'notSent') legs.push({ ...identity, status, ...(why ? { error: why } : {}) });
    else if (status === 'unknown' && why) legs.push({ ...identity, status, error: why });
    else return undefined;
  }
  return utf8ToBytes(JSON.stringify(legs)).length <= LEGS_BYTES_MAX ? legs : undefined;
}

export interface DelegatedOrderCancelQueuedFrame {
  t: 'order.cancelQueued';
  d: DelegatedOrderBinding & { fingerprint: string };
}

export const DELEGATED_SUBMIT_MAX = 64 * 1024;
export const DELEGATED_ACK_MAX = 4 * 1024;
export const DELEGATED_OUTCOME_MAX = 8 * 1024;
const UUID7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

export function createDelegatedIntentId(unixMs: number, random: (bytes: Uint8Array) => void): string {
  if (!Number.isSafeInteger(unixMs) || unixMs < 0 || unixMs > 0xffffffffffff) throw new Error('Invalid intent time');
  const bytes = new Uint8Array(16);
  random(bytes);
  let timestamp = BigInt(unixMs);
  for (let i = 5; i >= 0; i--) {
    bytes[i] = Number(timestamp & 255n);
    timestamp >>= 8n;
  }
  bytes[6] = 0x70 | (bytes[6] & 15);
  bytes[8] = 0x80 | (bytes[8] & 63);
  const hex = bytesToHex(bytes);
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function delegatedIntentTime(id: string): number {
  if (!UUID7.test(id)) throw new Error('Invalid UUIDv7 intent key');
  return Number.parseInt(id.slice(0, 13).replace('-', ''), 16);
}

export function delegatedClientOrderId(id: string): string {
  delegatedIntentTime(id);
  return id;
}

/** Version-one canonical intent: ASCII keys, finite binary64 numbers, UTF-8 JSON. */
export function delegatedIntentFingerprint(
  accountName: string,
  intentId: string,
  params: unknown,
  seq: number,
): string {
  delegatedIntentTime(intentId);
  if (!Number.isSafeInteger(seq) || seq < 1) throw new Error('Invalid intent sequence');
  const canonical = canonicalValue({ accountName, intentId, params, seq }, 0);
  const bytes = utf8ToBytes(`tealstreet/delegated-order/v1\0${canonical}`);
  if (bytes.length > DELEGATED_SUBMIT_MAX) throw new Error('Intent parameters are too large');
  return bytesToHex(sha256(bytes));
}

export function canonicalValue(value: unknown, depth: number): string {
  if (depth > 32) throw new Error('Intent parameters are too deep');
  if (value === null || typeof value === 'boolean') return JSON.stringify(value);
  if (typeof value === 'number') {
    if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value)))
      throw new Error('Unsafe intent number');
    return JSON.stringify(value);
  }
  if (typeof value === 'string') {
    for (let i = 0; i < value.length; i++) {
      const c = value.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdbff) {
        const next = value.charCodeAt(++i);
        if (!(next >= 0xdc00 && next <= 0xdfff)) throw new Error('Invalid Unicode intent string');
      } else if (c >= 0xdc00 && c <= 0xdfff) throw new Error('Invalid Unicode intent string');
    }
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) return `[${value.map((v) => canonicalValue(v, depth + 1)).join(',')}]`;
  if (value && typeof value === 'object') {
    const fields = value as Record<string, unknown>;
    return `{${Object.keys(fields)
      .sort()
      .map((key) => {
        if ([...key].some((char) => char.charCodeAt(0) > 127)) throw new Error('Non-ASCII intent parameter key');
        return `${JSON.stringify(key)}:${canonicalValue(fields[key], depth + 1)}`;
      })
      .join(',')}}`;
  }
  throw new Error('Invalid intent parameter');
}
