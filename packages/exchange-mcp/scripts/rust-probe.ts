import { createExchange } from '@tealstreet/safe-cex/src/index';
import { RustEngineExchange, setRustEngineTransport } from '@tealstreet/safe-cex/src/exchanges/rust/rust.exchange';
import { virtualClock } from '@tealstreet/safe-cex/src/utils/virtualClock';

import { registerBundledRustEngine } from '../src/rust-engine';

const bundled = registerBundledRustEngine();
if (bundled) {
  const coinbase = await bundled('coinbasespot');
  const binance = await bundled('binance');
  if (coinbase === binance || (await bundled('coinbase')) !== coinbase || (await bundled('binancepm')) !== binance)
    throw new Error('Packaged engine families are not isolated');
  let rejected = false;
  try {
    new binance.WasmEngine('coinbase', '{}');
  } catch {
    rejected = true;
  }
  if (!rejected) throw new Error('Packaged Binance artifact included another engine family');
  console.log('rust-smoke: isolated Binance and Coinbase families initialized');
}
const requests: string[] = [];
const errors: unknown[] = [];
// The executable probe owns one documented public product input. Unexpected
// requests fail rather than silently escaping onto the network.
setRustEngineTransport(() => ({
  now: () => Date.now(),
  connect(_url, handlers) {
    queueMicrotask(handlers.onOpen);
    return { send() {}, close() {} };
  },
  async request({ method, url }) {
    requests.push(`${method} ${url}`);
    if (method !== 'GET' || !url.startsWith('https://api.coinbase.com/api/v3/brokerage/market/products'))
      throw new Error(`Untaped ${method} ${url}`);
    return {
      status: 200,
      headers: '',
      body: new TextEncoder().encode(
        JSON.stringify({
          products: [
            {
              product_id: 'BTC-USD',
              product_type: 'SPOT',
              base_currency_id: 'BTC',
              quote_currency_id: 'USD',
              status: 'TRADING',
              base_increment: '0.001',
              price_increment: '0.01',
              base_min_size: '0.001',
              base_max_size: '100',
              price: '110',
              volume_24h: '20',
              price_percentage_change_24h: '2',
            },
          ],
          num_products: 1,
          pagination: { has_next: false },
        }),
      ),
    };
  },
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
}));
// Public clock sync is a separate host service; this offline probe must not
// contact its real Bybit time endpoint.
virtualClock.start = async () => {};
const exchange = await createExchange('coinbasespot', {
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
  if (exchange.store.markets[0]?.symbol !== 'BTC-USD') throw new Error('Native catalog did not load');
  console.log('rust-smoke: native Coinbase market loaded');
} finally {
  clearTimeout(timeout);
  exchange.dispose();
  virtualClock.stop();
}
