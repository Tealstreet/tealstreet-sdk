import type { TwapOptions } from './TwapOptions';
import type { TwapState } from './TwapState';

import { TwapStatus } from './TwapStatus';

/** How long past its next slice a restored running TWAP may still resume. */
export const TWAP_RESTORE_SCHEDULE_GRACE_MS = 60_000;

/**
 * Why a `twap:state` row must not run, or undefined when it may. A row comes
 * from one tab's sessionStorage, which a reopened tab can bring back long after
 * the TWAP stopped running. Mirrors the exchange server's `validate::restored`.
 */
export const twapRestoreRefusal = (
  options: TwapOptions | undefined,
  state: TwapState | undefined,
  nowMs: number,
): string | undefined => {
  if (!options || !state) return 'it has no options or state';
  if (state.accountName !== options.accountName) return 'its state names another account than its options';
  if (state.status === TwapStatus.Completed) return 'it already completed';
  if (state.status === TwapStatus.Idle) return 'it never started';
  const sizes = state.ordersSizes;
  // A last slice still being placed is settled on restore, not refused.
  if (!Array.isArray(sizes) || (sizes.length === 0 && !state.inFlight)) return 'no slices remain';
  if (!sizes.every((size) => Number.isFinite(size) && size >= 0)) return 'its slice sizes are invalid';
  if (!(Number.isFinite(options.size) && options.size > 0)) return 'its size is invalid';
  const inFlight = state.inFlight;
  // A definitive lookup miss re-places the in-flight slice, so it counts as remaining.
  if (inFlight && !(Number.isFinite(inFlight.size) && inFlight.size > 0)) return 'its in-flight slice is invalid';
  const remaining = sizes.reduce((sum, size) => sum + size, 0) + (inFlight?.size ?? 0);
  if (remaining > options.size * (1 + 1e-9)) return 'its remaining slices exceed the TWAP size';
  if (sizes.length + (state.ordersExecuted || 0) + (inFlight ? 1 : 0) > options.ordersCount) {
    return 'its remaining and executed slices exceed ordersCount';
  }
  if (state.status === TwapStatus.Running) {
    const intervalMs = (options.duration / options.ordersCount) * 60_000;
    const latestMs =
      state.nextOrderAt + intervalMs * (1 + (options.randomnessPercentage || 0) / 100) + TWAP_RESTORE_SCHEDULE_GRACE_MS;
    if (!Number.isFinite(latestMs) || nowMs > latestMs) {
      return inFlight
        ? 'its schedule lapsed, so no tab was running it; a slice may have been in flight, check open orders and fills'
        : 'its schedule lapsed, so no tab was running it';
    }
  }
  return undefined;
};
