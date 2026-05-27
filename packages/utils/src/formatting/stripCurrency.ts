/**
 * Strips the trailing currency suffix from a formatted value string.
 * Examples:
 *   "0.001 BTC" -> "0.001"
 *   "+$0.09 USD" -> "+$0.09"
 *   "$1,234.56 USDT" -> "$1,234.56"
 *   "123" -> "123" (no change if no space)
 *
 * @param value - The formatted string with potential currency suffix
 * @returns The string with trailing " CURRENCY" removed
 */
export function stripCurrency(value: string): string {
  if (!value) return value;
  const lastSpaceIndex = value.lastIndexOf(' ');
  if (lastSpaceIndex === -1) return value;
  return value.slice(0, lastSpaceIndex);
}
