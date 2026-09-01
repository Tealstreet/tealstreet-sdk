import { describe, it, expect, vi, beforeEach } from 'vitest';
import { StopCommand } from './StopCommand';
import type { CommandContext } from '../Command';
import {
  OrderSide,
  OrderType,
  PositionSide,
  MarginType,
  PositionMode,
  type Position,
} from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

function pos(p: Partial<Position> = {}): Position {
  return {
    id: p.id ?? 'pos1',
    symbol: p.symbol ?? 'BTCUSDT',
    side: p.side ?? PositionSide.Long,
    entryPrice: p.entryPrice ?? 50_000,
    notional: p.notional ?? 50_000,
    leverage: p.leverage ?? 1,
    marginType: p.marginType ?? MarginType.Isolated,
    positionMode: p.positionMode ?? PositionMode.OneWay,
    unrealizedPnl: p.unrealizedPnl ?? 0,
    contracts: p.contracts ?? 1,
    liquidationPrice: p.liquidationPrice ?? 0,
  } as Position;
}

function makeExchange(positions: Position[] = []): ExchangeLike {
  return {
    // A REAL venue id, not 'test': every write now goes through the shared
    // order policy, which asks `exchangeMetadata` about round-down, infinity
    // stops and the reduce-only clamp. A made-up name answers "unknown venue"
    // and no longer exercises the branches these tests are about.
    name: 'binance',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50_000, mark: 50_000 },
        { symbol: 'ETHUSDT', last: 3_000, mark: 3_000 },
      ],
      orders: [],
      positions,
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    placeOrder: vi.fn().mockResolvedValue(['order-1']),
  } as unknown as ExchangeLike;
}

function ctx(args: string[], exchange: ExchangeLike, symbol = 'BTCUSDT'): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'a',
    exchange,
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

function lastOrder(ex: ExchangeLike) {
  const calls = (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls;
  return calls[calls.length - 1][0];
}

function nthOrder(ex: ExchangeLike, n: number) {
  const calls = (ex.placeOrder as ReturnType<typeof vi.fn>).mock.calls;
  return calls[n][0];
}

describe('StopCommand — preconditions', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('errors when not connected', async () => {
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], null as unknown as ExchangeLike));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Not connected/);
  });

  it("errors when exchange doesn't support placeOrder", async () => {
    const ex = makeExchange([pos()]);
    delete (ex as unknown as { placeOrder?: unknown }).placeOrder;
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/does not support/);
  });

  it('errors when there is no position to stop', async () => {
    const ex = makeExchange();
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/No position/);
  });

  it('errors when ticker price is unavailable', async () => {
    const ex = makeExchange([pos()]);
    ex.store!.tickers = [];
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/Current price is not available/);
  });

  it('returns "Not implemented" on a malformed command', async () => {
    const ex = makeExchange([pos()]);
    const r = await cmd.execute(ctx(['stop', 'forty', 'nine'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toBe('Not implemented');
  });
});

describe('StopCommand — market stops on a single position', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('absolute price below mark on a long → StopLoss', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long, contracts: 1 })]);
    await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('BTCUSDT');
    expect(order.side).toBe(OrderSide.Sell); // long → sell to exit
    expect(order.type).toBe(OrderType.StopLoss);
    expect(order.reduceOnly).toBe(true);
    expect(order.price).toBe(49000);
    expect(order.triggerPrice).toBe(49000); // market stop: limit == trigger
    expect(order.amount).toBe(0); // unsized → use position
  });

  it('absolute price above mark on a long → TakeProfit', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '52000'], ex));
    expect(lastOrder(ex).type).toBe(OrderType.TakeProfit);
  });

  it('absolute price above mark on a short → StopLoss', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short, contracts: 0.5 })]);
    await cmd.execute(ctx(['stop', 'at', '52000'], ex));
    const order = lastOrder(ex);
    expect(order.side).toBe(OrderSide.Buy); // short → buy to exit
    expect(order.type).toBe(OrderType.StopLoss);
  });

  it('absolute price below mark on a short → TakeProfit', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Short })]);
    await cmd.execute(ctx(['stop', 'at', '48000'], ex));
    expect(lastOrder(ex).type).toBe(OrderType.TakeProfit);
  });

  it('relative price (-100) becomes absolute via processPrice', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '-100'], ex));
    expect(lastOrder(ex).price).toBe(49900); // 50000 - 100
  });

  it('percent (-2%) becomes absolute via processPrice', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '-2%'], ex));
    expect(lastOrder(ex).price).toBe(49000); // 50000 - 2%
  });

  it('explicit symbol scoping picks the named instrument', async () => {
    const ex = makeExchange([pos({ symbol: 'ETHUSDT', side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '2900', 'ETHUSDT'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('ETHUSDT');
    expect(order.price).toBe(2900);
  });
});

describe('StopCommand — sized stops (partial)', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('"$500 at 1%" places a sized stop at +1%', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', '$500', 'at', '1%'], ex));
    const order = lastOrder(ex);
    expect(order.price).toBe(50500); // +1%
    // $500 / 50000 = 0.01 (rounded to 0.001 amount precision)
    expect(order.amount).toBe(0.01);
  });

  it('"500$ at -1%" suffix form also resolves', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', '500$', 'at', '-1%'], ex));
    expect(lastOrder(ex).amount).toBe(0.01);
  });
});

describe('StopCommand — hedge mode disambiguation', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('errors when both sides are open and no buy/sell specified', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    const r = await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/More than one position/);
    expect(ex.placeOrder).not.toHaveBeenCalled();
  });

  it('"stop buy at <p>" targets the short position', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'buy', 'at', '52000'], ex));
    const order = lastOrder(ex);
    // 'buy' side means we're closing the SHORT position (short → buy to exit).
    expect(order.side).toBe(OrderSide.Buy);
  });

  it('"stop sell at <p>" targets the long position', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', 'at', '49000'], ex));
    expect(lastOrder(ex).side).toBe(OrderSide.Sell);
  });

  it('"stop sell $500 at -1%" — sized hedge stop on the long', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', '$500', 'at', '-1%'], ex));
    const order = lastOrder(ex);
    expect(order.side).toBe(OrderSide.Sell);
    expect(order.amount).toBe(0.01);
    expect(order.price).toBe(49500);
  });
});

describe('StopCommand — Phase 3: limit stops via `trigger <abs>`', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('"trigger <abs>" makes `at` the limit price and trigger the trigger', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    // Long, mark=50000. `at -100` → limit = 49900. `trigger 49500` →
    // trigger fires when price <= 49500 (below mark = stop loss).
    await cmd.execute(ctx(['stop', 'at', '-100', 'trigger', '49500'], ex));
    const order = lastOrder(ex);
    expect(order.price).toBe(49900); // limit
    expect(order.triggerPrice).toBe(49500); // trigger (absolute)
    expect(order.type).toBe(OrderType.StopLoss); // trigger < mark
  });

  it('TP/SL classification uses trigger, not limit', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    // limit price below mark, but trigger ABOVE mark → TakeProfit
    await cmd.execute(ctx(['stop', 'at', '49500', 'trigger', '52000'], ex));
    expect(lastOrder(ex).type).toBe(OrderType.TakeProfit);
  });

  it('errors when `trigger` has no argument', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    const r = await cmd.execute(ctx(['stop', 'at', '49000', 'trigger'], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(/trigger: missing/);
  });

  it.each([
    ['abc', /must be an absolute positive price/],
    ['-1000', /must be an absolute positive price/],
    ['0', /must be an absolute positive price/],
  ])('rejects malformed trigger value "%s"', async (badArg, pattern) => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    const r = await cmd.execute(ctx(['stop', 'at', '49000', 'trigger', badArg], ex));
    expect(r.success).toBe(false);
    expect(r.error).toMatch(pattern);
  });

  it('locks in: trigger is absolute-only — relative forms are NOT applied to it', async () => {
    // The trigger arg is parseFloat'd directly, not through processPrice.
    // `+100` looks like a number in some parsers, but parseFloat('+100')
    // returns 100 — accepted as absolute. Verify that path doesn't
    // accidentally re-anchor against current price.
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', 'at', '49000', 'trigger', '+100'], ex));
    expect(lastOrder(ex).triggerPrice).toBe(100); // absolute, NOT 50100
  });

  it('sized trigger stop carries amount through', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long })]);
    await cmd.execute(ctx(['stop', '$500', 'at', '-1%', 'trigger', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.amount).toBe(0.01);
    expect(order.price).toBe(49500);
    expect(order.triggerPrice).toBe(49000);
  });

  it('hedge mode trigger stop on the long side', async () => {
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', 'at', '-1%', 'trigger', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.side).toBe(OrderSide.Sell);
    expect(order.triggerPrice).toBe(49000);
  });
});

describe('StopCommand — multi-order paths', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('places exactly one order for a single position', async () => {
    const ex = makeExchange([pos()]);
    await cmd.execute(ctx(['stop', 'at', '49000'], ex));
    expect(ex.placeOrder).toHaveBeenCalledTimes(1);
  });

  it('"stop buy at" only fires for the short side, "stop sell at" only fires for the long side', async () => {
    // Same context, two separate command runs — verify each side
    // hits its own position when both are open.
    const ex = makeExchange([
      pos({ id: 'l', side: PositionSide.Long, contracts: 1 }),
      pos({ id: 's', side: PositionSide.Short, contracts: 0.5 }),
    ]);
    await cmd.execute(ctx(['stop', 'sell', 'at', '49000'], ex));
    await cmd.execute(ctx(['stop', 'buy', 'at', '52000'], ex));
    expect(ex.placeOrder).toHaveBeenCalledTimes(2);
    expect(nthOrder(ex, 0).side).toBe(OrderSide.Sell);
    expect(nthOrder(ex, 1).side).toBe(OrderSide.Buy);
  });
});

describe('StopCommand — suffix-USD size regression', () => {
  let cmd: StopCommand;
  beforeEach(() => {
    cmd = new StopCommand();
  });

  it('e2e: `stop 1000$ at 49000` (suffix size) places a sized stop on the focused symbol', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long, contracts: 1 })]);
    await cmd.execute(ctx(['stop', '1000$', 'at', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('BTCUSDT');
    expect(order.price).toBe(49000);
    // $1000 / 50000 = 0.02 BTC contracts
    expect(order.amount).toBeCloseTo(0.02);
  });

  it('e2e: `stop 1000$ at 49000` with a base="1000" market still uses focused symbol', async () => {
    const ex = makeExchange([pos({ side: PositionSide.Long, contracts: 1 })]);
    ex.store!.markets = [
      ...(ex.store!.markets ?? []),
      { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
    ];
    ex.store!.tickers = [
      ...(ex.store!.tickers ?? []),
      { symbol: '1000USDT', last: 0.5 } as never,
    ];
    await cmd.execute(ctx(['stop', '1000$', 'at', '49000'], ex));
    const order = lastOrder(ex);
    expect(order.symbol).toBe('BTCUSDT');
    expect(order.amount).toBeCloseTo(0.02);
  });
});
