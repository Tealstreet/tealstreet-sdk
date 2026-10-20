import type { Exchange } from '../../exchanges/base';
import type { CrossTradeConfig, CrossTradeCopier } from '../../crossTrade/types';
import type { Ticker } from '../../types';

import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { describe, expect, it, vi } from 'vitest';

import { CrossTradeManager } from '../../crossTrade/runtime/CrossTradeManager';
import { BaseExchange } from '../../exchanges/base';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

const align8 = (n: number) => Math.ceil(n / 8) * 8;
type OrderRow = { id: string; parentId?: string; amount: number; filled: number; remaining: number; native?: number; enums?: number };

// Independent encoder of the documented format-1 layout. The optional native
// tail must not change offsets in the original seven-column ORDERS table.
const fixture = ({ native = false, inverse = true, orders = [] as OrderRow[], positions = false, markets = true, contractSize = NaN } = {}) => {
  const strings = ['BTCUSD', 'BTC', 'USD', ...orders.flatMap((row) => [row.id, ...(row.parentId ? [row.parentId] : [])])];
  const ref = (s: string | undefined) => s === undefined ? 0xffffffff : strings.indexOf(s);
  const sections: Array<{ kind: number; bytes: Uint8Array }> = [];
  const table = (kind: number, words: number[], columns: number[][]) => {
    const bytes = new Uint8Array(align8(words.length * 4) + columns.reduce((n, column) => n + column.length * 8, 0));
    const view = new DataView(bytes.buffer);
    words.forEach((word, i) => view.setUint32(i * 4, word, true));
    let offset = align8(words.length * 4);
    for (const column of columns) for (const value of column) {
      view.setFloat64(offset, value, true);
      offset += 8;
    }
    sections.push({ kind, bytes });
  };
  if (markets) table(2, [1, 0, 0, 1, 2, inverse ? 3 : 1], [1, 0.1, 1, 100000, 1, 100, NaN, contractSize, NaN, NaN].map((n) => [n]));
  const columns = [
    orders.map(() => 54321), orders.map((o) => o.amount), orders.map((o) => o.filled),
    orders.map((o) => o.remaining), orders.map(() => 1234), orders.map(() => NaN), orders.map(() => NaN),
  ];
  if (native) columns.push(orders.map((o) => o.native ?? NaN));
  table(7, [
    orders.length,
    ...orders.flatMap((o) => [ref(o.id), 0xffffffff, ref(o.parentId)]),
    ...orders.map(() => 0), ...orders.map((o) => o.enums ?? 256),
  ], columns);
  if (positions) table(8, [1, 0, (2 << 8) | (2 << 16)], [49000, 1000, 10, 0.002, 0.001, 0.02, 40000, 100, 1234].map((n) => [n]));
  const encoded = strings.map((s) => new TextEncoder().encode(s));
  const bytes = new Uint8Array(align8(4 + (strings.length + 1) * 4 + encoded.reduce((n, s) => n + s.length, 0)));
  const view = new DataView(bytes.buffer);
  view.setUint32(0, strings.length, true);
  let end = 0;
  encoded.forEach((s, i) => {
    view.setUint32(4 + i * 4, end, true);
    bytes.set(s, 4 + (strings.length + 1) * 4 + end);
    end += s.length;
  });
  view.setUint32(4 + strings.length * 4, end, true);
  sections.push({ kind: 1, bytes });
  const buffer = new ArrayBuffer(16 + sections.reduce((n, section) => n + 8 + section.bytes.length, 0));
  const header = new DataView(buffer);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, sections.length, true);
  let offset = 16;
  for (const section of sections) {
    header.setUint16(offset, section.kind, true);
    header.setUint32(offset + 4, section.bytes.length, true);
    new Uint8Array(buffer, offset + 8, section.bytes.length).set(section.bytes);
    offset += 8 + section.bytes.length;
  }
  return new Frame(buffer);
};

describe('Rust frames preserve inverse native units', () => {
  it('converts COIN-M USD notionals to native contract counts in worker and main stores', () => {
    const value = fixture({
      contractSize: 100,
      native: true,
      positions: true,
      orders: [{ id: 'coin-m', amount: 7, filled: 2, remaining: 5, native: 5 }],
    });
    const workerStore = new DefaultStore();
    const applier = new FrameApplier(workerStore, false, false, true);
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

  it('does not add inverse extras to linear positions or orders', () => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(fixture({ inverse: false, native: true, positions: true, orders: [
      { id: 'linear', amount: 1, filled: 0, remaining: 1, native: 1 },
    ] }));
    expect(store.positions[0].extra).toBeUndefined();
    expect(store.orders[0].extra).toBeUndefined();
  });

  it('keeps venue-specific legacy inverse sizing and exact native tails through the main-thread bridge', () => {
    for (const venue of ['bybitinverse', 'bitgetinverse'] as const) {
      for (const native of [false, true]) {
        const store = new DefaultStore();
        const exchange = new BaseExchange(venue, { key: '', secret: '' }, store);
        exchange.applyWorkerFrame(fixture({ native, positions: true, orders: [
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
