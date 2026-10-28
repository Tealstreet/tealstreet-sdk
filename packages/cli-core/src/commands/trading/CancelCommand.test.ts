import { describe, it, expect, vi, beforeEach } from 'vitest';
import { CancelCommand } from './CancelCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';
import type { Order } from '@tealstreet/safe-cex/src/types';

// Helper to create mock order
function createMockOrder(
  id: string,
  symbol: string,
  side: OrderSide,
  type: OrderType,
): Order {
  return {
    id,
    symbol,
    side,
    type,
    status: 'open',
    price: 50000,
    amount: 1,
    filled: 0,
    remaining: 1,
    createdAt: Date.now(),
  } as Order;
}

// Helper to create mock exchange
function createMockExchange(
  orders: Order[] = [],
  overrides: Partial<ExchangeLike> = {},
): ExchangeLike {
  return {
    name: 'test-exchange',
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
    emitter: {
      on: vi.fn(),
      off: vi.fn(),
      emit: vi.fn(),
    },
    cancelOrders: vi.fn().mockResolvedValue(undefined),
    cancelSymbolOrders: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  } as unknown as ExchangeLike;
}

// Helper to create mock context
function createMockContext(
  args: string[],
  exchange: ExchangeLike | null = null,
  symbol = 'BTCUSDT',
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'test-account',
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
  };
}

describe('CancelCommand', () => {
  let command: CancelCommand;

  beforeEach(() => {
    command = new CancelCommand();
  });

  describe('command properties', () => {
    it('has correct name', () => {
      expect(command.name).toBe('cancel');
    });

    it('has correct aliases', () => {
      expect(command.aliases).toContain('x');
    });

    it('has correct description', () => {
      expect(command.description).toBe('Cancel orders');
    });

    it('requires connection', () => {
      expect(command.requiresConnection).toBe(true);
    });
  });

  describe('error handling', () => {
    it('returns error when not connected to exchange', async () => {
      const ctx = createMockContext(['cancel']);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not connected to exchange');
    });
  });

  describe('cancel all orders', () => {
    it('cancels all orders for session symbol using cancelSymbolOrders', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['cancel'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelSymbolOrders).toHaveBeenCalledWith('BTCUSDT');
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Cancelling all BTCUSDT orders');
    });

    it('cancels all orders for specified symbol', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['cancel', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelSymbolOrders).toHaveBeenCalledWith('ETHUSDT');
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Cancelling all ETHUSDT orders');
    });

    it('falls back to cancelOrders when cancelSymbolOrders not available', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
        createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.Limit),
        createMockOrder('3', 'ETHUSDT', OrderSide.Buy, OrderType.Limit),
      ];
      const exchange = createMockExchange(orders, { cancelSymbolOrders: undefined });
      const ctx = createMockContext(['cancel'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[0], orders[1]]);
    });

    it('returns error when neither cancel method is available', async () => {
      const exchange = createMockExchange([], {
        cancelOrders: undefined,
        cancelSymbolOrders: undefined,
      });
      const ctx = createMockContext(['cancel'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Exchange does not support canceling orders');
    });
  });

  describe('cancel buy orders', () => {
    it('cancels only buy limit orders', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
        createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.Limit),
        createMockOrder('3', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
      ];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'buys'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[0], orders[2]]);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith(
        'Cancelling 2 BTCUSDT buys order(s)',
      );
    });

    it('cancels buy orders for specified symbol', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
        createMockOrder('2', 'ETHUSDT', OrderSide.Buy, OrderType.Limit),
      ];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'buys', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[1]]);
    });

    it('returns error when cancelOrders not available for filtered cancel', async () => {
      const exchange = createMockExchange([], { cancelOrders: undefined });
      const ctx = createMockContext(['cancel', 'buys'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Exchange does not support canceling specific orders');
    });
  });

  describe('cancel sell orders', () => {
    it('cancels only sell limit orders', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
        createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.Limit),
        createMockOrder('3', 'BTCUSDT', OrderSide.Sell, OrderType.Limit),
      ];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'sells'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[1], orders[2]]);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith(
        'Cancelling 2 BTCUSDT sells order(s)',
      );
    });
  });

  describe('cancel stop orders', () => {
    it('cancels only stop orders', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
        createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.StopLoss),
        createMockOrder('3', 'BTCUSDT', OrderSide.Buy, OrderType.TakeProfit),
      ];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'stops'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[1], orders[2]]);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Cancelling all BTCUSDT stops');
    });
  });

  describe('cancel buy stops', () => {
    it('cancels only buy stop orders', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.StopLoss),
        createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.StopLoss),
        createMockOrder('3', 'BTCUSDT', OrderSide.Buy, OrderType.TakeProfit),
        createMockOrder('4', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
      ];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'buy', 'stops'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[0], orders[2]]);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Cancelling all BTCUSDT buy stops');
    });
  });

  describe('cancel sell stops', () => {
    it('cancels only sell stop orders', async () => {
      const orders = [
        createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.StopLoss),
        createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.StopLoss),
        createMockOrder('3', 'BTCUSDT', OrderSide.Sell, OrderType.TakeProfit),
        createMockOrder('4', 'BTCUSDT', OrderSide.Sell, OrderType.Limit),
      ];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'sell', 'stops'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).toHaveBeenCalledWith([orders[1], orders[2]]);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Cancelling all BTCUSDT sell stops');
    });
  });

  describe('tokens on venues listing B / S / T markets', () => {
    // Blofin lists B-USDT and S-USDT, OKX lists S-USDT-SWAP. Symbol
    // resolution is a base-prefix match, so `buys` resolved to BUSDT,
    // `sells` / `stops` to SUSDT and an id suffix like `b3f9` to BUSDT, and
    // the symbol fallback cancelled every order on that market.
    function exchangeWithPrefixMarkets(orders: Order[]): ExchangeLike {
      const exchange = createMockExchange(orders);
      const store = exchange.store as unknown as { markets: unknown[]; tickers: unknown[] };
      for (const base of ['B', 'S', 'T']) {
        store.markets.push({ symbol: `${base}USDT`, base, quote: 'USDT', precision: { amount: 1, price: 0.0001 } });
        store.tickers.push({ symbol: `${base}USDT`, last: 1 });
      }
      return exchange;
    }

    const orders = [
      createMockOrder('1', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
      createMockOrder('2', 'BTCUSDT', OrderSide.Sell, OrderType.Limit),
      createMockOrder('3', 'BTCUSDT', OrderSide.Buy, OrderType.StopLoss),
      createMockOrder('4', 'BTCUSDT', OrderSide.Sell, OrderType.StopLoss),
    ];

    it.each([
      [['buys'], ['1']],
      [['sells'], ['2']],
      [['stops'], ['3', '4']],
      [['buy', 'stops'], ['3']],
      [['sell', 'stops'], ['4']],
    ])('cancel %j cancels only %j', async (filter, expectedIds) => {
      const exchange = exchangeWithPrefixMarkets(orders);
      const ctx = createMockContext(['cancel', ...filter], exchange, 'BTCUSDT');

      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelSymbolOrders).not.toHaveBeenCalled();
      const cancelled = vi.mocked(exchange.cancelOrders!).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(expectedIds);
    });

    it('errors on a stray keyword instead of cancelling everything', async () => {
      // `to` resolves to TUSDT; it only means something after `from <p1>`.
      const exchange = exchangeWithPrefixMarkets(orders);
      const ctx = createMockContext(['cancel', 'to'], exchange, 'BTCUSDT');

      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(exchange.cancelSymbolOrders).not.toHaveBeenCalled();
      expect(exchange.cancelOrders).not.toHaveBeenCalled();
    });

    it('treats a B-prefixed id suffix as an id, not as BUSDT', async () => {
      const idOrders = [...orders, createMockOrder('abc-b3f9', 'BTCUSDT', OrderSide.Buy, OrderType.Limit)];
      const exchange = exchangeWithPrefixMarkets(idOrders);
      const ctx = createMockContext(['cancel', 'b3f9'], exchange, 'BTCUSDT');

      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.cancelSymbolOrders).not.toHaveBeenCalled();
      const cancelled = vi.mocked(exchange.cancelOrders!).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['abc-b3f9']);
    });

    it('errors on an unknown S-prefixed token instead of cancelling SUSDT', async () => {
      const exchange = exchangeWithPrefixMarkets(orders);
      const ctx = createMockContext(['cancel', 'sx9'], exchange, 'BTCUSDT');

      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(exchange.cancelSymbolOrders).not.toHaveBeenCalled();
      expect(exchange.cancelOrders).not.toHaveBeenCalled();
    });
  });

  describe('unrecognized commands', () => {
    it('returns "Not implemented" for a filter that fails the id regex', async () => {
      // `unk!` has a `!` so it's neither a known keyword nor a valid
      // id-suffix (the id regex is `[A-Za-z0-9_-]{3,}`). Falls all the
      // way through to the default "Not implemented" branch.
      const exchange = createMockExchange();
      const ctx = createMockContext(['cancel', 'unk!'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not implemented');
    });

    it('treats an unknown but id-shaped token as a trailing-id match', async () => {
      // Word-shaped tokens that aren\'t keywords now fall into the
      // id-suffix branch; if no order matches, the user gets a clearer
      // "no order matching id" error rather than "Not implemented".
      const exchange = createMockExchange();
      const ctx = createMockContext(['cancel', 'noMatch1'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Order not found');
      expect(ctx.adapter.printError).toHaveBeenCalledWith(
        expect.stringMatching(/No open order matching id "noMatch1"/),
      );
    });

    it('returns not implemented for buy without stops', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['cancel', 'buy'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not implemented');
    });

    it('returns not implemented for sell without stops', async () => {
      const exchange = createMockExchange();
      const ctx = createMockContext(['cancel', 'sell'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not implemented');
    });
  });

  describe('cancel top/bottom by price', () => {
    function limitOrder(id: string, price: number, lastUpdate = 0): Order {
      return {
        id,
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        status: 'open',
        price,
        amount: 1,
        filled: 0,
        remaining: 1,
        lastUpdate,
        reduceOnly: false,
      } as Order;
    }

    it('cancels the top N by price (highest first)', async () => {
      const orders = [limitOrder('a', 50000), limitOrder('b', 51000), limitOrder('c', 49000)];
      const exchange = createMockExchange(orders);
      await command.execute(createMockContext(['cancel', 'top', '2'], exchange, 'BTCUSDT'));
      const arg = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(arg.map((o) => o.id)).toEqual(['b', 'a']);
    });

    it('cancels the bottom N by price (lowest first)', async () => {
      const orders = [limitOrder('a', 50000), limitOrder('b', 51000), limitOrder('c', 49000)];
      const exchange = createMockExchange(orders);
      await command.execute(createMockContext(['cancel', 'bottom', '1'], exchange, 'BTCUSDT'));
      const arg = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(arg.map((o) => o.id)).toEqual(['c']);
    });

    it('cancels first N by lastUpdate (oldest)', async () => {
      const orders = [
        limitOrder('a', 50000, 200),
        limitOrder('b', 51000, 100),
        limitOrder('c', 49000, 300),
      ];
      const exchange = createMockExchange(orders);
      await command.execute(createMockContext(['cancel', 'first', '2'], exchange, 'BTCUSDT'));
      const arg = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(arg.map((o) => o.id)).toEqual(['b', 'a']);
    });

    it('cancels last N by lastUpdate (newest)', async () => {
      const orders = [
        limitOrder('a', 50000, 200),
        limitOrder('b', 51000, 100),
        limitOrder('c', 49000, 300),
      ];
      const exchange = createMockExchange(orders);
      await command.execute(createMockContext(['cancel', 'last', '1'], exchange, 'BTCUSDT'));
      const arg = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(arg.map((o) => o.id)).toEqual(['c']);
    });

    it('rejects non-positive N', async () => {
      const exchange = createMockExchange([limitOrder('a', 50000)]);
      const result = await command.execute(createMockContext(['cancel', 'top', '0'], exchange, 'BTCUSDT'));
      expect(result.success).toBe(false);
    });

    it('no-ops when there are no open limit orders', async () => {
      const exchange = createMockExchange([]);
      const result = await command.execute(createMockContext(['cancel', 'top', '5'], exchange, 'BTCUSDT'));
      expect(result.success).toBe(true);
      expect(exchange.cancelOrders).not.toHaveBeenCalled();
    });
  });

  // ─── New ichibot-style filters ─────────────────────────────────────

  function limitOrderRO(
    id: string,
    side: OrderSide,
    reduceOnly: boolean,
    price = 50000,
    symbol = 'BTCUSDT',
  ): Order {
    return {
      id,
      symbol,
      side,
      type: OrderType.Limit,
      status: 'open',
      price,
      amount: 1,
      filled: 0,
      remaining: 1,
      lastUpdate: 0,
      reduceOnly,
    } as Order;
  }

  describe('cancel ro / xro', () => {
    it('cancel ro keeps only reduce-only limit orders', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Buy, true),
        limitOrderRO('b', OrderSide.Sell, false),
        limitOrderRO('c', OrderSide.Sell, true),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(createMockContext(['cancel', 'ro'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual(['a', 'c']);
    });

    it('cancel xro keeps only non-reduce-only limit orders', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Buy, true),
        limitOrderRO('b', OrderSide.Sell, false),
        limitOrderRO('c', OrderSide.Buy, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(createMockContext(['cancel', 'xro'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual(['b', 'c']);
    });

    it('cancel buys ro filters by side AND reduce-only', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Buy, true),
        limitOrderRO('b', OrderSide.Buy, false),
        limitOrderRO('c', OrderSide.Sell, true),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'buys', 'ro'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['a']);
    });

    it('cancel sells xro filters non-RO sells only', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Sell, false),
        limitOrderRO('b', OrderSide.Sell, true),
        limitOrderRO('c', OrderSide.Buy, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'sells', 'xro'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['a']);
    });

    it('respects the trailing symbol scope', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Buy, true, 50000, 'BTCUSDT'),
        limitOrderRO('b', OrderSide.Buy, true, 3000, 'ETHUSDT'),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'ro', 'ETHUSDT'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['b']);
    });

    it('errors when cancelOrders is not supported', async () => {
      const exchange = createMockExchange([], { cancelOrders: undefined });
      const r = await command.execute(createMockContext(['cancel', 'ro'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(false);
      expect(r.error).toMatch(/does not support/);
    });
  });

  describe('cancel limits', () => {
    it('cancels limit orders, leaving stops untouched', async () => {
      const orders = [
        createMockOrder('lim', 'BTCUSDT', OrderSide.Buy, OrderType.Limit),
        createMockOrder('sl', 'BTCUSDT', OrderSide.Sell, OrderType.StopLoss),
        createMockOrder('tp', 'BTCUSDT', OrderSide.Sell, OrderType.TakeProfit),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(createMockContext(['cancel', 'limits'], exchange, 'BTCUSDT'));
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['lim']);
    });
  });

  describe('cancel from <p1> to <p2>', () => {
    it('cancels limit orders whose price falls inside [p1, p2]', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Buy, false, 49000),
        limitOrderRO('b', OrderSide.Buy, false, 49500),
        limitOrderRO('c', OrderSide.Sell, false, 50500),
        limitOrderRO('d', OrderSide.Sell, false, 51000),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'from', '49250', 'to', '50750'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual(['b', 'c']);
    });

    it('treats p1, p2 in any order (lo/hi normalisation)', async () => {
      const orders = [
        limitOrderRO('a', OrderSide.Buy, false, 49500),
        limitOrderRO('b', OrderSide.Sell, false, 50500),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'from', '50750', 'to', '49250'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual(['a', 'b']);
    });

    it('errors on missing/malformed args', async () => {
      const exchange = createMockExchange([]);
      const r1 = await command.execute(
        createMockContext(['cancel', 'from'], exchange, 'BTCUSDT'),
      );
      expect(r1.success).toBe(false);
      expect(r1.error).toMatch(/Usage: cancel from/);

      const r2 = await command.execute(
        createMockContext(['cancel', 'from', 'abc', 'to', '50000'], exchange, 'BTCUSDT'),
      );
      expect(r2.success).toBe(false);

      const r3 = await command.execute(
        createMockContext(['cancel', 'from', '49000', '50000'], exchange, 'BTCUSDT'),
      );
      expect(r3.success).toBe(false);
    });

    it('respects symbol scope after the price range', async () => {
      const orders = [
        limitOrderRO('btc', OrderSide.Buy, false, 49500, 'BTCUSDT'),
        limitOrderRO('eth', OrderSide.Buy, false, 49500, 'ETHUSDT'),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(
          ['cancel', 'from', '49000', 'to', '50000', 'ETHUSDT'],
          exchange,
          'BTCUSDT',
        ),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['eth']);
    });

    it('does NOT cancel stop orders even when in range', async () => {
      const orders = [
        limitOrderRO('lim', OrderSide.Buy, false, 49500),
        createMockOrder('sl', 'BTCUSDT', OrderSide.Sell, OrderType.StopLoss),
      ];
      // Force the stop's price into range
      (orders[1] as { price: number }).price = 49500;
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'from', '49000', 'to', '50000'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['lim']);
    });

    it('reports zero-match cleanly', async () => {
      const orders = [limitOrderRO('a', OrderSide.Buy, false, 60000)];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'from', '49000', 'to', '50000'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      expect(exchange.cancelOrders).not.toHaveBeenCalled();
    });
  });

  describe('cancel <id-suffix>', () => {
    it('matches an exact order id', async () => {
      const orders = [
        limitOrderRO('order-abc-1234', OrderSide.Buy, false),
        limitOrderRO('order-xyz-9999', OrderSide.Sell, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'order-abc-1234'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['order-abc-1234']);
    });

    it('matches by trailing-id suffix', async () => {
      const orders = [
        limitOrderRO('order-abc-1234', OrderSide.Buy, false),
        limitOrderRO('order-xyz-9999', OrderSide.Sell, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', '1234'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['order-abc-1234']);
    });

    it('cancels multiple orders if the suffix matches more than one', async () => {
      const orders = [
        limitOrderRO('order-abc-1234', OrderSide.Buy, false),
        limitOrderRO('order-zzz-1234', OrderSide.Sell, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', '1234'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual([
        'order-abc-1234',
        'order-zzz-1234',
      ]);
    });

    it('returns "Order not found" when no order matches', async () => {
      const orders = [limitOrderRO('order-abc', OrderSide.Buy, false)];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'noMatch7'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(false);
      expect(r.error).toBe('Order not found');
    });

    it('does not absorb an id that happens to be a known symbol', async () => {
      // `BTCUSDT` is a valid symbol → the symbol-scope branch wins; the
      // id-suffix branch is never reached, so we just cancel all
      // BTCUSDT orders. Locks in the precedence so an id that *happens*
      // to look like a symbol can\'t be smuggled through.
      const orders = [limitOrderRO('a', OrderSide.Buy, false)];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'BTCUSDT'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      expect(exchange.cancelSymbolOrders).toHaveBeenCalledWith('BTCUSDT');
    });
  });

  describe('cancel <id1>,<id2>,… (comma-list — capture workflow)', () => {
    // This is the path `cancel @<var>` follows after substitution, when
    // @<var> was populated by `@<var> = buy …` (bracket orders return
    // 2-3 IDs comma-joined).

    it('cancels every order matching any of the comma-listed IDs', async () => {
      const orders = [
        limitOrderRO('order-abc-1234', OrderSide.Buy, false),
        limitOrderRO('order-def-5678', OrderSide.Sell, false),
        limitOrderRO('order-xyz-9999', OrderSide.Buy, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'order-abc-1234,order-def-5678'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual([
        'order-abc-1234',
        'order-def-5678',
      ]);
    });

    it('comma-list works with suffix matching on each piece', async () => {
      const orders = [
        limitOrderRO('order-abc-1234', OrderSide.Buy, false),
        limitOrderRO('order-def-5678', OrderSide.Sell, false),
      ];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', '1234,5678'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id).sort()).toEqual([
        'order-abc-1234',
        'order-def-5678',
      ]);
    });

    it('rejects mixed valid/invalid pieces — falls through to "Not implemented"', async () => {
      // If any piece isn't ID-shaped, the whole arg falls through to
      // the unknown-command branch. We don't try to be clever about
      // partial matches.
      const orders = [limitOrderRO('order-abc-1234', OrderSide.Buy, false)];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'order-abc-1234,??'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(false);
      expect(exchange.cancelOrders).not.toHaveBeenCalled();
    });

    it('reports all unmatched ids in the error message', async () => {
      const orders = [limitOrderRO('order-abc', OrderSide.Buy, false)];
      const exchange = createMockExchange(orders);
      const ctx = createMockContext(['cancel', 'nomatch1,nomatch2'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(false);
      expect(ctx.adapter.printError).toHaveBeenCalledWith(
        expect.stringMatching(/No open order matching ids "nomatch1", "nomatch2"/),
      );
    });

    it('skips whitespace and empty pieces (e.g. trailing comma)', async () => {
      const orders = [limitOrderRO('order-abc-1234', OrderSide.Buy, false)];
      const exchange = createMockExchange(orders);
      const r = await command.execute(
        createMockContext(['cancel', 'order-abc-1234,'], exchange, 'BTCUSDT'),
      );
      expect(r.success).toBe(true);
      const cancelled = (exchange.cancelOrders as ReturnType<typeof vi.fn>).mock.calls[0][0] as Order[];
      expect(cancelled.map((o) => o.id)).toEqual(['order-abc-1234']);
    });
  });
});
