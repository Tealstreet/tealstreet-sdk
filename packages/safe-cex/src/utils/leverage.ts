/**
 * Leverage calculation utilities
 * Shared between CLI and web for consistent calculations
 */

/**
 * Calculate account leverage for a single position
 * This represents how much of the account balance this position uses
 *
 * @param notional - Position notional value (absolute)
 * @param accountBalance - Total account balance
 * @returns Account leverage ratio, or null for unavailable valuation
 */
export function calculatePositionAccountLeverage(notional: number | null, accountBalance: number): number | null {
  if (typeof notional !== 'number' || !Number.isFinite(notional) || !Number.isFinite(accountBalance)) return null;
  if (accountBalance <= 0) return 0;
  const leverage = Math.abs(notional) / accountBalance;
  return Number.isFinite(leverage) ? leverage : null;
}

/**
 * Calculate total account effective leverage across all positions
 * This represents the overall account exposure
 *
 * @param positions - Array of positions with notional values
 * @param accountBalance - Total account balance
 * @returns Total account leverage ratio, or null for an incomplete scope
 */
export function calculateTotalAccountLeverage(
  positions: ReadonlyArray<{ notional: number | null }>,
  accountBalance: number,
): number | null {
  return calculatePositionAccountLeverage(calculateTotalNotional(positions), accountBalance);
}

/**
 * Calculate total notional across positions
 *
 * @param positions - Array of positions with notional values
 * @returns Sum of absolute notional values, or null for an incomplete scope
 */
export function calculateTotalNotional(positions: ReadonlyArray<{ notional: number | null }>): number | null {
  let total = 0;
  for (const { notional } of positions) {
    if (typeof notional !== 'number' || !Number.isFinite(notional)) return null;
    total += Math.abs(notional);
  }
  return Number.isFinite(total) ? total : null;
}
