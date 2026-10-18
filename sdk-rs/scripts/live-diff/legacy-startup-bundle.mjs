// Adapt recorded startup bytes to the canonical per-dial drain harness. No live I/O.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { serialize } from 'node:v8';
import vm from 'node:vm';
import { ts } from '../../safe-cex-store/tests/source_loader.mjs';
import { hash, readKey } from './recorder.mjs';
import { streamKey } from './stream-relay.mjs';
const [reportPath, outputPath] = process.argv.slice(2);
assert(reportPath && outputPath, 'usage: legacy-startup-bundle.mjs REPORT.json OUTPUT.bundle.v8');
const report = JSON.parse(readFileSync(reportPath, 'utf8'));
assert(['lighter', 'apexomni'].includes(report.exchange), 'explicit startup venues only');
const requests = report.requests.filter((r) => r.implementation === 'ts' && r.fixture);
const ws = requests.filter((r) => r.fixture.endsWith('.ws.json'));
assert.equal(ws.length, 1, 'legacy capture contains one literal physical socket');
const socket = ws[0];
let url = new URL(socket.path, `wss://${socket.host}`).href;
let socketQuerySource;
if (report.exchange === 'apexomni') {
  // The legacy recorder retained pathname only. Recover its literal stable query
  // from the maintained declaration; streamKey still removes runtime timestamps.
  const sourceUrl = new URL('../../../packages/safe-cex/src/exchanges/apexomni/apexomni.types.ts', import.meta.url);
  const source = readFileSync(sourceUrl, 'utf8');
  const ast = ts.createSourceFile(sourceUrl.pathname, source, ts.ScriptTarget.Latest, true);
  const declaration = ast.statements
    .filter(ts.isVariableStatement)
    .flatMap((s) => [...s.declarationList.declarations])
    .find((d) => d.name.getText(ast) === 'BASE_WS_URL');
  assert(declaration?.initializer, 'maintained Apex BASE_WS_URL declaration');
  const actual = vm.runInNewContext('(' + declaration.initializer.getText(ast) + ')').public.linear.livenet;
  const parsed = new URL(actual);
  assert.equal(parsed.hostname, socket.host);
  assert.equal(parsed.pathname, socket.path);
  url = actual;
  socketQuerySource = { file: sourceUrl.pathname, hash: hash(source), owner: 'BASE_WS_URL.public.linear.livenet' };
}
const framesBytes = readFileSync(socket.fixture);
const legacy = JSON.parse(framesBytes);
assert.equal(hash(JSON.stringify(legacy)), socket.frameHash, 'exact recorded WebSocket frame sequence');
assert(Array.isArray(legacy) && legacy.length);
const tape = {
  connections: [{ id: 0, key: streamKey(url), occurrence: 0, url, openedAtMs: 0 }],
  frames: legacy.map((f) => {
    assert(typeof f.base64 === 'string' && typeof f.binary === 'boolean');
    assert(Number.isFinite(f.afterMs) && f.afterMs >= 0);
    return { connection: 0, atMs: f.afterMs, kind: 'message', base64: f.base64, binary: f.binary };
  }),
  durationMs: Math.max(...legacy.map((f) => f.afterMs)),
};
const provenance = [];
const rest = requests
  .filter((r) => !r.fixture.endsWith('.ws.json'))
  .map((r) => {
    const bytes = readFileSync(r.fixture);
    assert.equal(hash(bytes), r.responseHash ?? r.liveResponseHash, 'exact recorded HTTP bytes');
    provenance.push({ method: r.method, host: r.host, path: r.path, fixture: r.fixture, hash: hash(bytes) });
    return [
      readKey(r.method, new URL(r.path, `https://${r.host}`), ''),
      {
        status: r.status,
        headers: { 'content-type': 'application/json' },
        bytes,
      },
    ];
  });
assert.equal(new Set(rest.map(([key]) => key)).size, rest.length);
const bundle = {
  exchange: report.exchange,
  tape,
  symbol: 'BTC',
  candleTo: Date.parse(report.at),
  rest,
  capture: {
    windowMs: tape.durationMs,
    frames: legacy.length,
    connections: 1,
    legacyReport: resolve(reportPath),
    legacyFrames: socket.fixture,
    legacyFramesHash: hash(framesBytes),
    socketQuerySource,
    rest: provenance,
    timing: 'Recorded relative frame times preserved; single captured socket only; no live-window claim.',
  },
};
writeFileSync(outputPath, serialize(bundle), { mode: 0o600 });
console.log(
  JSON.stringify({
    output: resolve(outputPath),
    exchange: report.exchange,
    frames: legacy.length,
    rest: rest.length,
    hash: hash(readFileSync(outputPath)),
  }),
);
