// CI-only observations of actual Chrome worker module readiness, never a timing gate.
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { arch, cpus, platform, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { venueFamilies } from '../hosts/wasm/js/venues.mjs';
import { buildInputFingerprint } from './ci-wasm.mjs';
import { Cdp, evaluateWorker } from './browser-cdp.mjs';

export const SAMPLE_COUNT = 3;
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const requireValue = (value, message) => { if (!value) throw new Error(message); };
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const sha = (root) => {
  const result = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' });
  requireValue(result.status === 0, 'Cannot identify source commit');
  return result.stdout.trim();
};
const requireCi = (env) => requireValue(env.GITHUB_ACTIONS === 'true' && /^\d+$/.test(env.GITHUB_RUN_ID ?? ''), 'Browser measurement requires CI');
export const assetSpecs = (layout) => {
  requireValue(['families', 'combined'].includes(layout), 'Invalid asset layout');
  return (layout === 'families' ? venueFamilies : ['combined']).map((key) => ({
    key, layout, stem: `web/${layout === 'families' ? `${key}/` : ''}safe_cex_wasm`,
  }));
};

export function archiveBrowserAssets({ root = process.cwd(), layout, destination, env = process.env }) {
  requireCi(env);
  const assets = {};
  const metadata = { schema: 1, layout, sourceHead: sha(root), runId: env.GITHUB_RUN_ID, inputsSha256: buildInputFingerprint(root), assets };
  for (const spec of assetSpecs(layout)) {
    const bytes = {};
    for (const extension of ['.js', '_bg.wasm']) {
      const relative = spec.stem + extension;
      const data = readFileSync(join(root, 'sdk-rs/hosts/wasm/pkg', relative));
      mkdirSync(dirname(join(destination, relative)), { recursive: true });
      writeFileSync(join(destination, relative), data);
      bytes[extension] = { sha256: hash(data), bytes: data.length };
    }
    assets[spec.key] = bytes;
  }
  writeFileSync(join(destination, `manifest-${layout}.json`), `${JSON.stringify(metadata, null, 2)}\n`);
  return metadata;
}

export function readBrowserInputs(directory, expected) {
  const assets = [];
  for (const layout of ['families', 'combined']) {
    const manifest = JSON.parse(readFileSync(join(directory, `manifest-${layout}.json`), 'utf8'));
    requireValue(manifest.schema === 1 && manifest.layout === layout, 'Invalid browser manifest');
    for (const key of ['sourceHead', 'runId', 'inputsSha256']) requireValue(manifest[key] === expected[key], `Browser manifest ${key} mismatch`);
    const specs = assetSpecs(layout);
    requireValue(JSON.stringify(Object.keys(manifest.assets ?? {}).sort()) === JSON.stringify(specs.map((s) => s.key).sort()), 'Incomplete browser asset set');
    for (const spec of specs) {
      const files = {};
      for (const extension of ['.js', '_bg.wasm']) {
        const data = readFileSync(join(directory, spec.stem + extension));
        const recorded = manifest.assets[spec.key][extension];
        requireValue(data.length > 0 && recorded?.bytes === data.length && recorded.sha256 === hash(data), `Browser asset hash/size mismatch: ${spec.key}${extension}`);
        files[extension] = { data, ...recorded };
      }
      assets.push({ ...spec, files });
    }
  }
  return assets;
}

export function browserServer(assets, root = process.cwd()) {
  const routes = new Map();
  const add = (path, data, type) => routes.set(path, { data, type, sha256: hash(data) });
  add('/', Buffer.from('<!doctype html><title>WASM measurement</title>'), 'text/html');
  add('/browser-worker.mjs', readFileSync(join(root, 'sdk-rs/harness/browser-cold-start-worker.mjs')), 'text/javascript');
  for (const name of ['engine.web.mjs', 'family-loader.mjs', 'venues.mjs'])
    add(`/sdk-rs/hosts/wasm/js/${name}`, readFileSync(join(root, 'sdk-rs/hosts/wasm/js', name)), 'text/javascript');
  for (const asset of assets) for (const extension of ['.js', '_bg.wasm'])
    add(`/sdk-rs/hosts/wasm/pkg/${asset.stem}${extension}`, asset.files[extension].data, extension === '.js' ? 'text/javascript' : 'application/wasm');
  const requests = [];
  const server = createServer((request, response) => {
    const path = new URL(request.url, 'http://127.0.0.1').pathname;
    const route = request.method === 'GET' ? routes.get(path) : undefined;
    requests.push({ path, status: route ? 200 : 404, ...(route ? { sha256: route.sha256 } : {}) });
    response.writeHead(route ? 200 : 404, { 'Content-Type': route?.type ?? 'text/plain', 'Cache-Control': 'no-store' });
    response.end(route?.data ?? 'Not found');
  });
  return { server, requests };
}

export function validateMeasurements(assets, samples) {
  const keys = [...venueFamilies, 'combined'];
  requireValue(assets.length === keys.length && keys.every((key) => assets.filter((a) => a.key === key).length === 1), 'Incomplete measured asset set');
  requireValue(samples.length === keys.length * SAMPLE_COUNT, 'Incomplete browser sample set');
  for (const asset of assets) {
    const rows = samples.filter((s) => s.key === asset.key);
    requireValue(rows.length === SAMPLE_COUNT && new Set(rows.map((s) => s.sample)).size === SAMPLE_COUNT && rows.every((s) => Number.isInteger(s.sample) && s.sample >= 0 && s.sample < SAMPLE_COUNT), `Incomplete samples for ${asset.key}`);
    for (const row of rows) {
      requireValue(row.realm === 'DedicatedWorkerGlobalScope' && row.secureContext === true && typeof row.userAgent === 'string' && row.userAgent.includes('Chrome/'), 'Measurement did not run in a secure Chrome worker');
      requireValue(typeof row.browser === 'string' && row.browser.includes('Chrome/') && typeof row.jsVersion === 'string' && row.jsVersion.length > 0, 'Missing browser version provenance');
      for (const key of ['chromeLaunchMs', 'bootstrapMs', 'loaderImportMs', 'initializeMs', 'moduleReadyMs', 'roundTripMs'])
        requireValue(Number.isFinite(row[key]) && row[key] >= 0, `Invalid measurement ${asset.key}.${key}`);
      const expected = `/sdk-rs/hosts/wasm/pkg/${asset.stem}_bg.wasm`;
      requireValue(row.wasmRequests?.length === 1 && row.wasmRequests[0].path === expected && row.wasmRequests[0].status === 200 && row.wasmRequests[0].sha256 === asset.files['_bg.wasm'].sha256, `Wrong WASM fetched for ${asset.key}`);
    }
  }
}

const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
export function measurementTable(assets, samples) {
  validateMeasurements(assets, samples);
  const lines = ['| Artifact | WASM bytes | Samples | Worker bootstrap ms | Module ready ms | End-to-end ms |', '| --- | ---: | ---: | ---: | ---: | ---: |'];
  for (const asset of assets) {
    const rows = samples.filter((s) => s.key === asset.key);
    const ms = (key) => median(rows.map((r) => r[key])).toFixed(2);
    lines.push(`| ${asset.key} | ${asset.files['_bg.wasm'].bytes} | ${rows.length} | ${ms('bootstrapMs')} | ${ms('moduleReadyMs')} | ${ms('roundTripMs')} |`);
  }
  return lines.join('\n') + '\n';
}

export function completeMeasurements(assets, report, out, log = console.log) {
  requireValue(Array.isArray(report.errors) && report.errors.length === 0, 'Cannot complete a browser report with errors');
  const table = measurementTable(assets, report.samples);
  report.warnings = assets.flatMap((asset) => {
    const rows = report.samples.filter((row) => row.key === asset.key);
    const medianMs = median(rows.map((row) => row.moduleReadyMs));
    const thresholdMs = asset.key === 'combined' ? 125 : 75;
    if (medianMs <= thresholdMs) return [];
    const runtimes = [...new Map(rows.map(({ browser, jsVersion }) => [
      JSON.stringify([browser, jsVersion]), { browser, jsVersion },
    ])).values()];
    const cohort = Object.fromEntries(['platform', 'arch', 'cpu', 'image', 'imageOS', 'node']
      .map((key) => [key, report[key] ?? 'unknown']));
    return [{ key: asset.key, medianMs, thresholdMs, sourceHead: report.sourceHead, runId: report.runId, cohort: { ...cohort, runtimes } }];
  });
  writeFileSync(join(out, 'table.md'), table);
  report.status = 'complete';
  log(table);
  for (const warning of report.warnings) {
    const data = JSON.stringify(warning).replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A');
    log(`::warning title=WASM module readiness::${data}`);
  }
}

export function debuggingPort(text) {
  const [port, path, ...extra] = text.trim().split('\n');
  requireValue(/^\d+$/.test(port) && Number(port) > 0 && Number(port) <= 65535 && /^\/devtools\/browser\/[\w-]+$/.test(path) && !extra.length, 'Invalid DevToolsActivePort');
  return Number(port);
}

const chromeState = (child, phase) => ({
  phase, executable: child.spawnfile, pid: child.pid ?? null,
  exitCode: child.exitCode, signal: child.signalCode,
});

export async function waitForChrome(child, profile, {
  now = () => performance.now(), wait = pause,
  readPort = () => readFileSync(join(profile, 'DevToolsActivePort'), 'utf8'),
} = {}) {
  const started = now(), deadline = started + 15_000;
  let spawned = false, ended, failure, portFile = 'missing';
  const onSpawn = () => { spawned = true; };
  const onExit = (code, signal) => { ended = { code, signal }; };
  const onError = (error) => { failure = error; };
  child.on('spawn', onSpawn).on('exit', onExit).on('error', onError);
  const refused = (message, cause) => Object.assign(new Error(message, { cause }), {
    launch: { ...chromeState(child, spawned ? 'devtools-ready' : 'spawn'),
      exitCode: ended?.code ?? child.exitCode, signal: ended?.signal ?? child.signalCode,
      elapsedMs: now() - started, portFile },
  });
  const check = () => {
    if (failure) throw refused(`Chrome spawn failed: ${failure.message}`, failure);
    if (ended || child.exitCode !== null || child.signalCode !== null)
      throw refused('Chrome exited before debugging readiness');
    if (now() >= deadline) throw refused('Chrome debugging port timed out');
  };
  try {
    for (;;) {
      check();
      if (spawned) {
        let text;
        try { text = readPort(); } catch (error) {
          if (error.code !== 'ENOENT') throw refused('Cannot read Chrome debugging metadata', error);
        }
        if (text !== undefined) {
          portFile = 'partial';
          if (text.trim().split('\n').length >= 2) {
            let port;
            try { port = debuggingPort(text); }
            catch (error) { portFile = 'invalid'; throw refused(error.message, error); }
            portFile = 'ready';
            check();
            return port;
          }
        }
      }
      await wait(Math.min(100, deadline - now()));
    }
  } finally {
    child.off('spawn', onSpawn).off('exit', onExit).off('error', onError);
  }
}

export async function stopChrome(child, cdp, signalGroup = process.kill) {
  // Chrome's parent may exit before its renderers/zygotes stop writing. Every
  // launch owns a separate POSIX process group; close gracefully, then reap the
  // entire group even if the parent has already exited.
  const exited = child.exitCode !== null || child.signalCode !== null
    ? Promise.resolve() : new Promise((resolve) => child.once('exit', resolve));
  try {
    if (cdp && !cdp.closed) await Promise.race([cdp.send('Browser.close').catch(() => {}), pause(1000)]);
  } finally { cdp?.close(); }
  if (!child.pid) return; // spawn failure created no process/group.
  const signal = (name) => {
    try { signalGroup(-child.pid, name); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
  };
  signal('SIGTERM');
  await Promise.race([exited, pause(1000)]);
  signal('SIGKILL');
  await Promise.race([exited, pause(1000)]);
}

async function sampleChrome(origin, asset) {
  const profile = mkdtempSync(join(tmpdir(), 'wasm-browser-cold-'));
  const start = performance.now();
  const child = spawn(process.env.CHROME_BIN || 'google-chrome', [
    '--headless=new', '--no-sandbox', '--disable-dev-shm-usage', '--disable-background-networking',
    '--no-first-run', '--no-default-browser-check', '--remote-debugging-address=127.0.0.1',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
  ], { detached: true, stdio: ['ignore', 'ignore', 'pipe'] });
  let stderr = '';
  let phase = 'spawn';
  let cdp;
  child.stderr.on('data', (data) => { stderr = (stderr + data).slice(-4000); });
  try {
    const port = await waitForChrome(child, profile);
    phase = 'target';
    const target = await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(origin)}`, { method: 'PUT', signal: AbortSignal.timeout(30_000) });
    requireValue(target.ok, 'Cannot create Chrome target');
    const page = await target.json();
    requireValue(typeof page.webSocketDebuggerUrl === 'string' && page.webSocketDebuggerUrl.startsWith(`ws://127.0.0.1:${port}/`), 'Invalid Chrome target socket');
    cdp = new Cdp(new WebSocket(page.webSocketDebuggerUrl));
    await cdp.send('Page.enable');
    phase = 'navigation';
    // Ensure same-origin navigation has committed before constructing the worker.
    for (let attempt = 0; attempt < 100; attempt++) {
      const state = await cdp.send('Runtime.evaluate', { expression: 'location.origin', returnByValue: true });
      if (state.result?.value === origin) break;
      requireValue(attempt < 99, 'Browser navigation timed out');
      await pause(50);
    }
    const version = await cdp.send('Browser.getVersion');
    const chromeLaunchMs = performance.now() - start;
    phase = 'worker';
    const measurement = await evaluateWorker(cdp, asset.layout, asset.key === 'combined' ? 'bybit' : asset.key);
    return { ...measurement, chromeLaunchMs, browser: version.product, jsVersion: version.jsVersion };
  } catch (error) {
    throw Object.assign(new Error(`${asset.key}: ${error.message}\n${stderr}`, { cause: error }), {
      launch: error.launch ?? chromeState(child, phase),
    });
  }
  finally {
    await stopChrome(child, cdp);
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  }
}

export async function measureBrowser({ inputs, out, root = process.cwd(), env = process.env }) {
  mkdirSync(out, { recursive: true });
  const report = { schema: 1, status: 'running', samples: [], errors: [], startedAt: new Date().toISOString() };
  let server;
  const save = () => writeFileSync(join(out, 'raw.json'), `${JSON.stringify(report, null, 2)}\n`);
  try {
    requireCi(env);
    const provenance = { sourceHead: sha(root), inputsSha256: buildInputFingerprint(root), runId: env.GITHUB_RUN_ID };
    Object.assign(report, provenance, { platform: platform(), arch: arch(), cpu: cpus()[0]?.model, node: process.version, image: env.ImageVersion, imageOS: env.ImageOS, workerSha256: hash(readFileSync(join(root, 'sdk-rs/harness/browser-cold-start-worker.mjs'))) });
    const assets = readBrowserInputs(inputs, provenance);
    report.assets = assets.map(({ key, layout, files }) => ({ key, layout, hashes: Object.fromEntries(Object.entries(files).map(([ext, f]) => [ext, { sha256: f.sha256, bytes: f.bytes }])) }));
    const http = browserServer(assets, root);
    server = http.server;
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
    const origin = `http://127.0.0.1:${server.address().port}`;
    for (const asset of assets) for (let sample = 0; sample < SAMPLE_COUNT; sample++) {
      http.requests.length = 0;
      let measured;
      try { measured = await sampleChrome(origin, asset); }
      catch (error) { report.failedSample = { key: asset.key, sample, requests: [...http.requests], launch: error.launch }; throw error; }
      report.samples.push({ key: asset.key, sample, ...measured, wasmRequests: http.requests.filter((r) => r.path.endsWith('.wasm')) });
      save();
      console.log(`${asset.key} sample ${sample + 1}/${SAMPLE_COUNT}: ${measured.moduleReadyMs.toFixed(2)} ms`);
    }
    completeMeasurements(assets, report, out);
  } catch (error) { report.status = 'failed'; report.errors.push(error.message); throw error; }
  finally { save(); if (server) await new Promise((resolve) => server.close(resolve)); }
  return report;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { archive: { type: 'string' }, inputs: { type: 'string' }, out: { type: 'string' } } });
  requireValue(values.out && !!values.archive !== !!values.inputs, 'Expected --out and either --archive families|combined or --inputs DIRECTORY');
  if (values.archive) archiveBrowserAssets({ layout: values.archive, destination: resolve(values.out) });
  else await measureBrowser({ inputs: resolve(values.inputs), out: resolve(values.out) });
}
