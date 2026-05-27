import { getSigDigitsFromPrecision } from '@tealstreet/math/src/getSigDigitsFromPrecision';
import { getCurrencySigDigits } from '../currency/currencyUtils';
import { formatValue } from './formatValue';

/**
 * Format a currency value using precision (step size) or currency defaults
 *
 * @param value - The numeric value to format
 * @param currency - Currency code (e.g., 'USD', 'BTC')
 * @param precision - Step size precision (e.g., 0.01 for 2 decimals)
 *
 * @example
 * formatCurrencyValue(1234.567, 'USD') // "1,234.57 USD"
 * formatCurrencyValue(0.12345678, 'BTC', 0.00000001) // "0.12345678 BTC"
 */
export const formatCurrencyValue = (value: number, currency: string, precision?: number): string => {
  const sigDigits = precision ? getSigDigitsFromPrecision(precision) : getCurrencySigDigits(currency);
  return formatValue(value, currency, false, sigDigits);
};

