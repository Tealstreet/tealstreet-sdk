import { afterEach, describe, expect, it, vi } from 'vitest';

import { executeCommand } from '../CommandExecutor';
import { WorkerPlatformAdapter } from '../../worker-host/WorkerPlatformAdapter';
import type { ExchangeLike } from '../../types';
import { ChaseCommand } from './ChaseCommand';

function fixture() {
  const write = vi.fn().mockResolvedValue(['entry']);
  const exchange = {
    name: 'bybit',
    store: {
      markets: ['BTCUSDT', 'SOLUSDT'].map((symbol) => ({
        symbol,
        base: symbol.slice(0, -4),
        quote: 'USDT',
        contractSize: 1,
        precision: { amount: 0.001, price: 0.1 },
      })),
      tickers: ['BTCUSDT', 'SOLUSDT'].map((symbol) => ({ symbol, last: 75800, mark: 75800, bid: 75800, ask: 75800 })),
      positions: [{ symbol: 'BTCUSDT', side: 'long', contracts: 1, entryPrice: 75000, notional: 75800, leverage: 1 }],
      orders: [
        { id: 'order-1', symbol: 'BTCUSDT', status: 'open', type: 'limit', side: 'buy', price: 75000, remaining: 1 },
      ],
      balance: { free: 1000, total: 1000, used: 0 },
    },
    placeOrder: write,
    placeOrders: write,
    updateOrder: write,
    cancelOrders: write,
    setLeverage: write,
    changeMarginType: write,
    nuke: write,
    nukePositions: write,
    nukeOrders: write,
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
  } as unknown as ExchangeLike;
  const startChase = vi.spyOn(ChaseCommand.prototype as any, 'chaseBuySellSymbol').mockResolvedValue('chase');
  const run = (raw: string) => executeCommand(raw, 'BTCUSDT', undefined, exchange, new WorkerPlatformAdapter(exchange));
  return { run, write, startChase };
}
afterEach(() => vi.restoreAllMocks());

const orders = [
  'buy $500',
  'sell $500',
  'buy BTCUSDT $500 at 75800.0',
  'sell SOLUSDT $500 best',
  'buy $500 stop 75700',
  'scale buy $500 into 5 from 75700 to 75800',
  'stop at 75700',
  'stop sell $500 at 75700 trigger 75750',
  'trail 1%',
  'be +100',
  'swarm buy $500 into 5',
  'twap buy $500 into 5 over 10',
  'twap chase buy $500 into 5 over 10',
  'chase buy $500',
  'close 50%',
  'close all',
  'nuke all',
  'bump +100',
  'move order-1 76000',
];

describe('unknown order arguments', () => {
  it.each(orders)('refuses unknown words before any write or task: %s', async (command) => {
    const { run, write, startChase } = fixture();
    const result = await run(`${command} surprise`);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/unknown argument/i);
    expect(write).not.toHaveBeenCalled();
    expect(startChase).not.toHaveBeenCalled();
    expect(result.taskAction).toBeUndefined();
  });

  it.each(orders)('explains sl= without sending anything: %s', async (command) => {
    const { run, write, startChase } = fixture();
    const result = await run(`${command} sl=75700`);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/did you mean.*stop 75700/i);
    expect(write).not.toHaveBeenCalled();
    expect(startChase).not.toHaveBeenCalled();
    expect(result.taskAction).toBeUndefined();
  });

  it.each(['BTCUSDT', 'SOLUSDT'])('refuses the reported entry on %s', async (symbol) => {
    const { run, write } = fixture();
    const result = await run(`buy ${symbol} $500 at 75800.0 sl=75700.0 tp=76100.0`);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/did you mean.*stop 75700.0/i);
    expect(write).not.toHaveBeenCalled();
  });

  it.each(['isolated', 'cross'])('refuses lev 50 %s without changing leverage or margin mode', async (mode) => {
    const { run, write } = fixture();
    const result = await run(`lev 50 ${mode}`);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/margin mode|unknown argument/i);
    expect(write).not.toHaveBeenCalled();
  });

  it('preserves the supported leverage-only command', async () => {
    const { run, write } = fixture();
    expect((await run('lev 50 BTCUSDT')).success).toBe(true);
    expect(write).toHaveBeenCalledExactlyOnceWith('BTCUSDT', 50);
  });

  it.each(['buy $500', 'sell $500 best', 'buy BTCUSDT,SOLUSDT $500 at 75800'])(
    'attaches both brackets through the existing entry path: %s',
    async (command) => {
      const { run, write } = fixture();
      expect((await run(`${command} stop 75700 tp 76100.04`)).success).toBe(true);
      expect(write).toHaveBeenCalledWith(expect.objectContaining({ stopLoss: 75700, takeProfit: 76100 }));
      expect(write.mock.calls.every(([order]) => order.takeProfit === 76100)).toBe(true);
    },
  );

  it('explains tp= without placing an unprotected entry', async () => {
    const { run, write } = fixture();
    const result = await run('buy $500 at 75800 tp=76100');
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/tp 76100|take-profit/i);
    expect(write).not.toHaveBeenCalled();
  });

  it.each([
    'tp',
    'tp oops',
    'tp 0',
    'tp 76100 surprise',
    'stop 75700 123',
    'at 75800 123',
    'best at 75800',
    'SOLUSDT at 75800',
    'at 75800 stop 75700 stop 75600',
    'tp 76100 tp 76200',
  ])('refuses malformed or unconsumed entry arguments: %s', async (tail) => {
    const { run, write } = fixture();
    expect((await run(`buy $500 ${tail}`)).success).toBe(false);
    expect(write).not.toHaveBeenCalled();
  });
});
