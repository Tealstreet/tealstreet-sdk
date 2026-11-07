import type { OrderBook } from '../../../types';

import { updateOrderBook } from '../../../utils/updateOrderBook';

export type BinanceDepthUpdate = Record<string, any>;

export type BinanceOrderBookSyncState = {
  snapshotLastUpdateId?: number;
  lastAppliedUpdateId?: number;
};

export type BinanceOrderBookApplyResult = 'applied' | 'ignored' | 'gap';

export const resetBinanceOrderBookSyncState = (state: BinanceOrderBookSyncState) => {
  state.snapshotLastUpdateId = undefined;
  state.lastAppliedUpdateId = undefined;
};

export const loadBinanceOrderBookSnapshot = (
  orderBook: OrderBook,
  snapshot: { bids: string[][]; asks: string[][]; lastUpdateId: number },
  state: BinanceOrderBookSyncState,
) => {
  orderBook.bids = snapshot.bids.map(([price, amount]) => ({
    price: parseFloat(price),
    amount: parseFloat(amount),
    total: 0,
  }));

  orderBook.asks = snapshot.asks.map(([price, amount]) => ({
    price: parseFloat(price),
    amount: parseFloat(amount),
    total: 0,
  }));

  state.snapshotLastUpdateId = snapshot.lastUpdateId;
  state.lastAppliedUpdateId = undefined;
};

export const getBufferedBinanceDepthUpdates = (updates: BinanceDepthUpdate[], snapshotLastUpdateId: number) => {
  return updates.filter((update) => Number(update.u) > snapshotLastUpdateId);
};

export const applyBinanceDepthUpdate = (
  orderBook: OrderBook,
  update: BinanceDepthUpdate,
  state: BinanceOrderBookSyncState,
  { requirePreviousFinalUpdateId = false }: { requirePreviousFinalUpdateId?: boolean } = {},
): BinanceOrderBookApplyResult => {
  const finalUpdateId = Number(update.u);
  const firstUpdateId = Number(update.U);
  const localUpdateId = state.lastAppliedUpdateId ?? state.snapshotLastUpdateId;

  if (!Number.isFinite(finalUpdateId) || localUpdateId === undefined) {
    return 'gap';
  }

  if (finalUpdateId <= localUpdateId) {
    return 'ignored';
  }

  if (state.lastAppliedUpdateId === undefined) {
    if (!Number.isFinite(firstUpdateId) || firstUpdateId > localUpdateId + 1) {
      return 'gap';
    }
  } else if (requirePreviousFinalUpdateId && Number.isFinite(Number(update.pu))) {
    if (Number(update.pu) !== state.lastAppliedUpdateId) {
      return 'gap';
    }
  } else if (!Number.isFinite(firstUpdateId) || firstUpdateId > localUpdateId + 1) {
    return 'gap';
  }

  const bids = update.b;
  if (bids) {
    for (let i = 0; i < bids.length; i++) {
      const o = bids[i];
      updateOrderBook(orderBook, 'bids', {
        price: parseFloat(o[0]),
        amount: parseFloat(o[1]),
        total: 0,
      });
    }
  }

  const asks = update.a;
  if (asks) {
    for (let i = 0; i < asks.length; i++) {
      const o = asks[i];
      updateOrderBook(orderBook, 'asks', {
        price: parseFloat(o[0]),
        amount: parseFloat(o[1]),
        total: 0,
      });
    }
  }

  state.lastAppliedUpdateId = finalUpdateId;
  return 'applied';
};
