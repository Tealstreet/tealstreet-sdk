import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from './reference/lighter/lighter.exchange';
import { DefaultStore } from '../store/store.base';
import type { Candle, ExchangeOptions } from '../types';

vi.mock('@tealstreet/poseidon-crypto', () => ({
  initWasm: vi.fn(),
  isWasmReady: vi.fn(() => true),
  getCryptoApi: vi.fn(() => ({})),
}));

vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {},
  OrderApi: class {},
  CandlestickApi: class {},
}));

vi.mock('./reference/lighter/lighter.api', () => ({
  createLighterSDKClient: vi.fn(() => ({
    get: vi.fn(),
    sendRawTx: vi.fn(),
  })),
}));

vi.mock('./reference/lighter/lighter.ws-public', () => ({
  LighterPublicWebsocket: class {
    connectAndSubscribe = vi.fn();
    maybeRecoverOhlcvStream = vi.fn();
    listenOHLCV = vi.fn(() => vi.fn());
    listenOrderBook = vi.fn();
    dispose = vi.fn();
  },
}));

vi.mock('./reference/lighter/lighter.ws-private', () => ({
  LighterPrivateWebsocket: class {
    connectAndSubscribe = vi.fn();
    dispose = vi.fn();
  },
}));

describe('LighterExchange OHLCV REST fallback dedupe', () => {
  const createExchange = (): LighterExchange => {
    const options = {
      key: 'lighter-key',
      secret: 'lighter-secret',
      apiKeyIndex: '1',
      applicationId: '123',
      testnet: false,
    } as ExchangeOptions;

    return new LighterExchange('lighter', options, new DefaultStore());
  };

  const candle: Candle = {
    timestamp: 1710700000,
    open: 100,
    high: 110,
    low: 90,
    close: 105,
    volume: 12,
  };

  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it('shares one REST fallback request across listeners of same symbol+interval', async () => {
    const exchange = createExchange();
    const fetchOHLCVMock = vi.spyOn(exchange, 'fetchOHLCV').mockResolvedValue([candle]);
    const ohlcvOptions = { symbol: 'BTCUSDT', interval: '1m' } as any;

    const firstListener = vi.fn();
    const secondListener = vi.fn();

    const unsubscribeFirst = exchange.listenOHLCV(ohlcvOptions, firstListener);
    const unsubscribeSecond = exchange.listenOHLCV(ohlcvOptions, secondListener);

    await Promise.resolve();
    await Promise.resolve();

    expect(fetchOHLCVMock).toHaveBeenCalledTimes(1);

    unsubscribeFirst();
    unsubscribeSecond();
  });

  it('cools down OHLCV REST fallback after a 403', async () => {
    const exchange = createExchange();
    const fetchOHLCVMock = vi
      .spyOn(exchange, 'fetchOHLCV')
      .mockRejectedValueOnce({ response: { status: 403 } })
      .mockResolvedValue([candle]);
    const ohlcvOptions = { symbol: 'BTCUSDT', interval: '1m' } as any;

    await (exchange as any).fetchLatestOhlcvWithSharedFallback(ohlcvOptions, Date.now());
    await (exchange as any).fetchLatestOhlcvWithSharedFallback(ohlcvOptions, Date.now());

    expect(fetchOHLCVMock).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(60_000);
    const latestCandle = await (exchange as any).fetchLatestOhlcvWithSharedFallback(ohlcvOptions, Date.now());

    expect(fetchOHLCVMock).toHaveBeenCalledTimes(2);
    expect(latestCandle).toEqual(candle);
  });
});
