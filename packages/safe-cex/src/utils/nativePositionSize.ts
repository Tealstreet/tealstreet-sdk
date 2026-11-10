/** Position quantities in base/quote units, with optional native-unit evidence. */
export interface NativeSizeInputs {
  readonly contracts?: number | null;
  readonly notional?: number | null;
  readonly extra?: { readonly contractsNative?: number | null };
}

export interface NativeSizeMarket {
  readonly contractSize?: number;
  readonly isInverse?: boolean;
}

/** Prefer coherent native evidence; conflicting or unavailable evidence is unknown.
 * Position.lastUpdate does not date the native field separately, so disagreement
 * cannot establish which quantity is current. Rounding belongs to the caller. */
export const nativePositionSize = (position: NativeSizeInputs, market: NativeSizeMarket): number | null => {
  const contractSize = market.contractSize === 0 ? 1 : (market.contractSize ?? 1);
  if (!Number.isFinite(contractSize) || contractSize <= 0) return null;

  const source = market.isInverse ? position.notional : position.contracts;
  let derived = typeof source === 'number' && Number.isFinite(source) ? Math.abs(source) / contractSize : null;
  if (derived !== null && !Number.isFinite(derived)) derived = null;

  const native = position.extra?.contractsNative;
  if (native === undefined || native === null) return derived;
  if (typeof native !== 'number' || !Number.isFinite(native)) return null;
  const amount = Math.abs(native);
  // Allow arithmetic roundoff only, never a quantity/lot-sized discrepancy.
  if (derived !== null && Math.abs(amount - derived) > Number.EPSILON * amount * 8) return null;
  return amount;
};
