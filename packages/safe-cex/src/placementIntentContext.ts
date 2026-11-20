import type { PlacementIntentIdentity } from './placementIntentLedger';

import { sha256 } from '@noble/hashes/sha2.js';
import { utf8ToBytes } from '@noble/hashes/utils.js';

import { createDelegatedIntentId, delegatedIntentTime } from './delegatedOrderFrames';
import { PlacementIntentError } from './placementIntentLedger';

export const PLACEMENT_INTENT_METHODS: ReadonlySet<string> = new Set(['placeOrder', 'placeOrders', 'placeOrderWithClientOrderId']);

let current: PlacementIntentIdentity | undefined;

/** Capture synchronously at entry; async continuations must carry this value explicitly. */
export const capturePlacementIntent = (): PlacementIntentIdentity | undefined => current;

/** Scope only the synchronous call: never leave ambient identity across an await. */
export const runWithPlacementIntent = <Result>(id: string | undefined, start: () => Result): Result => {
  let intent: PlacementIntentIdentity | undefined;
  if (id !== undefined) {
    try {
      intent = Object.freeze({ id, createdAt: delegatedIntentTime(id) });
    } catch {
      throw new PlacementIntentError('invalidIntentId');
    }
  }
  const previous = current;
  current = intent;
  try {
    return start();
  } finally {
    current = previous;
  }
};

/** Supply the shared synchronized protocol clock; never use the write-budget clock. */
export const mintPlacementIntent = (unixMs: number): PlacementIntentIdentity => {
  const id = createDelegatedIntentId(unixMs, (bytes) => { crypto.getRandomValues(bytes); });
  return Object.freeze({ id, createdAt: unixMs });
};

/** Stable sub-action UUID, never a venue client order ID or a new deadline. */
export const placementLegIntent = (parent: PlacementIntentIdentity, leg: string): PlacementIntentIdentity => {
  const createdAt = delegatedIntentTime(parent.id);
  if (createdAt !== parent.createdAt || !leg) throw new PlacementIntentError('invalidIntentId');
  const digest = sha256(utf8ToBytes(JSON.stringify([parent.id, leg])));
  const id = createDelegatedIntentId(createdAt, (bytes) => { bytes.set(digest.subarray(0, bytes.length)); });
  return Object.freeze({ id, createdAt });
};
