import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Balance } from '../generated/Balance';

/** A Balance section replaces its optional maps; an absent section changes nothing. */
export function readBalanceFrame(frame: Frame): Balance | undefined {
  return frame.balance();
}
