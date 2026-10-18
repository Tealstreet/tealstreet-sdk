#!/usr/bin/env node
import { fork, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deserialize, serialize } from 'node:v8';
import { differences } from './compare.mjs';
import { hash, makeCA, recordingProxy } from './recorder.mjs';
import { restVenueHosts as venueHosts } from './rest-venues.mjs';
import { publicRestPlan, resultStatus, tickerCoverage } from './rest-plan.mjs';

const here = dirname(fileURLToPath(import.meta.url)),
  root = resolve(here, '../../..');
const argv = process.argv.slice(2);
const option = (name, fallback) => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : fallback);
if (argv.some((a) => ['--row', '--trade'].includes(a))) throw new Error('REST comparison is strictly keyless');
const exchange = option('--venue', 'binance');
if (!venueHosts[exchange]) throw new Error(`No audited REST hosts for ${exchange}`);
const timeout = Number(option('--timeout-ms', '20000'));
if (!Number.isSafeInteger(timeout) || timeout < 1000 || timeout > 300000) throw new Error('Invalid timeout');
const output = resolve(option('--output', `.cache/live-diff/${exchange}-rest.json`));
const fixtures = resolve('.cache/live-diff/fixtures', exchange);
mkdirSync(fixtures, { recursive: true });
const directory = mkdtempSync(join(tmpdir(), 'safe-cex-rest-'));
const report = {
  kind: 'PUBLIC_REST_DIFF',
  exchange,
  at: new Date().toISOString(),
  revision: spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).stdout.trim(),
  rustRevision: option('--rust-revision'),
  addon: resolve(option('--addon', 'sdk-rs/safe-cex-ffi/dist/node/safe_cex_ffi.node')),
  runs: [],
  checks: [],
  limitations: [],
};
const frozenAddon = join(directory, 'safe_cex_ffi.node');
const cache = new Map(),
  sessions = [];
let bundle, ca;
const cleanRecords = (records) =>
  records.map(({ requestBytes, response, ...record }) => ({
    ...record,
    requestHash: requestBytes ? hash(requestBytes) : undefined,
  }));

async function session(implementation, label, capture = false) {
  const records = [];
  const proxy = await recordingProxy({
    directory,
    ca,
    cache,
    records,
    implementation,
    allowHost: (host) => venueHosts[exchange].includes(host),
    replayOnly: !capture,
    captureDirectory: capture ? fixtures : undefined,
  });
  const child = fork(join(here, 'rest-worker.mjs'), [implementation], {
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
      LIVE_DIFF_ADDON: frozenAddon,
    },
  });
  const pending = new Map(),
    diagnostics = [];
  let sequence = 0,
    dead = false;
  child.stderr.on('data', (data) => {
    if (diagnostics.length < 12) diagnostics.push(data.toString().slice(0, 1000));
  });
  child.on('message', (reply) => {
    const p = pending.get(reply.requestId);
    if (p) {
      clearTimeout(p.timer);
      pending.delete(reply.requestId);
      const { requestId, ...result } = reply;
      p.resolve(result);
    }
  });
  const failPending = (reason) => {
    dead = true;
    for (const p of pending.values()) {
      clearTimeout(p.timer);
      p.reject(new Error(reason));
    }
    pending.clear();
  };
  child.on('exit', () => failPending(`${label} worker exited`));
  child.on('error', (error) => failPending(error.message));
  const call = (method, args = [], extra = {}) =>
    new Promise((resolveCall, reject) => {
      if (dead) return reject(new Error(`${label} session stopped after timeout/exit`));
      const requestId = ++sequence;
      const timer = setTimeout(
        () => {
          pending.delete(requestId);
          dead = true;
          child.kill();
          reject(new Error(`${label} ${method} timeout ${timeout}ms`));
        },
        method === 'close' ? 3000 : timeout,
      );
      pending.set(requestId, { timer, resolve: resolveCall, reject });
      child.send({ requestId, method, args, ...extra });
    });
  const close = async () => {
    if (!dead) {
      try {
        await call('close');
      } catch {}
    }
    child.kill();
    await proxy.close();
  };
  const result = { implementation, label, records, diagnostics, call, close };
  sessions.push(result);
  return result;
}

async function invoke(s, spec) {
  if (spec.skip) return { skipped: spec.skip };
  const start = s.records.length;
  let reply;
  try {
    reply = await s.call(spec.method, spec.args);
  } catch (error) {
    reply = { timeout: /timeout/.test(error.message), failure: { name: 'ProbeError', message: error.message } };
  }
  return { ...reply, records: cleanRecords(s.records.slice(start)) };
}
const valueOnly = ({ records, ...value }) => value;
async function open(s, epoch) {
  const opened = await s.call('open', [], { exchange, epoch });
  if (opened.failure) throw new Error(`${s.label} open: ${opened.failure.message}`);
}
async function bootstrap(s) {
  if (exchange.startsWith('hyperliquid')) {
    const disabled = await s.call('disablePublicWebsockets');
    if (disabled.failure) return disabled;
    return invoke(s, { method: 'start', args: [] });
  }
  return invoke(s, { method: 'refreshMarkets', args: [] });
}
async function saveRun(s, results) {
  await s.close();
  sessions.splice(sessions.indexOf(s), 1);
  const bytes = serialize(results),
    fixture = join(fixtures, `${hash(bytes)}.${s.label}.rest-results.v8`);
  writeFileSync(fixture, bytes, { mode: 0o600 });
  report.runs.push({
    label: s.label,
    implementation: s.implementation,
    fixture,
    hash: hash(bytes),
    requests: cleanRecords(s.records),
    diagnostics: s.diagnostics,
  });
  return results;
}
try {
  const bytes = readFileSync(report.addon);
  report.addonSha256 = hash(bytes);
  writeFileSync(frozenAddon, bytes, { mode: 0o600 });
  ca = makeCA(directory);
  if (['breakoutprop', 'txflow', 'hyrotrader'].includes(exchange)) {
    report.status = exchange === 'hyrotrader' ? 'CONFIG_REQUIRED' : exchange === 'breakoutprop' ? 'AUTH_REQUIRED' : 'UNSUPPORTED_BY_TS';
    report.limitations.push(
      exchange === 'hyrotrader' ? 'TS HyroTrader requires an operator-configured REST origin, absent from the literal live host adapters.' : exchange === 'breakoutprop'
        ? 'TS DXtrade instruments/quotes require session login and accountCode.'
        : 'TS TXFlow is a Phase 0 scaffold without public REST.',
    );
  } else {
    let captureResults;
    const replay = option('--replay');
    if (replay) {
      bundle = deserialize(readFileSync(resolve(replay)));
      if (bundle.kind !== 'PUBLIC_REST_V1' || bundle.exchange !== exchange)
        throw new Error('Wrong REST bundle kind/venue');
      for (const [key, value] of bundle.rest) cache.set(key, value);
      report.bundle = resolve(replay);
      captureResults = bundle.captureResults;
    } else {
      const epoch = Date.now();
      const s = await session('ts', 'ts-capture', true);
      await open(s, epoch);
      const preflight = await bootstrap(s);
      const described = await s.call('$describe');
      if (described.failure) throw new Error(described.failure.message);
      const { symbol, plan } = publicRestPlan(exchange, described.value, epoch);
      captureResults = [preflight];
      for (const spec of plan) {
        const result = await invoke(s, spec);
        captureResults.push(result);
        console.log(
          `${exchange} capture ${spec.label}: ${result.skipped ?? result.failure?.message ?? (Array.isArray(result.value) ? result.value.length : 'ok')}`,
        );
      }
      await saveRun(s, captureResults);
      const rest = [];
      for (const [key, value] of cache) {
        try {
          rest.push([key, await value]);
        } catch {}
      }
      bundle = {
        kind: 'PUBLIC_REST_V1',
        exchange,
        epoch,
        symbol,
        description: described.value,
        plan,
        rest,
        captureResults,
      };
      const bytes = serialize(bundle);
      report.bundle = join(fixtures, `${hash(bytes)}.rest-bundle.v8`);
      writeFileSync(report.bundle, bytes, { mode: 0o600 });
    }
    report.epochMs = bundle.epoch;
    report.symbol = bundle.symbol;
    report.bundleSha256 = hash(readFileSync(report.bundle));
    const specs = [
      { label: 'bootstrap-markets', method: exchange.startsWith('hyperliquid') ? 'start' : 'refreshMarkets', args: [] },
      ...bundle.plan,
    ];
    // All replay proxies fail closed on a cache miss. No native read can fetch
    // moving live data and masquerade as a mapper comparison.
    const outputs = [];
    for (const [implementation, label] of [
      ['ts', 'ts-replay'],
      ['rust', 'rust-1'],
      ['rust', 'rust-2'],
    ]) {
      const s = await session(implementation, label);
      await open(s, bundle.epoch);
      const results = [await bootstrap(s)];
      for (const spec of bundle.plan) results.push(await invoke(s, spec));
      outputs.push(await saveRun(s, results));
    }
    for (const [i, spec] of specs.entries()) {
      const [ts, native1, native2] = outputs.map((run) => run[i]);
      const tsRepeat = differences(valueOnly(captureResults[i]), valueOnly(ts));
      const nativeRepeat = differences(valueOnly(native1), valueOnly(native2));
      const stable = tsRepeat.length === 0 && nativeRepeat.length === 0;
      const records = [captureResults[i], ts, native1, native2].flatMap((r) => r.records ?? []);
      const check = {
        label: spec.label,
        method: spec.method,
        args: spec.args,
        ...resultStatus(spec, valueOnly(ts), valueOnly(native1), records, stable),
        repeatability: { ts: tsRepeat, native: nativeRepeat },
        tsFailure: ts.failure,
        nativeFailure: native1.failure,
        tsCount: Array.isArray(ts.value) ? ts.value.length : undefined,
        nativeCount: Array.isArray(native1.value) ? native1.value.length : undefined,
        requests: records,
      };
      if (spec.label === 'tickers-funding-open-interest')
        check.coverage = { ts: tickerCoverage(ts), native: tickerCoverage(native1) };
      report.checks.push(check);
      console.log(`${exchange} ${spec.label}: ${check.status}`);
    }
    report.status = report.checks.some((c) => c.status === 'DIFF')
      ? 'DIFF'
      : report.checks.some((c) => ['TIMEOUT', 'BLOCKED', 'INPUT_DIFF', 'NONDETERMINISTIC'].includes(c.status))
        ? 'INCOMPLETE'
        : report.checks.some((c) => c.status === 'PASS')
          ? 'PASS_WITH_COVERAGE_LIMITS'
          : 'NO_PUBLIC_COVERAGE';
  }
} catch (error) {
  report.status = 'BLOCKED';
  report.limitations.push(error.message);
  console.error(`${exchange}: ${error.message}`);
} finally {
  for (const s of sessions) await s.close();
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  console.log(`Report: ${output}`);
}
process.exitCode = report.status === 'PASS_WITH_COVERAGE_LIMITS' ? 0 : 1;
