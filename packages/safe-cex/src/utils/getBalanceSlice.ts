import { Balance, Market } from '../types';
import { getMarginCurrency } from './sizing';

export const getBalanceSlice = (balance?: Balance, market?: Market): Balance | null => {
  if (!balance) return null;
  if (!market) return balance;

  const marginCurrency = getMarginCurrency(market);
  if (marginCurrency === '') return balance;
  const currencyBalance = balance.currencies?.[marginCurrency];
  if (currencyBalance) return currencyBalance;
  if (market.isInverse && balance.info?.coinMargined) {
    return { total: 0, free: 0, used: 0, upnl: 0 };
  }
  return balance;
};
