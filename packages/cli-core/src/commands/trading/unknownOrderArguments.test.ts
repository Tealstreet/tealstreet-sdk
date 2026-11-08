import { afterEach, describe, expect, it, vi } from 'vitest';

import { ensureCommandsRegistered, executeCommand } from '../CommandExecutor';
import { commandRegistry } from '../CommandRegistry';
import type { CommandContext } from '../Command';
import type { ChaserTerminate } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderOptions';
import { WorkerPlatformAdapter } from '../../worker-host/WorkerPlatformAdapter';
import type { ExchangeLike } from '../../types';
import {
  OrderSide,
  OrderStatus,
  OrderType,
  PositionSide,
  type Market,
  type Ticker,
  type Position,
  type PlaceOrderOpts,
  type UpdateOrderOpts,
  type Order,
} from '@tealstreet/safe-cex/src/types';
import { ChaseCommand } from './ChaseCommand';

class TestChaseCommand extends ChaseCommand {
  override chaseBuySellSymbol = vi
    .fn<
      (
        ctx: CommandContext,
        exchange: ExchangeLike,
        symbol: string,
        side: OrderSide,
        size: string,
        reduceOnly: boolean,
        terminate?: ChaserTerminate,
      ) => Promise<string>
    >()
    .mockResolvedValue('chase');
}

function position(symbol = 'BTCUSDT', side = PositionSide.Long): Position {
  return {
    id: symbol + side,
    symbol,
    side,
    contracts: 1,
    entryPrice: 75000,
    notional: 75800,
    leverage: 1,
    unrealizedPnl: 0,
    liquidationPrice: null,
  };
}

function market(symbol: string): Market {
  return {
    id: symbol,
    symbol,
    base: symbol.slice(0, -4),
    quote: 'USDT',
    active: true,
    contractSize: 1,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 1000000 }, leverage: { min: 1, max: 100 } },
  };
}

function ticker(symbol: string): Ticker {
  return {
    id: symbol,
    symbol,
    last: 75800,
    mark: 75800,
    bid: 75800,
    ask: 75800,
    index: 75800,
    percentage: 0,
    openInterest: 0,
    fundingRate: 0,
    fundingTime: 0,
    volume: 0,
    quoteVolume: 0,
  };
}

function openOrder(symbol = 'BTCUSDT', side = OrderSide.Buy): Order {
  return {
    id: 'order-1',
    symbol,
    status: OrderStatus.Open,
    type: OrderType.Limit,
    side,
    price: 75000,
    remaining: 1,
    amount: 1,
    filled: 0,
    reduceOnly: false,
    lastUpdate: 0,
  };
}

type WriteInput = PlaceOrderOpts | PlaceOrderOpts[] | UpdateOrderOpts | Order[] | string | undefined;
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
        'SELLCOINUSDT',
        'SHORTUSDT',
        'NEXTUSDT',
        'TOPUSDT',
        'LONGUSDT',
        'SHOWUSDT',
      ]
    : ['BTCUSDT', 'SOLUSDT'];
  const write = vi.fn<(input: WriteInput, ...rest: unknown[]) => Promise<string[]>>().mockResolvedValue(['entry']);
  const exchange = {
    name: 'bybit',
    store: {
      markets: symbols.map(market),
      tickers: symbols.map(ticker),
      positions: [position()],
      orders: [openOrder()],
      balance: { free: 1000, total: 1000, used: 0, upnl: 0 },
    },
    placeOrder: write,
    placeOrders: write,
    updateOrder: write,
    cancelOrders: async (orders: Order[]) => {
      await write(orders);
    },
    setLeverage: async (symbol: string, leverage: number) => {
      await write(symbol, leverage);
    },
    nuke: async () => {
      await write(undefined);
    },
    nukePositions: async () => {
      await write(undefined);
    },
    nukeOrders: async () => {
      await write(undefined);
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
  } satisfies ExchangeLike;
  ensureCommandsRegistered();
  const chase = new TestChaseCommand();
  commandRegistry.register(chase);
  const startChase = chase.chaseBuySellSymbol;
  const adapter = new WorkerPlatformAdapter(exchange);
  const run = (raw: string) => executeCommand(raw, 'BTCUSDT', undefined, exchange, adapter);
  const placedOrders = () =>
    write.mock.calls.flatMap(([input]) => {
      if (typeof input !== 'object' || input === null) throw new Error('Unexpected placement observation');
      if (Array.isArray(input)) {
        return input.map((order) => {
          if ('symbol' in order && 'amount' in order) return order;
          throw new Error('Unexpected placement observation');
        });
      }
      if ('symbol' in input && 'type' in input) return [input];
      throw new Error('Unexpected placement observation');
    });
  const updatedOrders = () =>
    write.mock.calls.map(([input]) => {
      if (typeof input === 'object' && input !== null && 'order' in input) return input;
      throw new Error('Unexpected amendment observation');
    });
  return { run, write, startChase, exchange, placedOrders, updatedOrders };
}
afterEach(() => {
  commandRegistry.register(new ChaseCommand());
  vi.restoreAllMocks();
});

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
      const { run, write, placedOrders } = fixture();
      expect((await run(`${command} stop 75700 tp 76100.04`)).success).toBe(true);
      expect(write).toHaveBeenCalledWith(expect.objectContaining({ stopLoss: 75700, takeProfit: 76100 }));
      expect(placedOrders().every((order) => order.takeProfit === 76100)).toBe(true);
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
        const { run, write, exchange, placedOrders } = fixture(true);
        exchange.name = name;
        expect((await run(command)).success).toBe(true);
        expect(write).toHaveBeenCalledTimes(1);
        expect(placedOrders()[0]?.symbol).toBe(command.endsWith('SOLUSDT') ? 'SOLUSDT' : 'BTCUSDT');
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
      const { run, write, updatedOrders } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(write).toHaveBeenCalledTimes(1);
      expect(updatedOrders()[0]?.order.symbol).toBe('BTCUSDT');
    },
  );

  it.each(['buy NEXT $500', 'buy $500 NEXT', 'sell NEXT $500 at 75800', 'buy $500 NEXT best', 'buy $500 "reduce "'])(
    'accepts a loaded base alias in a symbol position: %s',
    async (command) => {
      const { run, write, placedOrders } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(write).toHaveBeenCalledTimes(1);
      expect(placedOrders()[0]?.symbol).toBe(command.includes('reduce') ? 'REDUCEUSDT' : 'NEXTUSDT');
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
      const { run, write, placedOrders } = fixture(true);
      expect((await run(command)).success).toBe(true);
      expect(placedOrders().map((order) => order.symbol)).toEqual(['BTCUSDT', 'SOLUSDT']);
    },
  );

  it('closes distinct symbols once each after resolving aliases', async () => {
    const { run, write, exchange, placedOrders } = fixture(true);
    const positions = exchange.store.positions;
    positions.push({ ...positions[0], symbol: 'SOLUSDT' });
    expect((await run('close 50% BTC SOLUSDT')).success).toBe(true);
    expect(placedOrders().map((order) => [order.symbol, order.amount])).toEqual([
      ['BTCUSDT', 0.5],
      ['SOLUSDT', 0.5],
    ]);
  });

  it('preserves global closing of both hedge positions on the same market', async () => {
    const { run, write, exchange, placedOrders } = fixture(true);
    const positions = exchange.store.positions;
    positions.push({ ...positions[0], side: PositionSide.Short });
    expect((await run('close all')).success).toBe(true);
    expect(placedOrders().map((order) => [order.symbol, order.side])).toEqual([
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
    const { run, write, exchange, placedOrders } = fixture(true);
    const positions = exchange.store.positions;
    positions.push({ ...positions[0], symbol: 'REDUCEUSDT' });
    expect((await run('buy reduce $500')).success).toBe(true);
    expect(write).toHaveBeenCalledWith(expect.objectContaining({ symbol: 'REDUCEUSDT', reduceOnly: false }));
  });

  it('permits a real modifier after a leading symbol with the same name', async () => {
    const { run, write, exchange, placedOrders } = fixture(true);
    const positions = exchange.store.positions;
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
    const { run, write, exchange, placedOrders } = fixture(true);
    const positions = exchange.store.positions;
    positions.push({ ...positions[0], symbol: 'REDUCEUSDT' });
    expect((await run('swarm sell reduce $500 into 2 reduce')).success).toBe(true);
    expect(write).toHaveBeenCalledTimes(2);
    expect(placedOrders().every((order) => order.symbol === 'REDUCEUSDT' && order.reduceOnly)).toBe(true);
  });

  it('does not skip a stop symbol because another slot uses that word as grammar', async () => {
    const { run, write, exchange, placedOrders } = fixture(true);
    const positions = exchange.store.positions;
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
    const { run, write, exchange, placedOrders } = fixture(true);
    const store = exchange.store;
    store.markets.push({ ...store.markets[0], symbol: '1000PEPEUSDT', base: '1000PEPE' });
    store.tickers.push({ ...store.tickers[0], symbol: '1000PEPEUSDT' });
    store.positions.push(
      { ...store.positions[0], symbol: 'SOLUSDT', leverage: 7 },
      { ...store.positions[0], symbol: '1000PEPEUSDT', leverage: 3 },
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

describe('order symbol slots beat side words', () => {
  it.each(['BUY', 'buy', 'BUYUSDT', 'buyusdt', 'SELL', 'sell', 'SELLCOIN', 'sellcoin'])(
    'stops the trailing %s market instead of the focused short',
    async (alias) => {
      const { run, exchange, placedOrders } = fixture(true);
      const symbol = alias.toUpperCase().endsWith('USDT') ? alias.toUpperCase() : alias.toUpperCase() + 'USDT';
      exchange.store.positions = [position('BTCUSDT', PositionSide.Short), position(symbol)];
      const result = await run(`stop at 75700 ${alias}`);
      expect(result.success).toBe(true);
      expect(placedOrders()).toEqual([expect.objectContaining({ symbol, side: OrderSide.Sell })]);
    },
  );

  it.each(['buy', 'BUY', 'sell', 'SELL'])('retains leading %s as the stop side', async (side) => {
    const { run, exchange, placedOrders } = fixture(true);
    exchange.store.positions = [
      position('BTCUSDT', side.toLowerCase() === 'buy' ? PositionSide.Short : PositionSide.Long),
    ];
    expect((await run(`stop ${side} at 75700`)).success).toBe(true);
    expect(placedOrders()).toEqual([expect.objectContaining({ symbol: 'BTCUSDT', side: side.toLowerCase() })]);
  });

  it.each(['long', 'LONG', 'short', 'SHORT'])(
    'closes the trailing %s market instead of the focused position',
    async (alias) => {
      const { run, exchange, placedOrders } = fixture(true);
      const symbol = alias.toUpperCase() + 'USDT';
      exchange.store.positions = [position(), position(symbol)];
      expect((await run(`close 50% ${alias}`)).success).toBe(true);
      expect(placedOrders()).toEqual([expect.objectContaining({ symbol })]);
    },
  );

  it.each([
    'buy $500 SYMBOL',
    'sell $500 SYMBOL',
    'buy $500 at 75800 SYMBOL',
    'sell $500 best SYMBOL',
    'buy $500 stop 75700 tp 76100 SYMBOL',
    'sell $500 stop 75900 tp 75000 SYMBOL',
    'scale buy $500 into 2 from 75700 to 75800 SYMBOL',
    'swarm buy $500 into 2 SYMBOL',
    'trail 1% SYMBOL',
    'be +100 SYMBOL',
    'close 50% SYMBOL',
  ])('keeps the trailing side-word alias in %s', async (template) => {
    for (const alias of ['BUY', 'buy', 'SELL', 'sell', 'SELLCOIN', 'sellcoin']) {
      const { run, exchange, placedOrders } = fixture(true);
      const symbol = alias.toUpperCase() + 'USDT';
      exchange.store.positions.push(position(symbol));
      expect((await run(template.replace('SYMBOL', template.startsWith('scale ') ? symbol : alias))).success).toBe(
        true,
      );
      expect(placedOrders().length).toBeGreaterThan(0);
      expect(placedOrders().every((order) => order.symbol === symbol)).toBe(true);
    }
  });

  it.each(['chase buy $500 SYMBOL', 'chase sell $500 SYMBOL'])('keeps the chase symbol in %s', async (template) => {
    for (const alias of ['BUY', 'buy', 'SELL', 'sell']) {
      const { run, startChase } = fixture(true);
      expect((await run(template.replace('SYMBOL', alias))).success).toBe(true);
      expect(startChase.mock.calls.map((call) => call[2])).toEqual([alias.toUpperCase() + 'USDT']);
    }
  });

  it.each(['twap buy $500 into 2 over 10 SYMBOL', 'twap chase sell $500 into 2 over 10 SYMBOL'])(
    'keeps the task symbol in %s',
    async (template) => {
      for (const alias of ['BUY', 'buy', 'SELL', 'sell']) {
        const { run } = fixture(true);
        const result = await run(template.replace('SYMBOL', alias));
        expect(result.success).toBe(true);
        expect(result.taskAction?.params?.symbol).toBe(alias.toUpperCase() + 'USDT');
      }
    },
  );

  it.each(['BUY', 'buy', 'SELL', 'sell'])(
    'bumps orders on trailing %s without changing the selector',
    async (alias) => {
      const { run, exchange, updatedOrders } = fixture(true);
      const symbol = alias.toUpperCase() + 'USDT';
      exchange.store.orders.push({ ...exchange.store.orders[0], id: 'target', symbol });
      expect((await run(`bump +100 ${alias}`)).success).toBe(true);
      expect(updatedOrders().map((update) => update.order.symbol)).toEqual([symbol]);
    },
  );
});

describe('ambiguous trailing modifier aliases refuse before dispatch', () => {
  it.each([
    'buy $500 reduce',
    'buy $500 reduce best',
    'buy $500 reduce stop 75700',
    'chase buy $500 reduce to 1%',
    'twap buy $500 reduce into 2 over 10',
    'twap chase buy $500 into 2 over 10 timeout 30% next reduce',
    'swarm buy $500 into 2 reduce irregular',
    'scale buy $500 into 2 from 75700 to 75800 reduce taker',
    'sell $500 at 75800 reduce',
    'buy $500 best reduce',
    'chase buy $500 reduce',
    'twap buy $500 into 2 over 10 reduce',
    'twap chase buy $500 into 2 over 10 reduce',
    'swarm buy $500 into 2 reduce',
    'scale buy $500 into 2 from 75700 to 75800 reduce',
  ])('refuses %s with a typed notSent error when REDUCE is a loaded alias', async (command) => {
    const { run, write, startChase } = fixture(true);
    for (const token of ['reduce', 'REDUCE']) {
      const result = await run(command.replace('reduce', token));
      expect(result.success).toBe(false);
      expect(result.errorDetails).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
      expect(write).not.toHaveBeenCalled();
      expect(startChase).not.toHaveBeenCalled();
      expect(result.taskAction).toBeUndefined();
    }
  });
});

it.each(['buy REDUCEUSDT $500 reduce', 'sell $500 REDUCEUSDT reduce', 'buy $500 at 75800 REDUCEUSDT reduce'])(
  'retains an unambiguous explicit scope before a modifier: %s',
  async (command) => {
    const { run, exchange, placedOrders } = fixture(true);
    exchange.store.positions.push(position('REDUCEUSDT'));
    expect((await run(command)).success).toBe(true);
    expect(placedOrders()).toEqual([expect.objectContaining({ symbol: 'REDUCEUSDT', reduceOnly: true })]);
  },
);

it.each(['buy', 'BUY', 'sell', 'SELL'])(
  'separates leading stop side %s from the trailing matching alias',
  async (side) => {
    const { run, exchange, placedOrders } = fixture(true);
    const symbol = side.toUpperCase() + 'USDT';
    exchange.store.positions = [
      position(),
      position(symbol, side.toLowerCase() === 'buy' ? PositionSide.Short : PositionSide.Long),
    ];
    expect((await run(`stop ${side} at 75700 ${side}`)).success).toBe(true);
    expect(placedOrders()).toEqual([expect.objectContaining({ symbol, side: side.toLowerCase() })]);
  },
);

it.each(['long', 'LONG', 'short', 'SHORT'])(
  'retains leading close selector %s with colliding loaded aliases',
  async (side) => {
    const { run, exchange, placedOrders } = fixture(true);
    exchange.store.positions = [position('BTCUSDT', PositionSide.Long), position('BTCUSDT', PositionSide.Short)];
    expect((await run(`close ${side} 50%`)).success).toBe(true);
    expect(placedOrders()).toEqual([
      expect.objectContaining({
        symbol: 'BTCUSDT',
        side: side.toLowerCase() === 'long' ? OrderSide.Sell : OrderSide.Buy,
      }),
    ]);
  },
);

it.each(['buy', 'BUY', 'sell', 'SELL'])('keeps leading bump side %s in its selector slot', async (side) => {
  const { run, exchange, updatedOrders } = fixture(true);
  const order = exchange.store.orders[0];
  const orderSide = side.toLowerCase() === 'buy' ? OrderSide.Buy : OrderSide.Sell;
  exchange.store.orders = [
    { ...order, side: orderSide },
    { ...order, id: 'alias', symbol: side.toUpperCase() + 'USDT', side: orderSide },
  ];
  expect((await run(`bump ${side} +100`)).success).toBe(true);
  expect(updatedOrders().map((update) => update.order.symbol)).toEqual(['BTCUSDT']);
});

it.each(['BUY', 'buy', 'SELL', 'sell'])('reads a complete side-word leverage alias %s', async (alias) => {
  const { run, exchange, write } = fixture(true);
  const symbol = alias.toUpperCase() + 'USDT';
  exchange.store.positions.push(position(symbol));
  expect((await run(`lev ${alias}`)).success).toBe(true);
  expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-log', { message: `${symbol}: 1x` });
  expect(write).not.toHaveBeenCalled();
});

describe('catalogued modifier aliases do not depend on ticker readiness', () => {
  const commands = [
    'buy $500 reduce',
    'sell $500 reduce',
    'chase buy $500 reduce',
    'scale buy $500 into 2 from 75700 to 75800 reduce',
    'swarm buy $500 into 2 reduce',
    'twap buy $500 into 2 over 10 reduce',
    'twap chase buy $500 into 2 over 10 reduce',
  ];
  it.each(commands.flatMap((command) => ['ready', 'missing-alias', 'omitted'].map((state) => ({ command, state }))))(
    'refuses $command with ticker state $state before writing on the focused market',
    async ({ command, state }) => {
      for (const token of ['reduce', 'REDUCE']) {
        const { exchange, write, startChase } = fixture(true);
        const { tickers, ...withoutTickers } = exchange.store;
        const testExchange: ExchangeLike = {
          ...exchange,
          store:
            state === 'omitted'
              ? withoutTickers
              : {
                  ...exchange.store,
                  tickers: state === 'missing-alias' ? tickers.filter((row) => row.symbol !== 'REDUCEUSDT') : tickers,
                },
        };
        expect(testExchange.store?.markets?.some((row) => row.symbol === 'REDUCEUSDT')).toBe(true);
        const result = await executeCommand(
          command.replace('reduce', token),
          'BTCUSDT',
          undefined,
          testExchange,
          new WorkerPlatformAdapter(testExchange),
        );
        expect(
          result.success,
          JSON.stringify({
            command,
            state,
            token,
            writes: write.mock.calls,
            chaseSymbols: startChase.mock.calls.map((row) => row[2]),
            taskSymbol: result.taskAction?.params?.symbol,
          }),
        ).toBe(false);
        expect(result.errorDetails).toMatchObject({ kind: 'invalidInput', outcome: 'notSent' });
        expect(write).not.toHaveBeenCalled();
        expect(startChase).not.toHaveBeenCalled();
        expect(result.taskAction).toBeUndefined();
      }
    },
  );
});

it('placement observations reject an unexpected extra write instead of silently filtering it', async () => {
  const { write, placedOrders } = fixture();
  await write({
    symbol: 'BUYUSDT',
    side: OrderSide.Buy,
    type: OrderType.Limit,
    amount: 1,
    price: 75800,
    takeProfit: 76100,
  });
  await write('BTCUSDT', 50);
  expect(write.mock.calls).toHaveLength(2);
  expect(() => placedOrders()).toThrow('Unexpected placement observation');
});

it('amendment observations reject an unexpected extra write instead of silently filtering it', async () => {
  const { write, updatedOrders } = fixture();
  await write({ order: openOrder(), update: { price: 75800 } });
  await write('BTCUSDT', 50);
  expect(write.mock.calls).toHaveLength(2);
  expect(() => updatedOrders()).toThrow('Unexpected amendment observation');
});
