import { describe, expect, it, vi } from 'vitest';

import { decodeHyperliquidFastAssetCtxs } from '../reference/hyperliquid/hyperliquid.fast-asset-ctxs';
import { HyperliquidPublicWebSocket } from '../reference/hyperliquid/hyperliquid.ws-public';

// Raw-DEFLATE + base64 fixture matching the live fastAssetCtxs wire format.
const FAST_ASSET_CTXS_FIXTURE =
  'q1ZyCnFWsqpWyk0syg6oULJSMjQw0DMyVdJRys1MgQuYKtXqKFVUVlmFBPs4oig3MjXQMwTJOjm6oEj4JfohGeKZl5aZl1lSqVRbCwA=';

const createWebsocket = (positions: unknown[] = []) => {
  const btcTicker = { id: 'BTC', symbol: 'BTC-PERP', mark: 90, last: 91, midPrice: 91 };
  const tslaTicker = { id: 'xyz:TSLA', symbol: 'XYZ|TSLA-PERP', mark: 240, last: 241, midPrice: 241 };
  const parent = {
    isDisposed: false,
    name: 'hyperliquid',
    options: { testnet: false },
    store: {
      markets: [],
      positions,
      tickersById: { BTC: btcTicker, 'xyz:TSLA': tslaTicker },
      update: vi.fn(),
      updateTicker: vi.fn((ticker, update) => Object.assign(ticker, update)),
    },
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    syncBalanceFromLivePositions: vi.fn(),
  };
  const websocket = new HyperliquidPublicWebSocket(parent as any);
  const send = vi.fn();
  (websocket as any).send = send;
  (websocket as any).ws = { readyState: WebSocket.OPEN, close: vi.fn() };
  return { websocket, parent, send, btcTicker, tslaTicker };
};

describe('Hyperliquid fastAssetCtxs', () => {
  it('decodes the raw-DEFLATE base64 snapshot-and-diff contract', () => {
    expect(decodeHyperliquidFastAssetCtxs(FAST_ASSET_CTXS_FIXTURE)).toEqual({
      BTC: { markPx: '100.25', midPx: '100.5' },
      'xyz:TSLA': { markPx: '250.1' },
      BAD: { markPx: 'NaN', midPx: 'Infinity' },
    });
    expect(() => decodeHyperliquidFastAssetCtxs('not-base64')).toThrow();
  });

  it('subscribes fastAssetCtxs alongside the allMids fallback', () => {
    const { websocket, send } = createWebsocket();

    websocket.subscribePublicTopics();

    const payloads = send.mock.calls.map(([payload]) => JSON.parse(payload));
    expect(payloads).toContainEqual({ method: 'subscribe', subscription: { type: 'allMids' } });
    expect(payloads).toContainEqual({ method: 'subscribe', subscription: { type: 'fastAssetCtxs' } });
  });

  it('applies finite mark/mid diffs by coin and leaves absent fields unchanged', () => {
    const { websocket, parent, btcTicker, tslaTicker } = createWebsocket([{}]);

    websocket.handleFastAssetCtxs(FAST_ASSET_CTXS_FIXTURE);

    expect(btcTicker).toMatchObject({ mark: 100.25, last: 100.5, midPrice: 100.5 });
    expect(tslaTicker).toMatchObject({ mark: 250.1, last: 241, midPrice: 241 });
    expect(parent.store.updateTicker).toHaveBeenCalledTimes(2);
    expect(parent.syncBalanceFromLivePositions).toHaveBeenCalledWith(new Set(['BTC-PERP']));
  });

  it('keeps prior ticker state on malformed compressed data and still accepts allMids fallback', () => {
    const { websocket, parent, btcTicker } = createWebsocket();

    websocket.onMessage({
      data: JSON.stringify({ channel: 'fastAssetCtxs', data: 'not-base64' }),
    } as MessageEvent);

    expect(btcTicker).toMatchObject({ mark: 90, last: 91, midPrice: 91 });
    expect(parent.store.updateTicker).not.toHaveBeenCalled();

    websocket.onMessage({
      data: JSON.stringify({ channel: 'allMids', data: { mids: { BTC: '92.5' } } }),
    } as MessageEvent);

    expect(btcTicker).toMatchObject({ mark: 90, last: 92.5, midPrice: 92.5 });
  });
});
