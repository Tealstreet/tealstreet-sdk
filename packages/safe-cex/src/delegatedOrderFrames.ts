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
    error: { message: string; outcome?: 'notSent' | 'unknown' } | null;
  };
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
