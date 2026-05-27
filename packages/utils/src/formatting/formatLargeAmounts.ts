/**
 * Options for formatting large amounts with K/M/B suffixes
 */
export interface FormatLargeAmountsOptions {
  /** Number of decimal places for large numbers (default: 3) */
  largePrecision?: number;
  /** Minimum size before applying K/M/B formatting (default: 1000) */
  minSize?: number;
  /** Round small numbers to largePrecision (default: true) */
  roundSmallPrecision?: boolean;
  /** Precision for small numbers that would round to 0 */
  smallPrecision?: number;
  /** Offset for breakpoint thresholds (power of 10) */
  breakpointOffset?: number;
}

const ROUNDING_ERROR_DECIMALS = 10;

const normalizeRoundingDust = (num: number): number => {
  if (!Number.isFinite(num)) {
    return num;
  }

  return parseFloat(num.toFixed(ROUNDING_ERROR_DECIMALS));
};

const getNonZeroFallbackPrecision = (num: number, precision: number): number => {
  const absNum = Math.abs(num);
  if (!Number.isFinite(absNum) || absNum <= 0) {
    return precision;
  }

  return Math.max(precision, Math.ceil(-Math.log10(absNum)));
};

/**
 * Format a number with K/M/B suffixes for large values
 *
 * @example
 * formatLargeAmounts(1500, 2) // "1.500K"
 * formatLargeAmounts(1500000, 2) // "1.500M"
 * formatLargeAmounts(50, 2) // "50.00"
 */
export const formatLargeAmounts = (num: number, precision: number, options: FormatLargeAmountsOptions = {}): string => {
  const {
    largePrecision = 3,
    minSize = 1_000,
    roundSmallPrecision = true,
    smallPrecision,
    breakpointOffset = 0,
  } = options;

  const absNum = Math.abs(num);
  const offsetValue = breakpointOffset ? Math.pow(10, breakpointOffset) : 1;

  if (absNum > 1_000_000_000 * offsetValue && absNum > minSize) {
    return (num / 1_000_000_000).toFixed(largePrecision) + 'B';
  }
  if (absNum > 1_000_000 * offsetValue && absNum > minSize) {
    return (num / 1_000_000).toFixed(largePrecision) + 'M';
  }
  if (absNum > 1_000 * offsetValue && absNum > minSize) {
    return (num / 1_000).toFixed(largePrecision) + 'K';
  }

  const displayPrecision = roundSmallPrecision ? Math.min(precision, largePrecision) : precision;
  const res = num.toFixed(displayPrecision);

  // if using precision rounds to 0 but number isn't zero, don't use precision
  if (parseFloat(res) === 0 && num !== 0) {
    const normalizedNum = normalizeRoundingDust(num);
    if (normalizedNum === 0) {
      return (0).toFixed(displayPrecision);
    }

    return normalizedNum.toFixed(smallPrecision ?? getNonZeroFallbackPrecision(normalizedNum, precision));
  }
  return res;
};
