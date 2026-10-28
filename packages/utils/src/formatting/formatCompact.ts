import { getSigDigitsFromPrecision } from '@tealstreet/math/src/getSigDigitsFromPrecision';

/**
 * Format a price for compact display (with $ prefix)
 * Suitable for CLI prompts and status displays.
 *
 * @param price - The price value
 * @param precision - Step size precision (e.g., 0.01)
 *
 * @example
 * formatPriceCompact(97234.56) // "$97,234.56"
 * formatPriceCompact(0.00012345, 0.00000001) // "$0.00012345"
 */
export const formatPriceCompact = (price: number, precision?: number): string => {
  // Use precision if available, otherwise use sensible defaults
  const decimals = precision
    ? getSigDigitsFromPrecision(precision)
    : price >= 1000
      ? 2
      : price >= 1
        ? 2
        : 4;

  if (price >= 10000) {
    return (
      '$' +
      price.toLocaleString('en-US', {
        minimumFractionDigits: Math.min(decimals, 2),
        maximumFractionDigits: Math.min(decimals, 2),
      })
    );
  } else {
    return '$' + price.toFixed(decimals);
  }
};

/**
 * Format a size/amount for compact display
 * Uses k/M/B suffixes for large values.
 *
 * @param size - The size value
 * @param precision - Step size precision (e.g., 0.001); steps >= 1 use no fractional digits
 *
 * @example
 * formatSizeCompact(0.005) // "0.0050"
 * formatSizeCompact(15000) // "15.0k"
 * formatSizeCompact(1500000) // "1.50M"
 */
export const formatSizeCompact = (size: number, precision?: number): string => {
  const decimals = precision ? (precision >= 1 ? 0 : getSigDigitsFromPrecision(precision)) : size >= 1 ? 2 : 4;

  if (size >= 1_000_000) {
    return (size / 1_000_000).toFixed(2) + 'M';
  } else if (size >= 10_000) {
    return (size / 1_000).toFixed(1) + 'k';
  } else {
    return size.toFixed(decimals);
  }
};
