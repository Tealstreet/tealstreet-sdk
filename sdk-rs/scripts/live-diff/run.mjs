#!/usr/bin/env node
import { fork, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { differences, byteDifference, outcome } from './compare.mjs';
import { hash, makeCA, recordingProxy } from './recorder.mjs';
import { venueHosts as hosts } from './venues.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../..');
const args = process.argv.slice(2);
function option(name, fallback) {
  const index = args.indexOf(name);
  return index < 0 ? fallback : args[index + 1];
}
const exchange = option('--venue', 'binance');
if (!hosts[exchange]) throw new Error(`Unsupported venue ${exchange}; add its exact REST hosts before running`);
const rowIndex = option('--row');
const timeout = Number(option('--timeout-ms', '60000'));
if (!Number.isSafeInteger(timeout) || timeout < 1000 || timeout > 300000)
  throw new Error('--timeout-ms must be 1000..300000');
const trade = args.includes('--trade');
const capturePublic = args.includes('--capture-public');
if (capturePublic && rowIndex) throw new Error('--capture-public is restricted to keyless public mode');
if (trade && !rowIndex) throw new Error('--trade requires an explicit test-account --row from yarn keys');
if (rowIndex && !/^\d+$/.test(rowIndex)) throw new Error('--row must be a sheet row number');
// Read credentials only from the authorized CLI, in memory; no credential file.
let row;
if (rowIndex) {
  if (!existsSync(join(homedir(), '.tealstreet-audit/google.json'))) {
    throw new Error(
      'Google login needed: corepack yarn keys google-login in ~/cs/tealstreet-next; continuing public mode needs no row',
    );
  }
  const result = spawnSync(
    'corepack',
    [
      'yarn',
      '--silent',
      '--cwd',
      join(homedir(), 'cs/tealstreet-next'),
      'keys',
      'list',
      '--exchange',
      exchange,
      '--json',
      '--show-secrets',
    ],
    { encoding: 'utf8', timeout: 30000, maxBuffer: 4 * 1024 * 1024 },
  );
  if (result.status !== 0) throw new Error('yarn keys unavailable; complete Google sign-in, then retry');
  try {
    row = JSON.parse(result.stdout).find((value) => value.rowIndex === Number(rowIndex));
  } catch {
    throw new Error('yarn keys did not return JSON; credentials were not logged');
  }
  if (!row || !row.apiKey || !row.secret) throw new Error('Selected row has no complete credentials');
}
const report = {
  exchange,
  revision: spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).stdout.trim(),
  at: new Date().toISOString(),
  mode: row ? (trade ? 'test-account-trade' : 'test-account-read') : 'public',
  addon: resolve(option('--addon', join(root, 'sdk-rs/safe-cex-ffi/dist/node/safe_cex_ffi.node'))),
  rustRevision: option('--rust-revision'),
  checks: [],
  requests: [],
  limitations: [],
};
const directory = mkdtempSync(join(tmpdir(), 'safe-cex-live-diff-'));
const cache = new Map(),
  records = [],
  workers = [],
  proxies = [];
let failed = false;
function worker(implementation, proxy, cert) {
  const child = fork(join(here, 'worker.mjs'), [implementation], {
    cwd: root,
    execArgv: ['--import', 'tsx'],
    serialization: 'advanced',
    stdio: ['ignore', 'ignore', 'ignore', 'ipc'],
    env: {
      ...process.env,
      HTTPS_PROXY: proxy.url,
      HTTP_PROXY: proxy.url,
      ALL_PROXY: proxy.url,
      https_proxy: proxy.url,
      http_proxy: proxy.url,
      all_proxy: proxy.url,
      NO_PROXY: '127.0.0.1,localhost',
      no_proxy: '127.0.0.1,localhost',
      SSL_CERT_FILE: cert,
      NODE_EXTRA_CA_CERTS: cert,
      TSX_TSCONFIG_PATH: join(root, 'packages/safe-cex/tsconfig.json'),
      LIVE_DIFF_ADDON: report.addon,
    },
  });
  const pending = new Map();
  let sequence = 0;
  child.on('message', (reply) => {
    const waiter = pending.get(reply.requestId);
    if (waiter) {
      clearTimeout(waiter.timer);
      pending.delete(reply.requestId);
      waiter.resolve(reply);
    }
  });
  child.on('exit', () => {
    for (const waiter of pending.values()) {
      clearTimeout(waiter.timer);
      waiter.reject(new Error(`${implementation} worker exited`));
    }
    pending.clear();
  });
  const call = (method, methodArgs = [], extra = {}) =>
    new Promise((resolveCall, reject) => {
      const requestId = ++sequence;
      const timer = setTimeout(
        () => {
          pending.delete(requestId);
          reject(new Error(`${implementation} ${method} timed out after ${timeout}ms`));
        },
        method === 'close' ? 5000 : timeout,
      );
      pending.set(requestId, { resolve: resolveCall, reject, timer });
      child.send({ requestId, method, args: methodArgs, ...extra });
    });
  workers.push({ child, call });
  return call;
}
const secretValues = [row?.apiKey, row?.secret, row?.passphrase].filter(Boolean);
function redact(value) {
  const text = JSON.stringify(value, (_, item) => {
    if (typeof item === 'bigint') return `${item}n`;
    if (typeof item === 'string') {
      for (const secret of secretValues) item = item.split(secret).join('<credential>');
    }
    return item;
  });
  return JSON.parse(text);
}
try {
  const ca = makeCA(directory);
  for (const implementation of ['ts', 'rust'])
    proxies.push(
      await recordingProxy({
        directory,
        ca,
        cache,
        records,
        implementation,
        allowHost: (hostname) => hosts[exchange].includes(hostname),
        validateLive: Boolean(row),
        captureDirectory: capturePublic ? join(root, '.cache/live-diff/fixtures', exchange) : undefined,
      }),
    );
  const ts = worker('ts', proxies[0], ca.cert),
    rust = worker('rust', proxies[1], ca.cert);
  const options = {
    key: row?.apiKey ?? '',
    secret: row?.secret ?? '',
    passphrase: row?.passphrase ?? '',
    publicDataOnly: !row,
    tealstreetProxy: false,
    tealstreetTunnel: false,
    tealstreetWsRoute: false,
    hostedMode: true,
    extra: { recvWindow: 60000 },
    tuning: { consoleLogging: false },
  };
  for (const call of [ts, rust]) {
    const opened = await call('open', [], { exchange, options });
    if (opened.failure) throw new Error(`Failed to open account: ${opened.failure.name}: ${opened.failure.message}`);
  }
  async function check(method, methodArgs = [], label = method) {
    const start = records.length,
      time = Date.now();
    let left, right, tsMs, rustStart;
    try {
      left = await ts(method, methodArgs, { time });
      tsMs = Date.now() - time;
      rustStart = Date.now();
      right = await rust(method, methodArgs, { time });
    } catch (error) {
      report.checks.push({
        method: label,
        status: /timed out/.test(error.message) ? 'TIMEOUT' : 'BLOCKED',
        tsMs,
        rustMs: rustStart ? Date.now() - rustStart : undefined,
        reason: error.message,
        tsCount: Array.isArray(left?.value) ? left.value.length : undefined,
      });
      throw error;
    }
    const requests = records.slice(start);
    const check = {
      method: label,
      ...outcome(method, left, right, requests),
      tsFailure: left.failure,
      rustFailure: right.failure,
      tsErrors: left.errors,
      rustErrors: right.errors,
      tsMs,
      rustMs: Date.now() - rustStart,
      count: Array.isArray(left.value) ? left.value.length : undefined,
      requests: requests.length,
      liveResponses: requests.filter((request) => request.liveStatus !== undefined).length,
    };
    if (check.status !== 'PASS') failed = true;
    report.checks.push(check);
    console.log(`${exchange} ${label}: ${check.status}${check.count !== undefined ? ` (${check.count} records)` : ''}`);
    return { left: left.value, right: right.value, check, requests };
  }
  // Hyperliquid intentionally inherits the base fetchMarkets/fetchTickers
  // rejections in both implementations. Its supported public API is startup
  // plus Store delivery, with sockets disabled to keep the recorded REST oracle.
  if (exchange === 'hyperliquid') await check('$startPublic', [], 'public startup');
  else await check('refreshMarkets');
  const markets = await check(exchange === 'hyperliquid' ? '$markets' : 'fetchMarkets', [], 'markets');
  const tickers = await check(exchange === 'hyperliquid' ? '$tickers' : 'fetchTickers', [], 'tickers');
  if (row) {
    await check('validateAccount', [], 'account validation');
    await check('fetchAndSetPrivateSnapshot');
    await check('$balance', [], 'balance');
    await check('fetchPositions', [null]);
    await check('fetchOrders', [null]);
  } else
    report.limitations.push(
      'Google keys login/test-account row required for balance, positions, open orders and place/cancel.',
    );
  if (trade) {
    if (exchange !== 'binance') throw new Error('Live order phase currently supports Binance USD-M only');
    if (report.checks.some((check) => check.status !== 'PASS'))
      throw new Error('Read-only preflight did not pass; order phase stopped');
    // Let the normal account startup load actual hedge mode and order state
    // before placing. Session-key creation is allowed only in the order phase.
    for (const proxy of proxies) proxy.writes(true);
    try {
      const startup = await check('$startPrivate', [], 'order preflight startup');
      if (startup.check.status !== 'PASS') throw new Error('Account startup did not pass; order phase stopped');
    } finally {
      for (const proxy of proxies) proxy.writes(false);
    }
    // Same small notional/symbol/distance used by the authorized keys CLI's
    // Binance test config; avoid BTC's larger minimum notional and price bands.
    const market = markets.left.find((value) => value.symbol === 'DOGEUSDT');
    const ticker = tickers.left.find((value) => value.symbol === 'DOGEUSDT');
    if (!market?.active || !Number.isFinite(ticker?.bid) || ticker.bid <= 0)
      throw new Error('Missing active DOGE market/ticker');
    const price = Math.floor((ticker.bid * 0.985) / market.precision.price) * market.precision.price;
    const amount =
      Math.ceil(Math.max(market.limits.amount.min, 10 / price) / market.precision.amount) * market.precision.amount;
    if (!(price > 0 && amount > 0 && amount * price <= 15)) throw new Error('Invalid far-limit price/size');
    const order = {
      symbol: market.symbol,
      side: 'buy',
      type: 'limit',
      price,
      amount,
      timeInForce: 'PostOnly',
      clientOrderId: `tsdiff${Date.now()}`,
    };
    report.order = order;
    const outgoing = [];
    // Sequential real orders, each cancelled before the other is submitted.
    // Reusing the caller ID is permitted after Binance cancellation.
    const time = Date.now();
    for (const [index, call] of [ts, rust].entries()) {
      const start = records.length;
      proxies[index].writes(true);
      let ids;
      try {
        const placed = await call('placeOrder', [order], { time });
        if (placed.failure || !placed.value?.length)
          throw new Error('Placement failed; reconcile by caller clientOrderId before retrying');
        ids = placed.value;
        cache.clear();
        const observed = await call('fetchOrders', [null], { time });
        if (!observed.value?.some((value) => ids.includes(value.id)))
          throw new Error('Placed order was not visible in a fresh open-order read');
      } finally {
        try {
          // If the placement response was lost, reconcile once by caller ID.
          if (!ids) {
            const lookup = await call('findOrderByClientOrderId', [order.symbol, order.clientOrderId], { time });
            if (lookup.value?.found) ids = [lookup.value.order.id];
          }
          if (ids?.length) {
            const cancelled = await call(
              'cancelOrders',
              [ids.map((id) => ({ id, symbol: order.symbol, type: 'limit' }))],
              { time: Date.now() },
            );
            if (cancelled.failure || cancelled.errors)
              throw new Error('Cancellation failed; inspect test account immediately');
            cache.clear();
            const remaining = await call('fetchOrders', [null], { time: Date.now() });
            if (
              remaining.failure ||
              remaining.errors ||
              !Array.isArray(remaining.value) ||
              remaining.value.some((value) => ids.includes(value.id))
            )
              throw new Error('Cancellation could not be confirmed; inspect test account immediately');
          }
        } finally {
          proxies[index].writes(false);
        }
      }
      outgoing.push(
        records
          .slice(start)
          .filter((value) => value.method === 'POST' && !value.read && !value.blocked)
          .map((value) => value.requestBytes),
      );
      // Never replay stale open-order snapshots during the real order lifecycle.
      cache.clear();
    }
    const orderDiff = differences(
      outgoing[0].map((value) => hash(value)),
      outgoing[1].map((value) => hash(value)),
    );
    report.checks.push({
      method: 'placement-wire-bytes',
      status: orderDiff.length ? 'DIFF' : 'PASS',
      diffs: outgoing[0].map((value, index) => byteDifference(value, outgoing[1][index] ?? '')).filter(Boolean),
    });
    if (orderDiff.length) failed = true;
  }
} catch (error) {
  failed = true;
  report.limitations.push(error.message);
  console.error(secretValues.reduce((message, secret) => message.split(secret).join('<credential>'), error.message));
} finally {
  for (const { call, child } of workers) {
    if (child.connected) {
      try {
        await call('close');
      } catch {}
      child.kill();
    }
  }
  for (const proxy of proxies) await proxy.close();
  rmSync(directory, { recursive: true, force: true });
  report.requests = records.map(({ requestBytes, response, ...metadata }) => ({
    ...metadata,
    requestHash: requestBytes ? hash(requestBytes) : undefined,
  }));
  report.status = failed
    ? 'INCOMPLETE_OR_DIFF'
    : row
      ? trade
        ? 'PASS_WITH_TRADE'
        : 'PASS_READ_ONLY'
      : 'PASS_PUBLIC_ONLY';
  const output = resolve(option('--output', join(root, '.cache/live-diff', `${exchange}.json`)));
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(redact(report), null, 2) + '\n', { mode: 0o600 });
  console.log(`Report: ${output}`);
}
process.exitCode = failed ? 1 : 0;
