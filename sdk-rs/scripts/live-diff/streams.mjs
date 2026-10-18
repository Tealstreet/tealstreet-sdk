#!/usr/bin/env node
import { fork, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { serialize, deserialize } from 'node:v8';
import { hash, makeCA, recordingProxy } from './recorder.mjs';
import { buildConnectHook, connectHookEnvironment } from './ws-recorder.mjs';
import { streamRelay } from './stream-relay.mjs';
import { publicStreams, tradeCapability } from './stream-venues.mjs';
import { venueHosts } from './venues.mjs';
import { compareSequences, qualifyComparisons, frameTopics } from './stream-compare.mjs';

const here = dirname(fileURLToPath(import.meta.url)),
  root = resolve(here, '../../..');
const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const exchange = option('--venue', 'binance'),
  adapter = publicStreams[exchange];
if (!adapter) throw new Error('Add an explicit public stream host adapter for this venue');
const captureMs = Number(option('--capture-ms', '60000')),
  timeout = Number(option('--timeout-ms', '60000'));
if (!Number.isSafeInteger(captureMs) || captureMs < 1000 || captureMs > 60000)
  throw new Error('--capture-ms must be 1000..60000; 60000 is the live gate');
if (!Number.isSafeInteger(timeout) || timeout < 1000 || timeout > 300000)
  throw new Error('--timeout-ms must be 1000..300000');
if (args.some((arg) => ['--row', '--trade'].includes(arg))) throw new Error('Streams probe is strictly keyless');
const directory = mkdtempSync(join(tmpdir(), 'safe-cex-streams-'));
const output = resolve(option('--output', join(root, '.cache/live-diff', `${exchange}-streams.json`)));
const fixtureDirectory = join(root, '.cache/live-diff/fixtures', exchange);
mkdirSync(fixtureDirectory, { recursive: true });
const report = {
  exchange,
  at: new Date().toISOString(),
  revision: spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).stdout.trim(),
  addon: resolve(option('--addon', join(root, 'sdk-rs/safe-cex-ffi/dist/node/safe_cex_ffi.node'))),
  node: process.version,
  rustRevision: option('--rust-revision'),
  captureMs,
  limitations: [
    tradeCapability,
    'Real production timers and clocks are retained. Repeatability is checked exactly; clock/throttle nondeterminism blocks parity claims.',
  ],
  runs: [],
};
report.addonSha256 = hash(readFileSync(report.addon));
const options = {
  key: '',
  secret: '',
  publicDataOnly: true,
  tealstreetProxy: false,
  tealstreetTunnel: false,
  tealstreetWsRoute: false,
  hostedMode: true,
  tuning: { consoleLogging: false, activeTickerSymbols: [adapter.symbol] },
};
const cache = new Map(),
  sessions = [];
let ca,
  hook,
  symbol,
  candleTo = Date.now();
async function session(implementation, mode, tape, label) {
  const requests = [],
    sockets = [];
  const relay = await streamRelay({ directory, ca, hosts: adapter.hosts, mode, tape, records: sockets });
  const proxy = await recordingProxy({
    directory,
    ca,
    cache,
    records: requests,
    implementation,
    allowHost: (host) => venueHosts[exchange].includes(host),
    replayOnly: mode === 'replay',
    captureDirectory: mode === 'capture' ? fixtureDirectory : undefined,
  });
  const child = fork(join(here, 'stream-worker.mjs'), [implementation], {
    cwd: root,
    execArgv: ['--import', 'tsx'],
    serialization: 'advanced',
    stdio: ['ignore', 'ignore', 'pipe', 'ipc'],
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
      SSL_CERT_FILE: ca.cert,
      NODE_EXTRA_CA_CERTS: ca.cert,
      TSX_TSCONFIG_PATH: join(root, 'packages/safe-cex/tsconfig.json'),
      LIVE_DIFF_ADDON: report.addon,
      ...connectHookEnvironment(hook, relay.ports),
    },
  });
  const diagnostics = [],
    pending = new Map();
  let sequence = 0;
  child.stderr.on('data', (bytes) => {
    if (diagnostics.length < 12) diagnostics.push(bytes.toString().slice(0, 2000));
  });
  child.on('message', (reply) => {
    const p = pending.get(reply.requestId);
    if (!p) return;
    clearTimeout(p.timer);
    pending.delete(reply.requestId);
    if (reply.failure) p.reject(new Error(`${label}: ${reply.failure.name}: ${reply.failure.message}`));
    else p.resolve(reply.value);
  });
  child.on('exit', (code, signal) => {
    for (const p of pending.values()) {
      clearTimeout(p.timer);
      p.reject(new Error(`${label} exited ${code}/${signal}`));
    }
    pending.clear();
  });
  child.on('error', (error) => {
    for (const p of pending.values()) {
      clearTimeout(p.timer);
      p.reject(error);
    }
    pending.clear();
  });
  const call = (method, methodArgs = [], extra = {}) =>
    new Promise((resolveCall, reject) => {
      const requestId = ++sequence,
        timer = setTimeout(
          () => {
            pending.delete(requestId);
            reject(new Error(`${label} ${method} timeout after ${timeout}ms`));
          },
          method === 'close' ? 3000 : timeout,
        );
      pending.set(requestId, { resolve: resolveCall, reject, timer });
      child.send({ requestId, method, args: methodArgs, ...extra });
    });
  const result = {
    implementation,
    label,
    requests,
    sockets,
    diagnostics,
    relay,
    call,
    async close() {
      try {
        if (child.connected) await call('close');
      } catch {}
      child.kill();
      await relay.close();
      await proxy.close();
    },
  };
  sessions.push(result);
  await call('open', [], { exchange, options });
  await call('start');
  console.log(`${exchange}: ${label} started`);
  return result;
}
const cleanRecords = (requests) =>
  requests.map(({ requestBytes, response, ...metadata }) => ({
    ...metadata,
    requestHash: requestBytes ? hash(requestBytes) : undefined,
  }));
try {
  ca = makeCA(directory);
  hook = buildConnectHook(directory, join(here, 'ws-connect.c'));
  const replayPath = option('--replay');
  let tape;
  if (replayPath) {
    const bundle = deserialize(readFileSync(resolve(replayPath)));
    if (bundle.exchange !== exchange) throw new Error('Replay venue does not match bundle');
    ({ tape, symbol, candleTo } = bundle);
    for (const [key, value] of bundle.rest) cache.set(key, value);
    report.capture = { ...bundle.capture, reusedBundle: resolve(replayPath) };
  } else {
    tape = {};
    console.log(`${exchange}: capture startup`);
    const capture = await session('ts', 'capture', tape, 'capture-ts');
    const { markets } = await capture.call('describe');
    const market =
      markets.find((m) => m.active && m.base === 'BTC' && m.quote === 'USDT') ??
      markets.find((m) => m.active && m.base === 'BTC') ??
      markets.find((m) => m.active && m.symbol === adapter.symbol);
    if (!market) throw new Error('No active BTC market; stream subscription stopped');
    symbol = market.symbol;
    report.symbol = symbol;
    await capture.call('subscribe', [symbol, '1m', candleTo]);
    if (!(await capture.relay.ready())) throw new Error('No public socket opened during capture');
    console.log(`${exchange}: capturing ${captureMs}ms (${symbol})`);
    const started = performance.now();
    await new Promise((resolveWait) => setTimeout(resolveWait, captureMs));
    report.capture = {
      ...capture.relay.freeze(fixtureDirectory),
      windowMs: performance.now() - started,
      requests: cleanRecords(capture.requests),
      sockets: capture.sockets,
    };
    await capture.close();
    sessions.splice(sessions.indexOf(capture), 1);
    const rest = await Promise.all([...cache].map(async ([key, value]) => [key, await value]));
    const bundle = serialize({ exchange, tape, symbol, candleTo, rest, capture: report.capture });
    report.capture.bundle = join(fixtureDirectory, `${hash(bundle)}.bundle.v8`);
    writeFileSync(report.capture.bundle, bundle, { mode: 0o600 });
  }
  report.symbol = symbol;
  report.liveWindowPassed = report.capture.windowMs >= 60000;
  report.capture.topics = frameTopics(tape);
  if (!tape.frames.some((f) => f.kind === 'message')) throw new Error('Capture has no public message frames');
  console.log(`${exchange}: four fresh replays (${tape.frames.length} frames)`);
  const startups = await Promise.allSettled(
    ['ts', 'ts', 'rust', 'rust'].map((implementation, i) =>
      session(implementation, 'replay', tape, `${implementation}-${(i % 2) + 1}`),
    ),
  );
  if (startups.some((s) => s.status === 'rejected'))
    throw new Error(
      startups
        .filter((s) => s.status === 'rejected')
        .map((s) => s.reason.message)
        .join('; '),
    );
  const replaySessions = startups.map((s) => s.value);
  const warmups = await Promise.allSettled(
    replaySessions.map(async (s) => {
      await s.call('subscribe', [symbol, '1m', candleTo]);
      if (!(await s.relay.ready())) throw new Error(`${s.label}: captured sockets did not open`);
      // Flush startup Store deliveries before observing parser-facing replay.
      await s.call('barrier');
    }),
  );
  if (warmups.some((s) => s.status === 'rejected'))
    throw new Error(
      warmups
        .filter((s) => s.status === 'rejected')
        .map((s) => s.reason.message)
        .join('; '),
    );
  await Promise.all(replaySessions.map((s) => s.call('begin')));
  const results = await Promise.all(
    replaySessions.map(async (s) => {
      const receipt = await s.relay.play();
      const barrierStart = performance.now();
      let barrier = { label: s.label, status: 'fulfilled' };
      try {
        await s.call('barrier');
        console.log(`${exchange}: ${s.label} barrier complete`);
      } catch (error) {
        barrier = { label: s.label, status: 'rejected', reason: error.message };
      }
      barrier.durationMs = performance.now() - barrierStart;
      const value = await s.call('drain');
      await s.close();
      sessions.splice(sessions.indexOf(s), 1);
      return { receipt, barrier, value };
    }),
  );
  const receipts = results.map((r) => r.receipt),
    values = results.map((r) => r.value);
  report.barriers = results.map((r) => r.barrier);
  for (const [i, s] of replaySessions.entries()) {
    const bytes = serialize(values[i]);
    const fixture = join(fixtureDirectory, `${hash(bytes)}.${s.label}.outputs.v8`);
    writeFileSync(fixture, bytes, { mode: 0o600 });
    report.runs.push({
      label: s.label,
      implementation: s.implementation,
      receipt: receipts[i],
      fixture,
      outputHash: hash(bytes),
      outputCount: values[i].outputs.length,
      requests: cleanRecords(s.requests),
      sockets: s.sockets,
      diagnostics: s.diagnostics,
    });
  }
  report.repeatability = {
    ts: compareSequences(values[0].outputs, values[1].outputs),
    rust: compareSequences(values[2].outputs, values[3].outputs),
  };
  report.parity = compareSequences(values[0].outputs, values[2].outputs);
  report.coverage = Object.fromEntries(
    ['store:tickers', 'orderbook', 'candles'].map((channel) => [
      channel,
      {
        ts: report.parity[channel]?.leftCount ?? 0,
        rust: report.parity[channel]?.rightCount ?? 0,
      },
    ]),
  );
  const inputDiff = report.runs.some(
    (r) =>
      r.receipt.dropped ||
      r.receipt.inputHash !== r.receipt.expectedInputHash ||
      r.sockets.some((s) => s.failure) ||
      r.requests.some((q) => q.blocked || q.transportError),
  );
  const unstable = Object.values(report.repeatability).some((ch) => Object.values(ch).some((c) => c.status === 'DIFF'));
  report.status = inputDiff
    ? 'INPUT_DIFF'
    : unstable
      ? 'NONDETERMINISTIC'
      : Object.values(report.parity).some((c) => c.status === 'DIFF')
        ? 'DIFF'
        : 'PASS';
  if (report.barriers.some((v) => v.status === 'rejected')) {
    report.status = 'TIMEOUT';
    report.comparisonsProvisional = true;
    report.limitations.push(
      'Native barrier failed; retained partial outputs are diagnostic evidence, not completed parity results.',
    );
  }
  qualifyComparisons(report.parity, report.repeatability, { inputDiff, incomplete: report.comparisonsProvisional });
  // A venue cannot pass on empty streams, even when all four empty sequences agree.
  if (
    report.status === 'PASS' &&
    ['store:tickers', 'orderbook', 'candles'].some((c) => report.parity[c]?.status !== 'PASS')
  )
    report.status = 'INCOMPLETE_COVERAGE';
  if (report.status === 'PASS' && !report.liveWindowPassed) report.status = 'SMOKE_ONLY';
} catch (error) {
  report.status = /timeout/.test(error.message) ? 'TIMEOUT' : 'BLOCKED';
  report.failure = error.message;
  console.error(error.message);
} finally {
  for (const s of sessions) {
    report.partial ??= [];
    report.partial.push({
      label: s.label,
      requests: cleanRecords(s.requests),
      sockets: s.sockets,
      diagnostics: s.diagnostics,
    });
  }
  await Promise.all(sessions.map((s) => s.close()));
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  console.log(`${exchange}: ${report.status}; report ${output}`);
}
process.exitCode = report.status === 'PASS' ? 0 : 1;
