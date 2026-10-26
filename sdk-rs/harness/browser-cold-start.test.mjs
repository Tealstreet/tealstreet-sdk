import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import {
  archiveBrowserAssets, assetSpecs, browserServer, completeMeasurements, debuggingPort, measurementTable,
  readBrowserInputs, stopChrome, validateMeasurements, waitForChrome,
} from './browser-cold-start.mjs';
import { Cdp, evaluateWorker } from './browser-cdp.mjs';

const hash = (data) => createHash('sha256').update(data).digest('hex');
const expected = { sourceHead: 'a'.repeat(40), inputsSha256: 'b'.repeat(64), runId: '123' };
function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'browser-assets-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  for (const layout of ['families', 'combined']) {
    const manifest = { schema: 1, layout, ...expected, assets: {} };
    for (const spec of assetSpecs(layout)) {
      manifest.assets[spec.key] = {};
      for (const extension of ['.js', '_bg.wasm']) {
        const data = Buffer.from(`${spec.key}${extension}`);
        const path = join(directory, spec.stem + extension);
        mkdirSync(dirname(path), { recursive: true });
        writeFileSync(path, data);
        manifest.assets[spec.key][extension] = { bytes: data.length, sha256: hash(data) };
      }
    }
    writeFileSync(join(directory, `manifest-${layout}.json`), JSON.stringify(manifest));
  }
  return { directory, assets: readBrowserInputs(directory, expected) };
}
function samplesFor(assets) {
  return assets.flatMap((asset) => [0, 1, 2].map((sample) => ({
    key: asset.key, sample, realm: 'DedicatedWorkerGlobalScope', secureContext: true,
    userAgent: 'HeadlessChrome/123', browser: 'Chrome/123', jsVersion: '12.3', chromeLaunchMs: 20, bootstrapMs: 2,
    loaderImportMs: 3, initializeMs: 4, moduleReadyMs: 7, roundTripMs: 10,
    wasmRequests: [{ path: `/sdk-rs/hosts/wasm/pkg/${asset.stem}_bg.wasm`, status: 200, sha256: asset.files['_bg.wasm'].sha256 }],
  })));
}

test('requires the complete authoritative web asset set with same-run provenance', (t) => {
  const { directory, assets } = fixture(t);
  assert.equal(assets.length, 29);
  for (const key of Object.keys(expected))
    assert.throws(() => readBrowserInputs(directory, { ...expected, [key]: 'wrong' }), new RegExp(key));
  const path = join(directory, 'manifest-families.json');
  const manifest = JSON.parse(readFileSync(path));
  delete manifest.assets.bybit;
  writeFileSync(path, JSON.stringify(manifest));
  assert.throws(() => readBrowserInputs(directory, expected), /Incomplete browser asset set/);
});

test('rejects corrupted and missing bytes before starting a browser', (t) => {
  const { directory } = fixture(t);
  const path = join(directory, 'web/safe_cex_wasm_bg.wasm');
  const original = readFileSync(path);
  writeFileSync(path, Buffer.alloc(original.length, 42));
  assert.throws(() => readBrowserInputs(directory, expected), /hash\/size mismatch/);
  rmSync(path);
  assert.throws(() => readBrowserInputs(directory, expected), /ENOENT/);
  assert.throws(() => archiveBrowserAssets({ env: {}, layout: 'combined', destination: directory }), /requires CI/);
});

test('measurement requires all 87 fresh-worker samples, not a partial successful table', (t) => {
  const { assets } = fixture(t);
  const rows = samplesFor(assets);
  validateMeasurements(assets, rows);
  assert.equal(measurementTable(assets, rows).split('\n').length, 32);
  assert.throws(() => measurementTable(assets, rows.slice(1)), /Incomplete browser sample set/);
  const duplicate = structuredClone(rows);
  duplicate[0].sample = 1;
  assert.throws(() => validateMeasurements(assets, duplicate), /Incomplete samples/);
  assert.throws(() => validateMeasurements(assets.slice(1), rows), /Incomplete measured asset set/);
});

test('rejects wrong realms, invalid timers and missing/wrong/duplicate WASM fetches', (t) => {
  const { assets } = fixture(t);
  const cases = [
    (r) => { r.realm = 'Window'; },
    (r) => { r.secureContext = false; },
    (r) => { r.userAgent = 'Node'; },
    (r) => { delete r.browser; },
    (r) => { delete r.jsVersion; },
    (r) => { r.moduleReadyMs = NaN; },
    (r) => { r.roundTripMs = -1; },
    (r) => { r.wasmRequests = []; },
    (r) => { r.wasmRequests[0].path = '/wrong.wasm'; },
    (r) => { r.wasmRequests[0].sha256 = 'wrong'; },
    (r) => { r.wasmRequests[0].status = 404; },
    (r) => { r.wasmRequests.push(r.wasmRequests[0]); },
  ];
  for (const corrupt of cases) {
    const rows = samplesFor(assets);
    corrupt(rows[0]);
    assert.throws(() => validateMeasurements(assets, rows));
  }
});

test('loopback server serves exact public allowlist with WASM MIME and no-store', async (t) => {
  const { assets } = fixture(t);
  const { server, requests } = browserServer(assets);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve) => server.close(resolve)));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const path = `/sdk-rs/hosts/wasm/pkg/${assets[0].stem}_bg.wasm`;
  const response = await fetch(origin + path);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('content-type'), 'application/wasm');
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.equal(hash(Buffer.from(await response.arrayBuffer())), assets[0].files['_bg.wasm'].sha256);
  assert.equal((await fetch(origin + '/sdk-rs/hosts/wasm/js/engine.web.mjs')).headers.get('content-type'), 'text/javascript');
  for (const path of ['/.env', '/package.json', '/sdk-rs/Cargo.toml', '/%2e%2e/%2e%2e/etc/passwd'])
    assert.equal((await fetch(origin + path)).status, 404);
  assert.equal((await fetch(origin + path, { method: 'POST' })).status, 404);
  assert.equal(requests[0].sha256, assets[0].files['_bg.wasm'].sha256);
});

test('Chrome debugging metadata rejects nonlocal/malformed ports and paths', () => {
  assert.equal(debuggingPort('12345\n/devtools/browser/test-123\n'), 12345);
  for (const value of ['0\n/devtools/browser/id', '65536\n/devtools/browser/id', '123\nhttp://remote', '123\n/devtools/browser/id\nextra'])
    assert.throws(() => debuggingPort(value), /Invalid DevToolsActivePort/);
});

test('Chrome cleanup stops surviving children even after parent exit', async () => {
  for (const exitCode of [0, null]) {
    const child = Object.assign(new EventEmitter(), { pid: 1234, exitCode, signalCode: null });
    const signals = [];
    let disposed = false;
    await stopChrome(child, {
      send: async (method) => { assert.equal(method, 'Browser.close'); throw new Error('Socket closed during shutdown'); },
      close: () => { disposed = true; },
    }, (pid, signal) => {
      signals.push([pid, signal]);
      child.exitCode = 0;
      child.emit('exit');
    });
    assert.equal(disposed, true);
    assert.deepEqual(signals, [[-1234, 'SIGTERM'], [-1234, 'SIGKILL']]);
  }
  await stopChrome({ pid: undefined, exitCode: null, signalCode: null, once() {} }, undefined, () => assert.fail('No spawned group'));
  await stopChrome({ pid: 1234, exitCode: 0, signalCode: null }, undefined, () => { throw Object.assign(new Error('gone'), { code: 'ESRCH' }); });
});

class Socket extends EventTarget {
  readyState = 1;
  sent = [];
  send(data) { this.sent.push(JSON.parse(data)); }
  message(value) { this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(value) })); }
  close() { this.dispatchEvent(new Event('close')); }
}

test('CDP correlates out-of-order replies while ignoring events', async () => {
  const socket = new Socket();
  const cdp = new Cdp(socket);
  try {
    const one = cdp.send('one');
    const two = cdp.send('two');
    await Promise.resolve();
    socket.message({ method: 'event', params: {} });
    socket.message({ id: 2, result: { value: 'two' } });
    socket.message({ id: 1, result: { value: 'one' } });
    assert.deepEqual(await Promise.all([one, two]), [{ value: 'one' }, { value: 'two' }]);
  } finally { cdp.close(); }
});

test('CDP rejects protocol errors, malformed data and closed sockets', async () => {
  for (const action of [
    (s) => s.message({ id: 1, error: { message: 'bad method' } }),
    (s) => s.message(null),
    (s) => s.message([]),
    (s) => s.dispatchEvent(new MessageEvent('message', { data: '{' })),
    (s) => s.close(),
    (s) => s.dispatchEvent(new Event('error')),
  ]) {
    const socket = new Socket();
    const cdp = new Cdp(socket);
    const request = cdp.send('test');
    const rejected = assert.rejects(request, /CDP/);
    await Promise.resolve();
    action(socket);
    await rejected;
    cdp.close();
  }
});

test('CDP bounds connection and command waits and clears pending requests', async () => {
  for (const readyState of [0, 1]) {
    const socket = new Socket();
    socket.readyState = readyState;
    const cdp = new Cdp(socket, 10);
    await assert.rejects(cdp.send('never'), /timed out/);
    assert.equal(cdp.pending.size, 0);
    cdp.close();
  }
});

test('worker evaluation rejects browser promise exceptions or missing values', async () => {
  for (const response of [{ exceptionDetails: { text: 'worker error' } }, {}, { result: {} }])
    await assert.rejects(evaluateWorker({ send: async () => response }, 'families', 'bybit'), /Browser/);
  assert.deepEqual(await evaluateWorker({ send: async () => ({ result: { value: { moduleReadyMs: 5 } } }) }, 'families', 'bybit'), { moduleReadyMs: 5 });
});

function completionFixture(t) {
  const { directory, assets } = fixture(t);
  const report = {
    ...expected, status: 'running', errors: [], samples: samplesFor(assets),
    platform: 'linux', arch: 'x64', cpu: 'test CPU', image: 'test image',
    imageOS: 'ubuntu24', node: 'v22.0.0',
  };
  const logs = [];
  return { directory, assets, report, logs, complete: () => completeMeasurements(assets, report, directory, (line) => logs.push(line)) };
}
const setSampleModuleMs = (row, ms) => {
  row.moduleReadyMs = ms;
  row.initializeMs = ms - row.loaderImportMs;
  row.roundTripMs = ms + row.bootstrapMs + 1;
};
const setModuleMs = (report, key, ms) => {
  for (const row of report.samples) if (row.key === key) setSampleModuleMs(row, ms);
};

test('slow complete reports publish median warnings without failing or losing samples', (t) => {
  const f = completionFixture(t);
  setModuleMs(f.report, 'bybit', 76);
  setModuleMs(f.report, 'combined', 126);
  const samples = structuredClone(f.report.samples);
  f.complete();
  assert.equal(f.report.status, 'complete');
  assert.deepEqual(f.report.errors, []);
  assert.deepEqual(f.report.samples, samples);
  assert.deepEqual(f.report.warnings.map(({ key, medianMs, thresholdMs }) => ({ key, medianMs, thresholdMs })), [
    { key: 'bybit', medianMs: 76, thresholdMs: 75 },
    { key: 'combined', medianMs: 126, thresholdMs: 125 },
  ]);
  assert.equal(readFileSync(join(f.directory, 'table.md'), 'utf8'), measurementTable(f.assets, samples));
  assert.equal(f.logs.filter((line) => line.startsWith('::warning ')).length, 2);
  assert.ok(f.logs.includes(measurementTable(f.assets, samples)));
  assert.deepEqual(JSON.parse(JSON.stringify(f.report.warnings)), f.report.warnings);
});

test('warning thresholds use unrounded medians and a separate combined allowance', (t) => {
  for (const [family, combined, keys] of [
    [75, 125, []],
    [75.00001, 125.00001, ['bybit', 'combined']],
    [80, 80, ['bybit']],
  ]) {
    const f = completionFixture(t);
    setModuleMs(f.report, 'bybit', family);
    setModuleMs(f.report, 'combined', combined);
    f.complete();
    assert.deepEqual(f.report.warnings.map((w) => w.key), keys);
    assert.equal(f.report.status, 'complete');
  }
});

test('an isolated slow sample is retained without a median warning', (t) => {
  const f = completionFixture(t);
  setSampleModuleMs(f.report.samples.find((row) => row.key === 'bybit'), 159.2);
  f.complete();
  assert.deepEqual(f.report.warnings, []);
  assert.equal(f.report.samples.find((row) => row.key === 'bybit').moduleReadyMs, 159.2);
  assert.equal(f.logs.filter((line) => line.startsWith('::warning ')).length, 0);
});

test('warning records and safe annotations retain the runner and every observed Chrome/JS cohort', (t) => {
  const f = completionFixture(t);
  setModuleMs(f.report, 'bybit', 100);
  const changed = f.report.samples.find((row) => row.key === 'bybit');
  changed.browser = 'Chrome/124';
  changed.jsVersion = '12.4';
  f.report.cpu = 'CPU%0A\r\n::error::injected';
  f.complete();
  const warning = f.report.warnings[0];
  assert.equal(warning.sourceHead, expected.sourceHead);
  assert.equal(warning.runId, expected.runId);
  assert.deepEqual(warning.cohort, {
    platform: 'linux', arch: 'x64', cpu: f.report.cpu, image: 'test image',
    imageOS: 'ubuntu24', node: 'v22.0.0', runtimes: [
      { browser: 'Chrome/124', jsVersion: '12.4' },
      { browser: 'Chrome/123', jsVersion: '12.3' },
    ],
  });
  const annotations = f.logs.filter((line) => line.startsWith('::warning '));
  assert.equal(annotations.length, 1);
  const prefix = '::warning title=WASM module readiness::';
  assert.ok(annotations[0].startsWith(prefix));
  assert.doesNotMatch(annotations[0], /[\r\n]/);
  assert.ok(annotations[0].includes('%250A'));
  const data = annotations[0].slice(prefix.length).replaceAll('%0D', '\r').replaceAll('%0A', '\n').replaceAll('%25', '%');
  assert.deepEqual(JSON.parse(data), warning);
});

test('completion rejects existing errors and invalid samples before publishing success or warnings', (t) => {
  for (const corrupt of [
    (r) => { r.errors.push('worker failed'); },
    (r) => { r.samples.pop(); },
    (r) => { r.samples[0].realm = 'Window'; },
    (r) => { r.samples[0].moduleReadyMs = Infinity; },
    (r) => { r.samples[0].wasmRequests[0].sha256 = 'wrong'; },
  ]) {
    const f = completionFixture(t);
    setModuleMs(f.report, 'bybit', 100);
    corrupt(f.report);
    const errors = [...f.report.errors];
    assert.throws(f.complete);
    assert.deepEqual(f.report.errors, errors);
    assert.notEqual(f.report.status, 'complete');
    assert.equal(f.report.warnings, undefined);
    assert.equal(existsSync(join(f.directory, 'table.md')), false);
    assert.deepEqual(f.logs, []);
  }
});

function launchFixture() {
  const child = Object.assign(new EventEmitter(), {
    pid: 1234, exitCode: null, signalCode: null, spawnfile: 'fixture-chrome',
  });
  let clock = 0, reads = 0;
  const fixture = {
    child,
    options: {
      now: () => clock,
      wait: async (ms) => { clock += ms; fixture.tick(clock); },
      readPort: () => { reads++; return fixture.read(); },
    },
    tick: () => {},
    read: () => { throw Object.assign(new Error('not ready'), { code: 'ENOENT' }); },
    get clock() { return clock; },
    get reads() { return reads; },
  };
  return fixture;
}
const assertLaunchListenersRemoved = (child) => {
  for (const name of ['spawn', 'exit', 'error']) assert.equal(child.listenerCount(name), 0);
};

test('Chrome launch waits for spawn and complete metadata under the original 15s bound', async () => {
  const f = launchFixture();
  f.tick = () => f.child.emit('spawn');
  f.read = () => f.clock < 200 ? '12345\n' : '12345\n/devtools/browser/fixture\n';
  assert.equal(await waitForChrome(f.child, '/fixture', f.options), 12345);
  assert.equal(f.clock, 200);
  assert.equal(f.reads, 2);
  assertLaunchListenersRemoved(f.child);
});

test('Chrome spawn failures and normal/signaled exits fail before readiness with diagnostics', async () => {
  for (const kind of ['error', 'exit', 'signal']) {
    const f = launchFixture();
    f.tick = () => {
      if (kind === 'error') f.child.emit('error', new Error('fixture spawn refusal'));
      else {
        f.child.exitCode = kind === 'exit' ? 7 : null;
        f.child.signalCode = kind === 'signal' ? 'SIGKILL' : null;
        f.child.emit('exit', f.child.exitCode, f.child.signalCode);
      }
    };
    await assert.rejects(waitForChrome(f.child, '/fixture', f.options), (error) => {
      assert.match(error.message, kind === 'error' ? /spawn failed/ : /exited before debugging readiness/);
      assert.equal(error.launch.phase, 'spawn');
      assert.equal(error.launch.executable, 'fixture-chrome');
      assert.equal(error.launch.pid, 1234);
      assert.equal(error.launch.elapsedMs, 100);
      assert.equal(error.launch.signal, kind === 'signal' ? 'SIGKILL' : null);
      assert.equal(error.launch.exitCode, kind === 'exit' ? 7 : null);
      return true;
    });
    assert.equal(f.reads, 0);
    assertLaunchListenersRemoved(f.child);
  }
});

test('Chrome termination wins over concurrently published ready metadata', async () => {
  const f = launchFixture();
  f.tick = () => f.child.emit('spawn');
  f.read = () => {
    f.child.signalCode = 'SIGTERM';
    f.child.emit('exit', null, 'SIGTERM');
    return '12345\n/devtools/browser/fixture\n';
  };
  await assert.rejects(waitForChrome(f.child, '/fixture', f.options), (error) => {
    assert.match(error.message, /exited before debugging readiness/);
    assert.equal(error.launch.phase, 'devtools-ready');
    assert.equal(error.launch.signal, 'SIGTERM');
    return true;
  });
  assertLaunchListenersRemoved(f.child);
});

test('Chrome spawn/missing/partial readiness cannot pass at or after 15 seconds', async () => {
  for (const kind of ['spawn', 'missing', 'partial', 'equality']) {
    const f = launchFixture();
    f.tick = () => { if (kind !== 'spawn') f.child.emit('spawn'); };
    if (kind === 'partial') f.read = () => '12345\n';
    if (kind === 'equality') f.read = () => f.clock >= 15_000
      ? '12345\n/devtools/browser/fixture\n' : '';
    await assert.rejects(waitForChrome(f.child, '/fixture', f.options), (error) => {
      assert.match(error.message, /debugging port timed out/);
      assert.equal(error.launch.elapsedMs, 15_000);
      assert.equal(error.launch.phase, kind === 'spawn' ? 'spawn' : 'devtools-ready');
      assert.equal(error.launch.portFile, ['partial', 'equality'].includes(kind) ? 'partial' : 'missing');
      return true;
    });
    assert.equal(f.clock, 15_000);
    assertLaunchListenersRemoved(f.child);
  }
});

test('Chrome metadata and filesystem refusals stay fail closed with launch diagnostics', async () => {
  for (const kind of ['metadata', 'filesystem']) {
    const f = launchFixture();
    f.tick = () => f.child.emit('spawn');
    f.read = () => {
      if (kind === 'filesystem') throw Object.assign(new Error('fixture denied'), { code: 'EACCES' });
      return '12345\nhttp://remote';
    };
    await assert.rejects(waitForChrome(f.child, '/fixture', f.options), (error) => {
      assert.match(error.message, kind === 'metadata' ? /Invalid DevToolsActivePort/ : /Cannot read Chrome debugging metadata/);
      assert.equal(error.launch.phase, 'devtools-ready');
      assert.equal(error.launch.elapsedMs, 100);
      return true;
    });
    assertLaunchListenersRemoved(f.child);
  }
});
