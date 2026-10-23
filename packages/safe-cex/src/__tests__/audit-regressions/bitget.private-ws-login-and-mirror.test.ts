import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { DefaultStore } from '../../store/store.base';

import '../../transport/registerTealstreetTransport';

const createExchange = () => {
  const store = new DefaultStore();
  const exchange = new BitgetExchange(
    'bitget',
    { key: 'k', secret: 's', passphrase: 'p', tuning: { consoleLogging: false } },
    store,
  );
  exchange.xhr = { get: vi.fn(), post: vi.fn() } as any;
  exchange.unlimitedXHR = {
    post: vi.fn().mockResolvedValue({ data: { code: '00000', data: { orderId: 'tp-1' } } }),
  } as any;
  return exchange;
};

const frame = (payload: Record<string, unknown>) => ({ data: JSON.stringify(payload) }) as MessageEvent;

describe('Bitget private WS: main-thread mirror and rejected logins', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('a never-started instance (the web mirror) does not arm the post-write watchdog', async () => {
    const exchange = createExchange();
    const emit = vi.spyOn(exchange.emitter, 'emit');
    const reconnect = vi.spyOn((exchange as any).privateWebsocket, 'reconnect').mockImplementation(() => {});
    const fetchOrders = vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);

    await exchange.placeAlgoOrders([{ symbol: 'BTCUSDT', planType: 'profit_plan', triggerPrice: '51000' }]);
    await vi.advanceTimersByTimeAsync(9_000);

    expect(exchange.store.socketHealth?.privateDegradedReason).toBeUndefined();
    expect(emit).not.toHaveBeenCalledWith('warning', expect.anything(), expect.anything());
    expect(reconnect).not.toHaveBeenCalled();
    expect(fetchOrders).not.toHaveBeenCalled();
    exchange.dispose();
  });

  it('reports a rejected login once per streak and reconnects', () => {
    const exchange = createExchange();
    const ws = (exchange as any).privateWebsocket;
    const emit = vi.spyOn(exchange.emitter, 'emit');
    const reconnect = vi.spyOn(ws, 'reconnect').mockImplementation(() => {});
    vi.spyOn(ws, 'auth' as any).mockImplementation(async () => {});
    vi.spyOn(ws, 'ping').mockImplementation(() => {});

    ws.onOpen();
    ws.onMessage(frame({ event: 'error', code: 30015, msg: 'Invalid sign' }));
    ws.onOpen();
    ws.onMessage(frame({ event: 'error', code: 30015, msg: 'Invalid sign' }));

    const errors = emit.mock.calls.filter(([name]) => name === 'error');
    expect(errors).toHaveLength(1);
    expect(String(errors[0][1])).toContain('Bitget private WS login rejected: 30015 Invalid sign');
    expect(reconnect).toHaveBeenCalledTimes(2);
    exchange.dispose();
  });

  it('a refused subscribe after login is reported without reconnecting', () => {
    const exchange = createExchange();
    const ws = (exchange as any).privateWebsocket;
    const emit = vi.spyOn(exchange.emitter, 'emit');
    const reconnect = vi.spyOn(ws, 'reconnect').mockImplementation(() => {});
    vi.spyOn(ws, 'auth' as any).mockImplementation(async () => {});
    vi.spyOn(ws, 'ping').mockImplementation(() => {});
    vi.spyOn(ws, 'subscribe' as any).mockImplementation(() => {});

    ws.onOpen();
    ws.onMessage(frame({ event: 'login', code: 0 }));
    ws.onMessage(frame({ event: 'error', code: 30001, msg: 'channel does not exist' }));

    const errors = emit.mock.calls.filter(([name]) => name === 'error');
    expect(errors).toHaveLength(1);
    expect(String(errors[0][1])).toContain('Bitget private WS subscription rejected: 30001 channel does not exist');
    expect(reconnect).not.toHaveBeenCalled();
    exchange.dispose();
  });
});
