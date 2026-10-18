#!/usr/bin/env node
import { fork, spawn, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deserialize, serialize } from 'node:v8';
import { driveOperation } from './rest-clock-driver.mjs';
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
const realtimeClock = argv.includes('--realtime-clock');
const linearOnly = argv.includes('--linear-only');
if (linearOnly && !['bybit', 'bybitdemo'].includes(exchange))
  throw new Error('--linear-only is a Bybit bootstrap profile');
const timeout = Number(option('--timeout-ms', '20000'));
if (!Number.isSafeInteger(timeout) || timeout < 1000 || timeout > 300000) throw new Error('Invalid timeout');
const sampleAfter = Number(option('--sample-after-ms', '0'));
if (!Number.isSafeInteger(sampleAfter) || sampleAfter < 0) throw new Error('Invalid sample delay');
const output = resolve(option('--output', `.cache/live-diff/${exchange}-rest.json`));
const fixtures = resolve('.cache/live-diff/fixtures', exchange);
mkdirSync(fixtures, { recursive: true });
const directory = mkdtempSync(join(tmpdir(), 'safe-cex-rest-'));
const report = {
  kind: 'PUBLIC_REST_DIFF',
  exchange,
  at: new Date().toISOString(),
  revision: spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).stdout.trim(),
  clockMode: realtimeClock ? 'REALTIME_DIAGNOSTIC_ONLY' : 'DRIVEN_CLOCK',
  rustRevision: option('--rust-revision'),
  addon: resolve(option('--addon', 'sdk-rs/safe-cex-ffi/dist/node/safe_cex_ffi.node')),
  harnessHashes: Object.fromEntries(
    [
      'rest.mjs',
      'rest-worker.mjs',
      'rest-plan.mjs',
      'rest-clock-driver.mjs',
      'rest-venues.mjs',
      'recorder.mjs',
      'compare.mjs',
    ].map((file) => [file, hash(readFileSync(join(here, file)))]),
  ),
  runs: [],
  checks: [],
  limitations: [
    'Clock advances in1s steps only while an operation is pending and terminal HTTP is idle; per-operation traces must repeat and match across sides. Scheduling stalls and incomplete bootstrap are provisional, not mapper attribution. Funding/OI are exposed through tickers; populated coverage requires finite nonzero values.',
  ],
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
      clearTimeout(p.profileTimer);
      pending.delete(reply.requestId);
      const { requestId, ...result } = reply;
      p.resolve(result);
    }
  });
  const failPending = (reason) => {
    dead = true;
    for (const p of pending.values()) {
      clearTimeout(p.timer);
      clearTimeout(p.profileTimer);
      p.reject(new Error(reason));
    }
    pending.clear();
  };
  child.on('exit', () => failPending(`${label} worker exited`));
  child.on('error', (error) => failPending(error.message));
  const call = (method, args = [], extra = {}) =>
    new Promise((resolveCall, reject) => {
      if (dead) return reject(Object.assign(new Error('session stopped after timeout/exit'), { notRun: true }));
      const requestId = ++sequence;
      const profileTimer =
        implementation === 'rust' && sampleAfter && method !== 'close' && !method.startsWith('$')
          ? setTimeout(() => {
              const path = `${output}.${label}.${requestId}.sample.txt`;
              mkdirSync(dirname(path), { recursive: true });
              call('$receipt').then(
                (receipt) => diagnostics.push({ method, receipt }),
                (error) => diagnostics.push(`receipt: ${error.message}`),
              );
              const sampler = spawn('sample', [String(child.pid), '3', '-file', path], { stdio: 'ignore' });
              sampler.on('error', (error) => diagnostics.push(`sample failed: ${error.message}`));
              diagnostics.push(`CPU sample: ${path}; pid=${child.pid}; method=${method}`);
            }, sampleAfter)
          : undefined;
      const timer = setTimeout(
        () => {
          pending.delete(requestId);
          clearTimeout(profileTimer);
          dead = true;
          child.kill();
          reject(Object.assign(new Error(`${method} timeout ${timeout}ms`), { timeout: true }));
        },
        method === 'close' ? 3000 : timeout,
      );
      pending.set(requestId, { timer, profileTimer, resolve: resolveCall, reject });
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
  const result = { implementation, label, records, diagnostics, call, close, atMs: 0, operationIndex: 0 };
  sessions.push(result);
  return result;
}

async function invoke(s, spec) {
  if (spec.skip) return { skipped: spec.skip };
  const start = s.records.length;
  if (!realtimeClock) {
    const atMs = (s.operationIndex++ || 0) * 60000;
    try {
      await s.call('$advance', [atMs]);
      s.atMs = atMs;
    } catch (error) {
      return {
        probeFailure: true,
        notRun: true,
        failure: { name: 'ProbeError', message: error.message },
        records: cleanRecords(s.records.slice(start)),
      };
    }
  }
  const reply = await driveOperation(s, spec, { realtimeClock });
  return { ...reply, records: cleanRecords(s.records.slice(start)) };
}
const valueOnly = ({ records, clock, ...value }) => value;
async function open(s, epoch) {
  let opened;
  try {
    opened = await s.call('open', [], {
      exchange,
      epoch,
      realtimeClock,
      accountOptions: bundle?.accountOptions ?? {
        enableOptionMarkets: !linearOnly && ['bybit', 'bybitdemo'].includes(exchange),
      },
    });
  } catch (error) {
    throw new Error(`${s.label} open: ${error.message}`);
  }
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
  const requestBytes = serialize(s.records.map(({ response, ...record }) => record));
  const requestFixture = join(fixtures, `${hash(requestBytes)}.${s.label}.rest-requests.v8`);
  writeFileSync(requestFixture, requestBytes, { mode: 0o600 });
  const bytes = serialize(results),
    fixture = join(fixtures, `${hash(bytes)}.${s.label}.rest-results.v8`);
  writeFileSync(fixture, bytes, { mode: 0o600 });
  report.runs.push({
    label: s.label,
    implementation: s.implementation,
    fixture,
    requestFixture,
    requestHash: hash(requestBytes),
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
    report.status =
      exchange === 'hyrotrader'
        ? 'CONFIG_REQUIRED'
        : exchange === 'breakoutprop'
          ? 'AUTH_REQUIRED'
          : 'UNSUPPORTED_BY_TS';
    report.limitations.push(
      exchange === 'hyrotrader'
        ? 'TS HyroTrader requires an operator-configured REST origin, absent from the literal live host adapters.'
        : exchange === 'breakoutprop'
          ? 'TS DXtrade instruments/quotes require session login and accountCode.'
          : 'TS TXFlow is a Phase 0 scaffold without public REST.',
    );
  } else {
    let captureResults;
    const replay = option('--replay');
    if (replay) {
      bundle = deserialize(readFileSync(resolve(replay)));
      if (!['PUBLIC_REST_V1', 'PUBLIC_REST_V2'].includes(bundle.kind) || bundle.exchange !== exchange)
        throw new Error('Wrong REST bundle kind/venue');
      for (const [key, value] of bundle.rest) cache.set(key, value);
      report.bundle = resolve(replay);
      captureResults = bundle.captureResults;
    } else {
      const epoch = Date.now();
      const s = await session('ts', 'ts-capture', true);
      await open(s, epoch);
      const preflight = await bootstrap(s);
      if (preflight.timeout || preflight.probeFailure) {
        await saveRun(s, [preflight]);
        throw new Error(`TS bootstrap incomplete: ${preflight.failure?.message}`);
      }
      const described = await s.call('$describe');
      if (described.failure) throw new Error(described.failure.message);
      const { symbol, plan } = publicRestPlan(exchange, described.value, epoch, { enableOptions: !linearOnly });
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
        kind: 'PUBLIC_REST_V2',
        exchange,
        epoch,
        accountOptions: { enableOptionMarkets: !linearOnly && ['bybit', 'bybitdemo'].includes(exchange) },
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
    if (bundle.kind === 'PUBLIC_REST_V1')
      report.limitations.push(
        'Legacy bundle has no capture clock trace; use a fresh V2 capture for final clock qualification.',
      );
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
      if (captureResults[i].clock) differences(captureResults[i].clock, ts.clock, '$clock', tsRepeat);
      const nativeRepeat = differences(valueOnly(native1), valueOnly(native2));
      differences(native1.clock, native2.clock, '$clock', nativeRepeat);
      const stable = tsRepeat.length === 0 && nativeRepeat.length === 0;
      const records = [captureResults[i], ts, native1, native2].flatMap((r) => r.records ?? []);
      const check = {
        label: spec.label,
        method: spec.method,
        args: spec.args,
        ...resultStatus(
          spec,
          { ...valueOnly(ts), clock: ts.clock },
          { ...valueOnly(native1), clock: native1.clock },
          records,
          stable,
        ),
        clockTraces: { capture: captureResults[i].clock, ts: ts.clock, native1: native1.clock, native2: native2.clock },
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
      : report.checks.some((c) =>
            ['TIMEOUT', 'BLOCKED', 'INPUT_DIFF', 'NONDETERMINISTIC', 'CLOCK_DIFF', 'NOT_RUN'].includes(c.status),
          )
        ? 'INCOMPLETE'
        : report.checks.some((c) => c.status === 'PASS' && c.label !== 'bootstrap-markets')
          ? 'PASS_WITH_COVERAGE_LIMITS'
          : 'NO_PUBLIC_COVERAGE';
    if (realtimeClock || bundle.kind === 'PUBLIC_REST_V1') report.status = 'DIAGNOSTIC_ONLY';
  }
} catch (error) {
  report.status = 'BLOCKED';
  report.limitations.push(error.message);
  console.error(`${exchange}: ${error.message}`);
} finally {
  for (const s of [...sessions]) await saveRun(s, [{ probeFailure: true, notRun: true }]);
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { mode: 0o600 });
  console.log(`Report: ${output}`);
}
process.exitCode = report.status === 'PASS_WITH_COVERAGE_LIMITS' ? 0 : 1;
