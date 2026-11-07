import type { AxiosAdapter, AxiosInstance } from 'axios';
import type { Market } from '../../types';

import { AxiosError } from 'axios';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { TDG } from '@tealstreet/grvt-client';
import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';

const instruments = [
  'BTC_USDT_Perp',
  'BTC_USDT_Fut_30Dec26',
  'BTC_USDT_Fut_31Mar27',
  'BTC_USDT_Call_30Dec26_50000',
  'BTC_USDT_Call_30Dec26_60000',
  'BTC_USDT_Put_30Dec26_50000',
];
const order = (instrument: string, id: string, client = '1') => ({
  sa: '777',
  oi: id,
  l: [{ i: instrument }],
  m: { co: client },
});
const instances: GrvtExchange[] = [];
afterEach(() => {
  instances.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

const fixture = (initial = instruments.map((instrument, i) => order(instrument, `0x${i + 1}`, String(i + 1)))) => {
  const store = new DefaultStore();
  store.update({
    markets: instruments.map((id): Market => ({
      id,
      symbol: id.toUpperCase(),
      base: 'BTC',
      quote: 'USDT',
      active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
    })),
  });
  const exchange = new GrvtExchange('grvt', { key: '', secret: '' }, store);
  instances.push(exchange);
  exchange.grvtTradingAccountId = '777';
  exchange.apiExtras = { sessionCookie: 'fixture-session', accountId: 'fixture-account' };
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  const current = new Map(initial.map((row) => [row.oi, row]));
  const requests: Array<{ path: string; body: Record<string, unknown> }> = [];
  let snapshot: unknown = { r: initial };
  let readStatus = 200;
  let readFailure: Error | undefined;
  let writeFailure: Error | undefined;
  let cancelReply: { data: unknown; status: number } | undefined;
  let once = false;
  let onRead: (() => Promise<void> | void) | undefined;
  let onCancel: (() => void) | undefined;
  const adapter: AxiosAdapter = async (config) => {
    const path = new URL(config.url!, config.baseURL).pathname;
    const body = JSON.parse(config.data);
    requests.push({ path, body });
    if (path.endsWith('/open_orders')) {
      await onRead?.();
      if (readFailure) throw readFailure;
      return { status: readStatus, statusText: 'OK', headers: {}, config, data: snapshot };
    }
    if (path.endsWith('/cancel_order')) {
      onCancel?.();
      const reply = cancelReply;
      if (once) {
        cancelReply = undefined;
        once = false;
      }
      if (reply && reply.status >= 400) {
        throw new AxiosError(
          'authentication rejected',
          AxiosError.ERR_BAD_REQUEST,
          config,
          {},
          {
            status: reply.status,
            statusText: 'Unauthorized',
            headers: {},
            config,
            data: reply.data,
          },
        );
      }
      const found = [...current.values()].find((row) => (body.oi ? row.oi === body.oi : row.m.co === body.co));
      if (!found) throw new Error('Cancellation targeted an unknown native order');
      current.delete(found.oi);
      if (writeFailure) throw new AxiosError(writeFailure.message, 'ECONNRESET', config, {});
      return {
        status: reply?.status ?? 200,
        statusText: 'OK',
        headers: {},
        config,
        data: reply ? reply.data : { r: found },
      };
    }
    if (path.endsWith('/cancel_all_orders')) {
      current.clear();
      return { status: 200, statusText: 'OK', headers: {}, config, data: { r: { a: true } } };
    }
    throw new Error(`Unexpected GRVT request: ${path}`);
  };
  exchange.tradingXhr.defaults.adapter = adapter;
  const authenticate = vi.fn().mockResolvedValue({});
  exchange.grvtClient = {
    tdgClient: new TDG({ host: 'https://trades.grvt.io' }, exchange.tradingXhr as AxiosInstance),
    authenticatedEndpoint: authenticate,
  } as unknown as NonNullable<GrvtExchange['grvtClient']>;
  return {
    exchange,
    current,
    requests,
    authenticate,
    setSnapshot: (value: unknown, status = 200) => {
      snapshot = value;
      readStatus = status;
    },
    setAcknowledgement: (data: unknown, status = 200) => {
      cancelReply = { data, status };
    },
    rejectAuthenticationOnce: () => {
      cancelReply = { data: { c: 1000, m: 'session expired' }, status: 401 };
      once = true;
    },
    setReadFailure: (error: Error) => {
      readFailure = error;
    },
    setWriteFailure: (error: Error) => {
      writeFailure = error;
    },
    onRead: (callback: () => Promise<void> | void) => {
      onRead = callback;
    },
    onCancel: (callback: () => void) => {
      onCancel = callback;
    },
  };
};

describe('GRVT exact instrument cancellation', () => {
  it.each(instruments)('cancels only %s and preserves same-base expiries, strikes and kinds', async (instrument) => {
    const f = fixture();
    const index = instruments.indexOf(instrument);
    await f.exchange.cancelSymbolOrders(instrument.toUpperCase());
    expect(f.requests).toEqual([
      { path: '/lite/v1/open_orders', body: { sa: '777' } },
      { path: '/lite/v1/cancel_order', body: { sa: '777', oi: `0x${index + 1}` } },
    ]);
    expect([...f.current.values()].map((row) => row.l[0].i)).toEqual(instruments.filter((id) => id !== instrument));
  });

  it('routes an unassigned native ID by its exact uint64 client ID, ignoring stale local ID mappings', async () => {
    const f = fixture([order(instruments[0], '0x00', '18446744073709551615')]);
    f.exchange._nonceToOrderId.set('18446744073709551615', '0xdead');
    await f.exchange.cancelSymbolOrders(instruments[0].toUpperCase());
    expect(f.requests[1]).toEqual({ path: '/lite/v1/cancel_order', body: { sa: '777', co: '18446744073709551615' } });
    expect(f.current.size).toBe(0);
    expect(f.exchange._nonceToOrderId.get('18446744073709551615')).toBe('0xdead');
  });

  it('accepts the official full envelope and native instrument spelling without case folding', async () => {
    const f = fixture();
    f.setSnapshot({
      result: [
        {
          sub_account_id: '777',
          order_id: '0x1',
          legs: [{ instrument: instruments[0] }],
          metadata: { client_order_id: '1' },
        },
        {
          sub_account_id: '777',
          order_id: '0x2',
          legs: [{ instrument: instruments[0].toUpperCase() }],
          metadata: { client_order_id: '2' },
        },
      ],
    });
    await f.exchange.cancelSymbolOrders(instruments[0].toUpperCase());
    expect(f.requests.filter((r) => r.path.endsWith('/cancel_order')).map((r) => r.body)).toEqual([
      { sa: '777', oi: '0x1' },
    ]);
  });

  it('does not manufacture a broad write for an empty authoritative snapshot', async () => {
    const f = fixture();
    f.setSnapshot({ r: [] });
    await f.exchange.cancelSymbolOrders(instruments[0].toUpperCase());
    expect(f.requests).toEqual([{ path: '/lite/v1/open_orders', body: { sa: '777' } }]);
  });

  it.each([
    null,
    {},
    { r: null },
    { r: {} },
    { r: [], result: [] },
    { r: [], n: 'next' },
    { r: [], next: 'next' },
    { r: [], truncated: true },
    { r: [], c: 1000, m: 'session expired' },
    { r: [order(instruments[0], '0x1'), { oi: '0x2', l: [{ i: instruments[1] }] }] },
    { r: [order(instruments[0], '0x1'), { ...order(instruments[1], '0x2', '2'), sa: '888' }] },
    { r: [{ ...order(instruments[0], '0x1'), l: [] }] },
    { r: [{ ...order(instruments[0], '0x1'), l: [{ i: instruments[0] }, { i: instruments[1] }] }] },
    { r: [{ ...order(instruments[0], '0x1'), l: [{ i: 7 }] }] },
    { r: [{ ...order(instruments[0], '0x1'), oi: 123 }] },
    { r: [order(instruments[0], 'not-a-native-id')] },
    { r: [order(instruments[0], `0x${'f'.repeat(33)}`)] },
    { r: [order(instruments[0], '0x1'), order(instruments[1], '0x01', '2')] },
    { r: [{ sa: '777', l: [{ i: instruments[0] }] }] },
    { r: [{ ...order(instruments[0], '0x00'), m: { co: '01' } }] },
    { r: [order(instruments[0], '0x00', '0')] },
    { r: [{ ...order(instruments[0], '0x00'), m: { co: 9007199254740992 } }] },
    { r: [{ ...order(instruments[0], '0x00'), m: { co: '18446744073709551616' } }] },
    { r: [order(instruments[0], '0x1'), order(instruments[1], '0x1', '2')] },
    { r: [order(instruments[0], '0x1'), order(instruments[1], '0x2')] },
  ])('rejects malformed, ambiguous or incomplete snapshots before the first write: %j', async (snapshot) => {
    const f = fixture();
    f.setSnapshot(snapshot);
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
    expect(f.requests).toEqual([{ path: '/lite/v1/open_orders', body: { sa: '777' } }]);
    expect(f.current.size).toBe(instruments.length);
  });

  it('rejects a partial-content HTTP response before any write', async () => {
    const f = fixture();
    f.setSnapshot({ r: [order(instruments[0], '0x1')] }, 206);
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('snapshot');
    expect(f.requests).toHaveLength(1);
  });

  it('propagates read and authentication failures without writes', async () => {
    const f = fixture();
    f.authenticate.mockRejectedValueOnce(new Error('session refresh failed'));
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('session refresh failed');
    expect(f.requests).toEqual([]);
    f.setReadFailure(new Error('snapshot unavailable'));
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('snapshot unavailable');
    expect(f.requests).toHaveLength(1);
  });

  it('never retries an individual cancellation after an ambiguous connection reset', async () => {
    const f = fixture([order(instruments[0], '0x1'), order(instruments[0], '0x2', '2')]);
    f.setWriteFailure(new Error('response lost after acceptance'));
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('No response received');
    expect(f.current.size).toBe(1);
    expect(f.requests.map((r) => r.path)).toEqual(['/lite/v1/open_orders', '/lite/v1/cancel_order']);
  });

  it.each(['before', 'read', 'write'] as const)('stops cancellation on disposal during %s', async (when) => {
    const f = fixture([order(instruments[0], '0x1'), order(instruments[0], '0x2', '2')]);
    if (when === 'before') f.exchange.dispose();
    if (when === 'read') f.onRead(() => f.exchange.dispose());
    if (when === 'write') f.onCancel(() => f.exchange.dispose());
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('disposed');
    expect(f.requests.filter((r) => r.path.endsWith('/cancel_order'))).toHaveLength(when === 'write' ? 1 : 0);
    expect(f.requests.some((r) => r.path.endsWith('/cancel_all_orders'))).toBe(false);
    expect(f.authenticate).toHaveBeenCalledTimes(when === 'before' ? 0 : when === 'read' ? 1 : 2);
  });

  it('stops after disposal during asynchronous authentication, before requesting a snapshot', async () => {
    const f = fixture();
    f.authenticate.mockImplementationOnce(async () => {
      f.exchange.dispose();
      return {};
    });
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('disposed');
    expect(f.requests).toEqual([]);
  });

  it('does not write if the cancellation account changes while the snapshot is in flight', async () => {
    const f = fixture();
    f.onRead(() => {
      f.exchange.grvtTradingAccountId = '888';
    });
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('account changed');
    expect(f.requests).toHaveLength(1);
  });

  it('propagates authentication failure between the validated snapshot and the first cancel', async () => {
    const f = fixture();
    f.authenticate.mockResolvedValueOnce({}).mockRejectedValueOnce(new Error('session unavailable before write'));
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow(
      'session unavailable before write',
    );
    expect(f.requests).toHaveLength(1);
  });

  it('rejects unknown symbols before authentication or transport effects', async () => {
    const f = fixture();
    await expect(f.exchange.cancelSymbolOrders('UNKNOWN')).rejects.toThrow('Unknown GRVT cancellation market');
    expect(f.authenticate).not.toHaveBeenCalled();
    expect(f.requests).toEqual([]);
  });

  it.each(['orders', 'native-id', 'creation'] as const)(
    'fails closed if client-ID lifecycle evidence changes during write authentication: %s',
    async (change) => {
      const f = fixture([order(instruments[0], '0x00', '9')]);
      f.authenticate.mockResolvedValueOnce({}).mockImplementationOnce(async () => {
        if (change === 'orders')
          f.exchange.store.update({
            orders: [
              {
                id: '9',
                symbol: instruments[1].toUpperCase(),
                type: OrderType.Limit,
                side: OrderSide.Buy,
                status: OrderStatus.Open,
                amount: 1,
                filled: 0,
                remaining: 1,
                price: 1,
                reduceOnly: false,
                lastUpdate: 1700000000000,
              },
            ],
          });
        if (change === 'native-id') f.exchange._nonceToOrderId.set('9', '0xdead');
        if (change === 'creation') f.exchange._nonceCreationTimes.set('9', 1700000000000);
        return {};
      });
      await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('identity changed');
      expect(f.requests).toEqual([{ path: '/lite/v1/open_orders', body: { sa: '777' } }]);
    },
  );

  it('ignores unassigned client IDs when exact native order identities are available', async () => {
    const f = fixture([order(instruments[0], '0x1', '0'), order(instruments[1], '0x2', '0')]);
    await f.exchange.cancelSymbolOrders(instruments[0].toUpperCase());
    expect(f.requests[1]).toEqual({ path: '/lite/v1/cancel_order', body: { sa: '777', oi: '0x1' } });
    expect([...f.current.keys()]).toEqual(['0x2']);
  });

  it.each([
    ['read', 'native-id'],
    ['read', 'creation'],
    ['authentication', 'native-id'],
    ['authentication', 'creation'],
  ] as const)(
    'rejects fallback lifecycle changes during initial %s without any partial writes: %s',
    async (phase, change) => {
      const f = fixture([order(instruments[0], '0x1', '1'), order(instruments[0], '0x00', '9')]);
      const before = f.exchange.store.updateIds.orders;
      const mutate = () => {
        if (change === 'native-id') f.exchange._nonceToOrderId.set('9', '0xdead');
        else f.exchange._nonceCreationTimes.set('9', 1700000000000);
      };
      if (phase === 'read') f.onRead(mutate);
      else
        f.authenticate.mockImplementationOnce(async () => {
          mutate();
          return {};
        });
      await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('identity changed');
      expect(f.exchange.store.updateIds.orders).toBe(before);
      expect(f.requests).toEqual([{ path: '/lite/v1/open_orders', body: { sa: '777' } }]);
      expect(f.current.size).toBe(2);
    },
  );

  it.each([
    { r: { a: true } },
    { result: { ack: true } },
    { r: { a: 'true' } },
    { result: { ack: 'true' } },
    {
      result: {
        sub_account_id: '777',
        order_id: '0x1',
        legs: [{ instrument: instruments[0] }],
        metadata: { client_order_id: '1' },
      },
    },
  ])('accepts explicit native ACKs and an exact full-order proof: %j', async (ack) => {
    const f = fixture([order(instruments[0], '0x1')]);
    f.setAcknowledgement(ack);
    await f.exchange.cancelSymbolOrders(instruments[0].toUpperCase());
    expect(f.requests).toHaveLength(2);
    expect(f.current.size).toBe(0);
  });

  it.each([
    {},
    { r: {} },
    { r: null },
    { r: [] },
    { r: { a: false } },
    { r: { bogus: true } },
    { c: 1000, m: 'session rejected' },
    { code: 1000, message: 'session rejected' },
    { r: { a: true }, next: 'more' },
    { r: order(instruments[0], '0xdead') },
    { r: order(instruments[1], '0x1') },
    { r: { ...order(instruments[0], '0x1'), sa: '888' } },
  ])('rejects HTTP200 error/missing/mismatched ACKs and stops remaining writes: %j', async (ack) => {
    const f = fixture([order(instruments[0], '0x1'), order(instruments[0], '0x2', '2')]);
    f.setAcknowledgement(ack);
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
    expect(f.requests.map((r) => r.path)).toEqual(['/lite/v1/open_orders', '/lite/v1/cancel_order']);
    expect(f.current.has('0x2')).toBe(true);
  });

  it.each(
    ['e', 'error', 'c', 'code'].flatMap((alias) =>
      ['envelope', 'native-order', 'client-order', 'snapshot-row'].map((boundary) => [alias, boundary] as const),
    ),
  )('rejects explicit %s at the %s boundary despite otherwise valid success proof', async (alias, boundary) => {
    const target = order(instruments[0], boundary === 'client-order' ? '0x00' : '0x1', '9');
    const sibling = order(instruments[0], '0x2', '2');
    const f = fixture([target, sibling]);
    const error = alias === 'c' || alias === 'code' ? 1000 : { code: 1000, message: 'rejected' };
    if (boundary === 'snapshot-row') f.setSnapshot({ r: [{ ...target, [alias]: error }, sibling] });
    else
      f.setAcknowledgement(
        boundary === 'envelope' ? { r: { a: true }, [alias]: error } : { r: { ...target, [alias]: error } },
      );
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
    expect(f.requests.map((r) => r.path)).toEqual(
      boundary === 'snapshot-row' ? ['/lite/v1/open_orders'] : ['/lite/v1/open_orders', '/lite/v1/cancel_order'],
    );
    expect(f.current.has('0x2')).toBe(true);
  });

  it.each(['e', 'error', 'c', 'code'])(
    'does not ignore explicit null/zero/false %s fields in an Order proof',
    async (alias) => {
      for (const value of [null, 0, false]) {
        const f = fixture([order(instruments[0], '0x1'), order(instruments[0], '0x2', '2')]);
        f.setAcknowledgement({ r: { ...order(instruments[0], '0x1'), [alias]: value } });
        await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
        expect(f.requests.map((r) => r.path)).toEqual(['/lite/v1/open_orders', '/lite/v1/cancel_order']);
        expect(f.current.has('0x2')).toBe(true);
      }
    },
  );

  it.each(['a', 'ack'].flatMap((alias) => [false, true, null].map((value) => [alias, value] as const)))(
    'rejects mixed Order+%s:%j rather than falling through to Order proof',
    async (alias, value) => {
      const f = fixture([order(instruments[0], '0x1'), order(instruments[0], '0x2', '2')]);
      f.setAcknowledgement({ r: { ...order(instruments[0], '0x1'), [alias]: value } });
      await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
      expect(f.requests.map((r) => r.path)).toEqual(['/lite/v1/open_orders', '/lite/v1/cancel_order']);
      expect(f.current.has('0x2')).toBe(true);
    },
  );

  it('rejects a mismatched client-ID order acknowledgement', async () => {
    const f = fixture([order(instruments[0], '0x00', '9')]);
    f.setAcknowledgement({ r: order(instruments[0], '0x9', '8') });
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('acknowledgement');
    expect(f.requests).toHaveLength(2);
  });

  it('does not accept partial-content cancellation acknowledgements', async () => {
    const f = fixture([order(instruments[0], '0x1')]);
    f.setAcknowledgement({ r: { a: true } }, 206);
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow('acknowledgement');
    expect(f.requests).toHaveLength(2);
  });

  it('does not resend a client-only cancel across the HTTP401/code1000 authentication mutation path', async () => {
    const f = fixture([order(instruments[0], '0x00', '9')]);
    f.rejectAuthenticationOnce();
    const reauthenticate = vi.spyOn(f.exchange, 'authenticate').mockImplementation(async () => {
      f.exchange._nonceToOrderId.set('9', '0xdead');
      f.exchange._nonceCreationTimes.set('9', 1700000000001);
      f.current.delete('0x00');
      f.current.set('0xdead', order(instruments[1], '0xdead', '9'));
      f.exchange.apiExtras = { sessionCookie: 'fresh', accountId: 'fixture-account' };
      return true;
    });
    await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
    expect(reauthenticate).not.toHaveBeenCalled();
    expect(f.requests.map((r) => r.path)).toEqual(['/lite/v1/open_orders', '/lite/v1/cancel_order']);
    expect(f.current.has('0x00')).toBe(true);
  });

  it('retains one safe definitive-auth retry of the same exact native order ID', async () => {
    const f = fixture([order(instruments[0], '0x1')]);
    f.rejectAuthenticationOnce();
    const reauthenticate = vi.spyOn(f.exchange, 'authenticate').mockImplementation(async () => {
      f.exchange.apiExtras = { sessionCookie: 'fresh', accountId: 'fixture-account' };
      return true;
    });
    await f.exchange.cancelSymbolOrders(instruments[0].toUpperCase());
    expect(reauthenticate).toHaveBeenCalledOnce();
    expect(f.requests).toEqual([
      { path: '/lite/v1/open_orders', body: { sa: '777' } },
      { path: '/lite/v1/cancel_order', body: { sa: '777', oi: '0x1' } },
      { path: '/lite/v1/cancel_order', body: { sa: '777', oi: '0x1' } },
    ]);
    expect(f.current.size).toBe(0);
  });

  it.each(['disposal', 'main-account', 'subaccount'] as const)(
    'does not resend native cancellation if %s changes inside automatic reauthentication',
    async (change) => {
      const f = fixture([order(instruments[0], '0x1')]);
      f.rejectAuthenticationOnce();
      vi.spyOn(f.exchange, 'authenticate').mockImplementation(async () => {
        f.exchange.apiExtras = {
          sessionCookie: 'fresh',
          accountId: change === 'main-account' ? 'different' : 'fixture-account',
        };
        if (change === 'subaccount') f.exchange.grvtTradingAccountId = '888';
        if (change === 'disposal') f.exchange.dispose();
        return true;
      });
      await expect(f.exchange.cancelSymbolOrders(instruments[0].toUpperCase())).rejects.toThrow();
      expect(f.requests.map((r) => r.path)).toEqual(['/lite/v1/open_orders', '/lite/v1/cancel_order']);
      expect(f.current.has('0x1')).toBe(true);
    },
  );

  it('preserves the intentionally account-wide global cancellation contract', async () => {
    const f = fixture();
    await f.exchange.cancelAllOrders();
    expect(f.requests).toEqual([{ path: '/lite/v1/cancel_all_orders', body: { sa: '777' } }]);
    expect(f.current.size).toBe(0);
  });
});
