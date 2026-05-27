import { isNotNumber } from './isNotNumber';

export const handleRoundingError = (v: unknown): number => {
  if (isNotNumber(v)) {
    return 0;
  }
  return parseFloat(parseFloat(String(v)).toFixed(10));
};
