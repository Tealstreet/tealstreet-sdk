import { describe, it, expect, vi, beforeEach } from 'vitest';
import { CloseCommand } from './CloseCommand';
import type { CommandContext } from '../Command';
import { OrderSide, OrderType, PositionSide } from '@tealstreet/safe-cex/src/types';
import type { ExchangeLike } from '../../types';
import { MemoryPreprocessStore } from '../../preprocess/memoryStore';

// Helper to create mock exchange
function createMockExchange(
  positions: Array<{ symbol: string; side: PositionSide; contracts: number }> = [],
  overrides: Partial<ExchangeLike> = {},
): ExchangeLike {
  return {
    name: 'test-exchange',
    store: {
      markets: [
        { symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', precision: { amount: 0.001, price: 0.01 } },
        { symbol: 'ETHUSDT', base: 'ETH', quote: 'USDT', precision: { amount: 0.01, price: 0.01 } },
        { symbol: 'WIFUSDT', base: 'WIF', quote: 'USDT', precision: { amount: 1, price: 0.0001 } },
      ],
      tickers: [
        { symbol: 'BTCUSDT', last: 50000 },
        { symbol: 'ETHUSDT', last: 3000 },
        { symbol: 'WIFUSDT', last: 2.5 },
      ],
      orders: [],
      positions,
    },
    emitter: {
      on: vi.fn(),
      off: vi.fn(),
      emit: vi.fn(),
    },
    placeOrder: vi.fn().mockResolvedValue(['order-123']),
    ...overrides,
  } as unknown as ExchangeLike;
}

// Helper to create mock context
function createMockContext(
  args: string[],
  exchange: ExchangeLike | null = null,
  symbol = 'BTCUSDT',
  preprocessStore?: MemoryPreprocessStore,
): CommandContext {
  return {
    raw: args.join(' '),
    args: { _: args },
    symbol,
    accountName: 'test-account',
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
  } as CommandContext;
}

describe('CloseCommand', () => {
  let command: CloseCommand;

  beforeEach(() => {
    command = new CloseCommand();
  });

  describe('command properties', () => {
    it('has correct name', () => {
      expect(command.name).toBe('close');
    });

    it('has correct aliases', () => {
      expect(command.aliases).toContain('c');
    });

    it('has correct description', () => {
      expect(command.description).toBe('Close a position fully or partially');
    });

    it('requires connection', () => {
      expect(command.requiresConnection).toBe(true);
    });
  });

  describe('error handling', () => {
    it('returns error when not connected to exchange', async () => {
      const ctx = createMockContext(['close']);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Not connected to exchange');
    });

    it('returns error when exchange does not support placing orders', async () => {
      const exchange = createMockExchange([], { placeOrder: undefined });
      const ctx = createMockContext(['close'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Exchange does not support placing orders');
    });

    it('returns error when both long and short are specified', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      const ctx = createMockContext(['close', 'long', 'short'], exchange);
      const result = await command.execute(ctx);

      expect(result.success).toBe(false);
      expect(result.error).toBe('Cannot have both long and short closing requests');
    });
  });

  describe('closing positions', () => {
    it('closes long position with sell order', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.5 },
      ]);
      const ctx = createMockContext(['close'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Sell,
          type: OrderType.Market,
          amount: 0.5,
          reduceOnly: true,
        }),
      );
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith('Positions closed');
    });

    it('closes short position with buy order', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Short, contracts: -0.5 },
      ]);
      const ctx = createMockContext(['close'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'BTCUSDT',
          side: OrderSide.Buy,
          type: OrderType.Market,
          amount: 0.5,
          reduceOnly: true,
        }),
      );
    });

    it('closes only long positions when "long" specified', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.5 },
        { symbol: 'BTCUSDT', side: PositionSide.Short, contracts: -0.3 },
      ]);
      const ctx = createMockContext(['close', 'long'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          side: OrderSide.Sell,
          amount: 0.5,
        }),
      );
    });

    it('closes only short positions when "short" specified', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.5 },
        { symbol: 'BTCUSDT', side: PositionSide.Short, contracts: -0.3 },
      ]);
      const ctx = createMockContext(['close', 'short'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          side: OrderSide.Buy,
          amount: 0.3,
        }),
      );
    });

    it('closes position for specific symbol', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.5 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 1.0 },
      ]);
      const ctx = createMockContext(['close', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          amount: 1.0,
        }),
      );
    });

    it('closes short position for specific symbol', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Short, contracts: -0.5 },
        { symbol: 'ETHUSDT', side: PositionSide.Short, contracts: -1.0 },
      ]);
      const ctx = createMockContext(['close', 'short', 'ETHUSDT'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
      expect(exchange.placeOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          symbol: 'ETHUSDT',
          side: OrderSide.Buy,
          amount: 1.0,
        }),
      );
    });

    it('closes multiple symbols specified with comma', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0.5 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 1.0 },
      ]);
      const ctx = createMockContext(['close', 'BTCUSDT,ETHUSDT'], exchange, 'WIFUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(2);
    });

    it('does nothing when no matching positions', async () => {
      const exchange = createMockExchange([
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 1.0 },
      ]);
      const ctx = createMockContext(['close'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('skips positions with zero contracts', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 0 },
      ]);
      const ctx = createMockContext(['close'], exchange, 'BTCUSDT');
      const result = await command.execute(ctx);

      expect(result.success).toBe(true);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });
  });

  describe('partial close (Phase 3)', () => {
    it('closes the given USD notional', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      await command.execute(createMockContext(['close', '$1000'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.reduceOnly).toBe(true);
      expect(order.type).toBe(OrderType.Market);
      expect(order.side).toBe(OrderSide.Sell); // Long → Sell
      // 1000 / 50000 = 0.02
      expect(order.amount).toBeCloseTo(0.02);
    });

    it('accepts the suffix USD form too', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      await command.execute(createMockContext(['close', '1000$'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBeCloseTo(0.02);
    });

    it('closes the given percent of position', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Short, contracts: 0.6 },
      ]);
      await command.execute(createMockContext(['close', '50%'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.side).toBe(OrderSide.Buy); // Short → Buy
      expect(order.amount).toBeCloseTo(0.3);
    });

    it('caps at the position size — 200% does not flip', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      await command.execute(createMockContext(['close', '200%'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBe(1);
    });

    it('errors on hedge ambiguity unless long/short specified', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'BTCUSDT', side: PositionSide.Short, contracts: 0.5 },
      ]);
      const result = await command.execute(createMockContext(['close', '50%'], exchange, 'BTCUSDT'));
      expect(result.success).toBe(false);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
    });

    it('rejects bare numbers per Phase 0 decision 1', async () => {
      // 'close 100' looks like a symbol attempt; since 100 is not a
      // market and not a size token, it falls through to the default
      // (full close) on focused symbol. The behavior we care about is
      // that we do NOT silently treat 100 as USD or coins.
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      await command.execute(createMockContext(['close', '100'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBe(1); // full close, not 100 anything
    });

    it('e2e: `close 1000$` with a base="1000" market still partial-closes correctly', async () => {
      // The validateSymbols fix locks in: '1000$' is never absorbed
      // as a symbol, even when an exchange has a market whose base
      // prefix-matches '1000'.
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      exchange.store!.markets = [
        ...exchange.store!.markets!,
        { symbol: '1000USDT', base: '1000', quote: 'USDT', precision: { amount: 1, price: 0.0001 } } as never,
      ];
      exchange.store!.tickers = [
        ...exchange.store!.tickers!,
        { symbol: '1000USDT', last: 0.5 } as never,
      ];
      await command.execute(createMockContext(['close', '1000$'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.symbol).toBe('BTCUSDT');
      expect(order.reduceOnly).toBe(true);
      // $1000 / 50000 = 0.02 BTC
      expect(order.amount).toBeCloseTo(0.02);
    });
  });

  describe('global close (longs / shorts / allpos / all)', () => {
    function multiPositions() {
      return [
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 },
        { symbol: 'SOLUSDT', side: PositionSide.Short, contracts: 100 },
        { symbol: 'WIFUSDT', side: PositionSide.Short, contracts: 200 },
      ];
    }

    it('"close longs" closes every long position with reduce-only sells', async () => {
      const exchange = createMockExchange(multiPositions());
      const ctx = createMockContext(['close', 'longs'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(calls).toHaveLength(2);
      expect(calls.every((o) => o.side === OrderSide.Sell)).toBe(true);
      expect(calls.every((o) => o.reduceOnly === true)).toBe(true);
      expect(calls.map((o) => o.symbol).sort()).toEqual(['BTCUSDT', 'ETHUSDT']);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith(
        'Closing 2 long position(s)',
      );
    });

    it('"close shorts" closes every short position with reduce-only buys', async () => {
      const exchange = createMockExchange(multiPositions());
      const ctx = createMockContext(['close', 'shorts'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(calls).toHaveLength(2);
      expect(calls.every((o) => o.side === OrderSide.Buy)).toBe(true);
      expect(calls.map((o) => o.symbol).sort()).toEqual(['SOLUSDT', 'WIFUSDT']);
    });

    it('"close allpos" and "close all" both close every position', async () => {
      for (const head of ['allpos', 'all']) {
        const exchange = createMockExchange(multiPositions());
        const ctx = createMockContext(['close', head], exchange, 'BTCUSDT');
        const r = await command.execute(ctx);
        expect(r.success).toBe(true);
        const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls;
        expect(calls).toHaveLength(4);
        expect(ctx.adapter.printSuccess).toHaveBeenCalledWith(
          'Closing 4 open position(s)',
        );
      }
    });

    it('"close longs <pct>" partial-closes every long', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 },
      ]);
      const ctx = createMockContext(['close', 'longs', '50%'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(calls).toHaveLength(2);
      const bySymbol = Object.fromEntries(calls.map((o) => [o.symbol, o]));
      expect(bySymbol.BTCUSDT.amount).toBeCloseTo(0.5);
      expect(bySymbol.ETHUSDT.amount).toBeCloseTo(5);
      expect(ctx.adapter.printSuccess).toHaveBeenCalledWith(
        'Partial close placed on 2 long position(s)',
      );
    });

    it('"close longs $1000" partial-closes by USD per leg', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 },
      ]);
      // BTC@50_000 → $1000 = 0.02. ETH@3000 → $1000 ≈ 0.333.
      await command.execute(createMockContext(['close', 'longs', '$1000'], exchange, 'BTCUSDT'));
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      const bySymbol = Object.fromEntries(calls.map((o) => [o.symbol, o]));
      expect(bySymbol.BTCUSDT.amount).toBeCloseTo(0.02);
      expect(bySymbol.ETHUSDT.amount).toBeCloseTo(0.333, 2);
    });

    it('reports the no-op case rather than firing zero orders', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      const ctx = createMockContext(['close', 'shorts'], exchange, 'BTCUSDT');
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
      expect(ctx.adapter.printInfo).toHaveBeenCalledWith(
        '(no shorts positions to close)',
      );
    });

    it('skips zero-contract positions even with `allpos`', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 0 },
      ]);
      await command.execute(createMockContext(['close', 'allpos'], exchange, 'BTCUSDT'));
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls;
      expect(calls).toHaveLength(1);
      expect(calls[0][0].symbol).toBe('BTCUSDT');
    });

    it('caps partial close at the position size on each leg', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      // 200% should not flip the position — capped at 100%.
      await command.execute(createMockContext(['close', 'longs', '200%'], exchange, 'BTCUSDT'));
      const order = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls[0][0];
      expect(order.amount).toBeCloseTo(1);
    });
  });

  describe('whitelist', () => {
    it('skips whitelisted symbols on `close allpos`', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 },
        { symbol: 'SOLUSDT', side: PositionSide.Short, contracts: 100 },
      ]);
      const store = new MemoryPreprocessStore();
      store.addToWhitelist('ETHUSDT');
      const ctx = createMockContext(['close', 'allpos'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(calls).toHaveLength(2);
      expect(calls.map((o) => o.symbol).sort()).toEqual(['BTCUSDT', 'SOLUSDT']);
      expect(ctx.adapter.printWarning).toHaveBeenCalledWith(
        'Skipping whitelisted: ETHUSDT',
      );
    });

    it('skips whitelisted on `close longs`', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 },
      ]);
      const store = new MemoryPreprocessStore();
      store.addToWhitelist('BTCUSDT');
      const ctx = createMockContext(['close', 'longs'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(calls.map((o) => o.symbol)).toEqual(['ETHUSDT']);
    });

    it('reports the no-op-after-whitelist case with the skipped list', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      const store = new MemoryPreprocessStore();
      store.addToWhitelist('BTCUSDT');
      const ctx = createMockContext(['close', 'longs'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).not.toHaveBeenCalled();
      expect(ctx.adapter.printInfo).toHaveBeenCalledWith(
        '(no longs positions to close) (skipped whitelisted: BTCUSDT)',
      );
    });

    it('symbol-scoped close still fires on a whitelisted symbol (whitelist only gates the global path)', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
      ]);
      const store = new MemoryPreprocessStore();
      store.addToWhitelist('BTCUSDT');
      // `close BTCUSDT` (not `close allpos`) — explicit symbol target,
      // user knew what they were doing. Whitelist should NOT block it.
      const ctx = createMockContext(['close', 'BTCUSDT'], exchange, 'BTCUSDT', store);
      const r = await command.execute(ctx);
      expect(r.success).toBe(true);
      expect(exchange.placeOrder).toHaveBeenCalledTimes(1);
    });

    it('case-insensitive: whitelisting "btcusdt" matches a position with symbol "BTCUSDT"', async () => {
      const exchange = createMockExchange([
        { symbol: 'BTCUSDT', side: PositionSide.Long, contracts: 1 },
        { symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 10 },
      ]);
      const store = new MemoryPreprocessStore();
      store.addToWhitelist('btcusdt'); // lowercase add → uppercased internally
      const ctx = createMockContext(['close', 'allpos'], exchange, 'BTCUSDT', store);
      await command.execute(ctx);
      const calls = (exchange.placeOrder as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
      expect(calls.map((o) => o.symbol)).toEqual(['ETHUSDT']);
    });
  });
});
