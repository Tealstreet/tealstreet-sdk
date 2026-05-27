import { isNotNumber } from './isNotNumber';

export const toFixedFloat = (v: unknown, precision: number): number => {
  if (isNotNumber(v)) {
    return 0;
  }
  return parseFloat(parseFloat(String(v)).toFixed(precision));
};
