import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ExchangeName, Market } from '../../types';
import type { EngineCapabilities } from '@tealstreet/safe-cex-wasm/host';

import { BitunixExchange } from '../reference/bitunix/bitunix.exchange';
import { mapBitunixClosedPosition } from '../../exchanges/bitunix/bitunix.income';
import { mapBlofinClosedPosition } from '../../exchanges/blofin/blofin.income';
import { mapBybitClosedPnl } from '../../exchanges/bybit/bybit.closed-pnl';
import { mapOkxClosedPosition } from '../../exchanges/okx/okx.closed-pnl';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';

type Command = (command: Record<string, unknown>) => Promise<unknown>;

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

const make = (name: ExchangeName, rows: unknown, options: Record<string, unknown> = {}, capabilities: Partial<EngineCapabilities> = {}) => {
  const exchange = new RustEngineExchange(name, { key: 'k', secret: 's', passphrase: 'p', ...options }, new DefaultStore());
  exchanges.push(exchange);
  adoptEngineCapabilities(exchange, capabilities);
  const command = vi
    .spyOn(exchange as unknown as { command: Command }, 'command')
    .mockImplementation(async () => (rows instanceof Error ? Promise.reject(rows) : rows));
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

const BYBIT_ROWS = [
  { symbol: 'ETHUSDT', side: 'Buy', closedSize: '1', avgEntryPrice: '2000', avgExitPrice: '1990', cumExitValue: '1990', closedPnl: '9', leverage: '5', openFee: '0.1', closeFee: '0.1', createdTime: '1699000000000', updatedTime: '1699000000001' },
  { symbol: 'BTCUSD', side: 'Sell', closedSize: '1000', avgEntryPrice: '95000', avgExitPrice: '96000', cumExitValue: '0.0104', closedPnl: '0.0001', leverage: '10', createdTime: '1700000000000', updatedTime: '1700000000001' },
];

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

describe('Rust closed PnL reads the engine rows through the TS mappers', () => {
  it('selects row format and inverse interpretation from the engine record', async () => {
    const { exchange } = make('binance', [BYBIT_ROWS[0]], {}, {
      closedPositions: true, closedPositionFormat: 'bybit', inverseClosedPositions: true,
    });
    await expect(exchange.fetchClosedPnl({ symbol: 'ETHUSDT', accountName: 'fixture' })).resolves.toEqual([
      mapBybitClosedPnl(BYBIT_ROWS[0], 'fixture', true),
    ]);
  });

  it('bybit pages through the engine and maps like the TS venue, newest close first', async () => {
    const { exchange, command } = make('bybit', BYBIT_ROWS);
    const entries = await exchange.fetchClosedPnl({ symbol: 'ETHUSDT', from: 1, to: 2, accountName: 'acct' });
    expect(command).toHaveBeenCalledWith(
      { op: 'fetchClosedPositions', symbol: 'ETHUSDT', from: 1, to: 2 },
      { report: true },
    );
    expect(entries).toEqual([
      mapBybitClosedPnl(BYBIT_ROWS[1], 'acct', true),
      mapBybitClosedPnl(BYBIT_ROWS[0], 'acct', false),
    ]);
    expect(exchange.capabilities.closedPnl).toBe(true);
  });

  it('bybitinverse treats every row as inverse, as its TS venue does', async () => {
    const rows = [{ ...BYBIT_ROWS[1], symbol: 'BTCUSDH25' }];
    const { exchange } = make('bybitinverse', rows);
    await expect(exchange.fetchClosedPnl({ symbol: 'BTCUSDH25', accountName: 'a' })).resolves.toEqual([
      mapBybitClosedPnl(rows[0], 'a', true),
    ]);
  });

  it('okx converts contracts with the row market, including inverse swaps by price', async () => {
    const linear = { instId: 'BTC-USDT-SWAP', direction: 'long', closeTotalPos: '3', openAvgPx: '40000', closeAvgPx: '41000', realizedPnl: '1.5', fee: '-0.1', cTime: '1709000000000', uTime: '1709500000000', lever: '10' };
    const inverse = { ...linear, instId: 'BTC-USD-SWAP', direction: 'short', closeTotalPos: '2', closeAvgPx: '50000', uTime: '1709600000000' };
    const { exchange } = make('okx', [linear, inverse]);
    const markets = [
      market({}),
      market({ id: 'BTC-USD-SWAP', symbol: 'BTCUSD', quote: 'USD', contractSize: 100, isInverse: true }),
    ];
    exchange.store.update({ markets });
    const entries = await exchange.fetchClosedPnl({ symbol: undefined as unknown as string, accountName: 'acct' });
    expect(entries).toHaveLength(2);
    expect(entries[0]).toEqual(
      mapOkxClosedPosition(inverse, markets[1], (contracts, m, price) => (contracts * m.contractSize!) / price, 'acct'),
    );
    expect(entries[0].contracts).toBeCloseTo(0.004);
    expect(entries[1]).toEqual(mapOkxClosedPosition(linear, markets[0], (contracts, m) => contracts * m.contractSize!, 'acct'));
    expect(entries[1].contracts).toBeCloseTo(0.03);
  });

  it('blofin resolves the row instrument to its symbol', async () => {
    const row = { positionId: 'p', historyId: '7', instId: 'BTC-USDT', direction: 'long', openAveragePrice: '60000', closeAveragePrice: '61000', closePositions: '1', realizedPnl: '10', fee: '-0.1', createTime: '1700000000000', updateTime: '1700000001000' };
    const { exchange } = make('blofin', [row]);
    exchange.store.update({ markets: [market({ id: 'BTC-USDT', contractSize: 0.001 })] });
    await expect(exchange.fetchClosedPnl({ symbol: 'BTCUSDT', accountName: 'acct' })).resolves.toEqual([
      mapBlofinClosedPosition(row, () => 'BTCUSDT', 'acct'),
    ]);
  });

  it('bitunix maps raw history-position rows like the TS venue, newest close first', async () => {
    const { exchange, command } = make('bitunix', BITUNIX_ROWS);
    const entries = await exchange.fetchClosedPnl({ symbol: 'BTCUSDT', from: 1, to: 2, accountName: 'acct' });
    expect(command).toHaveBeenCalledWith(
      { op: 'fetchClosedPositions', symbol: 'BTCUSDT', from: 1, to: 2 },
      { report: true },
    );
    expect(entries).toEqual([
      mapBitunixClosedPosition(BITUNIX_ROWS[1], 'acct'),
      mapBitunixClosedPosition(BITUNIX_ROWS[0], 'acct'),
    ]);
    const ts = tsBitunix(BITUNIX_ROWS);
    expect(entries).toEqual(await ts.fetchClosedPnl({ symbol: 'BTCUSDT', from: 1, to: 2, accountName: 'acct' }));
    ts.dispose();
    expect(exchange.capabilities.closedPnl).toBe(true);
  });

  it('okx and blofin read an empty symbol account-wide, as TS', async () => {
    for (const name of ['okx', 'blofin'] as const) {
      const { exchange, command } = make(name, []);
      await exchange.fetchClosedPnl({ symbol: '', accountName: 'a' });
      expect(command).toHaveBeenCalledWith(
        { op: 'fetchClosedPositions', symbol: undefined, from: undefined, to: undefined },
        { report: true },
      );
    }
  });

  it('a failed engine read reports once and yields an empty history, as TS does', async () => {
    const { exchange } = make('okx', new Error('OKX closed positions: OKX 50011: Too Many Requests'));
    await expect(exchange.fetchClosedPnl({ symbol: 'BTCUSDT', accountName: 'a' })).resolves.toEqual([]);
  });

  it('public sessions read nothing and venues without the endpoint report no closed-PnL capability', async () => {
    const { exchange, command } = make('bybit', BYBIT_ROWS, { publicDataOnly: true });
    await expect(exchange.fetchClosedPnl({ symbol: 'BTCUSDT', accountName: 'a' })).resolves.toEqual([]);
    expect(command).not.toHaveBeenCalled();
    for (const name of ['bybitspot', 'binance', 'hyperliquid'] as const) {
      expect(make(name, []).exchange.capabilities.closedPnl).toBe(false);
    }
  });
});


it.each(['bybit', 'okx', 'blofin', 'bitunix'] as const)('%s retains capped empty reported closed-PnL metadata', async (name) => {
  const { exchange } = make(name, Object.defineProperty([], 'incomplete', { value: true }));
  const rows = await exchange.fetchClosedPnl({ symbol: 'BTCUSDT' });
  expect(rows).toEqual([]);
  expect(rows).toHaveProperty('incomplete', true);
});
