import { Frame } from '@tealstreet/safe-cex-wasm/wire';
import { describe, expect, it } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

const tokenId = '123456789012345678901234567890123456789012345678901234567890123456789';
const align8 = (n: number) => Math.ceil(n / 8) * 8;

// Independent format-1 encoder of the documented prediction extension. IDs stay
// strings, shares stay base units, and optional tables never shift old columns.
const fixture = ({ markets = true, metadata = true, positions = true, holdings = true, flags = 2, empty = false, orderOnly = false, book = '' } = {}) => {
  const strings = [tokenId, 'Yes', 'pUSD', '0xcondition', 'Will café open?', 'Exact description', '2026-12-31T00:00:00Z', 'cafe-open', 'order'];
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
  if (!orderOnly) {
    if (markets) {
      table(2, [1, 0, 0, 1, 2, 5], [0.01, 0.001, 5, Infinity, 1, 1, NaN, NaN, NaN, NaN].map((value) => [value]));
      if (metadata) table(13, [1, 0, 3, 1, 4, 5, 6, 7, 1], [[0.001]]);
    }
    if (book === 'full') table(5, [0, 1, 1, 0], [[0.49, 2, 2, 0.51, 3, 3]]);
    if (book === 'delta') table(15, [0, 2, 1, 0], [[0.49, 0, 0.48, 4, 0.51, 5]]);
    if (positions) {
      table(8, empty ? [0] : [1, 0, 0], [0.4, 6, 1, 2, NaN, 10, 0, NaN, NaN].map((value) => empty ? [] : [value]));
      if (holdings) table(14, empty ? [0] : [1, 0, flags], []);
    }
  }
  else table(7, [1, 8, 0xffffffff, 0xffffffff, 0, 256], [0.5, 10, 0, 10, 1000, NaN, NaN].map((value) => [value]));

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
  return new Frame(structuredClone(buffer, { transfer: [buffer] }));
};

const marketInfo = {
  tokenId, conditionId: '0xcondition', outcome: 'Yes', tickSize: 0.001, negRisk: true,
  question: 'Will café open?', description: 'Exact description', endDate: '2026-12-31T00:00:00Z', slug: 'cafe-open',
};

describe('Rust prediction market frame metadata', () => {
  it('retains token identities, settlement flags and shares in both worker and main-thread receivers', () => {
    for (const main of [false, true]) {
      const store = new DefaultStore();
      const exchange = new BaseExchange('polymarket', { key: '', secret: '' }, store);
      const applier = new FrameApplier(store);
      const apply = (frame: Frame) => main ? exchange.applyWorkerFrame(frame.buffer) : applier.apply(frame);
      apply(fixture());
      expect(store.markets[0].info).toEqual(marketInfo);
      expect(store.markets[0]).toMatchObject({ id: tokenId, quote: 'pUSD', precision: { amount: 0.01, price: 0.001 } });
      expect(store.positions[0]).toMatchObject({ contracts: 10, notional: 6, leverage: 1, info: { tokenId, conditionId: '0xcondition', redeemable: false, mergeable: true } });
      apply(fixture({ orderOnly: true }));
      expect(store.markets[0].info).toEqual(marketInfo);
      expect(store.positions[0].info.mergeable).toBe(true);
      apply(fixture({ markets: false, flags: 1 }));
      expect(store.positions[0].info).toEqual({ tokenId, conditionId: '0xcondition', redeemable: true, mergeable: false });
      expect(store.markets[0].info).toEqual(marketInfo);
      exchange.dispose();
    }
  });

  it('clears flags and metadata on explicit empty or replacement tables', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(fixture());
    applier.apply(fixture({ markets: false, holdings: false }));
    expect(store.positions[0].info).toEqual({ tokenId, conditionId: '0xcondition', redeemable: false, mergeable: false });
    applier.apply(fixture({ markets: false, empty: true }));
    expect(store.positions).toEqual([]);
    applier.apply(fixture({ metadata: false, holdings: false }));
    expect(store.markets[0].info).toBeUndefined();
    expect(store.positions[0].info).toBeUndefined();
  });

  it('leaves ordinary venue frames without prediction metadata', () => {
    const store = new DefaultStore();
    new FrameApplier(store).apply(fixture({ metadata: false, holdings: false }));
    expect(store.markets[0].info).toBeUndefined();
    expect(store.positions[0].info).toBeUndefined();
  });

  it('keeps prediction sections 13/14 distinct from interleaved book deltas at section 15', () => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(fixture({ book: 'full' }));
    const update = fixture({ book: 'delta', flags: 1 });
    expect(update.predictionMarkets()!.count).toBe(1);
    expect(update.predictionHoldings()!.flags[0]).toBe(1);
    expect([...update.bookDeltas()]).toHaveLength(1);
    applier.apply(update);
    expect(store.markets[0].info).toEqual(marketInfo);
    expect(store.positions[0].info).toEqual({ tokenId, conditionId: '0xcondition', redeemable: true, mergeable: false });
    expect(applier.books(update)).toEqual([{ symbol: tokenId, orderbook: {
      bids: [{ price: 0.48, amount: 4, total: 4 }],
      asks: [{ price: 0.51, amount: 5, total: 5 }],
    } }]);
  });
});
