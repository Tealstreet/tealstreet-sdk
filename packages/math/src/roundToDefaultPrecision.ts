import { getValueSigDigits } from './getValueSigDigits';
import { isNotNumber } from './isNotNumber';
import type { MaybeNumber } from './types';

export function roundToDefaultPrecision(v: MaybeNumber, offset = 0): number {
  if (isNotNumber(v)) return 0;
  const numV = Number(v);
  return parseFloat(numV.toFixed(getValueSigDigits(v) + offset));
}
