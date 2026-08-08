import type { QuickSizeSlot } from '../api/WebApi.types';

/**
 * The UI-only token names, and the string → number conversion the host
 * applies when adapting `UITokenResolvers` (`@tealstreet/cli-core`) to
 * `WebAdapter`.
 *
 * The resolvers return complete tokens (`"$100"`, `"50000"`) rather than bare
 * numbers because the CLI's substitute layer requires that — see
 * `packages/cli-core/src/preprocess/types.ts`. Script authors want numbers,
 * so the conversion happens once, here, instead of in every consumer.
 */
export const UI_TOKENS = Object.freeze({
  click: '$click',
  orderSize: '$ordersize',
  /** `$quick1` … `$quick7`. */
  quick: (slot: QuickSizeSlot): string => `$quick${slot}`,
});

export const QUICK_SIZE_SLOTS: readonly QuickSizeSlot[] = Object.freeze([1, 2, 3, 4, 5, 6, 7] as const);

export const isQuickSizeSlot = (value: number): value is QuickSizeSlot =>
  Number.isInteger(value) && value >= 1 && value <= 7;

/**
 * Parse a resolver token into a number. Handles the bare-numeric form
 * (`$click` → `"50000"`) and the `$`-prefixed size form (`"$100"`).
 * Returns `undefined` for a missing value or any token shape that has no
 * numeric meaning — notably percent-sized quick buttons.
 */
export const parseUIToken = (token: string | undefined): number | undefined => {
  if (token === undefined) return undefined;
  const trimmed = token.trim();
  if (trimmed === '') return undefined;
  // Percent tokens carry no notional meaning without a position.
  if (trimmed.endsWith('%')) return undefined;
  const numeric = trimmed.startsWith('$') ? trimmed.slice(1) : trimmed;
  // `Number('')` is 0, not NaN — a bare `"$"` must read as unavailable, not
  // as a zero-size order.
  if (numeric.trim() === '') return undefined;
  const parsed = Number(numeric);
  return Number.isFinite(parsed) ? parsed : undefined;
};
