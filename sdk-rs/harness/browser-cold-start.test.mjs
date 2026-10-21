import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import {
  archiveBrowserAssets, assetSpecs, browserServer, debuggingPort, measurementTable,
  readBrowserInputs, validateMeasurements,
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
    userAgent: 'HeadlessChrome/123', chromeLaunchMs: 20, bootstrapMs: 2,
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
