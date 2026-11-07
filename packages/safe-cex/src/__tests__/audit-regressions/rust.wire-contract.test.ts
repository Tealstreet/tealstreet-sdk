import type { Frame } from '@tealstreet/safe-cex-wasm/wire';

import { expect, it } from 'vitest';

import { FrameApplier, FrameApplyError } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';

const frame = (overrides: object = {}) =>
  ({
    markets: () => undefined,
    tickers: () => undefined,
    tickerDeltas: () => undefined,
    orders: () => undefined,
    positions: () => undefined,
    balance: () => undefined,
    status: () => undefined,
    settings: () => undefined,
    fills: () => undefined,
    candles: () => undefined,
    ...overrides,
  }) as unknown as Frame;
// `units` is the QUANTITY_UNITS index each market carries in flag bits 8..12.
const markets = (symbols: string[], units = 0) => ({
  count: symbols.length,
  id: (i: number) => symbols[i],
  symbol: (i: number) => symbols[i],
  base: (i: number) => symbols[i],
  quote: () => 'USD',
  flags: Float64Array.from(symbols, () => 1 | (units << 8)),
  ...Object.fromEntries(
    [
      'precisionAmount',
      'precisionPrice',
      'minAmount',
      'maxAmount',
      'minLeverage',
      'maxLeverage',
      'minPrice',
      'contractSize',
      'takerFee',
      'makerFee',
    ].map((key) => [key, Float64Array.from(symbols, () => 1)]),
  ),
});
const fills = (symbol?: string) => ({
  count: 1,
  market: Uint32Array.of(0),
  side: Uint32Array.of(0),
  rows: Float64Array.of(100, 0.5),
  orderId: () => 'tracked',
  ...(symbol !== undefined && { symbol: () => symbol }),
});

it('keeps the last good table and uses explicit fill identity when new market metadata is malformed', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  applier.apply(frame({ markets: () => markets(['A', 'B']) }));
  const broken = frame({
    markets: () => markets(['B', 'A']),
    derivativeProducts: () => {
      throw new Error('bad product column');
    },
    fills: () => fills('B'),
    balance: () => ({ used: 1, free: 2, total: 3, upnl: 0 }),
  });
  expect(() => applier.apply(broken)).toThrow(FrameApplyError);
  expect(store.markets.map((row) => row.symbol)).toEqual(['A', 'B']);
  expect(store.balance.total).toBe(3);
  expect(applier.fills(broken)[0]).toMatchObject({ orderId: 'tracked', symbol: 'B', amount: 0.5 });
  const pending = frame({ fills: () => fills('B') });
  expect(() => applier.apply(pending)).toThrow(FrameApplyError);
  expect(applier.fills(pending)[0].symbol).toBe('B');
  const legacy = frame({ fills: () => fills() });
  expect(() => applier.apply(legacy)).toThrow(FrameApplyError);
  expect(() => applier.fills(legacy)).toThrow(/identity/);
  applier.apply(frame({ markets: () => markets(['B', 'A']) }));
  expect(() => applier.apply(frame())).not.toThrow();
});

it.each(
  ([['native', 1], ['contracts', 3], ['inverse', 2]] as const).flatMap(([profile, units]) =>
    [[], ['OTHER']].map((table) => [profile, units, table] as const),
  ),
)(
  'retains orphan identity without deriving NaN %s position quantities (units %i, table %j)',
  (_profile, units, table) => {
    const store = new DefaultStore();
    const applier = new FrameApplier(store);
    applier.apply(frame({ markets: () => markets(['ORPHAN'], units) }));
    const positions = {
      count: 1,
      market: Uint32Array.of(0xffffffff),
      enums: Uint32Array.of(0),
      id: () => 'orphan-position',
      symbol: () => 'ORPHAN',
      ...Object.fromEntries(
        [
          'entryPrice',
          'notional',
          'leverage',
          'unrealizedPnl',
          'realizedPnl',
          'contracts',
          'liquidationPrice',
          'marginSize',
          'lastUpdate',
        ].map((key) => [key, Float64Array.of(1)]),
      ),
    };
    // Orphan rows keep the venue's units: the table's, or the last table's when it empties.
    applier.apply(frame({ markets: () => markets([...table], units), positions: () => positions }));
    expect(store.positions[0]).toMatchObject({ id: 'orphan-position', symbol: 'ORPHAN' });
    expect(store.positions[0].extra).toBeUndefined();
  },
);

it('leaves unknown fill sides and derivative kinds absent', () => {
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  const product = {
    count: 1,
    market: Uint32Array.of(0),
    kind: Uint32Array.of(99),
    expiry: Float64Array.of(100),
    strike: Float64Array.of(2),
    underlying: () => 'A',
    style: () => 'european',
    settlement: () => 'cash',
    settleAsset: () => 'USD',
    premiumAsset: () => 'USD',
  };
  const current = frame({
    markets: () => markets(['A']),
    derivativeProducts: () => product,
    fills: () => ({ ...fills('A'), side: Uint32Array.of(99) }),
  });
  applier.apply(current);
  expect(store.markets[0].info.instrumentKind).toBeUndefined();
  expect(store.markets[0].option).toBeUndefined();
  expect(applier.fills(current)[0].side).toBeUndefined();
});
