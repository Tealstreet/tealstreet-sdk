import { isNotNumber } from './isNotNumber';

export const handleRoundingError = (v: unknown): number => {
  if (isNotNumber(v)) {
    return 0;
  }
  const numV = typeof v === 'number' ? v : Number(v);
  return parseFloat(numV.toFixed(10));
};
