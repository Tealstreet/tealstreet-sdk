import type { PlacementIntentIdentity } from './placementIntentLedger';

export const capturePlacementIntent = (): PlacementIntentIdentity | undefined => undefined;

export const runWithPlacementIntent = <Result>(_id: string | undefined, start: () => Result): Result => start();
