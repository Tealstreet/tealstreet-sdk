import type { Market, Order, Position } from '@tealstreet/safe-cex/src/types';
import type { CoreAdapter } from './AdapterContext';
import type { HostBindings, HostWriteEvent } from './HostBindings';

import { describe, expect, it, vi } from 'vitest';

import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';

import { isErrorReported, markErrorReported } from '@tealstreet/safe-cex/src/commandOutcome';

import { ScriptBatchOrderError } from '../index';

import { CORE_WRITE_API_PATHS } from '../runtime/runtimeSurface';
import { createCoreAdapter } from './createCoreAdapter';

/**
 * The body is the thing three hosts used to write for themselves. These tests
 * are what stop it drifting back: they run it over a fake `HostBindings` and
 * assert the behaviours the old copies DISAGREED on — rounding, the reduce-only
 * fallback, the combined-balance default, and the write observer firing on a
 * failure as well as a success.
 */

const market = (over: Partial<Market> = {}): Market =>
  ({
    symbol: 'BTCUSDT',
    precision: { amount: 0.1, price: 0.5 },
    limits: { amount: { min: 0, max: 0 } },
    isSpot: false,
    ...over,
  }) as unknown as Market;

const order = (over: Partial<Order> = {}): Order =>
  ({
    id: 'o1',
    symbol: 'BTCUSDT',
    side: 'sell',
    type: OrderType.Limit,
    status: 'open',
    price: 60_000,
    amount: 1,
    remaining: 1,
    filled: 0,
    reduceOnly: false,
    ...over,
  }) as unknown as Order;

const position = (over: Partial<Position> = {}): Position =>
  ({ symbol: 'BTCUSDT', side: 'long', contracts: 2, notional: 120_000, ...over }) as unknown as Position;

interface Fake {
  readonly host: HostBindings;
  readonly placeOrder: ReturnType<typeof vi.fn>;
  readonly cancelOrders: ReturnType<typeof vi.fn>;
  readonly writes: HostWriteEvent[];
}

const fakeHost = (over: Partial<HostBindings> & { exchangeName?: string } = {}): Fake => {
  const placeOrder = vi.fn(async () => ['order-1']);
  const cancelOrders = vi.fn(async () => undefined);
  const writes: HostWriteEvent[] = [];
  const store = {
    markets: [market()],
    tickers: [{ symbol: 'BTCUSDT', mark: 60_000, last: 60_000 }],
    positions: [position()],
    orders: [order()],
    balance: { total: 1000, free: 800, used: 200, upnl: 5 },
  };
  const host: HostBindings = {
    getExchange: () => ({ name: (over.exchangeName ?? 'binance') as never, store, placeOrder, cancelOrders }) as never,
    getStore: () => store as never,
    readActiveAccount: () => 'main',
    readActiveSymbol: () => 'BTCUSDT',
    readStreamingAccounts: () => ['main'],
    readAllAccounts: () => [{ name: 'main', exchange: 'binance' as never }],
    readOrderbook: () => undefined,
    onWrite: (event) => writes.push(event),
    ...over,
  };
  return { host, placeOrder, cancelOrders, writes };
};

describe('createCoreAdapter', () => {
  it('rounds an order size — the thing the CLI never did', () => {
    const { host, placeOrder } = fakeHost();
    return createCoreAdapter(host)
      .placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: OrderType.Market, amount: 1.24 } as never, 'main')
      .then(() => {
        expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 1.2 }));
      });
  });

  it('keeps the raw size on a reduce-only market order that rounds to zero', async () => {
    const { host, placeOrder } = fakeHost();
    await createCoreAdapter(host).placeOrder(
      { symbol: 'BTCUSDT', side: 'sell', type: OrderType.Market, amount: 0.04, reduceOnly: true } as never,
      'main',
    );
    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 0.04 }));
  });

  it('refuses the same size when it is not reduce-only', async () => {
    const { host } = fakeHost();
    await expect(
      createCoreAdapter(host).placeOrder(
        { symbol: 'BTCUSDT', side: 'sell', type: OrderType.Market, amount: 0.04 } as never,
        'main',
      ),
    ).rejects.toThrow(/below the venue lot step/);
  });

  it('reports a failed write to the observer, not just a successful one', async () => {
    const { host, writes } = fakeHost();
    await expect(
      createCoreAdapter(host).placeOrder(
        { symbol: 'NOPE', side: 'buy', type: OrderType.Market, amount: 1 } as never,
        'main',
      ),
    ).rejects.toThrow(/unknown market/);
    expect(writes).toHaveLength(1);
    expect(writes[0]).toMatchObject({ method: 'placeOrder', ok: false });
    expect(writes[0].error).toMatch(/unknown market/);
  });

  describe('partial batch failures', () => {
    const batch = [
      { symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Market, amount: 1 },
      { symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Market, amount: 2 },
    ];

    it('retains accepted IDs and the exact outcome in the failed write event', async () => {
      const failure = Object.assign(new Error('second leg response lost'), {
        partialOrderIds: ['accepted-1'],
        outcome: 'unknown',
      });
      const placeOrders = vi.fn().mockRejectedValue(failure);
      const { host, writes } = fakeHost({ getExchange: () => ({ placeOrders }) });
      const success = vi.fn();
      await createCoreAdapter(host)
        .placeOrders(batch, 'main')
        .then(success, () => undefined);

      expect(success).not.toHaveBeenCalled();
      expect(placeOrders).toHaveBeenCalledTimes(1);
      expect(writes).toEqual([
        {
          method: 'placeOrders',
          account: 'main',
          summary: 'placeOrders 2x BTCUSDT',
          ok: false,
          error: 'second leg response lost',
          partialOrderIds: ['accepted-1'],
          outcome: 'unknown',
        },
      ]);
      expect(writes[0]).not.toHaveProperty('orderIds');
    });

    it('rejects with a typed partial failure without changing the SDK message', async () => {
      const failure = Object.assign(new Error('second leg rejected'), {
        partialOrderIds: ['accepted-1'],
        outcome: 'rejected',
      });
      const { host } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
      const success = vi.fn();
      const caught = await createCoreAdapter(host)
        .placeOrders(batch, 'main')
        .then(success, (error: unknown) => error);

      expect(success).not.toHaveBeenCalled();
      expect(caught).toMatchObject({
        name: 'ScriptBatchOrderError',
        message: 'second leg rejected',
        partialOrderIds: ['accepted-1'],
        outcome: 'rejected',
        cause: failure,
      });
    });

    it('keeps ordinary failures unchanged and never reports them as success', async () => {
      const failure = new Error('batch refused before sending');
      const { host, writes } = fakeHost({
        getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }),
      });
      await expect(createCoreAdapter(host).placeOrders(batch, 'main')).rejects.toBe(failure);
      expect(writes).toEqual([
        {
          method: 'placeOrders',
          account: 'main',
          summary: 'placeOrders 2x BTCUSDT',
          ok: false,
          error: failure.message,
        },
      ]);
    });

    it('retains canonical SDK facts and reporting state without copying arbitrary fields', async () => {
      const fields = {
        partialOrderIds: ['accepted-1'],
        outcome: 'unknown',
        kind: 'network',
        code: 'native-code',
        retryAfterMs: 250,
        command: 'placeOrders',
        commandId: 17,
        receiptKey: 'receipt-17',
        clientOrderIds: ['requested-1'],
        actualClientOrderIds: ['wire-1'],
        failedOrderIndex: 1,
        orderIdentities: [{ symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'wire-1' }],
      };
      const failure = Object.freeze(
        Object.assign(new Error('native message'), fields, { privateContext: 'not public' }),
      );
      markErrorReported(failure);
      expect(Object.hasOwn(failure, 'errorReported')).toBe(false);
      const { host } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
      const caught = await createCoreAdapter(host)
        .placeOrders(batch, 'main')
        .catch((error: unknown) => error);
      expect(caught).toBeInstanceOf(ScriptBatchOrderError);
      expect(caught).toMatchObject({
        ...fields,
        message: 'native message',
        name: 'ScriptBatchOrderError',
        cause: failure,
      });
      expect(isErrorReported(caught)).toBe(true);
      for (const key of Object.keys(fields)) expect(Object.hasOwn(caught as object, key)).toBe(true);
      expect(caught).not.toHaveProperty('privateContext');
    });

    it.each([undefined, 'notSent', 'rejected', 'unknown'] as const)(
      'copies immutable IDs and preserves the exact optional outcome %s',
      async (outcome) => {
        const ids = ['accepted-1', 'accepted-2'];
        const failure = Object.freeze(
          Object.assign(new Error('native message'), {
            partialOrderIds: ids,
            ...(outcome !== undefined && { outcome }),
            partialOrderCount: 2,
            totalOrderCount: 3,
          }),
        );
        const { host, writes } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
        const caught = await createCoreAdapter(host)
          .placeOrders(batch, 'main')
          .catch((error: unknown) => error);
        expect(caught).toBeInstanceOf(ScriptBatchOrderError);
        const partial = caught as ScriptBatchOrderError;
        expect(partial.message).toBe('native message');
        expect(partial.cause).toBe(failure);
        expect(Object.hasOwn(partial, 'partialOrderIds')).toBe(true);
        expect(Object.hasOwn(partial, 'outcome')).toBe(outcome !== undefined);
        expect(Object.hasOwn(writes[0], 'outcome')).toBe(outcome !== undefined);
        expect(partial.outcome).toBe(outcome);
        expect(partial.partialOrderCount).toBe(2);
        expect(partial.totalOrderCount).toBe(3);
        expect(writes[0]).toMatchObject({ ok: false, partialOrderCount: 2, totalOrderCount: 3 });
        expect(Object.isFrozen(partial.partialOrderIds)).toBe(true);
        ids.push('not-part-of-this-failure');
        expect(partial.partialOrderIds).toEqual(['accepted-1', 'accepted-2']);
        expect(writes[0].partialOrderIds).toEqual(['accepted-1', 'accepted-2']);
        expect(writes[0]).not.toHaveProperty('orderIds');
        expect(ScriptBatchOrderError.from(partial)).toBe(partial);
      },
    );

    it.each([[], ['accepted-1', 3], Array(1), 'accepted-1'].map((ids) => ({ ids })))(
      'leaves malformed/empty IDs unchanged (%j)',
      async ({ ids }) => {
        const failure = Object.assign(new Error('ordinary failure'), { partialOrderIds: ids, outcome: 'unknown' });
        const { host, writes } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
        await expect(createCoreAdapter(host).placeOrders(batch, 'main')).rejects.toBe(failure);
        expect(writes[0]).toMatchObject({ ok: false, error: failure.message });
        expect(writes[0]).not.toHaveProperty('partialOrderIds');
        expect(writes[0]).not.toHaveProperty('orderIds');
      },
    );

    it('preserves a failure whose partial metadata cannot be read', async () => {
      const failure = Object.defineProperty(new Error('native message'), 'partialOrderIds', {
        get() {
          throw new Error('unreadable metadata');
        },
      });
      const { host, writes } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
      await expect(createCoreAdapter(host).placeOrders(batch, 'main')).rejects.toBe(failure);
      expect(writes[0]).toMatchObject({ ok: false, error: 'native message' });
    });

    it('omits invalid outcomes and counts instead of inventing facts', async () => {
      const failure = Object.assign(new Error('native message'), {
        partialOrderIds: ['accepted-1'],
        outcome: 'knownPartial',
        partialOrderCount: -1,
        totalOrderCount: 1.5,
      });
      const { host, writes } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
      const caught = await createCoreAdapter(host)
        .placeOrders(batch, 'main')
        .catch((error: unknown) => error);
      expect(caught).toBeInstanceOf(ScriptBatchOrderError);
      for (const key of ['outcome', 'partialOrderCount', 'totalOrderCount']) {
        expect(caught).not.toHaveProperty(key);
        expect(writes[0]).not.toHaveProperty(key);
      }
    });

    it('keeps successful batch IDs on the success path only', async () => {
      const { host, writes } = fakeHost({
        getExchange: () => ({ placeOrders: vi.fn().mockResolvedValue(['id-1', 'id-2']) }),
      });
      await expect(createCoreAdapter(host).placeOrders(batch, 'main')).resolves.toEqual({ orderIds: ['id-1', 'id-2'] });
      expect(writes[0]).toMatchObject({ ok: true, orderIds: ['id-1', 'id-2'] });
      expect(writes[0]).not.toHaveProperty('partialOrderIds');
      expect(writes[0]).not.toHaveProperty('outcome');
    });

    it('keeps the observer text for a plain failure record while preserving its SDK message on the wrapper', async () => {
      const failure = { message: 'serialized native message', partialOrderIds: ['accepted-1'], outcome: 'rejected' };
      const { host, writes } = fakeHost({ getExchange: () => ({ placeOrders: vi.fn().mockRejectedValue(failure) }) });
      const caught = await createCoreAdapter(host).placeOrders(batch, 'main').catch((error: unknown) => error);
      expect(caught).toMatchObject({ name: 'ScriptBatchOrderError', message: failure.message, cause: failure });
      expect(writes[0]).toMatchObject({ ok: false, error: 'Unknown error', partialOrderIds: ['accepted-1'] });
    });

    it('keeps validation failures local and unwrapped', async () => {
      const placeOrders = vi.fn();
      const { host } = fakeHost({ getExchange: () => ({ placeOrders }) });
      await expect(createCoreAdapter(host).placeOrders([{ ...batch[0], amount: 0.04 }], 'main')).rejects.toMatchObject({
        name: 'ScriptValidationError',
      });
      expect(placeOrders).not.toHaveBeenCalled();
    });
  });

  it('cancels exactly the ids named, ignoring the other filter fields', async () => {
    const orders = [order({ id: 'a' }), order({ id: 'b', side: 'buy' }), order({ id: 'c' })];
    const { host, cancelOrders } = fakeHost({
      getStore: () => ({ orders }) as never,
    });
    const result = await createCoreAdapter(host).cancelOrders({ ids: ['a', 'c'], side: 'buy' } as never, 'main');
    expect(result).toEqual({ canceled: 2 });
    expect(cancelOrders).toHaveBeenCalledWith([orders[0], orders[2]]);
  });

  it('treats an EMPTY account list as no accounts, not as all of them', () => {
    // The three hand-rolled copies disagreed here: mobile read `accounts?.length`
    // and summed every streaming account for `[]`, web and CLI read `accounts ??`
    // and summed none.
    const { host } = fakeHost();
    const adapter = createCoreAdapter(host);
    expect(adapter.getCombinedBalance({ accounts: [] })).toMatchObject({ totalEquity: 0 });
    expect(adapter.getCombinedBalance({}).totalEquity).toBe(1000);
  });

  it('throws rather than reporting an empty candle list when the venue has none', async () => {
    const { host } = fakeHost();
    await expect(createCoreAdapter(host).getCandles('BTCUSDT', '1m' as never, { account: 'main' })).rejects.toThrow(
      /does not support OHLCV candles/,
    );
  });

  it('re-resolves an order id against the host store before cancelling', async () => {
    const { host } = fakeHost();
    await expect(createCoreAdapter(host).cancelOrder('missing', { account: 'main' })).rejects.toThrow(
      /no order with id 'missing'/,
    );
  });

  it('lets a zero-size stop through as a Full (infinity) TP/SL where supported', async () => {
    const { host, placeOrder } = fakeHost({ exchangeName: 'bybit' });
    await createCoreAdapter(host).placeOrder(
      { symbol: 'BTCUSDT', side: 'sell', type: OrderType.StopLoss, amount: 0, price: 58_000 } as never,
      'main',
    );
    expect(placeOrder.mock.calls[0][0].amount).toBe(0);
  });

  it('refuses a zero-size stop on a venue without infinity stops', async () => {
    // weex reports `supportsInfinityStops: false`; bybit reports true, which is
    // why the venue is named rather than assumed.
    const { host } = fakeHost({ exchangeName: 'weex' });
    await expect(
      createCoreAdapter(host).placeOrder(
        { symbol: 'BTCUSDT', side: 'sell', type: OrderType.StopLoss, amount: 0, price: 58_000 } as never,
        'main',
      ),
    ).rejects.toThrow(/Full \(Infinity\) TP\/SL/);
  });

  it('refuses a zero-size order that is not stop-like, on any venue', async () => {
    const { host } = fakeHost({ exchangeName: 'bybit' });
    await expect(
      createCoreAdapter(host).placeOrder(
        { symbol: 'BTCUSDT', side: 'sell', type: OrderType.Market, amount: 0 } as never,
        'main',
      ),
    ).rejects.toThrow(/below the venue lot step/);
  });

  it('does NOT extend the reduce-only fallback to a reduce-only LIMIT order', async () => {
    // The fallback belongs to closes. A limit order that rounds away is a
    // mistake worth refusing.
    const { host } = fakeHost();
    await expect(
      createCoreAdapter(host).placeOrder(
        { symbol: 'BTCUSDT', side: 'sell', type: OrderType.Limit, amount: 0.04, price: 60_000, reduceOnly: true } as never,
        'main',
      ),
    ).rejects.toThrow(/below the venue lot step/);
  });

  it('rounds size DOWN on hyperliquid SPOT and to nearest on its perps', async () => {
    // Rounding a spot buy up can exceed the balance held; the perps on the same
    // connection must keep rounding to nearest. Venue AND market, never either
    // alone.
    const spot = fakeHost({ exchangeName: 'hyperliquid', findMarket: () => market({ isSpot: true }) });
    await createCoreAdapter(spot.host).placeOrder(
      { symbol: 'BTCUSDT', side: 'buy', type: OrderType.Limit, amount: 1.29, price: 60_000 } as never,
      'main',
    );
    expect(spot.placeOrder.mock.calls[0][0].amount).toBe(1.2);

    const perp = fakeHost({ exchangeName: 'hyperliquid', findMarket: () => market({ isSpot: false }) });
    await createCoreAdapter(perp.host).placeOrder(
      { symbol: 'BTCUSDT', side: 'buy', type: OrderType.Limit, amount: 1.29, price: 60_000 } as never,
      'main',
    );
    expect(perp.placeOrder.mock.calls[0][0].amount).toBe(1.3);
  });

  it('rounds every price field, not only the order price', async () => {
    const { host, placeOrder } = fakeHost();
    await createCoreAdapter(host).placeOrder(
      {
        symbol: 'BTCUSDT',
        side: 'buy',
        type: OrderType.Limit,
        amount: 1,
        price: 60_000.27,
        stopLoss: 58_000.31,
        takeProfit: 62_000.44,
      } as never,
      'main',
    );
    const sent = placeOrder.mock.calls[0][0];
    expect([sent.price, sent.stopLoss, sent.takeProfit]).toEqual([60_000.5, 58_000.5, 62_000.5]);
  });

  it('rounds a standalone stop trigger, and carries the trigger type through', async () => {
    // `triggerPrice` was absent from PlaceOrderOptions until 1.3.0, so a
    // caller placing a bare stop went around this surface — which is how the
    // CLI's `stop` command sent an unrounded trigger to the venue.
    const { host, placeOrder } = fakeHost();
    await createCoreAdapter(host).placeOrder(
      {
        symbol: 'BTCUSDT',
        side: 'sell',
        type: OrderType.StopLoss,
        amount: 1,
        triggerPrice: 58_000.31,
        triggerType: 'mark_price',
        reduceOnly: true,
      } as never,
      'main',
    );
    const sent = placeOrder.mock.calls[0][0];
    expect(sent.triggerPrice).toBe(58_000.5);
    expect(sent.triggerType).toBe('mark_price');
  });

  describe('a host that declines writes declines ALL of them', () => {
    // Mobile refuses writes as defence-in-depth behind its script bridge. As a
    // declared property rather than hand-written throws, a write method added
    // later cannot silently escape the refusal.
    //
    // The call list is DERIVED from `CORE_WRITE_API_PATHS`, not typed out. The
    // hand-written version enumerated nine methods and missed the tenth —
    // `runCommand`, which is in that list and is the REPL escape hatch, so it
    // executed on a host that had declared itself read-only. A by-hand list
    // reproduces exactly the failure the declared property exists to prevent.
    const invoke: Record<string, (adapter: CoreAdapter) => unknown> = {
      placeOrder: (a) =>
        a.placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: OrderType.Market, amount: 1 } as never, 'main'),
      placeOrders: (a) => a.placeOrders([] as never, 'main'),
      cancelOrder: (a) => a.cancelOrder('o1', { account: 'main' }),
      cancelOrders: (a) => a.cancelOrders({} as never, 'main'),
      updateOrder: (a) => a.updateOrder('o1', { update: { price: 1 } } as never, 'main'),
      closePosition: (a) => a.closePosition({ symbol: 'BTCUSDT' } as never, 'main'),
      setLeverage: (a) => a.setLeverage('BTCUSDT', 5, 'main'),
      setPositionMode: (a) => a.setPositionMode('hedged' as never, 'main'),
      setMarginType: (a) => a.setMarginType('BTCUSDT', 'cross' as never, 'main'),
      runCommand: (a) => a.runCommand('buy $1000', { account: 'main', depth: 1 }),
    };

    const writeMethods = CORE_WRITE_API_PATHS.map((path) => path.slice(path.lastIndexOf('.') + 1));

    it('has a case for every write the SDK declares', () => {
      // If this fails, a write path was added and the refusal below never
      // exercised it — add it to `invoke` rather than deleting this.
      expect(Object.keys(invoke).sort()).toEqual([...writeMethods].sort());
    });

    it.each(writeMethods)('refuses %s', async (method) => {
      const { host, placeOrder } = fakeHost({ writesRefusedReason: 'Mobile script runtime is read-only' });
      await expect(Promise.resolve(invoke[method](createCoreAdapter(host)))).rejects.toThrow(/read-only/);
      expect(placeOrder).not.toHaveBeenCalled();
    });
  });

  it('still serves READS when writes are declined', () => {
    const { host } = fakeHost({ writesRefusedReason: 'Mobile script runtime is read-only' });
    const adapter = createCoreAdapter(host);
    expect(adapter.getPositions({ account: 'main' })).toHaveLength(1);
    expect(adapter.getCombinedBalance({}).totalEquity).toBe(1000);
  });

  it('sets margin type with (isolated, symbol) — not the symbol as the boolean', async () => {
    // safe-cex is `changeMarginType(isolated, symbol?)`. Passing `(symbol,
    // marginType)` coerces the symbol truthy and sets isolated on EVERY call,
    // which is what this did while the seam typed the argument as `unknown`.
    const changeMarginType = vi.fn(async () => undefined);
    const { host } = fakeHost({ getExchange: () => ({ name: 'binance', changeMarginType }) as never });

    await createCoreAdapter(host).setMarginType('BTCUSDT', 'cross' as never, 'main');
    expect(changeMarginType).toHaveBeenCalledWith(false, 'BTCUSDT');

    await createCoreAdapter(host).setMarginType('BTCUSDT', 'isolated' as never, 'main');
    expect(changeMarginType).toHaveBeenLastCalledWith(true, 'BTCUSDT');
  });

  it('amends with the field the caller named, and drops an explicit undefined', async () => {
    const updateOrder = vi.fn(async () => undefined);
    const { host } = fakeHost({
      getExchange: () => ({ name: 'binance', updateOrder, store: { orders: [order()] } }) as never,
      getStore: () => ({ orders: [order()], markets: [market()] }) as never,
    });

    await createCoreAdapter(host).updateOrder('o1', { update: { price: 60_000.27 } } as never, 'main');

    const sent = updateOrder.mock.calls[0][0] as { update: Record<string, unknown> };
    expect(sent.update).toEqual({ price: 60_000.5 });
    expect('amount' in sent.update).toBe(false);
  });

  it('closes each matching position and reports partial ids when a later leg throws', async () => {
    const placeOrder = vi
      .fn()
      .mockResolvedValueOnce(['id-1'])
      .mockRejectedValueOnce(new Error('venue said no'));
    const positions = [position({ side: 'long' }), position({ side: 'short', contracts: 1 })];
    const { host, writes } = fakeHost({
      getExchange: () => ({ name: 'binance', placeOrder, store: { positions } }) as never,
      getStore: () => ({ positions, markets: [market()] }) as never,
    });

    await expect(createCoreAdapter(host).closePosition({ symbol: 'BTCUSDT' } as never, 'main')).rejects.toThrow(
      /venue said no/,
    );
    // The first leg is already live — an observer that only saw the throw
    // would report the whole close as having done nothing.
    expect(writes.at(-1)).toMatchObject({ method: 'closePosition', ok: false, orderIds: ['id-1'] });
  });

  // A reconnect disposes the account's exchange and builds a new one under the
  // same name. `closePosition` awaits once per leg, so a hedge-mode close must
  // re-resolve: leg 2 sent to the object leg 1 was placed on is sent to a dead
  // exchange, and the short side silently stays open.
  it('resolves the exchange per leg, so a replacement mid-close reaches the new one', async () => {
    // Every `getExchange` hands back a fresh object, the way the controller does
    // after `fullyCleanupAccount` disposes one and `registerAccount` builds its
    // replacement. Asserting on WHICH `placeOrder` each leg used — rather than
    // on how many times `getExchange` was called — keeps this about the
    // invariant and not about where the pre-flight check happens to sit.
    const used: Array<ReturnType<typeof vi.fn>> = [];
    const positions = [position({ side: 'long' }), position({ side: 'short', contracts: 1 })];
    const { host } = fakeHost({
      getExchange: () => {
        const placeOrder = vi.fn(async () => [`id-${used.length + 1}`]);
        return ({
          name: 'binance',
          placeOrder: (opts: never) => {
            used.push(placeOrder);
            return placeOrder(opts);
          },
          store: { positions },
        }) as never;
      },
      getStore: () => ({ positions, markets: [market()] }) as never,
    });

    const result = await createCoreAdapter(host).closePosition({ symbol: 'BTCUSDT' } as never, 'main');

    expect(used).toHaveLength(2);
    // A captured handle would have sent both legs through one `placeOrder`.
    expect(used[0]).not.toBe(used[1]);
    expect(result.orderIds).toHaveLength(2);
  });

  it('sizes a bare closePosition in NATIVE contracts, not base', async () => {
    // `position.contracts` is BASE. The fallback used to hand it straight to
    // `placeOrder`, whose `amount` is NATIVE — so on an OKX-shaped market with
    // `contractSize: 1000` a close asked for a thousand times the position.
    const okx = market({ contractSize: 1000 } as never);
    const held = position({ contracts: 2000, notional: 120_000 });
    const { host, placeOrder } = fakeHost({
      getStore: () => ({ positions: [held], markets: [okx] }) as never,
      findMarket: () => okx,
    });

    await createCoreAdapter(host).closePosition({ symbol: 'BTCUSDT' } as never, 'main');

    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 2, reduceOnly: true }));
  });

  it('leaves an explicitly supplied close amount alone — it is already native', async () => {
    const okx = market({ contractSize: 1000 } as never);
    const held = position({ contracts: 2000, notional: 120_000 });
    const { host, placeOrder } = fakeHost({
      getStore: () => ({ positions: [held], markets: [okx] }) as never,
      findMarket: () => okx,
    });

    await createCoreAdapter(host).closePosition({ symbol: 'BTCUSDT', amount: 1 } as never, 'main');

    expect(placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 1 }));
  });

  it('asks the host to name the venue before guessing from the exchange handle', () => {
    const readExchangeNameForAccount = vi.fn(() => 'okx' as never);
    const { host } = fakeHost({ readExchangeNameForAccount });
    expect(createCoreAdapter(host).getActiveExchange()).toBe('okx');
    expect(readExchangeNameForAccount).toHaveBeenCalledWith('main');
  });

  it('clamps a reduce-only order on an auto-downsize venue, and not elsewhere', async () => {
    const binance = fakeHost();
    (await binance.host) && null;
    const hyper = fakeHost({ exchangeName: 'hyperliquid' });
    await createCoreAdapter(hyper.host).placeOrder(
      { symbol: 'BTCUSDT', side: 'sell', type: OrderType.Market, amount: 5, reduceOnly: true } as never,
      'main',
    );
    // 5 asked for, 2 contracts held, 1 already resting as a reduce-only sell
    // ahead of it — so 1 is the only size that does not over-reduce.
    expect(hyper.placeOrder.mock.calls[0][0].amount).toBe(1);

    await createCoreAdapter(binance.host).placeOrder(
      { symbol: 'BTCUSDT', side: 'sell', type: OrderType.Market, amount: 5, reduceOnly: true } as never,
      'main',
    );
    expect(binance.placeOrder.mock.calls[0][0].amount).toBe(5);
  });
});
