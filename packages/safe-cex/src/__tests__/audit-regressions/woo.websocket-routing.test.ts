import { afterEach, describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../reference/woo/woo.exchange';
import { DefaultStore } from '../../store/store.base';

const makeExchange = () =>
  new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      applicationId: 'application-id',
      tuning: { consoleLogging: false },
    } as any,
    new DefaultStore(),
  );

let exchange: WOOXExchange | undefined;

afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  vi.restoreAllMocks();
});

describe('WOO legacy public websocket exact routing', () => {
  it('routes an exact market topic to every listener with the unchanged frame', () => {
    exchange = makeExchange();
    const websocket = exchange.publicWebsocket;
    const first = vi.fn();
    const second = vi.fn();
    const topic = 'PERP_BTC_USDT@trades';
    websocket.messageHandlers = { [topic]: [first, second] };

    const frame = {
      topic,
      ts: 1_700_000_000_000,
      data: [{ s: 'PERP_BTC_USDT', p: 100, a: 0.1 }],
    };
    websocket.onMessage({ data: JSON.stringify(frame) } as MessageEvent);

    expect(first).toHaveBeenCalledOnce();
    expect(second).toHaveBeenCalledOnce();
    expect(first.mock.calls[0]?.[0]).toEqual(frame);
    expect(second.mock.calls[0]?.[0]).toBe(first.mock.calls[0]?.[0]);
    expect(websocket.messageHandlersStats[topic]).toMatchObject({ lastCallbacksCount: 2 });
  });

  it('does not route from nested payload fields or topic prefixes', () => {
    exchange = makeExchange();
    const websocket = exchange.publicWebsocket;
    const ticker = vi.fn();
    const trades = vi.fn();
    websocket.messageHandlers = {
      tickers: [ticker],
      'PERP_BTC_USDT@trades': [trades],
    };

    websocket.onMessage({
      data: JSON.stringify({
        topic: 'unknown',
        data: [{ topic: 'tickers', event: 'PERP_BTC_USDT@trades' }],
      }),
    } as MessageEvent);
    websocket.onMessage({
      data: JSON.stringify({ topic: 'PERP_BTC_USDT@trades-extra', data: [] }),
    } as MessageEvent);

    expect(ticker).not.toHaveBeenCalled();
    expect(trades).not.toHaveBeenCalled();
    expect(websocket.messageHandlersStats).toEqual({});
  });

  it('routes ping and pong as exact control events with event precedence', () => {
    exchange = makeExchange();
    const websocket = exchange.publicWebsocket;
    const ping = vi.fn();
    const pong = vi.fn();
    const ticker = vi.fn();
    websocket.messageHandlers = { ping: [ping], pong: [pong], tickers: [ticker] };

    websocket.onMessage({
      data: JSON.stringify({ event: 'ping', topic: 'tickers', ts: 1 }),
    } as MessageEvent);
    websocket.onMessage({ data: JSON.stringify({ event: 'pong', ts: 2 }) } as MessageEvent);

    expect(ping).toHaveBeenCalledOnce();
    expect(pong).toHaveBeenCalledOnce();
    expect(ticker).not.toHaveBeenCalled();
    expect(Object.keys(websocket.messageHandlersStats).sort()).toEqual(['ping', 'pong']);
  });

  it('ignores subscription acknowledgements even when they echo a topic', () => {
    exchange = makeExchange();
    const websocket = exchange.publicWebsocket;
    const ticker = vi.fn();
    websocket.messageHandlers = { tickers: [ticker] };

    websocket.onMessage({
      data: JSON.stringify({ event: 'subscribe', topic: 'tickers', success: true, data: 'tickers' }),
    } as MessageEvent);

    expect(ticker).not.toHaveBeenCalled();
    expect(websocket.messageHandlersStats).toEqual({});
  });
});
