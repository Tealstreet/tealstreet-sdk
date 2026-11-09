import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Balance } from '../generated/Balance';

/** Decode published fields without inventing omitted optional maps. */
export function readBalanceFrame(frame: Frame): Balance | undefined {
  return frame.balance();
}
