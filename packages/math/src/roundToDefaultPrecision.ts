import type { MaybeNumber } from './types';

import { getValueSigDigits } from './getValueSigDigits';
import { isNotNumber } from './isNotNumber';

export function roundToDefaultPrecision(v: MaybeNumber, offset = 0): number {
  if (isNotNumber(v)) return 0;
  const numV = typeof v === 'number' ? v : Number(v);
  return parseFloat(numV.toFixed(getValueSigDigits(numV) + offset));
}
