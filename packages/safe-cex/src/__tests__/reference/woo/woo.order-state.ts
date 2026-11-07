import type { Market } from '../../../types';

import { adjust } from '../../../utils/safe-math';

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

// REST snapshots are authoritative only when every executable known-market leaf
// is valid. Mapping a valid sibling must not conceal a malformed order.
export const assertWooAlgoSnapshotRow = (row: Record<string, any>, hasMarket: (symbol: string) => boolean): void => {
  if (!row || typeof row !== 'object') throw new Error('Invalid WOO algo order');
  const leaves = Array.isArray(row.childOrders) ? row.childOrders : [row];
  for (const leaf of leaves) {
    if (!leaf || typeof leaf !== 'object') throw new Error('Invalid WOO algo order');
    if (!hasMarket(leaf.symbol || row.symbol || '')) continue;
    if (!['STOP', 'STOP_LOSS', 'TAKE_PROFIT', 'TRAILING_STOP'].includes(leaf.algoType)) continue;
    const amount = Number(leaf.quantity);
    const filled = Number(leaf.totalExecutedQuantity ?? leaf.executed ?? 0);
    const extreme = Number(leaf.extremePrice);
    const callback = Number(leaf.callbackValue);
    const price =
      leaf.algoType === 'TRAILING_STOP'
        ? extreme + ((leaf.side ?? row.side) === 'BUY' ? callback : -callback)
        : Number(leaf.triggerPrice);
    if (
      leaf.quantity === undefined ||
      leaf.quantity === null ||
      leaf.quantity === '' ||
      !Number.isFinite(amount) ||
      amount < 0 ||
      !Number.isFinite(filled) ||
      filled < 0 ||
      !Number.isFinite(price) ||
      price <= 0 ||
      (leaf.algoType === 'TRAILING_STOP' && (!(extreme > 0) || !(callback > 0)))
    )
      throw new Error('Invalid WOO algo order');
  }
};
