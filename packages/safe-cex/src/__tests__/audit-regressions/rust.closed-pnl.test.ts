import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ExchangeName, Market } from '../../types';

import { mapBlofinClosedPosition } from '../../exchanges/blofin/blofin.income';
import { isBaseCapabilityFallback } from '../../exchanges/base';
import { mapBybitClosedPnl } from '../../exchanges/bybit/bybit.closed-pnl';
import { mapOkxClosedPosition } from '../../exchanges/okx/okx.closed-pnl';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

type Command = (command: Record<string, unknown>) => Promise<unknown>;

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

const make = (name: ExchangeName, rows: unknown, options: Record<string, unknown> = {}) => {
  const exchange = new RustEngineExchange(name, { key: 'k', secret: 's', passphrase: 'p', ...options }, new DefaultStore());
  exchanges.push(exchange);
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

describe('Rust closed PnL reads the engine rows through the TS mappers', () => {
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
    expect(isBaseCapabilityFallback(exchange.fetchClosedPnl, 'fetchClosedPnl')).toBe(false);
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

  it('a failed engine read reports once and yields an empty history, as TS does', async () => {
    const { exchange } = make('okx', new Error('OKX closed positions: OKX 50011: Too Many Requests'));
    await expect(exchange.fetchClosedPnl({ symbol: 'BTCUSDT', accountName: 'a' })).resolves.toEqual([]);
  });

  it('public sessions read nothing and venues without the endpoint keep the branded fallback', async () => {
    const { exchange, command } = make('bybit', BYBIT_ROWS, { publicDataOnly: true });
    await expect(exchange.fetchClosedPnl({ symbol: 'BTCUSDT', accountName: 'a' })).resolves.toEqual([]);
    expect(command).not.toHaveBeenCalled();
    for (const name of ['bybitspot', 'binance', 'hyperliquid'] as const) {
      expect(isBaseCapabilityFallback(make(name, []).exchange.fetchClosedPnl, 'fetchClosedPnl')).toBe(true);
    }
  });
});
