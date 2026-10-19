import type { Market } from '../../types';

import { adjust } from '../../utils/safe-math';

export const splitWooQuantity = (amount: number, market: Market): number[] => {
  const step = market.precision.amount;
  const ticks = Math.round(amount / step);
  const min = Math.max(1, Math.ceil(market.limits.amount.min / step));
  const max = Math.floor(market.limits.amount.max / step);
  if (!Number.isSafeInteger(ticks) || ticks < min || !(step > 0) || max < min) {
    throw new Error('WOO quantity is outside lot limits');
  }
  const count = Math.ceil(ticks / max);
  if (count > 1000 || count < 1) throw new Error('WOO order requires too many native lots');
  const base = Math.floor(ticks / count);
  if (base < min) throw new Error('WOO split leaves a below-minimum lot');
  const remainder = ticks - base * count;
  return Array.from({ length: count }, (_, index) => adjust((base + (index < remainder ? 1 : 0)) * step, step));
};
