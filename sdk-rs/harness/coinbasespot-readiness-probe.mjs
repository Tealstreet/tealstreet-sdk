// READ ONLY; do not merge. Node 22+, SDK dependencies installed with corepack yarn.
// From this branch, no Rust build is needed:
// mkdir -p /tmp/coinbasespot-readiness-wasm
// gh release download rust-beta-wasm-9e59adf002f -R Tealstreet/tealstreet-sdk \
//   --pattern beta-wasm-coinbase-safe_cex_wasm.js \
//   --pattern beta-wasm-coinbase-safe_cex_wasm_bg.wasm --dir /tmp/coinbasespot-readiness-wasm
// COINBASE_WASM_DIR=/tmp/coinbasespot-readiness-wasm node sdk-rs/harness/coinbasespot-readiness-probe.mjs
// The runner injects COINBASE_KEY, COINBASE_SECRET and optional COINBASE_PASSPHRASE
// into the environment; never pass them as arguments or print them. A downloaded
// Coinbase key JSON may include its private key, with COINBASE_SECRET empty.
// Output is ONLY flags, root/currency balances and counts, every 5s for 60s.
// This release matches the live lanes before the cash-root fix (#2970).
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { Frame, LOADED_FLAG } from '../hosts/wasm/js/wire.mjs';
import { gateTransport } from './gate-transport.mjs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function runProbe() {
  const state = {
    loaded: Object.fromEntries(Object.keys(LOADED_FLAG).map((name) => [name.toLowerCase(), false])),
    balance: { total: 0, free: 0, used: 0, upnl: 0 },
    currencies: {}, orderCount: 0, positionCount: 0,
  };
  const started = Date.now();
  const print = () => process.stdout.write(`${JSON.stringify({ elapsedSeconds: Math.floor((Date.now() - started) / 1000), ...state })}\n`);
  let host;
  let interval;
  try {
    const key = process.env.COINBASE_KEY;
    const secret = process.env.COINBASE_SECRET ?? '';
    if (!key) throw new Error();
    const { WasmEngine } = await loadProbeEngine(process.env.COINBASE_WASM_DIR);
    const native = gateTransport();
    const transport = readOnlyTransport(native);
    const engine = new WasmEngine('coinbasespot', JSON.stringify({
      key, secret, passphrase: process.env.COINBASE_PASSPHRASE ?? '', spotOnly: true, publicDataOnly: false,
    }));
    host = new EngineHost(engine, transport, {
      onFrame(buffer) {
        const frame = new Frame(buffer);
        const status = frame.status();
        if (status) state.loaded = Object.fromEntries(Object.entries(LOADED_FLAG).map(([name, flag]) => [name.toLowerCase(), Boolean(status.loaded & flag)]));
        const balance = frame.balance();
        if (balance) {
          state.balance = Object.fromEntries(['total', 'free', 'used', 'upnl'].map((name) => [name, balance[name]]));
          state.currencies = balance.currencies ?? {};
        }
        const orders = frame.orders();
        if (orders) state.orderCount = orders.count;
        const positions = frame.positions();
        if (positions) state.positionCount = positions.count;
      },
      // Never print engine notices, response bodies, socket frames, headers or errors.
      onLog() {}, onEvent() {},
    });
    host.start();
    print();
    interval = setInterval(print, 5000);
    await new Promise((resolve) => setTimeout(resolve, 60000));
    print();
  } catch {
    process.stderr.write('Coinbase read-only probe could not start or complete; no credential or error detail is printed.\n');
    process.exitCode = 1;
  } finally {
    clearInterval(interval);
    host?.dispose();
  }
}

export function readOnlyTransport(native) {
  return {
    ...native,
    request(request) {
      const url = new URL(request.url);
      const read = /^\/api\/v3\/brokerage\/(?:portfolios(?:\/[A-Za-z0-9_-]+)?|products|key_permissions|orders\/historical\/(?:batch|[A-Za-z0-9_-]+))$/.test(url.pathname);
      if (request.method !== 'GET' || request.body || url.origin !== 'https://api.coinbase.com' || !read)
        return Promise.reject(new Error('Read-only probe refused an outbound request'));
      return native.request(request);
    },
    connect(url, handlers) {
      if (!['wss://advanced-trade-ws.coinbase.com', 'wss://advanced-trade-ws-user.coinbase.com'].includes(url))
        throw new Error('Read-only probe refused an outbound socket');
      const connection = native.connect(url, handlers);
      return {
        close: () => connection.close(),
        send(text) {
          const message = JSON.parse(text);
          if (!['subscribe', 'unsubscribe'].includes(message.type) || !['heartbeats', 'user'].includes(message.channel))
            throw new Error('Read-only probe refused an outbound socket message');
          connection.send(text);
        },
      };
    },
  };
}

export async function loadProbeEngine(directory) {
  if (!directory) throw new Error('Set COINBASE_WASM_DIR to the downloaded release directory');
  const glue = await readFile(join(directory, 'beta-wasm-coinbase-safe_cex_wasm.js'), 'utf8');
  const bytes = await readFile(join(directory, 'beta-wasm-coinbase-safe_cex_wasm_bg.wasm'));
  const module = await import(`data:text/javascript;base64,${Buffer.from(glue).toString('base64')}`);
  await module.default({ module_or_path: bytes });
  return module;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await runProbe();
