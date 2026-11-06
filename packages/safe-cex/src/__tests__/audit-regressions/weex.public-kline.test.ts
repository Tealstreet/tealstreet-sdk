import { describe, expect, it, vi } from 'vitest';

import { WeexExchange } from '../reference/weex/weex.exchange';
import { DefaultStore } from '../../store/store.base';
import { LogSeverity } from '../../types';

const createExchange = () =>
  new WeexExchange(
    'weex' as any,
    { key: 'key', secret: 'secret', passphrase: 'passphrase' } as any,
    new DefaultStore(),
  );

describe('audit regression: WEEX public kline subscriptions', () => {
  it('rejects unsupported websocket intervals before sending a subscription', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    expect(() => websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '3m' }, vi.fn())).toThrow(
      'WEEX public WebSocket does not support 3m candles',
    );
    expect(log).toHaveBeenCalledWith('WEEX public WebSocket does not support 3m candles', LogSeverity.Warning);
    expect(send).not.toHaveBeenCalled();
  });

  it('keeps current V3 websocket-only 2h candles available', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);

    const cleanup = websocket.listenOHLCV({ symbol: 'BTCUSDT', interval: '2h' }, vi.fn());

    expect(JSON.parse(send.mock.calls[0][0])).toEqual(
      expect.objectContaining({ method: 'SUBSCRIBE', params: ['BTCUSDT@kline_2h_LAST_PRICE'] }),
    );
    cleanup();
    websocket.dispose();
  });

  it('logs negative subscription acknowledgements with the exchange message', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    websocket.onMessage({ data: JSON.stringify({ result: false, id: 7, msg: 'invalid interval' }) } as MessageEvent);

    expect(log).toHaveBeenCalledWith('WEEX public WS subscription failed: invalid interval', LogSeverity.Warning);
  });
});
