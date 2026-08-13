import type { IncomeCategory } from '../types';

/**
 * Normalise a binance-style `incomeType` string to a venue-neutral `IncomeCategory`.
 *
 * Shared by binance and weex, which both report binance's spellings. Unrecognised types
 * fall through to 'other' so their value is still counted, just not attributed — dropping
 * them would silently unbalance the reconciliation identity.
 */
const CATEGORY_BY_TYPE: Record<string, IncomeCategory> = {
  REALIZED_PNL: 'realized_pnl',
  DELIVERED_SETTELMENT: 'realized_pnl', // binance's own spelling
  FUNDING_FEE: 'funding',
  COMMISSION: 'commission',
  OPTIONS_PREMIUM_FEE: 'commission',
  POSITION_LIMIT_INCREASE_FEE: 'commission',
  COMMISSION_REBATE: 'rebate',
  REFERRAL_KICKBACK: 'rebate',
  API_REBATE: 'rebate',
  TRANSFER: 'transfer',
  INTERNAL_TRANSFER: 'transfer',
  CROSS_COLLATERAL_TRANSFER: 'transfer',
  COIN_SWAP_DEPOSIT: 'transfer',
  COIN_SWAP_WITHDRAW: 'transfer',
  AUTO_EXCHANGE: 'transfer',
  INSURANCE_CLEAR: 'liquidation',
};

export const incomeCategoryFromType = (incomeType: unknown): IncomeCategory => {
  return CATEGORY_BY_TYPE[String(incomeType ?? '').toUpperCase()] || 'other';
};
