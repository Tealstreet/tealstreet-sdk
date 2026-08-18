import type { TrackedLookupStatus } from './types';

const EXACT_DECIMAL = /^(?:0|[1-9]\d*)(?:\.\d+)?$/;

export const isExactDecimal = (value: unknown): value is string =>
  typeof value === 'string' && EXACT_DECIMAL.test(value);

export const isPositiveExactDecimal = (value: unknown): value is string =>
  isExactDecimal(value) && /[1-9]/.test(value);

export const compareExactDecimals = (left: string, right: string): number => {
  const [leftInteger, leftFraction = ''] = left.split('.');
  const [rightInteger, rightFraction = ''] = right.split('.');
  if (leftInteger.length !== rightInteger.length) {
    return leftInteger.length < rightInteger.length ? -1 : 1;
  }
  if (leftInteger !== rightInteger) {
    return leftInteger < rightInteger ? -1 : 1;
  }
  const scale = Math.max(leftFraction.length, rightFraction.length);
  const normalizedLeft = leftFraction.padEnd(scale, '0');
  const normalizedRight = rightFraction.padEnd(scale, '0');
  return normalizedLeft === normalizedRight ? 0 : normalizedLeft < normalizedRight ? -1 : 1;
};

export const trackedQuantitiesAreValid = (
  total: unknown,
  cumulativeFilled: unknown,
  status: TrackedLookupStatus,
): total is string => {
  if (!isPositiveExactDecimal(total) || !isExactDecimal(cumulativeFilled)) return false;
  const comparison = compareExactDecimals(cumulativeFilled, total);
  return comparison <= 0 && (status !== 'filled' || comparison === 0);
};
