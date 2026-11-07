import { prepareRecordedHttp } from '@tealstreet/safe-cex/src/__tests__/support/managedHttp';
import { createExchange } from '@tealstreet/safe-cex/src/index';
import { RustEngineExchange, setRustEngineTransport } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { virtualClock } from '@tealstreet/safe-cex/src/utils/virtualClock';

import { registerBundledRustEngine } from '../src/rust-engine';

registerBundledRustEngine();
const requests: string[] = [];
const errors: unknown[] = [];
const replies: Record<string, unknown> = {
  '/api/v3/exchangeInfo': {
    symbols: [
      {
        symbol: 'BTCUSDT',
        status: 'TRADING',
        baseAsset: 'BTC',
        quoteAsset: 'USDT',
        baseAssetPrecision: 8,
        quoteAssetPrecision: 8,
        filters: [
          { filterType: 'PRICE_FILTER', tickSize: '0.01' },
          { filterType: 'LOT_SIZE', stepSize: '0.001', minQty: '0.001', maxQty: '100' },
        ],
      },
    ],
  },
  '/api/v3/ticker/24hr': [
    {
      symbol: 'BTCUSDT',
      lastPrice: '110',
      bidPrice: '109',
      askPrice: '111',
      volume: '20',
      quoteVolume: '2200',
      priceChangePercent: '2',
    },
  ],
};
// The executable probe owns one documented public product input. Unexpected
// requests fail rather than silently escaping onto the network.
setRustEngineTransport(() => ({
  prepareHttp: prepareRecordedHttp,
  now: () => Date.now(),
  connect(_url, handlers) {
    queueMicrotask(handlers.onOpen);
    return { send() {}, close() {} };
  },
  async request({ method, url }) {
    requests.push(`${method} ${url}`);
    const parsed = new URL(url);
    if (method !== 'GET' || parsed.origin !== 'https://api.binance.com' || !Object.hasOwn(replies, parsed.pathname))
      throw new Error(`Untaped ${method} ${url}`);
    return {
      status: 200,
      headers: '',
      body: new TextEncoder().encode(JSON.stringify(replies[parsed.pathname])),
    };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
}));
// Public clock sync is a separate host service; this offline probe must not
// contact its real Bybit time endpoint.
virtualClock.start = async () => {};
const exchange = await createExchange('binancespot', {
  key: '',
  secret: '',
  engine: 'rust',
  tuning: { consoleLogging: false },
});
exchange.emitter.on('error', (error: unknown) => errors.push(error));
let timeout: ReturnType<typeof setTimeout> | undefined;
try {
  if (!(exchange instanceof RustEngineExchange)) throw new Error('Factory did not select Rust');
  const startup = exchange.start();
  await Promise.race([
    startup,
    new Promise<never>((_resolve, reject) => {
      timeout = setTimeout(
        () =>
          reject(
            new Error(`Rust startup timeout ${JSON.stringify({ requests, errors, loaded: exchange.store.loaded })}`),
          ),
        5000,
      );
    }),
  ]);
  if (exchange.store.markets[0]?.symbol !== 'BTCUSDT') throw new Error('Native catalog did not load');
  console.log('rust-smoke: native Binance Spot market loaded');
} finally {
  clearTimeout(timeout);
  exchange.dispose();
  virtualClock.stop();
}
