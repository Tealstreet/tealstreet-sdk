import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinancePublicWebsocket } from '../reference/binance/binance.ws-public';
import { BinancePMPublicWebsocket } from '../reference/binance/binancepm.ws-public';
import { BinancePublicWebsocket as BinanceSpotPublicWebsocket } from '../reference/binance/binancespot.ws-public';

const subjects = [
  ['futures', BinancePublicWebsocket],
  ['portfolio margin', BinancePMPublicWebsocket],
  ['spot', BinanceSpotPublicWebsocket],
] as const;

describe('Binance depth subscription refcounts', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it.each(subjects)('%s unsubscribe-before-connect leaves no phantom reconnect topic', (_name, Websocket) => {
    vi.useFakeTimers();
    const websocket = new Websocket({ isDisposed: false } as any);

    const unsubscribe = websocket.listenOrderBook('BTCUSDT', vi.fn());
    unsubscribe();

    expect(websocket.topics).not.toHaveProperty('btcusdt@depth');
    expect(websocket.topics['btcusdt@depth']).not.toBeNaN();
    vi.runOnlyPendingTimers();
    expect(websocket.topics).toEqual({});
  });

  it.each(subjects)('%s reconnect replay ignores non-positive legacy topic entries', (_name, Websocket) => {
    const websocket = new Websocket({ isDisposed: false } as any);
    websocket.topics = { 'btcusdt@depth': 0, 'ethusdt@depth': Number.NaN };
    const send = vi.spyOn(websocket, 'send').mockImplementation(() => undefined);

    websocket.subscribe();

    expect(send).not.toHaveBeenCalled();
  });
});
