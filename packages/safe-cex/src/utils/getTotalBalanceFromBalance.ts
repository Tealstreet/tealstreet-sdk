import { Balance, Market } from '../types';
import { getBalanceSlice } from './getBalanceSlice';

export const getTotalBalanceFromBalance = (balance?: Balance, market?: Market): number =>
  getBalanceSlice(balance, market)?.total || 0;