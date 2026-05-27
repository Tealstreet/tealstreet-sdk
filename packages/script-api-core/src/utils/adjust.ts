/**
 * Snap `value` to the nearest multiple of `step` with decimal-safe rounding.
 * Thin re-export of safe-cex's vetted `adjust` impl (used widely across the
 * trading runtime). The script-api narrows the signature to numeric-only to
 * keep the script-facing contract simple; safe-cex's underlying `step:
 * number | string` form still applies internally.
 *
 * See SPEC §2.1 api.utils.adjust.
 */
import { adjust as safeAdjust } from '@tealstreet/safe-cex/src/utils/safe-math';

export const adjust = (value: number, step: number): number => safeAdjust(value, step);
