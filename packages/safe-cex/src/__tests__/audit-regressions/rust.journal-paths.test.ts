import type { ExchangeName, Market } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { historyCompletenessOf } from '../../historyCompleteness';
import { mapBitunixPositionHistory } from '../../journal/bitunix.position-adapter';
import { mapBlofinPositionHistory } from '../../journal/blofin.position-adapter';
import { mapOkxPositionHistory } from '../../journal/okx.position-adapter';
import { FrameStore } from '../../store/frameStore';
import { PositionSide } from '../../types';
import { nativeFixture } from '../cutover/nativeFixture';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

type Command = (command: Record<string, unknown>, options?: { report?: boolean }) => Promise<unknown>;

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

const make = (name: ExchangeName, responses: Record<string, unknown>) => {
  const exchange = new RustEngineExchange(name, { key: 'k', secret: 's', passphrase: 'p' }, new FrameStore());
  exchanges.push(exchange);
  adoptEngineCapabilities(exchange);
  const command = vi.spyOn(exchange as unknown as { command: Command }, 'command').mockImplementation(async (c) => {
    const response = responses[String(c.op)] ?? [];
    if (response instanceof Error) throw response;
    return response;
  });
  return { exchange, command };
};

const market = (overrides: Partial<Market>): Market =>
  ({
    id: 'BTC-USDT-SWAP',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.01, price: 0.1 },
    limits: { amount: { min: 0.01, max: 1000 }, leverage: { min: 1, max: 100 } },
    contractSize: 0.01,
    ...overrides,
  }) satisfies Market;

const BITUNIX_ROWS = [
  {
    positionId: 'p1',
    symbol: 'BTCUSDT',
    side: 'BUY',
    qty: '0.5',
    maxQty: '0.5',
    entryPrice: '60000',
    closePrice: '61000',
    fee: '1.2',
    funding: '0',
    realizedPNL: '498.8',
    leverage: '10',
    marginMode: 'CROSS',
    positionMode: 'ONE_WAY',
    ctime: '1700000000000',
    mtime: '1700000500000',
  },
  {
    positionId: 'p2',
    symbol: 'ETHUSDT',
    side: 'SELL',
    qty: '2',
    maxQty: '3',
    entryPrice: '2000',
    closePrice: '1990',
    fee: '0.4',
    funding: '-0.1',
    realizedPNL: '19.6',
    leverage: '5',
    marginMode: 'ISOLATION',
    positionMode: 'HEDGE',
    ctime: '1700001000000',
    mtime: '1700002000000',
  },
];

/** The TS venue over the same raw rows: its pager is the only thing replaced. */
const tsBitunix = (rows: Array<Record<string, unknown>>) => {
  const exchange = new BitunixExchange(
    'bitunix',
    { key: 'k', secret: 's', tuning: { consoleLogging: false } },
    new FrameStore(),
  );
  vi.spyOn(
    exchange as unknown as { fetchBitunixHistoryPages: () => Promise<unknown> },
    'fetchBitunixHistoryPages',
  ).mockResolvedValue(rows);
  return exchange;
};

type Fold = { foldClosedPositionRecords: (...args: unknown[]) => Promise<unknown[]> };

describe('Rust journal paths follow the TS venue declarations', () => {
  const close = (orderId: string, at: number, entry = 100) => ({
    symbol: 'BTCUSDT',
    orderId,
    side: 'Sell',
    closedSize: '1',
    avgEntryPrice: String(entry),
    avgExitPrice: String(entry + 10),
    closedPnl: '10',
    openFee: '0',
    closeFee: '0',
    createdTime: String(at),
    updatedTime: String(at),
  });
  const fill = (id: string, at: number, side: 'buy' | 'sell', orderId = id, price = side === 'sell' ? 110 : 100) => ({
    id,
    orderId,
    symbol: 'BTCUSDT',
    timestamp: at / 1000,
    side,
    price,
    qty: 1,
  });

  const positionEvidence = (
    source: readonly { id: string; timestamp: number; qty: number }[],
    sizes: readonly number[],
    cashFlows: readonly number[],
  ) =>
    source.map((item, index) => ({
      tradeId: item.id,
      positionAfter: String(sizes[index]),
      executionQuantity: String(item.qty),
      symbol: 'BTCUSDT',
      timestamp: item.timestamp * 1000,
      tradeCashFlow: String(cashFlows[index]),
      income: cashFlows[index],
      incomeType: cashFlows[index] === 0 ? 'OTHER' : 'REALIZED_PNL',
      category: cashFlows[index] === 0 ? 'other' : 'realized_pnl',
      asset: 'USDT',
      tranId: `${item.id}:source`,
    }));

  it.each([5, -5, 0])('TEAL-7 Y2 removes signed funding %s from native net PnL', async (funding) => {
    const fills = [
      { ...fill('fund-open', 1000, 'buy', 'open'), commission: 0.4, commissionAsset: 'USDT' },
      { ...fill('fund-close', 4000, 'sell', 'close'), commission: 0.6, commissionAsset: 'USDT' },
    ];
    const rows = [{ ...close('close', 4000), closedPnl: String(10 - 1 + funding), openFee: '0.4', closeFee: '0.6' }];
    const [opened, closed] = positionEvidence(fills, [1, 0], [0, 10]);
    const income = [
      { ...opened, income: -0.4, incomeType: 'COMMISSION', category: 'commission', tradeCashFlow: '0' },
      { ...closed, tradeCashFlow: '10' },
      {
        ...closed,
        income: -0.6,
        incomeType: 'COMMISSION',
        category: 'commission',
        tranId: 'fund-close:fee',
        tradeCashFlow: '10',
      },
      ...(funding === 0
        ? []
        : [
            {
              symbol: 'BTCUSDT',
              asset: 'USDT',
              category: 'funding',
              incomeType: 'FUNDING_FEE',
              income: funding,
              timestamp: 2000,
              tranId: 'native-funding',
            },
          ]),
    ];
    const { exchange, command } = make('bybit', {
      fetchClosedPositions: rows,
      fetchOrderHistory: [
        { id: 'open', positionSide: 'both' },
        { id: 'close', positionSide: 'both' },
      ],
      fetchFills: fills,
      fetchIncome: income,
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1000, to: 5000 });
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({ realizedPnl: 10, fees: 1, funding, fundingAttributed: true, openedAt: 1000 });
    expect(command.mock.calls.every(([input]) => String(input.op).startsWith('fetch'))).toBe(true);
  });

  it.each([
    'missing cash flow',
    'conflicting cash flow',
    'missing fee',
    'empty fee',
    'wrong currency',
    'identity mismatch',
    'funded hedge',
  ])('TEAL-7 Y2 leaves %s incomplete instead of inventing financial context', async (problem) => {
    const fills = [fill('open', 1000, 'buy'), fill('close', 4000, 'sell')];
    const nativeClose: Record<string, unknown> = {
      ...close('close', 4000),
      closedPnl: '14',
      openFee: '0.4',
      closeFee: '0.6',
    };
    const income = positionEvidence(fills, [1, 0], [0, 10]);
    if (problem === 'missing cash flow') income[1].tradeCashFlow = '';
    if (problem === 'conflicting cash flow')
      income.push({ ...income[1], tradeCashFlow: '11', tranId: 'conflicting-source' });
    if (problem === 'missing fee') delete nativeClose.openFee;
    if (problem === 'empty fee') nativeClose.openFee = '';
    if (problem === 'wrong currency') income[1].asset = 'BTC';
    if (problem === 'identity mismatch') nativeClose.closedPnl = '999';
    const funding = {
      symbol: 'BTCUSDT',
      asset: 'USDT',
      category: 'funding',
      incomeType: 'FUNDING_FEE',
      income: 5,
      timestamp: 2000,
      tranId: 'funding',
    };
    const { exchange } = make('bybit', {
      fetchClosedPositions: [nativeClose],
      fetchOrderHistory: [
        { id: 'open', positionSide: problem === 'funded hedge' ? 'long' : 'both' },
        { id: 'close', positionSide: problem === 'funded hedge' ? 'long' : 'both' },
      ],
      fetchFills: fills,
      fetchIncome: [...income, funding],
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1000, to: 5000 });
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)?.incomplete).toBe(true);
  });

  it('TEAL-7 Y1 keeps two real reopenings at the same entry price separate', async () => {
    const rows = [close('c1', 1200), close('c2', 1400)];
    const fills = [
      fill('1', 1100, 'buy'),
      fill('2', 1200, 'sell', 'c1'),
      fill('3', 1300, 'buy'),
      fill('4', 1400, 'sell', 'c2'),
    ];
    const orders = fills.map((item) => ({ id: item.orderId, positionSide: 'both' }));
    const { exchange } = make('bybit', {
      fetchClosedPositions: rows,
      fetchOrderHistory: orders,
      fetchFills: fills,
      fetchIncome: positionEvidence(fills, [1, 0, 1, 0], [0, 10, 0, 10]),
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1000, to: 1500 });
    expect(records).toHaveLength(2);
    expect(records.map((record) => record.closedSize)).toEqual([1, 1]);
    expect(records.map((record) => record.closeCount)).toEqual([1, 1]);
  });

  it('TEAL-7 Y1 keeps one lifecycle together after an add changes its entry price', async () => {
    const rows = [close('c1', 2000), close('c2', 4000, 110)];
    const fills = [
      { ...fill('1', 1000, 'buy'), qty: 2 },
      fill('2', 2000, 'sell', 'c1'),
      fill('3', 3000, 'buy', 'add', 120),
      { ...fill('4', 4000, 'sell', 'c2', 115), qty: 2 },
    ];
    rows[1].closedSize = '2';
    rows[1].avgExitPrice = '115';
    const orders = fills.map((item) => ({ id: item.orderId, positionSide: 'both' }));
    const { exchange } = make('bybit', {
      fetchClosedPositions: rows,
      fetchOrderHistory: orders,
      fetchFills: fills,
      fetchIncome: positionEvidence(fills, [2, 1, 2, 0], [0, 10, 0, 10]),
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1000, to: 5000 });
    expect(records).toHaveLength(1);
    expect(records[0]).toMatchObject({ closedSize: 3, closeCount: 2, realizedPnl: 20 });
    expect(records[0].avgEntryPrice).toBeCloseTo(320 / 3, 10);
    expect(records[0].avgExitPrice).toBeCloseTo(340 / 3, 10);
  });

  it('TEAL-7 Y1 overlapping history upserts cannot turn two +10 closes into +30', async () => {
    const rows = [close('c1', 2000), close('c2', 4000)];
    const fills = [
      fill('1', 1000, 'buy'),
      fill('2', 2000, 'sell', 'c1'),
      fill('3', 3000, 'buy'),
      fill('4', 4000, 'sell', 'c2'),
    ];
    const orders = fills.map((item) => ({ id: item.orderId, positionSide: 'both' }));
    const first = make('bybit', {
      fetchClosedPositions: rows.slice(0, 1),
      fetchOrderHistory: orders.slice(0, 2),
      fetchFills: fills.slice(0, 2),
      fetchIncome: positionEvidence(fills.slice(0, 2), [1, 0], [0, 10]),
    }).exchange;
    const second = make('bybit', {
      fetchClosedPositions: rows,
      fetchOrderHistory: orders,
      fetchFills: fills,
      fetchIncome: positionEvidence(fills, [1, 0, 1, 0], [0, 10, 0, 10]),
    }).exchange;
    const stored = new Map<string, number>();
    for (const { exchange, to } of [
      { exchange: first, to: 2500 },
      { exchange: second, to: 5000 },
    ]) {
      for (const record of await exchange.fetchHistoricalPositions({ account: 'acct', from: 1000, to })) {
        stored.set(record.key, record.realizedPnl);
      }
    }
    expect([...stored.values()].reduce((sum, value) => sum + value, 0)).toBe(20);
    expect(stored.size).toBe(2);
  });

  it('bybit folds the reported rows with hedge legs joined from the order axis', async () => {
    const rows = [
      {
        symbol: 'BTCUSDT',
        orderId: 'o1',
        side: 'Sell',
        closedSize: '1',
        avgEntryPrice: '100',
        avgExitPrice: '110',
        closedPnl: '10',
        openFee: '0',
        closeFee: '0',
        createdTime: '1000',
        updatedTime: '1001',
      },
      {
        symbol: 'BTCUSDT',
        orderId: 'o2',
        side: 'Buy',
        closedSize: '1',
        avgEntryPrice: '120',
        avgExitPrice: '115',
        closedPnl: '5',
        openFee: '0',
        closeFee: '0',
        createdTime: '2000',
        updatedTime: '2001',
      },
    ];
    const fills = [
      fill('f1', 900, 'buy', 'open-long'),
      fill('f2', 1001, 'sell', 'o1', 110),
      fill('f3', 1900, 'sell', 'open-short', 120),
      fill('f4', 2001, 'buy', 'o2', 115),
    ];
    const orders = [
      { id: 'open-long', positionSide: PositionSide.Long },
      { id: 'open-short', positionSide: PositionSide.Short },
      { id: 'o1', positionSide: PositionSide.Long },
      { id: 'o2', positionSide: PositionSide.Short },
    ];
    const { exchange, command } = make('bybit', {
      fetchClosedPositions: rows,
      fetchOrderHistory: orders,
      fetchFills: fills,
      fetchIncome: positionEvidence(fills, [1, 0, -1, 0], [0, 10, 0, 5]),
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 3000 });
    expect(command).toHaveBeenCalledWith(
      { op: 'fetchClosedPositions', symbol: undefined, from: 1 - 14 * 86400000, to: 3000 },
      { report: false },
    );
    expect(records.map((record) => record.positionSide)).toEqual([PositionSide.Long, PositionSide.Short]);
    expect(records.map((record) => record.realizedPnl)).toEqual([10, 5]);
    expect(records).toHaveLength(2);
  });


  it.each([{ orders: [] }, { orders: [{ id: 'known', positionSide: PositionSide.Long }] }])(
    'keeps a closed-PnL range unresolved when the order axis omits a closing order: %j',
    async ({ orders }) => {
      const rows = [
        { symbol: 'BTCUSDT', orderId: 'known', side: 'Sell', closedSize: '1', avgEntryPrice: '100', avgExitPrice: '110', closedPnl: '10', updatedTime: '1001' },
        { symbol: 'BTCUSDT', orderId: 'missing', side: 'Sell', closedSize: '1', avgEntryPrice: '100', avgExitPrice: '120', closedPnl: '20', updatedTime: '1002' },
      ];
      const original = structuredClone(rows);
      const responses = { fetchClosedPositions: rows, fetchOrderHistory: orders };
      // Canonical bybit now joins native lifecycles (unresolvedLifecycle); this pins the
      // closed-PnL aggregate that the bybitdemo variant keeps (moved off 'bybit' in the #3679 master merge).
      const { exchange } = make('bybitdemo', responses);
      const unresolved = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2000 });
      // A partial aggregate could later acquire a different first-order key.
      // Retain the source rows and publish no confident aggregate until every join resolves.
      expect(unresolved).toEqual([]);
      expect(unresolved).toHaveProperty('incomplete', true);
      expect(unresolved).toHaveProperty('incompleteReason', 'unresolvedHedgeLeg');
      expect(rows).toEqual(original);
      responses.fetchOrderHistory = [
        { id: 'known', positionSide: PositionSide.Long },
        { id: 'missing', positionSide: PositionSide.Long },
      ];
      const repaired = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2000 });
      expect(repaired).toHaveLength(1);
      expect(repaired[0]).toMatchObject({ positionSide: PositionSide.Long, closeCount: 2, closedSize: 2 });
      expect(repaired).not.toHaveProperty('incomplete');
    },
  );

  it('retains a positively joined one-way leg', async () => {
    const row = { symbol: 'BTCUSDT', orderId: 'one-way', side: 'Sell', closedSize: '1', avgEntryPrice: '100', avgExitPrice: '110', closedPnl: '10', updatedTime: '1001' };
    // Variant closed-PnL aggregate path; canonical bybit's one-way join is in bybit.lifecycles.test.ts.
    const { exchange } = make('bybitdemo', {
      fetchClosedPositions: [row], fetchOrderHistory: [{ id: 'one-way', positionSide: 'both' }],
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2000 });
    expect(records).toHaveLength(1);
    expect(records[0].positionSide).toBe('both');
    expect(records).not.toHaveProperty('incomplete');
  });

  it('bybit refuses to present an unjoined history and propagates a failed row read', async () => {
    const failedOrders = make('hyrotrader', {
      fetchClosedPositions: [],
      fetchOrderHistory: new Error('down'),
    }).exchange;
    await expect(failedOrders.fetchHistoricalPositions({ account: 'a' })).rejects.toThrow(
      'bybit: could not read the order axis to resolve hedge legs',
    );
    const failedRows = make('bybitdemo', {
      fetchClosedPositions: new Error('bybit closed-PnL fetch failed: x'),
    }).exchange;
    await expect(failedRows.fetchHistoricalPositions({ account: 'a' })).rejects.toThrow(
      'bybit closed-PnL fetch failed: x',
    );
  });

  it('keeps the unaudited Hyro compatibility path on its existing two source axes', async () => {
    const { exchange, command } = make('hyrotrader', {
      fetchClosedPositions: [close('close', 2000)],
      fetchOrderHistory: [{ id: 'close', positionSide: 'both' }],
    });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1000, to: 3000 });
    expect(records).toHaveLength(1);
    expect(records[0].realizedPnl).toBe(10);
    expect(command.mock.calls.map(([input]) => input.op)).toEqual(['fetchClosedPositions', 'fetchOrderHistory']);
  });

  it('okx maps known linear rows but marks an unresolved historical market incomplete', async () => {
    const linear = {
      instId: 'BTC-USDT-SWAP',
      direction: 'long',
      closeTotalPos: '3',
      openMaxPos: '4',
      openAvgPx: '40000',
      closeAvgPx: '41000',
      realizedPnl: '1.5',
      fee: '-0.1',
      fundingFee: '0',
      cTime: '1709000000000',
      uTime: '1709500000000',
      type: '2',
    };
    const inverse = { ...linear, instId: 'BTC-USD-SWAP' };
    const unknown = { ...linear, instId: 'NOPE-USDT-SWAP' };
    const { exchange } = make('okxeu', { fetchClosedPositions: [linear, inverse, unknown] });
    const markets = [
      market({}),
      market({ id: 'BTC-USD-SWAP', symbol: 'BTCUSD', quote: 'USD', contractSize: 100, isInverse: true }),
    ];
    exchange.store.apply(
      nativeFixture(
        { markets },
        exchange.store.markets.map((row) => row.symbol),
      ),
    );
    const records = await exchange.fetchHistoricalPositions({ account: 'acct' });
    expect(records).toEqual([
      mapOkxPositionHistory(linear, {
        account: 'acct',
        toBase: (contracts) => contracts * 0.01,
        resolveSymbol: () => 'BTCUSDT',
        resolveSettleCurrency: () => 'USDT',
      }),
    ]);
    expect(historyCompletenessOf(records)).toEqual({ incomplete: true, reason: 'coverageGap' });
    // Keep the source rows/instId available without fabricating a normalized symbol or size.
    expect(await exchange.fetchClosedPositions({})).toEqual([linear, inverse, unknown]);
    expect(Object.getOwnPropertyDescriptor(records, 'incomplete')?.enumerable).toBe(false);
  });

  it('okx does not present an unresolved-only historical window as complete', async () => {
    const unknown = {
      instId: 'DELISTED-USDT-SWAP',
      direction: 'long',
      closeTotalPos: '3',
      openAvgPx: '40000',
      closeAvgPx: '41000',
      realizedPnl: '1.5',
      fee: '-0.1',
      cTime: '1709000000000',
      uTime: '1709500000000',
    };
    const { exchange } = make('okx', { fetchClosedPositions: [unknown] });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct' });
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)).toEqual({ incomplete: true, reason: 'coverageGap' });
    expect(await exchange.fetchClosedPositions({})).toEqual([unknown]);
  });

  it('okx keeps known inverse rows outside the linear journal grade', async () => {
    const inverse = {
      instId: 'BTC-USD-SWAP',
      direction: 'long',
      closeTotalPos: '3',
      openAvgPx: '40000',
      closeAvgPx: '41000',
      realizedPnl: '1.5',
      fee: '-0.1',
      cTime: '1709000000000',
      uTime: '1709500000000',
    };
    const { exchange } = make('okx', { fetchClosedPositions: [inverse] });
    exchange.store.apply(
      nativeFixture(
        { markets: [market({ id: 'BTC-USD-SWAP', symbol: 'BTCUSD', quote: 'USD', contractSize: 100, isInverse: true })] },
        exchange.store.markets.map((row) => row.symbol),
      ),
    );
    const records = await exchange.fetchHistoricalPositions({ account: 'acct' });
    expect(records).toEqual([]);
    expect(historyCompletenessOf(records)).toBeUndefined();
  });

  it('blofin maps rows through its position adapter', async () => {
    const row = {
      positionId: 'p',
      historyId: '7',
      instId: 'BTC-USDT',
      positionSide: 'long',
      direction: 'long',
      openAveragePrice: '60000',
      closeAveragePrice: '61000',
      closePositions: '1',
      realizedPnl: '10',
      fee: '-0.1',
      fundingFee: '0',
      createTime: '1700000000000',
      updateTime: '1700000001000',
    };
    const { exchange } = make('blofin', { fetchClosedPositions: [row] });
    exchange.store.apply(
      nativeFixture(
        { markets: [market({ id: 'BTC-USDT' })] },
        exchange.store.markets.map((row) => row.symbol),
      ),
    );
    const records = await exchange.fetchHistoricalPositions({ account: 'acct' });
    expect(records).toEqual([
      mapBlofinPositionHistory(row, {
        account: 'acct',
        resolveSymbol: () => 'BTCUSDT',
        resolveSettleCurrency: () => 'USDT',
      }),
    ]);
    expect(records).toHaveLength(1);
  });

  it('bitunix maps the reported closed axis through its position adapter, as the TS venue', async () => {
    const { exchange, command } = make('bitunix', { fetchClosedPositions: BITUNIX_ROWS });
    exchange.store.apply(
      nativeFixture(
        { markets: [market({ id: 'BTC/USDT' }), market({ id: 'ETH/USDT', symbol: 'ETHUSDT', quote: 'USDC' })] },
        exchange.store.markets.map((row) => row.symbol),
      ),
    );
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 });
    expect(command).toHaveBeenCalledWith(
      { op: 'fetchClosedPositions', symbol: undefined, from: 1, to: 2 },
      { report: false },
    );
    expect(records).toEqual(
      BITUNIX_ROWS.map((row) =>
        mapBitunixPositionHistory(row, {
          account: 'acct',
          resolveSettleCurrency: (s) => (s === 'ETHUSDT' ? 'USDC' : 'USDT'),
        }),
      ),
    );
    expect(records).toHaveLength(2);
    const ts = tsBitunix(BITUNIX_ROWS);
    ts.store.apply(
      nativeFixture(
        { markets: [market({ id: 'BTC/USDT' }), market({ id: 'ETH/USDT', symbol: 'ETHUSDT', quote: 'USDC' })] },
        ts.store.markets.map((row) => row.symbol),
      ),
    );
    expect(records).toEqual(await ts.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 }));
    ts.dispose();
  });

  it.each([
    ['bitget', false],
    ['bitgetusdc', false],
    ['bitgetinverse', false],
    ['hyperliquid', true],
    ['paradex', true],
  ] as const)('%s folds fills like TS (symbolScoped %s)', async (name, symbolScoped) => {
    const { exchange } = make(name, {});
    const fold = vi.spyOn(exchange as unknown as Fold, 'foldClosedPositionRecords').mockResolvedValue([]);
    await expect(exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 })).resolves.toEqual([]);
    expect(fold).toHaveBeenCalledWith({ account: 'acct', from: 1, to: 2 }, { symbolScoped });
  });

  it('bybitinverse declares no journal path, as its TS venue', async () => {
    const { exchange } = make('bybitinverse', { fetchClosedPositions: [] });
    await expect(exchange.fetchHistoricalPositions({ account: 'a' })).rejects.toThrow(
      'no journal records path declared',
    );
  });
});

it.each([
  ['bybit', 'fetchClosedPositions'],
  ['bybit', 'fetchOrderHistory'],
  ['okx', 'fetchClosedPositions'],
  ['blofin', 'fetchClosedPositions'],
  ['bitunix', 'fetchClosedPositions'],
  ['woo', 'fetchOrderHistory'],
] as const)('%s preserves incomplete %s through an empty derived history', async (name, axis) => {
  const responses = { fetchClosedPositions: [], fetchOrderHistory: [] };
  Object.defineProperty(responses[axis], 'incomplete', { value: true });
  const { exchange } = make(name, responses);
  const rows = await exchange.fetchHistoricalPositions({ account: 'account', from: 1, to: 2 });
  expect(rows).toEqual([]);
  expect(rows).toHaveProperty('incomplete', true);
  expect(Object.getOwnPropertyDescriptor(rows, 'incomplete')?.enumerable).toBe(false);
});
