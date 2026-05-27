/**
 * CCXT-compatible helper functions for handling precision
 * Transplanted from CCXT library for accurate precision calculation
 */

/**
 * Converts an integer precision to decimal amount
 * For example: integerPrecisionToAmount(10000) returns 0.0001
 * This is used by Kraken where contractValueTradePrecision is the denominator
 *
 * @param precision - Integer precision (denominator)
 * @returns Decimal amount string
 */
export function integerPrecisionToAmount(precision: number | string | undefined): string | undefined {
  if (precision === undefined || precision === null) {
    return undefined;
  }

  const precisionNumber = typeof precision === 'string' ? parseInt(precision) : precision;

  if (precisionNumber === 0) {
    return '1';
  }

  // Convert to decimal: 1 / precision
  // e.g., 10000 -> "0.0001"
  const result = (1 / precisionNumber).toString();
  return result;
}

/**
 * Counts the number of decimal places in a number
 * For example: countDecimals(0.0001) returns 4
 *
 * @param value - The decimal value as string or number
 * @returns Number of decimal places
 */
export function countDecimals(value: string | number | undefined): number {
  if (value === undefined || value === null) {
    return 0;
  }

  const str = String(value);

  // Handle scientific notation
  if (str.includes('e-') || str.includes('E-')) {
    const parts = str.split(/[eE]-/);
    const decimals = parts[0].split('.')[1] || '';
    return decimals.length + parseInt(parts[1]);
  }

  if (str.includes('.')) {
    const parts = str.split('.');
    // Remove trailing zeros
    const decimalPart = parts[1].replace(/0+$/, '');
    return decimalPart.length;
  }

  return 0;
}

/**
 * Converts a decimal precision to number of decimals
 * For example: decimalToPrecision(0.0001) returns 4
 *
 * @param decimal - The decimal precision value
 * @returns Number of decimal places
 */
export function decimalToPrecision(decimal: string | number | undefined): number {
  if (decimal === undefined || decimal === null) {
    return 0;
  }

  const str = String(decimal);

  // If it's "1", precision is 0
  if (parseFloat(str) === 1) {
    return 0;
  }

  // Count the decimal places
  return countDecimals(str);
}

/**
 * Parse precision from various formats
 * Handles both integer precision (denominator) and decimal precision
 *
 * @param value - The precision value (can be denominator or decimal)
 * @param isInteger - Whether the value is an integer precision (denominator)
 * @returns Number of decimal places
 */
export function parsePrecision(value: string | number | undefined, isInteger = false): number {
  if (value === undefined || value === null) {
    return 0;
  }

  if (isInteger) {
    // It's a denominator like 10000 -> convert to decimal then count
    const decimal = integerPrecisionToAmount(value);
    return decimalToPrecision(decimal);
  } else {
    // It's already a decimal like 0.0001
    return decimalToPrecision(value);
  }
}

/**
 * Precision to string for display
 * Converts precision number to decimal string
 *
 * @param precision - Number of decimal places
 * @returns Decimal string (e.g., 4 -> "0.0001")
 */
export function precisionToString(precision: number): string {
  if (precision === 0) {
    return '1';
  }

  let result = '0.';
  for (let i = 0; i < precision - 1; i++) {
    result += '0';
  }
  result += '1';

  return result;
}