import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '@tealstreet/safe-cex/src/exchanges/bybit/bybit.exchange';
import { OrderSide } from '@tealstreet/safe-cex/src/types';
import type { Market } from '@tealstreet/safe-cex/src/types';

import { createConfirmedOrders } from './ConfirmedOrders';
import type { ConfirmedAuditEvent, ConfirmedOrderHandle, ExactQuantity } from './types';

const market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
} as Market;

const exact: ExactQuantity = { decimal: '0.01', atoms: '10', atomScale: 3 };

const store = {
  subscribe: vi.fn(),
  markets: [market],
  marketsBySymbol: { BTCUSDT: market },
  tickersBySymbol: {},
  orders: [],
  positions: [],
} as never;

const exchange = () => {
  const instance = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, store);
  instance.placeOrder = vi.fn().mockResolvedValue(['oid-1']);
  instance.getOrderByClientOrderId = vi.fn().mockResolvedValue({
    retCode: 0,
    result: { category: 'linear', list: [] },
  });
  return instance;
};

describe('ConfirmedOrders', () => {
  it('exposes unconfirmed truth immediately after accepted placement', async () => {
    const orders = createConfirmedOrders({ exchange: exchange(), signerFingerprint: 'signer-1' });
    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    expect(orders.truth(handle).kind).toBe('unconfirmed');
    expect(handle.placement.kind).toBe('accepted');
  });

  it('joins concurrent identical places into one placeOrder', async () => {
    const adapter = exchange();
    const gate = Promise.withResolvers<string[]>();
    adapter.placeOrder = vi.fn(() => gate.promise);
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const input = {
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    };
    const first = orders.place(input);
    const second = orders.place(input);
    gate.resolve(['oid-1']);
    const [a, b] = await Promise.all([first, second]);
    expect(a.callerId).toBe(b.callerId);
    expect(adapter.placeOrder).toHaveBeenCalledTimes(1);
  });

  it('rejects a changed fingerprint for an existing identity before another place', async () => {
    const adapter = exchange();
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    expect(() => {
      void orders.place({
        identity: { ownerId: 'twap-1', unitId: 'slice-0' },
        order: { market, side: OrderSide.Sell },
        exact,
      });
    }).toThrow(/fingerprint/);
    expect(adapter.placeOrder).toHaveBeenCalledTimes(1);
  });

  it('fails unsupported markets and invalid quantity before placeOrder', async () => {
    const adapter = exchange();
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    expect(() => {
      void orders.place({
        identity: { ownerId: 'twap-1', unitId: 'slice-0' },
        order: { market: { ...market, isSpot: true } as Market, side: OrderSide.Buy },
        exact,
      });
    }).toThrow(/not available/);
    expect(() => {
      void orders.place({
        identity: { ownerId: 'twap-1', unitId: 'slice-1' },
        order: { market, side: OrderSide.Buy },
        exact: { decimal: '0', atoms: '0', atomScale: 0 },
      });
    }).toThrow(/positive/);
    expect(adapter.placeOrder).not.toHaveBeenCalled();
  });

  it('keeps empty Bybit lookup unconfirmed and never resends', async () => {
    const adapter = exchange();
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    const truth = await orders.lookup(handle);
    expect(truth.kind).toBe('unconfirmed');
    expect(adapter.placeOrder).toHaveBeenCalledTimes(1);
    expect(adapter.getOrderByClientOrderId).toHaveBeenCalledTimes(1);
  });

  it('applies found fill then latches fill decrease as incident', async () => {
    const adapter = exchange();
    adapter.getOrderByClientOrderId = vi
      .fn()
      .mockResolvedValueOnce({
        retCode: 0,
        result: {
          category: 'linear',
          list: [
            {
              orderLinkId: 'pending',
              orderId: 'oid-1',
              symbol: 'BTCUSDT',
              orderStatus: 'PartiallyFilled',
              qty: '0.01',
              cumExecQty: '0.006',
              avgPrice: '70000',
            },
          ],
        },
      })
      .mockResolvedValueOnce({
        retCode: 0,
        result: {
          category: 'linear',
          list: [
            {
              orderLinkId: 'pending',
              orderId: 'oid-1',
              symbol: 'BTCUSDT',
              orderStatus: 'PartiallyFilled',
              qty: '0.01',
              cumExecQty: '0.001',
              avgPrice: '70000',
            },
          ],
        },
      });
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    const wire = handle.wireClientOrderId;
    adapter.getOrderByClientOrderId = vi
      .fn()
      .mockResolvedValueOnce({
        retCode: 0,
        result: {
          category: 'linear',
          list: [
            {
              orderLinkId: wire,
              orderId: 'oid-1',
              symbol: 'BTCUSDT',
              orderStatus: 'PartiallyFilled',
              qty: '0.01',
              cumExecQty: '0.006',
            },
          ],
        },
      })
      .mockResolvedValueOnce({
        retCode: 0,
        result: {
          category: 'linear',
          list: [
            {
              orderLinkId: wire,
              orderId: 'oid-1',
              symbol: 'BTCUSDT',
              orderStatus: 'PartiallyFilled',
              qty: '0.01',
              cumExecQty: '0.001',
            },
          ],
        },
      });
    const open = await orders.lookup(handle);
    expect(open.kind).toBe('open');
    const incident = await orders.lookup(handle);
    expect(incident.kind).toBe('incident');
    if (incident.kind === 'incident') {
      expect(incident.reason).toBe('fill-decrease');
    }
    const latched = await orders.lookup(handle);
    expect(latched.kind).toBe('incident');
  });

  it('reserves an in-flight identity before audit and rejects changed fields synchronously', async () => {
    const adapter = exchange();
    let release: (ids: string[]) => void = () => undefined;
    adapter.placeOrder = vi.fn(
      () =>
        new Promise<string[]>((resolve) => {
          release = resolve;
        }),
    );
    const emit = vi.fn<(event: ConfirmedAuditEvent) => void>();
    const orders = createConfirmedOrders({
      exchange: adapter,
      signerFingerprint: 'signer-1',
      audit: { emit },
    });
    const first = orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });

    expect(() =>
      orders.place({
        identity: { ownerId: 'twap-1', unitId: 'slice-0' },
        order: { market, side: OrderSide.Sell },
        exact,
      }),
    ).toThrow(/fingerprint/);
    expect(adapter.placeOrder).toHaveBeenCalledTimes(1);
    expect(emit).toHaveBeenCalledTimes(1);

    release(['oid-1']);
    await first;
    expect(emit).toHaveBeenCalledTimes(2);
  });

  it('emits exactly the pre-placement and settled events with stable identity fields', async () => {
    const adapter = exchange();
    const events: ConfirmedAuditEvent[] = [];
    const orders = createConfirmedOrders({
      exchange: adapter,
      signerFingerprint: 'signer-1',
      audit: { emit: (event) => events.push(event) },
    });

    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy, reduceOnly: true },
      exact,
    });

    expect(events.map((event) => event.kind)).toEqual(['confirmed-place', 'confirmed-placed']);
    expect(events[0]).toMatchObject({
      ownerId: 'twap-1',
      unitId: 'slice-0',
      callerId: handle.callerId,
      wireClientOrderId: handle.wireClientOrderId,
      marketId: market.id,
      symbol: market.symbol,
      exact,
    });
    expect(events[1]).toMatchObject({
      ownerId: 'twap-1',
      unitId: 'slice-0',
      callerId: handle.callerId,
      wireClientOrderId: handle.wireClientOrderId,
      venueOrderId: 'oid-1',
    });
    expect(adapter.placeOrder).toHaveBeenCalledWith({
      symbol: market.symbol,
      type: 'market',
      side: OrderSide.Buy,
      reduceOnly: true,
      amount: 0.01,
      clientOrderId: handle.callerId,
    });
  });

  it('rejects inconsistent exact quantity fields before audit or placement', () => {
    const adapter = exchange();
    const emit = vi.fn<(event: ConfirmedAuditEvent) => void>();
    const orders = createConfirmedOrders({
      exchange: adapter,
      signerFingerprint: 'signer-1',
      audit: { emit },
    });

    expect(() =>
      orders.place({
        identity: { ownerId: 'twap-1', unitId: 'slice-0' },
        order: { market, side: OrderSide.Buy },
        exact: { decimal: '0.01', atoms: '11', atomScale: 3 },
      }),
    ).toThrow(/same positive quantity/);
    expect(adapter.placeOrder).not.toHaveBeenCalled();
    expect(emit).not.toHaveBeenCalled();
  });

  it('initializes a definitive placement rejection as zero-fill terminal truth', async () => {
    const adapter = exchange();
    adapter.placeOrder = vi.fn().mockRejectedValue({
      response: { data: { retCode: 110003, retMsg: 'quantity invalid' } },
    });
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });

    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    const truth = orders.truth(handle);

    expect(handle.placement.kind).toBe('rejected');
    expect(truth).toMatchObject({
      kind: 'terminal',
      observation: {
        status: 'rejected',
        total: exact,
        filled: { atoms: '0', atomScale: exact.atomScale },
        residual: exact,
      },
    });
  });

  it('forgets terminal work and releases both identity reservations', async () => {
    const adapter = exchange();
    adapter.placeOrder = vi.fn().mockRejectedValue({
      response: { data: { retCode: 110003, retMsg: 'quantity invalid' } },
    });
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const input = {
      identity: { ownerId: 'twap-forget', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    };

    const handle = await orders.place(input);
    expect(orders.truth(handle).kind).toBe('terminal');
    orders.forget(handle);

    expect(() => orders.truth(handle)).toThrow('Unknown confirmed-order handle');
    await expect(orders.place(input)).resolves.toMatchObject({ identity: input.identity });
    expect(adapter.placeOrder).toHaveBeenCalledTimes(2);
  });

  it('refuses to forget unconfirmed, open, or incident truth', async () => {
    const adapter = exchange();
    let lookupCount = 0;
    adapter.getOrderByClientOrderId = vi.fn().mockImplementation((_symbol, wireClientOrderId) => {
      lookupCount += 1;
      return Promise.resolve({
        retCode: 0,
        result: {
          category: 'linear',
          list: [{
            orderLinkId: wireClientOrderId,
            orderId: 'oid-1',
            symbol: market.id,
            orderStatus: 'PartiallyFilled',
            qty: '0.01',
            cumExecQty: lookupCount === 1 ? '0.006' : '0.005',
          }],
        },
      });
    });
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const handle = await orders.place({
      identity: { ownerId: 'twap-forget', unitId: 'slice-live' },
      order: { market, side: OrderSide.Buy },
      exact,
    });

    expect(() => orders.forget(handle)).toThrow(/truth is unconfirmed/);
    expect((await orders.lookup(handle)).kind).toBe('open');
    expect(() => orders.forget(handle)).toThrow(/truth is open/);
    expect((await orders.lookup(handle)).kind).toBe('incident');
    expect(() => orders.forget(handle)).toThrow(/truth is incident/);
    expect(orders.truth(handle).kind).toBe('incident');
  });

  it('uses exact lookup after a duplicate client-ID placement response', async () => {
    const adapter = exchange();
    adapter.placeOrder = vi.fn().mockRejectedValue({
      response: { data: { retCode: 110072, retMsg: 'OrderLinkedID is duplicate' } },
    });
    adapter.getOrderByClientOrderId = vi.fn().mockImplementation((_symbol, wireClientOrderId) =>
      Promise.resolve({
        retCode: 0,
        result: {
          category: 'linear',
          list: [{
            orderLinkId: wireClientOrderId,
            orderId: 'oid-retried',
            symbol: market.id,
            orderStatus: 'Filled',
            qty: '0.01',
            cumExecQty: '0.01',
          }],
        },
      }),
    );
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });

    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-retried' },
      order: { market, side: OrderSide.Buy },
      exact,
    });

    expect(handle.placement.kind).toBe('ambiguous');
    expect(orders.truth(handle).kind).toBe('unconfirmed');
    await expect(orders.lookup(handle)).resolves.toMatchObject({
      kind: 'terminal',
      observation: { status: 'filled', venueOrderId: 'oid-retried', filled: exact },
    });
    expect(orders.truth(handle)).toMatchObject({
      kind: 'terminal',
      observation: { venueOrderId: 'oid-retried' },
    });
    expect(adapter.getOrderByClientOrderId).toHaveBeenCalledOnce();
  });

  it('records indeterminate lookup reason without erasing last open truth', async () => {
    const adapter = exchange();
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    adapter.getOrderByClientOrderId = vi
      .fn()
      .mockResolvedValueOnce({
        retCode: 0,
        result: {
          category: 'linear',
          list: [{
            orderLinkId: handle.wireClientOrderId,
            orderId: 'oid-1',
            symbol: market.id,
            orderStatus: 'PartiallyFilled',
            qty: '0.01',
            cumExecQty: '0.006',
          }],
        },
      })
      .mockResolvedValueOnce({ retCode: 0, result: { category: 'linear', list: [] } });

    await orders.lookup(handle);
    const retained = await orders.lookup(handle);

    expect(retained).toMatchObject({
      kind: 'open',
      observation: {
        filled: { atoms: '6' },
        lastIndeterminateReason: 'bybit-empty-list-not-absence',
      },
    });
  });

  it('latches terminal regression and refuses a forged handle', async () => {
    const adapter = exchange();
    const orders = createConfirmedOrders({ exchange: adapter, signerFingerprint: 'signer-1' });
    const handle = await orders.place({
      identity: { ownerId: 'twap-1', unitId: 'slice-0' },
      order: { market, side: OrderSide.Buy },
      exact,
    });
    const row = (status: string, filled: string) => ({
      retCode: 0,
      result: {
        category: 'linear',
        list: [{
          orderLinkId: handle.wireClientOrderId,
          orderId: 'oid-1',
          symbol: market.id,
          orderStatus: status,
          qty: '0.01',
          cumExecQty: filled,
        }],
      },
    });
    adapter.getOrderByClientOrderId = vi
      .fn()
      .mockResolvedValueOnce(row('Filled', '0.01'))
      .mockResolvedValueOnce(row('PartiallyFilled', '0.006'));

    expect((await orders.lookup(handle)).kind).toBe('terminal');
    const incident = await orders.lookup(handle);
    expect(incident).toMatchObject({ kind: 'incident', reason: 'terminal-regression' });

    const forged = { ...handle } as ConfirmedOrderHandle;
    expect(() => orders.truth(forged)).toThrow('Unknown confirmed-order handle');
  });
});
