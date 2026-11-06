import { describe, expect, it, vi } from 'vitest';

import { WeexExchange } from '../reference/weex/weex.exchange';
import { DefaultStore } from '../../store/store.base';

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: Array.from({ length: 105 }, (_, index) => ({
      id: `SYMBOL${index}`,
      symbol: `SYMBOL${index}`,
      base: `BASE${index}`,
      quote: 'USDT',
      active: true,
      precision: { amount: 0.001, price: 0.1 },
      limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
    })) as any,
  });
  return new WeexExchange('weex' as any, { key: 'key', secret: 'secret', passphrase: 'passphrase' } as any, store);
};

describe('audit regression: WEEX dynamic ticker subscriptions', () => {
  it('diffs active ticker topics and stays below the reserved 90-channel ticker cap', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);

    websocket.onOpen();
    const initialMessages = send.mock.calls.map(([payload]) => JSON.parse(payload));
    const initialTickerTopics = initialMessages
      .filter((message) => message.method === 'SUBSCRIBE')
      .flatMap((message) => message.params)
      .filter((topic) => topic.endsWith('@ticker'));
    expect(initialTickerTopics).toHaveLength(90);

    send.mockClear();
    exchange.setActiveTickerSymbols(['SYMBOL1', 'SYMBOL99']);

    expect(send.mock.calls.map(([payload]) => JSON.parse(payload))).toEqual([
      expect.objectContaining({ method: 'UNSUBSCRIBE' }),
      expect.objectContaining({ method: 'SUBSCRIBE', params: ['SYMBOL99@ticker'] }),
    ]);
    expect(JSON.parse(send.mock.calls[0][0]).params).not.toContain('SYMBOL1@ticker');

    websocket.dispose();
  });

  it('removes its active-symbol listener on dispose', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);

    websocket.dispose();
    exchange.setActiveTickerSymbols(['SYMBOL1']);

    expect(send).not.toHaveBeenCalled();
  });

  it('does not claim a ticker diff was sent after the rolling operation budget is exhausted', () => {
    const exchange = createExchange();
    const websocket = exchange.publicWebsocket;
    websocket.ws = { readyState: WebSocket.OPEN } as WebSocket;
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);
    const log = vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    websocket.onOpen();
    const before = new Set((websocket as any).tickerTopics);
    (websocket as any).subscriptionOperationTimestamps = Array(240).fill(Date.now());
    send.mockClear();

    exchange.setActiveTickerSymbols(['SYMBOL99']);

    expect(send).not.toHaveBeenCalled();
    expect(new Set((websocket as any).tickerTopics)).toEqual(before);
    expect(log).toHaveBeenCalledWith(
      'WEEX public WS subscription operation limit reached; deferring changes until another symbol update or reconnect',
      expect.anything(),
    );
    websocket.dispose();
  });
});
