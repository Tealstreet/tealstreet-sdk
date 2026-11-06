import type { Exchange } from '../../exchanges/base';
import type { CrossTradeConfig, CrossTradeCopier } from '../../crossTrade/types';
import type { Ticker } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { CrossTradeManager } from '../../crossTrade/runtime/CrossTradeManager';
import { BaseExchange } from '../../exchanges/base';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import type { OrderRow } from '../fixtures/scxfFrame';
import { inverseUnitsFrame as fixture } from '../fixtures/scxfFrame';

describe('Rust frames preserve inverse native units', () => {
  it('keeps exact OKX inverse fills/counts and does not label USD PnL as native coin PnL', () => {
    const value = fixture({
      units: 3,
      contractSize: 100,
      native: true,
      positions: true,
      positionPnl: 100,
      orders: [{ id: 'inverse-okx', amount: 7, filled: 0.004321, remaining: 0.008765, native: 5, filledNative: 2 }],
    });
    expect(value.orders()!.filledInPlacementUnits![0]).toBe(2);
    const worker = new DefaultStore();
    new FrameApplier(worker).apply(value);
    const main = new DefaultStore();
    const exchange = new BaseExchange('okx', { key: '', secret: '' }, main);
    exchange.applyWorkerFrame(value.buffer);
    for (const store of [worker, main]) {
      expect(store.orders[0]).toMatchObject({ amount: 7, extra: { remainingNative: 5, remainingNotional: 500, filledInPlacementUnits: 2 } });
      expect(store.positions[0].extra).toEqual({ contractsNative: 10 });
      expect(store.positions[0].unrealizedPnl).toBe(100);
    }
    exchange.dispose();
  });

  it('converts COIN-M USD notionals to native contract counts in worker and main stores', () => {
    const value = fixture({
      units: 4,
      contractSize: 100,
      native: true,
      positions: true,
      orders: [{ id: 'coin-m', amount: 7, filled: 2, remaining: 5, native: 5 }],
    });
    const workerStore = new DefaultStore();
    const applier = new FrameApplier(workerStore);
    applier.apply(value);
    applier.apply(
      fixture({
        markets: false,
        positions: true,
        native: true,
        orders: [{ id: 'coin-m', amount: 7, filled: 2, remaining: 5, native: 5 }],
      }),
    );
    const mainStore = new DefaultStore();
    const exchange = new BaseExchange('binanceinverse', { key: '', secret: '' }, mainStore);
    exchange.applyWorkerFrame(value.buffer);
    for (const store of [workerStore, mainStore]) {
      expect(store.positions[0]).toMatchObject({
        notional: 1000,
        extra: { contractsNative: 10, unrealizedPnlNative: 0.002 },
      });
      expect(store.orders[0]).toMatchObject({
        amount: 7,
        filled: 2,
        remaining: 5,
        extra: { remainingNative: 5, remainingNotional: 500 },
      });
    }
    exchange.dispose();
  });


  it('decodes legacy seven-column frames and leaves unknown native quantities absent', () => {
    const frame = fixture({ orders: [{ id: 'full-stop', amount: 0, filled: 0, remaining: 0.00013 }] });
    expect(frame.orders()!.remainingNative).toBeUndefined();
    const store = new DefaultStore();
    new FrameApplier(store).apply(frame);
    expect(store.orders[0]).toMatchObject({ id: 'full-stop', amount: 0, remaining: 0.00013 });
    expect(store.orders[0].extra).toBeUndefined();
  });

  it('keeps exact leaves quantities for parents, brackets and amount-zero Full stops with rejection metadata', () => {
    const postOnly = (1 << 20) | (4 << 24) | 256;
    const frame = fixture({ native: true, orders: [
      { id: 'entry', amount: 10, filled: 2, remaining: 0.00013, native: 7, enums: postOnly },
      { id: 'entry__stop_loss', parentId: 'entry', amount: 10, filled: 0, remaining: 0.00013, native: 7, enums: (1 << 28) | (3 << 8) },
      { id: 'full-stop', amount: 0, filled: 0, remaining: 0.0002, native: 11, enums: 3 << 8 },
      { id: 'unknown', amount: 0, filled: 0, remaining: 0.0002 },
    ] });
    expect(Array.from(frame.orders()!.remainingNative!)).toEqual([7, 7, 11, NaN]);
    const store = new DefaultStore();
    new FrameApplier(store).apply(frame);
    expect(store.orders[0].extra).toEqual({ remainingNative: 7, remainingNotional: 7, rejectReason: 'PostOnly' });
    expect(store.orders[0].timeInForce).toBe('PostOnly');
    expect(store.orders[1]).toMatchObject({ parentId: 'entry', slTriggerType: 'mark_price', extra: { remainingNative: 7, remainingNotional: 7 } });
    expect(store.orders[2]).toMatchObject({ amount: 0, extra: { remainingNative: 11, remainingNotional: 11 } });
    expect(store.orders[3].extra).toBeUndefined();
  });

  it('retains inverse position extras on later position-only frames and closes a copier in native contracts', async () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(fixture());
    applier.apply(fixture({ markets: false, positions: true }));
    expect(store.positions[0]).toMatchObject({ contracts: 0.02, extra: { contractsNative: 1000, unrealizedPnlNative: 0.002 } });
    const ticker = { id: 'BTCUSD', symbol: 'BTCUSD', last: 50000 } as Ticker;
    store.update({ tickers: [ticker] });
    const masterStore = new DefaultStore();
    masterStore.update({ markets: store.markets, tickers: [ticker], positions: [] });
    const placeOrder = vi.fn().mockResolvedValue([]);
    const manager = new CrossTradeManager({ getExchange: () => undefined, placeOrder });
    const audit = manager as unknown as { _auditCopierPositions(config: CrossTradeConfig, copier: CrossTradeCopier, master: Exchange, copierExchange: Exchange): Promise<void> };
    await audit._auditCopierPositions(
      { enabled: true, masterAccount: 'master', masterSymbol: 'BTCUSD', masterWeight: 1, copiers: [] },
      { enabled: true, account: 'copier', weight: 1, execution: 'Market', weightMode: 'Absolute', symbolMappings: {} },
      { name: 'bybitinverse', store: masterStore } as unknown as Exchange,
      { name: 'bybitinverse', store } as unknown as Exchange,
    );
    expect(placeOrder).toHaveBeenCalledOnce();
    expect(placeOrder.mock.calls[0][5]).toBe(1000);
    expect(placeOrder.mock.calls[0][6]).toBe(true);
  });

  it('preserves explicit native order quantities without inventing inverse extras on linear positions', () => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(fixture({ inverse: false, native: true, positions: true, orders: [
      { id: 'linear', amount: 1, filled: 0, remaining: 1, native: 1 },
    ] }));
    expect(store.positions[0].extra).toBeUndefined();
    expect(store.orders[0].extra).toEqual({ remainingNative: 1 });
  });

  it('keeps venue-specific legacy inverse sizing and exact native tails through the main-thread bridge', () => {
    for (const venue of ['bybitinverse', 'bitgetinverse'] as const) {
      for (const native of [false, true]) {
        const store = new DefaultStore();
        const exchange = new BaseExchange(venue, { key: '', secret: '' }, store);
        exchange.applyWorkerFrame(fixture({ native, positions: true, units: venue === 'bitgetinverse' ? 2 : 0, orders: [
          { id: 'inverse', amount: 1000, filled: 100, remaining: 0.018, native: 777 },
        ] }).buffer);
        expect(store.positions[0].extra).toMatchObject({ contractsNative: 1000, unrealizedPnlNative: 0.002 });
        if (native) expect(store.orders[0].extra).toEqual({ remainingNative: 777, remainingNotional: 777 });
        else if (venue === 'bitgetinverse') expect(store.orders[0].extra).toEqual({ remainingNative: 900 });
        else expect(store.orders[0].extra).toBeUndefined();
        exchange.dispose();
      }
    }
  });
});

describe('OKX native contract count bridge', () => {
  it.each(['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const)('%s preserves native copier and chaser counts through worker frames', (name) => {
    const store = new DefaultStore();
    const exchange = new BaseExchange(name, { key: '', secret: '' }, store);
    const workerStore = new DefaultStore();
    const worker = new RustEngineExchange(name, { key: '', secret: '', publicDataOnly: true }, workerStore);
    const frame = fixture({ units: 3, contractSize: 100, native: true, positions: true, positionPnl: 100, orders: [
      { id: 'entry', amount: 10, filled: 0.004, remaining: 0.016, native: 8 },
      { id: 'full-stop', amount: 0, filled: 0, remaining: 0.02, native: 10, filledNative: 2, enums: 3 << 8 },
    ] });
    exchange.applyWorkerFrame(frame.buffer);
    (worker as unknown as { applier: FrameApplier }).applier.apply(frame);
    for (const receiver of [store, workerStore]) {
      expect(receiver.orders[0]).toMatchObject({ amount: 10, filled: 0.004, remaining: 0.016, extra: { remainingNative: 8, filledInPlacementUnits: 2 } });
      expect(receiver.orders[0].extra?.remainingNotional).toBe(800);
      expect(receiver.orders[1].extra).toEqual({ remainingNative: 10, remainingNotional: 1000, filledInPlacementUnits: 2 });
      expect(receiver.positions[0].extra).toEqual({ contractsNative: 10 });
      expect(receiver.positions[0].unrealizedPnl).toBe(100);
    }
    worker.dispose();
    exchange.dispose();
  });
  it('retains linear native placement counts separately from displayed base size', () => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(fixture({ units: 3, inverse: false, contractSize: 0.001, positions: true, orders: [
      { id: 'linear', amount: 10, filled: 0.002, remaining: 0.008 },
    ] }));
    expect(store.orders[0].extra).toEqual({ remainingNative: 8, filledInPlacementUnits: 2 });
    expect(store.positions[0].extra).toEqual({ contractsNative: 20 });
  });
  it('cannot infer exact inverse order leaves from a legacy base-sized remainder', () => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(fixture({ units: 3, contractSize: 100, orders: [
      { id: 'legacy', amount: 10, filled: 0.004, remaining: 0.016 },
    ] }));
    expect(store.orders[0].extra).toBeUndefined();
  });
});

describe('per-market quantity units from MARKETS flags', () => {
  const orders: OrderRow[] = [
    { id: 'exact', amount: 7, filled: 2, remaining: 0.004, native: 5 },
    { id: 'legacy', amount: 7, filled: 2, remaining: 0.5 },
  ];
  // QUANTITY_UNITS order: base, nativePlacement, inverseNative, contractCount, inverseContractSized.
  it.each([
    [0, { remainingNative: 5, remainingNotional: 5 }, undefined, { contractsNative: 1000, unrealizedPnlNative: 0.002 }],
    [1, { remainingNative: 5, remainingNotional: 500 }, { remainingNative: 0.005 }, { contractsNative: 10 }],
    [2, { remainingNative: 5, remainingNotional: 5 }, { remainingNative: 5 }, { contractsNative: 1000, unrealizedPnlNative: 0.002 }],
    [3, { remainingNative: 5, remainingNotional: 500, filledInPlacementUnits: 2 }, undefined, { contractsNative: 10 }],
    [4, { remainingNative: 5, remainingNotional: 500 }, undefined, { contractsNative: 10, unrealizedPnlNative: 0.002 }],
  ])('units %i gives its documented inverse order and position extras', (units, exact, legacy, position) => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(fixture({ units, contractSize: 100, native: true, positions: true, orders }));
    expect(store.orders[0].extra).toEqual(exact);
    expect(store.orders[1].extra).toEqual(legacy);
    expect(store.positions[0].extra).toEqual(position);
  });

  it('sizes a row whose market is missing from the table with the table units', () => {
    const orphan = (units: number) => {
      const store = new DefaultStore();
      new FrameApplier(store).apply(fixture({ units, contractSize: 100, native: true, orderMarket: 7, orders: [
        { id: 'orphan', amount: 7, filled: 2, remaining: 0.004, native: 5 },
      ] }));
      return store.orders[0].extra;
    };
    expect(orphan(3)).toEqual({ remainingNative: 5, filledInPlacementUnits: 2 });
    expect(orphan(0)).toEqual({ remainingNative: 5 });
  });
});
