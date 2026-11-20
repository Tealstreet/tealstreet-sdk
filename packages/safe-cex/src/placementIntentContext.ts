import type { PlacementIntentIdentity } from './placementIntentLedger';

import { delegatedIntentTime } from './delegatedOrderFrames';
import { PlacementIntentError } from './placementIntentLedger';

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
