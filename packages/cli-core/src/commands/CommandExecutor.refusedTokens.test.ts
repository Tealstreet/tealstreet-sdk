import { describe, expect, it, vi } from 'vitest';
import { executeCommand } from './CommandExecutor';
import { MemoryPreprocessStore } from '../preprocess';
import type { CLIPlatformAdapter, ExchangeLike } from '../types';

function exchange() {
  const called = (value: unknown) => vi.fn().mockResolvedValue(value);
  return {
    name: 'binance',
    store: {
      markets: [
        {
          symbol: 'BTCUSDT',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.5 },
          limits: { amount: { min: 0, max: 1e9 } },
        },
        {
          symbol: 'ETHUSDT',
          base: 'ETH',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.01, price: 0.1 },
          limits: { amount: { min: 0, max: 1e9 } },
        },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50000, mark: 50000, bid: 49999, ask: 50001 },
        { symbol: 'ETHUSDT', last: 3000, mark: 3000, bid: 2999, ask: 3001 },
      ],
      balance: { free: 100000, total: 100000, used: 0, upnl: 0 },
      positions: [
        {
          symbol: 'BTCUSDT',
          side: 'long',
          contracts: 1,
          notional: 50000,
          entryPrice: 40000,
          liquidationPrice: 30000,
          leverage: 1,
        },
      ],
      orders: [
        {
          id: 'abc',
          symbol: 'BTCUSDT',
          side: 'buy',
          type: 'limit',
          status: 'open',
          price: 49000,
          amount: 1,
          remaining: 1,
          filled: 0,
        },
        {
          id: 'sel',
          symbol: 'BTCUSDT',
          side: 'sell',
          type: 'limit',
          status: 'open',
          price: 51000,
          amount: 1,
          remaining: 1,
          filled: 0,
        },
      ],
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: called(['id']),
    placeOrders: called(['a', 'b']),
    updateOrder: called(undefined),
    cancelOrders: called(undefined),
    cancelSymbolOrders: called(undefined),
    nuke: called(undefined),
    setLeverage: called(undefined),
  };
}
const WRITES = [
  'placeOrder',
  'placeOrders',
  'updateOrder',
  'cancelOrders',
  'cancelSymbolOrders',
  'nuke',
  'setLeverage',
] as const;

async function run(command: string) {
  const ex = exchange();
  const adapter = new Proxy(
    {},
    { get: (_, key) => (key === 'listAccounts' || key === 'getActiveAccounts' ? () => [] : () => undefined) },
  ) as unknown as CLIPlatformAdapter;
  const result = await executeCommand(command, 'BTCUSDT', 't', ex as unknown as ExchangeLike, adapter);
  return { result, writes: WRITES.filter((name) => ex[name].mock.calls.length > 0) };
}

describe('order commands refuse tokens parseArgs would drop', () => {
  it.each([
    ['buy $90 at -5% stop 68080', '-5%'],
    ['buy $250 ETHUSDT at -0.5%', '-0.5%'],
    ['buy $100 stop -1%', '-1%'],
    ['buy $100 at 40000 -5 reduce', '-5'],
    ['buy $100 --reduce', '--reduce'],
    ['b $100 -x', '-x'],
    ['sell $100 -r reduce', '-r'],
    ['sell BTCUSDT $500 at -1% reduce', '-1%'],
    ['stop -1%', '-1%'],
    ['stop at -100', '-100'],
    ['stop at -1% trigger 18500', '-1%'],
    ['stop 50000 -1%', '-1%'],
    ['stop 40000 -1.5', '-1.5'],
    ['stop sell 0.01 at -100 ETHUSDT', '-100'],
    ['be -0.5%', '-0.5%'],
    ['be -0.5% ETHUSDT', '-0.5%'],
    ['move abc -5', '-5'],
    ['mv abc 49000 -5', '-5'],
    ['scale buy $1000 into 10 from -0.25% to -1% ETHUSDT', '-0.25%'],
    ['scale buy $180 into 2 from 1000 to 2000 -1 reduce', '-1'],
    ['bump -5', '-5'],
    ['bump sell -5 ETHUSDT', '-5'],
    ['bump buy bottom 5 -2 ETHUSDT', '-2'],
    ['bump -5 --buys', '-5'],
    ['bump 100 --buys ETHUSDT', '--buys'],
    ['close -50%', '-50%'],
    ['close -1 ETHUSDT', '-1'],
    ['close --all ETHUSDT', '--all'],
    ['trail -1%', '-1%'],
    ['chase buy $100 -1%', '-1%'],
    ['twap buy $100 -5m', '-5m'],
    ['swarm buy $100 into 2 --x reduce', '--x'],
    ['nuke --orders', '--orders'],
    ['nuke -5', '-5'],
    ['cancel --buys', '--buys'],
    ['x -5', '-5'],
    ['lev -5', '-5'],
    ['kill --all', '--all'],
  ])('%s', async (command, token) => {
    const { result, writes } = await run(command);
    expect(writes).toEqual([]);
    expect(result.success).toBe(false);
    expect(result.error).toContain(`"${token}"`);
  });

  it.each([
    ['buy $100', 'placeOrder'],
    ['buy $100 at 39600', 'placeOrder'],
    ['buy 50% stop 39200', 'placeOrder'],
    ['close', 'placeOrder'],
    ['close 50%', 'placeOrder'],
    ['stop 39600', 'placeOrder'],
    ['stop at 39200', 'placeOrder'],
    ['stop $500 at 1%', 'placeOrder'],
    ['be +3%', 'placeOrder'],
    ['move abc +10', 'updateOrder'],
    ['bump 100 --buys', 'updateOrder'],
    ['bump 100 -s', 'updateOrder'],
    ['scale buy $500 into 5 from 49949.001 to 49749.005', 'placeOrders'],
    ['cancel abc', 'cancelOrders'],
    ['lev 5', 'setLeverage'],
  ])('%s still works', async (command, write) => {
    const { result, writes } = await run(command);
    expect(result.error ?? '').not.toContain('refusing');
    expect(writes).toEqual([write]);
  });

  it.each(['orders --all', 'positions --all', 'echo -5'])('%s is not refused', async (command) => {
    const { result } = await run(command);
    expect(result.error ?? '').not.toContain('refusing');
  });

  it('track refuses a dash token rather than storing the source without it', async () => {
    const store = new MemoryPreprocessStore();
    const adapter = new Proxy({}, { get: () => () => undefined }) as unknown as CLIPlatformAdapter;
    const result = await executeCommand('track min @lo $last -2%', 'BTCUSDT', 't', null, adapter, undefined, store);
    expect(result.success).toBe(false);
    expect(result.error).toContain('"-2%"');
    expect(store.getTrackedVars()).toEqual({});
    await executeCommand('track min @lo $last', 'BTCUSDT', 't', null, adapter, undefined, store);
    expect(store.getTrackedVar('lo')?.source).toBe('$last');
  });

  it.each([
    'stop 0',
    'stop at 0',
    'stop at 100 trigger 0',
    'be 0',
    'move abc 0',
    'buy $100 at 0',
    'buy $100 stop 0',
    'scale buy $180 into 2 from 0 to 100',
  ])('%s places nothing', async (command) => {
    const { writes } = await run(command);
    expect(writes).toEqual([]);
  });
});
