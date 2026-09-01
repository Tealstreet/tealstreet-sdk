import { describe, it, expect, vi, beforeEach } from 'vitest';
import { BumpCommand } from './BumpCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderStatus, OrderType, type Order } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';

function order(o: Partial<Order> & { id: string; price: number; side: OrderSide; symbol?: string }): Order {
  return {
    id: o.id,
    symbol: o.symbol ?? 'BTCUSDT',
    side: o.side,
    type: OrderType.Limit,
    status: OrderStatus.Open,
    price: o.price,
    amount: 1,
    filled: 0,
    remaining: 1,
    reduceOnly: false,
    lastUpdate: 0,
  } as Order;
}

function makeExchange(orders: Order[] = []): ExchangeLike {
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
        { symbol: 'BTCUSDT', last: 50000 },
        { symbol: 'ETHUSDT', last: 3000 },
      ],
      orders,
      positions: [],
    },
    emitter: { on: vi.fn(), off: vi.fn(), emit: vi.fn() },
    updateOrder: vi.fn().mockResolvedValue(undefined),
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

function priceUpdates(ex: ExchangeLike): number[] {
  const calls = (ex.updateOrder as unknown as ReturnType<typeof vi.fn>).mock.calls;
  return calls.map((c) => (c[0] as { update: { price: number } }).update.price);
}

describe('BumpCommand — Phase 3 positional forms', () => {
  let cmd: BumpCommand;
  beforeEach(() => {
    cmd = new BumpCommand();
  });

  it('bumps all sides on the focused symbol with no extra args', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Buy }),
      order({ id: 'b', price: 200, side: OrderSide.Sell }),
    ]);
    await cmd.execute(ctx(['bump', '+10'], ex));
    expect(priceUpdates(ex).sort((a, b) => a - b)).toEqual([110, 210]);
  });

  it('bumps a specific instrument when given as a leading arg', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Buy, symbol: 'BTCUSDT' }),
      order({ id: 'b', price: 200, side: OrderSide.Buy, symbol: 'ETHUSDT' }),
    ]);
    await cmd.execute(ctx(['bump', 'ETHUSDT', '+5'], ex));
    expect(priceUpdates(ex)).toEqual([205]);
  });

  it('bumps only buys with positional `buy`', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Buy }),
      order({ id: 'b', price: 200, side: OrderSide.Sell }),
    ]);
    await cmd.execute(ctx(['bump', 'buy', '+10'], ex));
    expect(priceUpdates(ex)).toEqual([110]);
  });

  it('bumps only sells with positional `sell` plus trailing instrument', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Sell, symbol: 'ETHUSDT' }),
      order({ id: 'b', price: 200, side: OrderSide.Buy, symbol: 'ETHUSDT' }),
    ]);
    await cmd.execute(ctx(['bump', 'sell', '-2', 'ETHUSDT'], ex));
    expect(priceUpdates(ex)).toEqual([98]);
  });

  it('bumps top N by price', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Buy }),
      order({ id: 'b', price: 200, side: OrderSide.Buy }),
      order({ id: 'c', price: 150, side: OrderSide.Buy }),
    ]);
    await cmd.execute(ctx(['bump', 'top', '2', '+5'], ex));
    expect(priceUpdates(ex).sort((a, b) => a - b)).toEqual([155, 205]);
  });

  it('bumps bottom N by price scoped by side', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Buy }),
      order({ id: 'b', price: 200, side: OrderSide.Buy }),
      order({ id: 'c', price: 50, side: OrderSide.Sell }), // excluded
    ]);
    await cmd.execute(ctx(['bump', 'buy', 'bottom', '1', '+5'], ex));
    expect(priceUpdates(ex)).toEqual([105]);
  });

  it('legacy flag --buys still works', async () => {
    const ex = makeExchange([
      order({ id: 'a', price: 100, side: OrderSide.Buy }),
      order({ id: 'b', price: 200, side: OrderSide.Sell }),
    ]);
    await cmd.execute(
      ctx(['bump', '+10'], ex) as CommandContext & { args: { buys?: boolean; _: string[] } },
    );
    // Without the flag, both move
    expect(priceUpdates(ex).sort((a, b) => a - b)).toEqual([110, 210]);
  });
});
