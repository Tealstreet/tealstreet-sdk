import { numberToString, parseToInt } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import { safeString } from '@tealstreet/ccxt-compat/src/functions/type';
import Precise from '@tealstreet/ccxt-compat/src/Precise';

/**
 * Converts a display symbol to the Hyperliquid API coin format.
 * Only transforms HIP-3 symbols; core perps pass through unchanged.
 * - "XYZ|NQ-PERP" → "xyz:NQ" (HIP-3 needs conversion)
 * - "NQ-PERP" → "NQ-PERP" (core perp unchanged)
 */
export function symbolToCoin(symbol: string): string {
  // Only transform HIP-3 format (contains pipe separator)
  if (symbol.includes('|')) {
    const base = symbol.replace(/-PERP$/i, '');
    const [dex, asset] = base.split('|');
    // Convert back: uppercase DEX|ASSET → lowercase dex:ASSET
    return `${dex.toLowerCase()}:${asset}`;
  }

  // Core perp: return as-is
  return symbol;
}

/**
 * @method
 * @name calculatePricePrecision
 * @description Helper function to calculate the Hyperliquid DECIMAL_PLACES price precision
 * @param {float} price the price to use in the calculation
 * @param {int} amountPrecision the amountPrecision to use in the calculation
 * @param {int} maxDecimals the maxDecimals to use in the calculation
 * @returns {int} The calculated price precision
 */
export function calculatePricePrecision(price: number, amountPrecision: number, maxDecimals: number) {
  let pricePrecision = 0;
  const priceStr = numberToString(price);
  if (priceStr === undefined) {
    return 0;
  }
  const priceSplitted = priceStr.split('.');
  if (Precise.stringEq(priceStr, '0')) {
    // Significant digits is always 5 in this case
    const significantDigits = 5;
    // Integer digits is always 0 in this case (0 doesn't count)
    const integerDigits = 0;
    // Calculate the price precision
    pricePrecision = Math.min(maxDecimals - amountPrecision, significantDigits - integerDigits);
  } else if (Precise.stringGt(priceStr, '0') && Precise.stringLt(priceStr, '1')) {
    // Significant digits, always 5 in this case
    const significantDigits = 5;
    // Get the part after the decimal separator
    const decimalPart = safeString(priceSplitted, 1, '');
    if (!decimalPart) return 0;
    // Count the number of leading zeros in the decimal part
    let leadingZeros = 0;
    while (leadingZeros <= decimalPart.length && decimalPart[leadingZeros] === '0') {
      leadingZeros = leadingZeros + 1;
    }
    // Calculate price precision based on leading zeros and significant digits
    pricePrecision = leadingZeros + significantDigits;
    // Calculate the price precision based on maxDecimals - szDecimals and the calculated price precision from the previous step
    pricePrecision = Math.min(maxDecimals - amountPrecision, pricePrecision);
  } else {
    // Count the numbers before the decimal separator
    const integerPart = safeString(priceSplitted, 0, '');
    if (!integerPart) return 0;
    // Get significant digits, take the max() of 5 and the integer digits count
    const significantDigits = Math.max(5, integerPart.length);
    // Calculate price precision based on maxDecimals - szDecimals and significantDigits - integerPart.length
    pricePrecision = Math.min(maxDecimals - amountPrecision, significantDigits - integerPart.length);
  }
  return parseToInt(pricePrecision);
}
