import { afterEach, describe, expect, it, vi } from 'vitest';

import { executeCommand } from '../CommandExecutor';
import { WorkerPlatformAdapter } from '../../worker-host/WorkerPlatformAdapter';
import type { ExchangeLike } from '../../types';
import { PositionSide } from '@tealstreet/safe-cex/src/types';
import { ChaseCommand } from './ChaseCommand';

function fixture(extraMarkets = false) {
  const symbols = extraMarkets
    ? [
        'BTCUSDT',
        'SOLUSDT',
        'SUSDT',
        'IUSDT',
        'INTOUSDT',
        'OVERUSDT',
        'REDUCEUSDT',
        'BUYUSDT',
        'SELLUSDT',
        'NEXTUSDT',
        'TOPUSDT',
        'LONGUSDT',
        'SHOWUSDT',
      ]
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
  const adapter = new WorkerPlatformAdapter(exchange);
  const run = (raw: string) => executeCommand(raw, 'BTCUSDT', undefined, exchange, adapter);
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

it.each(['buy $500 po', 'sell $500 po'])('refuses incompatible modifiers: %s', async (command) => {
  const { run, write, startChase } = fixture(true);
  const result = await run(command);
  expect(result.success).toBe(false);
  expect(write).not.toHaveBeenCalled();
  expect(startChase).not.toHaveBeenCalled();
});

describe('resolved target and selector validation', () => {
  it.each([
    'buy BTCUSDT,BTCUSDT $500',
    'buy BTC,BTCUSDT $500',
    'buy SOLUSDT,BTC,BTCUSDT $500',
    'buy btc,BTCUSDT $500 at 75800',
    'sell $500 best BTCUSDT,BTC',
    'sell $500 BTCUSDT,BTCUSDT',
    'chase buy BTCUSDT,BTC $500',
    'chase sell $500 BTC,BTCUSDT',
    'close BTCUSDT,BTCUSDT',
    'close BTCUSDT BTCUSDT',
    'close 50% BTC BTCUSDT',
    'close SOLUSDT BTC,BTCUSDT',
    'close long BTCUSDT,BTC',
  ])('refuses duplicate resolved markets before any write or task: %s', async (command) => {
    const { run, write, startChase } = fixture(true);
    const result = await run(command);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/duplicate.*BTCUSDT/i);
    expect(write).not.toHaveBeenCalled();
    expect(startChase).not.toHaveBeenCalled();
    expect(result.taskAction).toBeUndefined();
  });

  it.each([
    'bump buy +100 --sells',
    'bump buys +100 -s',
    'bump sell +100 --buys',
    'bump sells +100 -b',
    'bump BTCUSDT buy top 1 +100 --sells',
  ])('refuses conflicting positional and flag sides: %s', async (command) => {
    const { run, write } = fixture(true);
    const result = await run(command);
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/conflict.*side|side.*conflict/i);
    expect(write).not.toHaveBeenCalled();
  });

  it.each(['bump buy +100', 'bump buys +100', 'bump +100 --buys', 'bump buy +100 --buys', 'bump top 1 +100'])(
    'preserves supported bump selectors even with matching market names: %s',
    async (command) => {
      const { run, write } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(write).toHaveBeenCalledTimes(1);
      expect(write.mock.calls[0][0].order.symbol).toBe('BTCUSDT');
    },
  );

  it.each(['buy NEXT $500', 'buy $500 NEXT', 'sell NEXT $500 at 75800', 'buy $500 NEXT best', 'buy $500 "reduce "'])(
    'accepts a loaded base alias in a symbol position: %s',
    async (command) => {
      const { run, write } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(write).toHaveBeenCalledTimes(1);
      expect(write.mock.calls[0][0].symbol).toBe(command.includes('reduce') ? 'REDUCEUSDT' : 'NEXTUSDT');
    },
  );

  it.each(['chase buy NEXT $500', 'chase buy $500 NEXT'])('chases the resolved alias: %s', async (command) => {
    const { run, startChase } = fixture(true);
    expect((await run(command)).success).toBe(true);
    expect(startChase.mock.calls.map((call) => call[2])).toEqual(['NEXTUSDT']);
  });

  it('accepts NEXT as a leverage symbol', async () => {
    const { run, write } = fixture(true);
    expect((await run('lev 50 NEXT')).success).toBe(true);
    expect(write).toHaveBeenCalledExactlyOnceWith('NEXTUSDT', 50);
  });

  it.each(['twap buy NEXT $500 into 5 over 10', 'twap buy $500 into 5 over 10 NEXT'])(
    'uses the alias in a parsed TWAP symbol slot: %s',
    async (command) => {
      const { run, write, startChase } = fixture(true);
      const result = await run(command);
      expect(result.success).toBe(true);
      expect(result.taskAction?.params?.symbol).toBe('NEXTUSDT');
      expect(write).not.toHaveBeenCalled();
      expect(startChase).not.toHaveBeenCalled();
    },
  );

  it.each(['buy BTC,SOLUSDT $500', 'sell $500 best BTC,SOLUSDT'])(
    'preserves distinct resolved targets: %s',
    async (command) => {
      const { run, write } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(write.mock.calls.map(([order]) => order.symbol)).toEqual(['BTCUSDT', 'SOLUSDT']);
    },
  );

  it('closes distinct symbols once each after resolving aliases', async () => {
    const { run, write, exchange } = fixture(true);
    const positions = exchange.store!.positions!;
    positions.push({ ...positions[0], symbol: 'SOLUSDT' });
    expect((await run('close 50% BTC SOLUSDT')).success).toBe(true);
    expect(write.mock.calls.map(([order]) => [order.symbol, order.amount])).toEqual([
      ['BTCUSDT', 0.5],
      ['SOLUSDT', 0.5],
    ]);
  });

  it('preserves global closing of both hedge positions on the same market', async () => {
    const { run, write, exchange } = fixture(true);
    const positions = exchange.store!.positions!;
    positions.push({ ...positions[0], side: PositionSide.Short });
    expect((await run('close all')).success).toBe(true);
    expect(write.mock.calls.map(([order]) => [order.symbol, order.side])).toEqual([
      ['BTCUSDT', 'sell'],
      ['BTCUSDT', 'buy'],
    ]);
  });

  it.each(['buy $500 reduce reduce', 'chase buy $500 reduce reduce', 'close long long'])(
    'refuses repeated grammar in its selector role despite loaded aliases: %s',
    async (command) => {
      const { run, write, startChase } = fixture(true);
      expect((await run(command)).success).toBe(false);
      expect(write).not.toHaveBeenCalled();
      expect(startChase).not.toHaveBeenCalled();
    },
  );
});

describe('symbols and grammar occupy separate parsed slots', () => {
  it('does not apply a leading symbol as a buy modifier', async () => {
    const { run, write, exchange } = fixture(true);
    const positions = exchange.store!.positions!;
    positions.push({ ...positions[0], symbol: 'REDUCEUSDT' });
    expect((await run('buy reduce $500')).success).toBe(true);
    expect(write).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'REDUCEUSDT', reduceOnly: false }));
  });

  it('permits a real modifier after a leading symbol with the same name', async () => {
    const { run, write, exchange } = fixture(true);
    const positions = exchange.store!.positions!;
    positions.push({ ...positions[0], symbol: 'REDUCEUSDT' });
    expect((await run('sell reduce $500 reduce')).success).toBe(true);
    expect(write).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'REDUCEUSDT', reduceOnly: true }));
  });

  it('does not apply a leading chase symbol as a reduce modifier', async () => {
    const { run, startChase } = fixture(true);
    expect((await run('chase buy reduce $500')).success).toBe(true);
    expect(startChase).toHaveBeenCalledTimes(1);
    expect(startChase.mock.calls[0][2]).toBe('REDUCEUSDT');
    expect(startChase.mock.calls[0][5]).toBe(false);
  });

  it('keeps TWAP symbol and keyword slots distinct', async () => {
    const { run } = fixture(true);
    const result = await run('twap buy into $500 into 5 over 10');
    expect(result.success).toBe(true);
    expect(result.taskAction?.params?.symbol).toBe('INTOUSDT');
  });

  it('permits a swarm modifier after a symbol with the same name', async () => {
    const { run, write, exchange } = fixture(true);
    const positions = exchange.store!.positions!;
    positions.push({ ...positions[0], symbol: 'REDUCEUSDT' });
    expect((await run('swarm sell reduce $500 into 2 reduce')).success).toBe(true);
    expect(write).toHaveBeenCalledTimes(2);
    expect(write.mock.calls.every(([order]) => order.symbol === 'REDUCEUSDT' && order.reduceOnly)).toBe(true);
  });

  it('does not skip a stop symbol because another slot uses that word as grammar', async () => {
    const { run, write, exchange } = fixture(true);
    const positions = exchange.store!.positions!;
    positions.push({ ...positions[0], symbol: 'BUYUSDT' });
    expect((await run('stop at 75700 BUY')).success).toBe(true);
    expect(write).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'BUYUSDT' }));
  });
});

it('keeps leverage show in its read-operation slot with a loaded SHOW alias', async () => {
  const { run, exchange, write } = fixture(true);
  expect((await run('lev show')).success).toBe(true);
  expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-log', { message: expect.stringContaining('BTCUSDT: 1x') });
  expect(write).not.toHaveBeenCalled();
});

describe('leverage symbol reads and scale leftover diagnostics', () => {
  it.each([
    ['lev BTCUSDT', 'BTCUSDT', 1],
    ['lev BTC', 'BTCUSDT', 1],
    ['leverage btc', 'BTCUSDT', 1],
    ['lev SOLUSDT', 'SOLUSDT', 7],
    ['lev show SOLUSDT', 'SOLUSDT', 7],
    ['lev show BTCUSDT', 'BTCUSDT', 1],
    ['lev 1000PEPEUSDT', '1000PEPEUSDT', 3],
  ])('reads the resolved loaded symbol without writes: %s', async (command, symbol, leverage) => {
    const { run, write, exchange } = fixture(true);
    const store = exchange.store!;
    store.markets!.push({ ...store.markets![0], symbol: '1000PEPEUSDT', base: '1000PEPE' });
    store.tickers!.push({ ...store.tickers![0], symbol: '1000PEPEUSDT' });
    store.positions!.push(
      { ...store.positions![0], symbol: 'SOLUSDT', leverage: 7 },
      { ...store.positions![0], symbol: '1000PEPEUSDT', leverage: 3 },
    );
    expect((await run(command)).success).toBe(true);
    expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-log', { message: `${symbol}: ${leverage}x` });
    expect(write).not.toHaveBeenCalled();
  });

  it.each(['lev BTCUSDT extra', 'lev BTCUSDT SOLUSDT', 'lev surprise', 'lev BTCUSDT 50'])(
    'refuses unknown or extra read operands: %s',
    async (command) => {
      const { run, write } = fixture(true);
      const result = await run(command);
      expect(result.success).toBe(false);
      expect(result.error).toMatch(/unknown argument/i);
      expect(write).not.toHaveBeenCalled();
    },
  );

  it.each(['BTCUSDT extra', 'taker BTCUSDT extra', 'BTCUSDT extra another'])(
    'names the unconsumed scale token after a valid symbol: %s',
    async (tail) => {
      const { run, write } = fixture(true);
      const result = await run(`scale buy $500 into 5 from 75700 to 75800 ${tail}`);
      expect(result.success).toBe(false);
      expect(result.error).toContain('Unknown argument "extra"');
      expect(result.error).not.toContain('Unknown argument "BTCUSDT"');
      expect(result.error).not.toContain('modifier');
      expect(write).not.toHaveBeenCalled();
    },
  );

  it('keeps a scale with a loaded symbol valid', async () => {
    const { run, write } = fixture(true);
    expect((await run('scale buy $500 into 5 from 75700 to 75800 BTCUSDT')).success).toBe(true);
    expect(write).toHaveBeenCalledTimes(1);
    expect(write.mock.calls[0][0]).toHaveLength(5);
  });
});
