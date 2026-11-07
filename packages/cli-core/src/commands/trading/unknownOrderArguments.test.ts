import { afterEach, describe, expect, it, vi } from 'vitest';

import { executeCommand } from '../CommandExecutor';
import { WorkerPlatformAdapter } from '../../worker-host/WorkerPlatformAdapter';
import type { ExchangeLike } from '../../types';
import { ChaseCommand } from './ChaseCommand';

function fixture(extraMarkets = false) {
  const symbols = extraMarkets
    ? ['BTCUSDT', 'SOLUSDT', 'SUSDT', 'IUSDT', 'INTOUSDT', 'OVERUSDT', 'REDUCEUSDT', 'BUYUSDT', 'SELLUSDT']
    : ['BTCUSDT', 'SOLUSDT'];
  const write = vi.fn().mockResolvedValue(['entry']);
  const exchange = {
    name: 'bybit',
    store: {
      markets: symbols.map((symbol) => ({
        symbol,
        base: symbol.slice(0, -4),
        quote: 'USDT',
        contractSize: 1,
        precision: { amount: 0.001, price: 0.1 },
      })),
      tickers: symbols.map((symbol) => ({ symbol, last: 75800, mark: 75800, bid: 75800, ask: 75800 })),
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
  return { run, write, startChase, exchange };
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

describe('complete order argument parsing', () => {
  it.each([
    'twap chase buy $500 into 5 over 10 fail next fail surprise',
    'twap chase buy $500 into 5 over 10 fail next fail next',
    'twap chase buy $500 into 5 over 10 timeout 30% next timeout isolated cross',
    'twap chase buy $500 into 5 over 10 timeout 30% next timeout 50% market',
    'move order-1 76000isolated',
    'move order-1 +100isolated',
    'bump +100isolated',
    'bump top 1isolated +100',
    'twap buy $500 into 5isolated over 10',
    'swarm buy $500 into 5isolated',
    'twap buy $500 into 5.5 over 10',
    'swarm buy $500 into 5.5',
    'twap buy $500 into 5 over 10isolated',
    'twap buy $500 duration=1hisolated slices=5',
    'twap buy $500 duration=1h slices=5isolated',
    'twap buy $500 duration=1h duration=2h slices=5',
    'twap buy $500 duration=1h slices=5 slices=10',
    'twap buy $500 into 5 over 10 slices=10',
    'twap buy $500 into 5 over 10 duration=1h',
    'stop buy sell at 75700',
    'stop sell buy at 75700',
    'stop buy buy at 75700',
    'swarm buy $500 into 5 reduce reduce',
    'swarm buy $500 into 5 irregular irregular',
    'swarm buy BTCUSDT $500 into 5 SOLUSDT',
    'chase buy BTCUSDT,XBTUSD $500',
    'twap buy BTCUSDT,XBTUSD $500 into 5 over 10',
    'swarm buy BTCUSDT,XBTUSD $500 into 5',
    'twap buy BTCUSDT,SOLUSDT $500 into 5 over 10',
    'swarm buy BTCUSDT,SOLUSDT $500 into 5',
  ])('refuses the whole command without write or task: %s', async (command) => {
    const { run, write, startChase } = fixture(true);
    const result = await run(command);
    expect(result.success).toBe(false);
    expect(write).not.toHaveBeenCalled();
    expect(startChase).not.toHaveBeenCalled();
    expect(result.taskAction).toBeUndefined();
  });

  describe.each(['bybit', 'binance-spot'])('complete symbols on %s', (name) => {
    it.each([
      'buy $500 surprise',
      'sell $500 at 75800 surprise',
      'buy $500 best surprise',
      'buy surprise $500',
      'lev 50 surprise',
      'chase buy $500 surprise',
      'buy $500 isolated',
      'stop at 75700 surprise',
      'trail 1% surprise',
      'be +100 surprise',
      'bump +100 surprise',
      'close 50% surprise',
    ])('refuses an arbitrary prefix without writes: %s', async (command) => {
      const { run, write, startChase, exchange } = fixture(true);
      exchange.name = name;
      const result = await run(command);
      expect(result.success).toBe(false);
      expect(write).not.toHaveBeenCalled();
      expect(startChase).not.toHaveBeenCalled();
      expect(result.taskAction).toBeUndefined();
    });

    it.each(['b $500', 's $500 at 75800', 'buy btc $500 best', 'sell $500 BTC', 'buy $500 SOLUSDT'])(
      'preserves complete symbol and alias forms: %s',
      async (command) => {
        const { run, write, exchange } = fixture(true);
        exchange.name = name;
        expect((await run(command)).success).toBe(true);
        expect(write).toHaveBeenCalledTimes(1);
        expect(write.mock.calls[0][0].symbol).toBe(command.endsWith('SOLUSDT') ? 'SOLUSDT' : 'BTCUSDT');
      },
    );
  });

  it.each(['chase sell %all% 20% reduce', 'chase sell 20% reduce %all%'])(
    'preserves all-position selection: %s',
    async (command) => {
      const { run, startChase } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(startChase.mock.calls.map((call) => call[2])).toEqual(['BTCUSDT']);
    },
  );

  it.each(['twap buy $500 into 5 over 10', 'twap chase buy $500 into 5 over 10 timeout 30% next fail market'])(
    'keeps grammar words out of symbol inference: %s',
    async (command) => {
      const { run, write, startChase } = fixture(true);
      const result = await run(command);
      expect(result.success).toBe(true);
      expect(result.taskAction?.params?.symbol).toBe('BTCUSDT');
      expect(write).not.toHaveBeenCalled();
      expect(startChase).not.toHaveBeenCalled();
    },
  );
});

it.each(['twap buy $500 duration=1h30m slices=5 SOLUSDT', 'twap buy $500 into 5 over 2.5 SOLUSDT'])(
  'keeps a real trailing TWAP symbol after parsed options: %s',
  async (command) => {
    const { run } = fixture(true);
    const result = await run(command);
    expect(result.success).toBe(true);
    expect(result.taskAction?.params?.symbol).toBe('SOLUSDT');
  },
);

it.each(['buy $500 "reduce "', 'buy $500 po', 'sell $500 po'])(
  'refuses grammar collisions and incompatible modifiers: %s',
  async (command) => {
    const { run, write, startChase } = fixture(true);
    const result = await run(command);
    expect(result.success).toBe(false);
    expect(write).not.toHaveBeenCalled();
    expect(startChase).not.toHaveBeenCalled();
  },
);
