const MAX_NUMBER_FORMAT_CACHE_SIZE = 16;
const numberFormatCache = new Map<number, Intl.NumberFormat>();

const getNumberFormatter = (
  minimumFractionDigits: number,
): Intl.NumberFormat => {
  const cachedFormatter = numberFormatCache.get(minimumFractionDigits);
  if (cachedFormatter) {
    return cachedFormatter;
  }

  const formatter = new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    useGrouping: true,
    minimumFractionDigits,
  });

  if (numberFormatCache.size >= MAX_NUMBER_FORMAT_CACHE_SIZE) {
    const oldestCacheKey = numberFormatCache.keys().next().value;
    if (oldestCacheKey !== undefined) {
      numberFormatCache.delete(oldestCacheKey);
    }
  }

  numberFormatCache.set(minimumFractionDigits, formatter);
  return formatter;
};

/**
 * Format a value with locale formatting (thousands separator, decimals)
 *
 * @param value - The numeric value to format
 * @param currencyCode - Currency code (e.g., 'USD', 'BTC') for suffix
 * @param hideCurrency - If true, shows $ prefix only (no currency suffix)
 * @param minimumFractionDigits - Number of decimal places (default: 2)
 *
 * @example
 * formatValue(1234.56, 'USD') // "$1,234.56"
 * formatValue(1234.56, 'USD', false) // "1,234.56 USD"
 * formatValue(0.00012345, 'BTC', false, 8) // "0.00012345 BTC"
 */
export const formatValue = (
  unconvertedValue: number | string,
  currencyCode: string = 'USD',
  hideCurrency: boolean = true,
  minimumFractionDigits = 2,
): string => {
  // Handle masked/placeholder values
  if (String(unconvertedValue).startsWith('*')) {
    return `${unconvertedValue} ${currencyCode}`;
  }

  const value = +unconvertedValue;
  const formatted = getNumberFormatter(minimumFractionDigits).format(value);

  if (hideCurrency) {
    return formatted;
  }

  // Replace $ with currency suffix
  return `${formatted.replace(/\$/, '')} ${currencyCode}`;
};
