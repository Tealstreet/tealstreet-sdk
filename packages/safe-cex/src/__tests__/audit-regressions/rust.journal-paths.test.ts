vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ExchangeName, Market } from '../../types';

import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { mapBitunixPositionHistory } from '../../journal/bitunix.position-adapter';
import { mapBlofinPositionHistory } from '../../journal/blofin.position-adapter';
import { mapOkxPositionHistory } from '../../journal/okx.position-adapter';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { PositionSide } from '../../types';

type Command = (command: Record<string, unknown>, options?: { report?: boolean }) => Promise<unknown>;

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

const make = (name: ExchangeName, responses: Record<string, unknown>) => {
  const exchange = new RustEngineExchange(name, { key: 'k', secret: 's', passphrase: 'p' }, new DefaultStore());
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
  { positionId: 'p1', symbol: 'BTCUSDT', side: 'BUY', qty: '0.5', maxQty: '0.5', entryPrice: '60000', closePrice: '61000', fee: '1.2', funding: '0', realizedPNL: '498.8', leverage: '10', marginMode: 'CROSS', positionMode: 'ONE_WAY', ctime: '1700000000000', mtime: '1700000500000' },
  { positionId: 'p2', symbol: 'ETHUSDT', side: 'SELL', qty: '2', maxQty: '3', entryPrice: '2000', closePrice: '1990', fee: '0.4', funding: '-0.1', realizedPNL: '19.6', leverage: '5', marginMode: 'ISOLATION', positionMode: 'HEDGE', ctime: '1700001000000', mtime: '1700002000000' },
];

/** The TS venue over the same raw rows: its pager is the only thing replaced. */
const tsBitunix = (rows: Array<Record<string, unknown>>) => {
  const exchange = new BitunixExchange('bitunix', { key: 'k', secret: 's', tuning: { consoleLogging: false } }, new DefaultStore());
  vi.spyOn(exchange as unknown as { fetchBitunixHistoryPages: () => Promise<unknown> }, 'fetchBitunixHistoryPages').mockResolvedValue(rows);
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
      income: cashFlows[index],
      incomeType: cashFlows[index] === 0 ? 'OTHER' : 'REALIZED_PNL',
      category: cashFlows[index] === 0 ? 'other' : 'realized_pnl',
      asset: 'USDT',
      tranId: `${item.id}:source`,
    }));

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

  it('okx maps linear rows with contract conversion and skips inverse and unknown markets', async () => {
    const linear = { instId: 'BTC-USDT-SWAP', direction: 'long', closeTotalPos: '3', openMaxPos: '4', openAvgPx: '40000', closeAvgPx: '41000', realizedPnl: '1.5', fee: '-0.1', fundingFee: '0', cTime: '1709000000000', uTime: '1709500000000', type: '2' };
    const inverse = { ...linear, instId: 'BTC-USD-SWAP' };
    const unknown = { ...linear, instId: 'NOPE-USDT-SWAP' };
    const { exchange } = make('okxeu', { fetchClosedPositions: [linear, inverse, unknown] });
    const markets = [market({}), market({ id: 'BTC-USD-SWAP', symbol: 'BTCUSD', quote: 'USD', contractSize: 100, isInverse: true })];
    exchange.store.update({ markets });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct' });
    expect(records).toEqual([
      mapOkxPositionHistory(linear, {
        account: 'acct',
        toBase: (contracts) => contracts * 0.01,
        resolveSymbol: () => 'BTCUSDT',
        resolveSettleCurrency: () => 'USDT',
      }),
    ]);
  });

  it('blofin maps rows through its position adapter', async () => {
    const row = { positionId: 'p', historyId: '7', instId: 'BTC-USDT', positionSide: 'long', direction: 'long', openAveragePrice: '60000', closeAveragePrice: '61000', closePositions: '1', realizedPnl: '10', fee: '-0.1', fundingFee: '0', createTime: '1700000000000', updateTime: '1700000001000' };
    const { exchange } = make('blofin', { fetchClosedPositions: [row] });
    exchange.store.update({ markets: [market({ id: 'BTC-USDT' })] });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct' });
    expect(records).toEqual([
      mapBlofinPositionHistory(row, { account: 'acct', resolveSymbol: () => 'BTCUSDT', resolveSettleCurrency: () => 'USDT' }),
    ]);
    expect(records).toHaveLength(1);
  });

  it('bitunix maps the reported closed axis through its position adapter, as the TS venue', async () => {
    const { exchange, command } = make('bitunix', { fetchClosedPositions: BITUNIX_ROWS });
    exchange.store.update({ markets: [market({ id: 'BTC/USDT' }), market({ id: 'ETH/USDT', symbol: 'ETHUSDT', quote: 'USDC' })] });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 });
    expect(command).toHaveBeenCalledWith({ op: 'fetchClosedPositions', symbol: undefined, from: 1, to: 2 }, { report: false });
    expect(records).toEqual(
      BITUNIX_ROWS.map((row) =>
        mapBitunixPositionHistory(row, { account: 'acct', resolveSettleCurrency: (s) => (s === 'ETHUSDT' ? 'USDC' : 'USDT') }),
      ),
    );
    expect(records).toHaveLength(2);
    const ts = tsBitunix(BITUNIX_ROWS);
    ts.store.update({ markets: [market({ id: 'BTC/USDT' }), market({ id: 'ETH/USDT', symbol: 'ETHUSDT', quote: 'USDC' })] });
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
    await expect(exchange.fetchHistoricalPositions({ account: 'a' })).rejects.toThrow('no journal records path declared');
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
