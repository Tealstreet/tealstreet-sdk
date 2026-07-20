import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ScaleCommand } from './ScaleCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderType, OrderTimeInForce, type PlaceOrderOpts } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';

function makeExchange(): ExchangeLike {
  return {
    name: 'test',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50_000 },
        { symbol: 'ETHUSDT', last: 3_000 },
      ],
      orders: [],
      positions: [],
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrders: vi.fn().mockImplementation(async (orders: PlaceOrderOpts[]) => orders.map((_, i) => `order-${i}`)),
  } as unknown as ExchangeLike;
}

function ctx(
  args: string[],
  exchange: ExchangeLike,
  symbol = 'BTCUSDT',
  preprocessStore?: MemoryPreprocessStore,
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'a',
    exchange,
    preprocessStore,
    adapter: {
      print: vi.fn(),
      printInfo: vi.fn(),
      printError: vi.fn(),
      printSuccess: vi.fn(),
      printWarning: vi.fn(),
      listAccounts: vi.fn().mockReturnValue([]),
      getAccount: vi.fn(),
      addAccount: vi.fn(),
      updateAccount: vi.fn(),
      removeAccount: vi.fn(),
      setDefaultAccount: vi.fn(),
      getDefaultSymbol: vi.fn(),
      setDefaultSymbol: vi.fn(),
      getActiveAccounts: vi.fn().mockReturnValue([]),
      createExchange: vi.fn(),
      startExchange: vi.fn(),
      stopExchange: vi.fn(),
    },
  } as unknown as CommandContext;
}

function placedOrders(ex: ExchangeLike): PlaceOrderOpts[] {
  const calls = (ex.placeOrders as ReturnType<typeof vi.fn>).mock.calls;
  // ScaleCommand calls placeOrders ONCE with the full batch.
  return calls[0]?.[0] ?? [];
}

describe('ScaleCommand — preconditions', () => {
  let cmd: ScaleCommand;
  beforeEach(() => {
    cmd = new ScaleCommand();
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(
      ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], null as unknown as ExchangeLike),
    );
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Not connected/);
  });

  it("errors when exchange doesn't support placeOrders", async () => {
    const ex = makeExchange();
    delete (ex as unknown as { placeOrders?: unknown }).placeOrders;
    const r = await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/does not support/);
  });

  it('errors when ticker price is unavailable', async () => {
    const ex = makeExchange();
    ex.store!.tickers = [];
    // notionalToSize throws when the ticker is missing. ScaleCommand
    // catches that into a CommandResult so the user sees a clean
    // error and not a stack trace.
    const r = await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/[Pp]rice/);
  });
});

describe('ScaleCommand — argument validation', () => {
  let cmd: ScaleCommand;
  beforeEach(() => {
    cmd = new ScaleCommand();
  });

  it.each([
    [['scale', 'sideways', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], 'invalid side'],
    [['scale', 'buy', '$1000', 'across', '5', 'from', '49000', 'to', '50000'], 'wrong "into" keyword'],
    [['scale', 'buy', '$1000', 'into', 'five', 'from', '49000', 'to', '50000'], 'count not numeric'],
    [['scale', 'buy', '$1000', 'into', '5', 'starting', '49000', 'to', '50000'], 'wrong "from" keyword'],
    [['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'till', '50000'], 'wrong "to" keyword'],
    [['scale', 'buy', '100', 'into', '5', 'from', '49000', 'to', '50000'], 'bare number size'],
  ])('rejects malformed input (%s)', async (args, _label) => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(args, ex));
    expect(r.success).toBe(false);
  });
});

describe('ScaleCommand — order distribution', () => {
  let cmd: ScaleCommand;
  beforeEach(() => {
    cmd = new ScaleCommand();
  });

  it('places exactly N orders', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    expect(placedOrders(ex)).toHaveLength(5);
  });

  it('splits total notional evenly across slices (size = total / N)', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    const orders = placedOrders(ex);
    // $1000 total / 50000 last-price = 0.02 BTC total. /5 slices = 0.004 each.
    expect(orders.every((o) => o.amount === 0.004)).toBe(true);
  });

  it('spaces prices evenly between from and to', async () => {
    const ex = makeExchange();
    // Round numbers: from 49000 to 50000 across 5 orders → step 250.
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    const prices = placedOrders(ex)
      .map((o) => o.price)
      .sort((a, b) => (a ?? 0) - (b ?? 0));
    expect(prices).toEqual([49000, 49250, 49500, 49750, 50000]);
  });

  it('handles relative price forms via processPrice', async () => {
    const ex = makeExchange();
    // From -2% to -1% (anchored to 50000) → from 49000 to 49500 across 6 → step 100.
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '6', 'from', '-2%', 'to', '-1%'], ex));
    const prices = placedOrders(ex)
      .map((o) => o.price)
      .sort((a, b) => (a ?? 0) - (b ?? 0));
    expect(prices).toEqual([49000, 49100, 49200, 49300, 49400, 49500]);
  });

  it('orders to/from in either direction — endpoints are min/max regardless', async () => {
    const ex = makeExchange();
    // from 50000 to 49000 (descending) — same orders should land.
    await cmd.execute(ctx(['scale', 'sell', '$1000', 'into', '5', 'from', '50000', 'to', '49000'], ex));
    const prices = placedOrders(ex)
      .map((o) => o.price)
      .sort((a, b) => (a ?? 0) - (b ?? 0));
    expect(prices).toEqual([49000, 49250, 49500, 49750, 50000]);
  });

  it('places sells with side=Sell', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'sell', '$1000', 'into', '3', 'from', '50000', 'to', '51000'], ex));
    expect(placedOrders(ex).every((o) => o.side === OrderSide.Sell)).toBe(true);
  });

  it('places limit orders with PostOnly by default', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '3', 'from', '49000', 'to', '50000'], ex));
    expect(placedOrders(ex).every((o) => o.type === OrderType.Limit)).toBe(true);
    expect(placedOrders(ex).every((o) => o.timeInForce === OrderTimeInForce.PostOnly)).toBe(true);
  });

  it('"taker" flag drops PostOnly so orders can cross', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '3', 'from', '49000', 'to', '50000', 'taker'], ex));
    expect(placedOrders(ex).every((o) => o.timeInForce === undefined)).toBe(true);
  });

  it('"reduce" flag sets reduceOnly=true on every slice', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'sell', '$1000', 'into', '3', 'from', '50000', 'to', '51000', 'reduce'], ex));
    expect(placedOrders(ex).every((o) => o.reduceOnly === true)).toBe(true);
  });

  it('handles trailing instrument scope', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$300', 'into', '3', 'from', '2900', 'to', '3000', 'ETHUSDT'], ex));
    const orders = placedOrders(ex);
    expect(orders.every((o) => o.symbol === 'ETHUSDT')).toBe(true);
    // $300 / 3000 last = 0.1 ETH total. /3 = 0.0333 → adjusted to 0.01 precision = 0.03.
    expect(orders.every((o) => o.amount === 0.03)).toBe(true);
  });
});

describe('ScaleCommand — boundary cases', () => {
  let cmd: ScaleCommand;
  beforeEach(() => {
    cmd = new ScaleCommand();
  });

  it('count = 2 is the realistic boundary — from/to land cleanly', async () => {
    // count = 2 → priceIncrement = (50000 - 49000) / 1 = 1000.
    // Two orders at 49000 and 50000.
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$200', 'into', '2', 'from', '49000', 'to', '50000'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(2);
    const prices = orders.map((o) => o.price).sort((a, b) => (a ?? 0) - (b ?? 0));
    expect(prices).toEqual([49000, 50000]);
  });

  it('rejects bare-number size up front (regex enforces $ prefix)', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['scale', 'buy', '1000', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    expect(r.success).toBe(false);
  });

  it('percent-of-balance entry: 50% scales across N orders', async () => {
    // 50% of $1000 free balance @ 1x leverage = $500 notional. /5
    // slices = $100/slice. /50000 last = 0.002 BTC, adjusted to
    // 0.001 precision → 0.002 each.
    const ex = makeExchange();
    ex.store!.balance = { used: 0, free: 1000, total: 1000, upnl: 0 } as never;
    await cmd.execute(ctx(['scale', 'buy', '50%', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    expect(orders.every((o) => o.amount === 0.002)).toBe(true);
  });

  it('percent-of-position exit: 50% reduce scales out', async () => {
    // 50% of a 1 BTC position = 0.5 BTC. /5 = 0.1/slice. The reduce
    // flag flips the percent-default to position scope.
    const ex = makeExchange();
    ex.store!.positions = [
      {
        id: 'p1',
        symbol: 'BTCUSDT',
        side: 'long',
        contracts: 1,
        entryPrice: 50000,
        notional: 50000,
        leverage: 1,
        unrealizedPnl: 0,
        liquidationPrice: 0,
      } as never,
    ];
    await cmd.execute(ctx(['scale', 'sell', '50%', 'into', '5', 'from', '50000', 'to', '51000', 'reduce'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    expect(orders.every((o) => o.amount === 0.1)).toBe(true);
    expect(orders.every((o) => o.reduceOnly === true)).toBe(true);
  });

  it('explicit %bal / %pos suffixes override the entry/exit default', async () => {
    const ex = makeExchange();
    ex.store!.balance = { used: 0, free: 1000, total: 1000, upnl: 0 } as never;
    await cmd.execute(ctx(['scale', 'buy', '50%bal', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    expect(placedOrders(ex).every((o) => o.amount === 0.002)).toBe(true);
  });

  it('1000$ suffix size form is accepted by the validator', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '1000$', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    expect(placedOrders(ex)).toHaveLength(5);
  });
});

describe('ScaleCommand — multi-order single placeOrders call', () => {
  let cmd: ScaleCommand;
  beforeEach(() => {
    cmd = new ScaleCommand();
  });

  it('issues exactly one placeOrders call (batched)', async () => {
    const ex = makeExchange();
    await cmd.execute(ctx(['scale', 'buy', '$1000', 'into', '10', 'from', '49000', 'to', '50000'], ex));
    expect(ex.placeOrders).toHaveBeenCalledTimes(1);
    const call = (ex.placeOrders as ReturnType<typeof vi.fn>).mock.calls[0][0];
    expect(call).toHaveLength(10);
  });
});

describe('ScaleCommand — suffix-USD size regression', () => {
  let cmd: ScaleCommand;
  beforeEach(() => {
    cmd = new ScaleCommand();
  });

  it('e2e: `scale buy 1000$ into 5 from 49000 to 50000` with a base="1000" market still scales BTCUSDT', async () => {
    const ex = makeExchange();
    ex.store!.markets = [
      ...(ex.store!.markets ?? []),
      { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
    ];
    ex.store!.tickers = [...(ex.store!.tickers ?? []), { symbol: '1000USDT', last: 0.5 } as never];
    await cmd.execute(ctx(['scale', 'buy', '1000$', 'into', '5', 'from', '49000', 'to', '50000'], ex));
    const orders = placedOrders(ex);
    expect(orders).toHaveLength(5);
    expect(orders.every((o) => o.symbol === 'BTCUSDT')).toBe(true);
    // $1000 / 50000 = 0.02 BTC total / 5 slices = 0.004 each
    expect(orders.every((o) => o.amount === 0.004)).toBe(true);
  });

  describe('fatfinger', () => {
    it('rejects when per-leg size exceeds the limit', async () => {
      const ex = makeExchange();
      const store = new MemoryPreprocessStore();
      // 5 legs of 0.004 BTC each. Limit at 0.001 → blocked.
      store.setFatfinger('BTCUSDT', 0.001);
      const r = await new ScaleCommand().execute(
        ctx(['scale', 'buy', '1000$', 'into', '5', 'from', '49000', 'to', '50000'], ex, 'BTCUSDT', store),
      );
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/fatfinger/);
      expect(ex.placeOrders).not.toHaveBeenCalled();
    });

    it('allows when per-leg size is at or below the limit (per-leg, not aggregate)', async () => {
      const ex = makeExchange();
      const store = new MemoryPreprocessStore();
      // 5 legs of 0.004 BTC each. Aggregate is 0.02. Limit at 0.005 →
      // each leg is fine, even though the total exceeds the limit.
      // Matches ichibot: fatfinger is per-individual-order.
      store.setFatfinger('BTCUSDT', 0.005);
      const r = await new ScaleCommand().execute(
        ctx(['scale', 'buy', '1000$', 'into', '5', 'from', '49000', 'to', '50000'], ex, 'BTCUSDT', store),
      );
      expect(r.success).toBe(true);
    });

    it('skips fatfinger on reduce-only scales', async () => {
      const ex = makeExchange();
      ex.store!.positions = [{ symbol: 'BTCUSDT', side: 'long', contracts: 1, entryPrice: 50_000 } as never];
      const store = new MemoryPreprocessStore();
      store.setFatfinger('BTCUSDT', 0.001);
      const r = await new ScaleCommand().execute(
        ctx(['scale', 'sell', '50%pos', 'into', '5', 'from', '50500', 'to', '51000', 'reduce'], ex, 'BTCUSDT', store),
      );
      expect(r.success).toBe(true);
    });
  });

  describe('result.data.orderIds — capture support', () => {
    it('returns every placed leg ID via data.orderIds', async () => {
      const ex = makeExchange();
      (ex.placeOrders as ReturnType<typeof vi.fn>).mockResolvedValue(['sl1', 'sl2', 'sl3', 'sl4', 'sl5']);
      const r = await new ScaleCommand().execute(
        ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex),
      );
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual(['sl1', 'sl2', 'sl3', 'sl4', 'sl5']);
    });

    it('coerces null/undefined placeOrders return to []', async () => {
      const ex = makeExchange();
      (ex.placeOrders as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
      const r = await new ScaleCommand().execute(
        ctx(['scale', 'buy', '$1000', 'into', '5', 'from', '49000', 'to', '50000'], ex),
      );
      expect(r.success).toBe(true);
      expect(r.data?.orderIds).toEqual([]);
    });
  });
});

// TSBUG-106: aggregate budget gates exposure-increasing ladders.
describe('ScaleCommand — budget', () => {
  it('blocks a ladder that exceeds the cap (and places nothing)', async () => {
    const ex = makeExchange();
    const store = new MemoryPreprocessStore();
    store.setBudget(500); // $1000 ladder > $500 cap
    const cmd = new ScaleCommand();
    const r = await cmd.execute(
      ctx(['scale', 'buy', '$1000', 'into', '10', 'from', '50000', 'to', '48500'], ex, 'BTCUSDT', store),
    );
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/budget/i);
    expect(ex.placeOrders).not.toHaveBeenCalled();
  });

  it('charges the whole ladder once when within the cap', async () => {
    const ex = makeExchange();
    const store = new MemoryPreprocessStore();
    store.setBudget(2000);
    const cmd = new ScaleCommand();
    const r = await cmd.execute(
      ctx(['scale', 'buy', '$1000', 'into', '10', 'from', '50000', 'to', '48500'], ex, 'BTCUSDT', store),
    );
    expect(r.success).toBe(true);
    // ~$1000 of notional charged (rounding per rung keeps it close).
    expect(store.getBudgetSpent()).toBeCloseTo(1000, 0);
  });
});
