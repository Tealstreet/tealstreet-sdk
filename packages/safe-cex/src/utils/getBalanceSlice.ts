import { Balance, Market } from '../types';
import { getMarginCurrency } from './sizing';

export const getBalanceSlice = (balance?: Balance, market?: Market): Balance | null => {
  if (!balance) return null;
  if (!market) return balance;

  const marginCurrency = getMarginCurrency(market);
  if (marginCurrency === '') return balance;
  return balance.currencies?.[marginCurrency] || balance;
};