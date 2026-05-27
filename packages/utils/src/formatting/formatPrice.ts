import { getSigDigitsFromPrecision } from '@tealstreet/math/src/getSigDigitsFromPrecision';
import { getCurrencySigDigits } from '../currency/currencyUtils';
import { formatValue } from './formatValue';

/**
 * Format a price/currency value using precision (step size) or currency defaults.
 * Pure function with no app-specific dependencies.
 *
 * @param value - The numeric value to format
 * @param precision - Step size precision (e.g., 0.01 for 2 decimals). If undefined, uses currency defaults.
 * @param currency - Currency code (e.g., 'USD', 'BTC'). Defaults to 'USD'.
 * @param hideCurrency - If true (default), shows $ prefix only. If false, shows currency suffix.
 *
 * @example
 * formatPrice(1234.567) // "$1,234.57"
 * formatPrice(1234.567, 0.01) // "$1,234.57"
 * formatPrice(89631.5, 0.1) // "$89,631.5"
 * formatPrice(1234.567, 0.01, 'USD', false) // "1,234.57 USD"
 */
export const formatPrice = (
  value: number | string,
  precision?: number,
  currency: string = 'USD',
  hideCurrency: boolean = true,
): string => {
  const sigDigits = precision ? getSigDigitsFromPrecision(precision) : getCurrencySigDigits(currency);
  return formatValue(value, currency, hideCurrency, sigDigits);
};

