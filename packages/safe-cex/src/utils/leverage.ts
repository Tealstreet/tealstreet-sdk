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
 * @returns Account leverage ratio (e.g., 0.10 means 10% of account)
 */
export function calculatePositionAccountLeverage(notional: number, accountBalance: number): number {
  if (accountBalance <= 0) return 0;
  return Math.abs(notional) / accountBalance;
}

/**
 * Calculate total account effective leverage across all positions
 * This represents the overall account exposure
 *
 * @param positions - Array of positions with notional values
 * @param accountBalance - Total account balance
 * @returns Total account leverage ratio
 */
export function calculateTotalAccountLeverage(
  positions: Array<{ notional: number }>,
  accountBalance: number,
): number {
  if (accountBalance <= 0) return 0;
  const totalNotional = positions.reduce((sum, pos) => sum + Math.abs(pos.notional), 0);
  return totalNotional / accountBalance;
}

/**
 * Calculate total notional across positions
 *
 * @param positions - Array of positions with notional values
 * @returns Sum of absolute notional values
 */
export function calculateTotalNotional(positions: Array<{ notional: number }>): number {
  return positions.reduce((sum, pos) => sum + Math.abs(pos.notional), 0);
}
