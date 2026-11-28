// READ ONLY; do not merge. Node 22+, SDK dependencies installed with corepack yarn.
// From this branch, no Rust build is needed:
// mkdir -p /tmp/coinbasespot-readiness-wasm
// gh release download rust-beta-wasm-9e59adf002f -R Tealstreet/tealstreet-sdk \
//   --pattern beta-wasm-coinbase-safe_cex_wasm.js \
//   --pattern beta-wasm-coinbase-safe_cex_wasm_bg.wasm --dir /tmp/coinbasespot-readiness-wasm
// COINBASE_WASM_DIR=/tmp/coinbasespot-readiness-wasm node sdk-rs/harness/coinbasespot-readiness-probe.mjs
// Optional HTTP routing (WebSockets remain direct):
// COINBASE_PROXY_PREFIX=https://proxies.tyo.tealstreet.io/api/exchange_proxy/ \
//   COINBASE_WASM_DIR=/tmp/coinbasespot-readiness-wasm node sdk-rs/harness/coinbasespot-readiness-probe.mjs
// The runner injects COINBASE_KEY, COINBASE_SECRET and optional COINBASE_PASSPHRASE
// into the environment; never pass them as arguments or print them. A downloaded
// Coinbase key JSON may include its private key, with COINBASE_SECRET empty.
// Output: flags/root/currencies/counts and frame/status counts every 5s for 60s,
// plus closed engine parse/validation kind/path/field summaries and request method/path
// refusals or transport failures and completed HTTP status/path. No queries,
// headers, response bodies, signing material or raw errors are printed.
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
    frameCount: 0, statusFrameCount: 0,
  };
  const started = Date.now();
  const print = () => process.stdout.write(`${JSON.stringify({ elapsedSeconds: Math.floor((Date.now() - started) / 1000), ...state })}\n`);
  let host;
  let interval;
  let flush;
  try {
    const key = process.env.COINBASE_KEY;
    const secret = process.env.COINBASE_SECRET ?? '';
    if (!key) throw new Error();
    const { WasmEngine } = await loadProbeEngine(process.env.COINBASE_WASM_DIR);
    const native = gateTransport();
    const transport = readOnlyTransport(native, { proxyPrefix: process.env.COINBASE_PROXY_PREFIX });
    const engine = new WasmEngine('coinbasespot', JSON.stringify({
      key, secret, passphrase: process.env.COINBASE_PASSPHRASE ?? '', spotOnly: true, publicDataOnly: false,
    }));
    host = createProbeHost(engine, transport, {
      onFrame(buffer) {
        state.frameCount++;
        try {
          const frame = new Frame(buffer);
          const status = frame.status();
          if (status) {
            state.statusFrameCount++;
            state.loaded = Object.fromEntries(Object.entries(LOADED_FLAG).map(([name, flag]) => [name.toLowerCase(), Boolean(status.loaded & flag)]));
          }
          const balance = frame.balance();
          if (balance) {
            state.balance = Object.fromEntries(['total', 'free', 'used', 'upnl'].map((name) => [name, balance[name]]));
            state.currencies = balance.currencies ?? {};
          }
          const orders = frame.orders();
          if (orders) state.orderCount = orders.count;
          const positions = frame.positions();
          if (positions) state.positionCount = positions.count;
        } catch {
          console.log(JSON.stringify({ kind: 'frame_decode', path: 'unknown', field: 'unknown' }));
        }
      },
      // HTTP engine diagnostics are observed at their actual request ID below.
      onLog() {}, onEvent() {},
      onStartupFailed() { console.log(JSON.stringify({ kind: 'startup_failed', path: 'unknown', field: 'unknown' })); },
    });
    host.start();
    flush = startProbeFlushing(host);
    print();
    interval = setInterval(print, 5000);
    await new Promise((resolve) => setTimeout(resolve, 60000));
    print();
  } catch {
    process.stderr.write('Coinbase read-only probe could not start or complete; no credential or error detail is printed.\n');
    process.exitCode = 1;
  } finally {
    clearInterval(interval);
    clearInterval(flush);
    host?.dispose();
  }
}

export function startProbeFlushing(host) {
  return setInterval(() => host.flush(), 250);
}

const fields = new Set(['products', 'product_type', 'product_id', 'status', 'base_currency_id',
  'quote_currency_id', 'base_increment', 'price_increment', 'base_min_size', 'base_max_size',
  'future_product_details', 'portfolios', 'uuid', 'type', 'breakdown', 'spot_positions', 'asset',
  'total_balance_crypto', 'available_to_trade_crypto', 'orders', 'order_id', 'order_configuration',
  'has_next', 'cursor', 'pagination', 'side', 'order_type', 'filled_size',
  'total_balance_fiat', 'available_to_trade_fiat', 'account_type']);

export function parseFailureSummary(message, path) {
  const named = /Coinbase invalid (?:spot cash balance: )?field `([^`]+)`/.exec(message);
  if (named && fields.has(named[1].replace(/^spot_positions\./, '')))
    return { kind: 'validation', path, field: named[1] };
  const validation = [
    ['Coinbase duplicate spot holding', 'asset', 'duplicate_asset'],
    ['Coinbase spot balance limit', 'spot_positions', 'row_limit'],
    ['null number', 'unknown', 'null_number'],
    ['blank number', 'unknown', 'blank_number'],
    ['invalid number', 'unknown', 'invalid_number'],
  ].find(([text]) => message.includes(text));
  if (validation) return { kind: 'validation', path, field: validation[1], reason: validation[2] };
  const missing = /missing field `([^`]+)`/.exec(message);
  const kind = missing ? 'missing_field' : message.includes('invalid type') ? 'invalid_type'
    : message.includes('invalid value') ? 'invalid_value'
      : /EOF while parsing|expected (?:value|ident|colon|comma)|trailing characters/.test(message) ? 'json_syntax' : 'validation';
  const field = missing && fields.has(missing[1]) ? missing[1]
    : /products pagination/.test(message) ? 'pagination' : /products cursor/.test(message) ? 'cursor'
      : /products|catalogue/.test(message) ? 'products' : 'unknown';
  return { kind, path, field };
}

export function diagnosticEvent(action, path) {
  if (action.op !== 'event') return;
  if (action.code === 'request.failed') return parseFailureSummary(action.message, path);
  const excluded = action.code === 'row.skipped'
    && /^Coinbase excluded spot account type (ACCOUNT_TYPE_[A-Z_]{1,67}|MISSING|UNKNOWN)$/.exec(action.message);
  if (excluded) return { kind: 'excluded_account_type', path, account_type: excluded[1] };
}

export function createProbeHost(engine, transport, handlers, log = (row) => console.log(JSON.stringify(row))) {
  const paths = new Map();
  // Inspect only actions returned while this exact HTTP response is reduced.
  for (const method of ['httpStart', 'httpChunk', 'httpEnd']) {
    const original = engine[method].bind(engine);
    engine[method] = (id, ...args) => {
      const actions = original(id, ...args);
      for (const action of JSON.parse(actions || '[]')) {
        const summary = diagnosticEvent(action, paths.get(id) ?? 'unknown');
        if (summary) log(summary);
      }
      return actions;
    };
  }
  const host = new EngineHost(engine, transport, handlers);
  const request = host.request.bind(host);
  host.request = async (action) => {
    paths.set(action.request, new URL(action.url).pathname);
    try { return await request(action); }
    finally { paths.delete(action.request); }
  };
  return host;
}

export function readOnlyTransport(native, { proxyPrefix, log = (line) => process.stdout.write(`${line}\n`) } = {}) {
  if (proxyPrefix) {
    const proxy = new URL(proxyPrefix);
    if (proxy.protocol !== 'https:' || proxy.username || proxy.password || proxy.search || proxy.hash)
      throw new Error('Read-only probe requires an HTTPS proxy prefix without credentials or query');
    proxyPrefix = `${proxy.href.replace(/\/+$/, '')}/`;
  }
  return {
    ...native,
    async request(request) {
      let url;
      try { url = new URL(request.url); } catch { /* Never log an unparseable URL. */ }
      const method = /^[A-Z]{1,12}$/.test(request.method) ? request.method : 'UNKNOWN';
      const path = url?.pathname ?? '[invalid-path]';
      const read = url?.pathname.startsWith('/api/v3/brokerage/');
      if (request.method !== 'GET' || request.body != null || url?.origin !== 'https://api.coinbase.com'
          || url.username || url.password || url.hash || !read) {
        log(`REFUSED ${method} ${path}`);
        throw new Error('Read-only probe refused an outbound request');
      }
      try {
        const response = await native.request(proxyPrefix ? { ...request, url: `${proxyPrefix}${url.href}` } : request);
        log(`COMPLETED ${response.status} ${path}`);
        if (response.status === 200 && /^\/api\/v3\/brokerage\/portfolios\/[^/]+$/.test(path)) {
          const report = (bytes) => {
            for (const shape of duplicatePortfolioShapes(bytes))
              log(JSON.stringify({ kind: 'duplicate_asset_shape', path, ...shape }));
          };
          if (response.body instanceof Uint8Array) report(response.body);
          else if (response.body?.[Symbol.asyncIterator]) {
            const original = response.body;
            return { ...response, body: (async function* () {
              const chunks = [];
              let size = 0;
              for await (const chunk of original) {
                size += chunk.byteLength;
                if (size <= 2_097_152) chunks.push(chunk);
                else chunks.length = 0;
                yield chunk;
              }
              if (size <= 2_097_152) {
                const bytes = new Uint8Array(size);
                let offset = 0;
                for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
                report(bytes);
              }
            })() };
          }
        }
        return response;
      } catch {
        log(`FAILED ${method} ${path}`);
        throw new Error('Read-only probe transport failed');
      }
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

export function duplicatePortfolioShapes(bytes) {
  try {
    const rows = JSON.parse(new TextDecoder().decode(bytes))?.breakdown?.spot_positions;
    if (!Array.isArray(rows) || rows.length > 1000) return [];
    const groups = new Map();
    for (const row of rows) {
      if (!row || typeof row !== 'object' || !/^[A-Z][A-Z0-9]{0,23}$/.test(row.asset)) continue;
      const group = groups.get(row.asset) ?? [];
      group.push(row);
      groups.set(row.asset, group);
    }
    return [...groups].filter(([, group]) => group.length > 1).map(([asset, group]) => ({
      asset, row_count: group.length,
      rows: group.map((row) => ({
        account_type: /^ACCOUNT_TYPE_[A-Z_]{1,48}$/.test(row.account_type) ? row.account_type : 'other_or_absent',
        is_cash: typeof row.is_cash === 'boolean' ? row.is_cash : null,
        has_account_uuid: typeof row.account_uuid === 'string' && row.account_uuid.length > 0,
        account_uuid_differs_from_first: row.account_uuid !== group[0].account_uuid,
        asset_img_present: Object.hasOwn(row, 'asset_img_url') || Object.hasOwn(row, 'asset_img'),
        allocation_present: Object.hasOwn(row, 'allocation'),
        cost_basis_present: Object.hasOwn(row, 'cost_basis'),
        differing_fields: ['account_type', 'is_cash', 'asset_img_url'].filter((key) => row[key] !== group[0][key]),
        keys: Object.keys(row).filter((key) => /^[a-z][a-z0-9_]{0,63}$/.test(key)).sort(),
      })),
    }));
  } catch { return []; }
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
