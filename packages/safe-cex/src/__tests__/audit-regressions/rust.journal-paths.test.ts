vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ExchangeName, Market } from '../../types';

import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { mapBlofinPositionHistory } from '../../journal/blofin.position-adapter';
import { aggregateBybitClosedPnl } from '../../journal/bybit.position-adapter';
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
    const response = responses[String(c.op)];
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
  }) as Market;

type Fold = { foldClosedPositionRecords: (...args: unknown[]) => Promise<unknown[]> };

describe('Rust journal paths follow the TS venue declarations', () => {
  it('bybit folds the reported rows with hedge legs joined from the order axis', async () => {
    const rows = [
      { symbol: 'BTCUSDT', orderId: 'o1', side: 'Sell', closedSize: '1', avgEntryPrice: '100', avgExitPrice: '110', closedPnl: '10', createdTime: '1000', updatedTime: '1001' },
      { symbol: 'BTCUSDT', orderId: 'o2', side: 'Buy', closedSize: '1', avgEntryPrice: '120', avgExitPrice: '115', closedPnl: '5', createdTime: '2000', updatedTime: '2001' },
    ];
    const orders = [
      { id: 'o1', positionSide: PositionSide.Long },
      { id: 'o2', positionSide: PositionSide.Short },
    ];
    const { exchange, command } = make('bybit', { fetchClosedPositions: rows, fetchOrderHistory: orders });
    const records = await exchange.fetchHistoricalPositions({ account: 'acct', from: 1, to: 2 });
    expect(command).toHaveBeenCalledWith({ op: 'fetchClosedPositions', symbol: undefined, from: 1, to: 2 }, { report: false });
    expect(records).toEqual(
      aggregateBybitClosedPnl(rows, {
        account: 'acct',
        resolveSettleCurrency: () => 'USDT',
        resolveLeg: (id) => (id === 'o1' ? PositionSide.Long : PositionSide.Short),
      }),
    );
    expect(records).toHaveLength(2);
  });

  it('bybit refuses to present an unjoined history and propagates a failed row read', async () => {
    const failedOrders = make('hyrotrader', { fetchClosedPositions: [], fetchOrderHistory: new Error('down') }).exchange;
    await expect(failedOrders.fetchHistoricalPositions({ account: 'a' })).rejects.toThrow(
      'bybit: could not read the order axis to resolve hedge legs',
    );
    const failedRows = make('bybitdemo', { fetchClosedPositions: new Error('bybit closed-PnL fetch failed: x') }).exchange;
    await expect(failedRows.fetchHistoricalPositions({ account: 'a' })).rejects.toThrow('bybit closed-PnL fetch failed: x');
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
