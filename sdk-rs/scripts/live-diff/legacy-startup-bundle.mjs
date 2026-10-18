// Adapt recorded startup bytes to the canonical per-dial drain harness. No live I/O.
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {serialize} from 'node:v8';
import {hash, readKey} from './recorder.mjs';
import {streamKey} from './stream-relay.mjs';
const [reportPath, outputPath] = process.argv.slice(2);
assert(reportPath && outputPath, 'usage: legacy-startup-bundle.mjs REPORT.json OUTPUT.bundle.v8');
const report = JSON.parse(readFileSync(reportPath, 'utf8'));
assert(['lighter','apexomni'].includes(report.exchange), 'explicit startup venues only');
const requests = report.requests.filter(r => r.implementation === 'ts' && r.fixture);
const ws = requests.filter(r => r.fixture.endsWith('.ws.json'));
assert.equal(ws.length, 1, 'legacy capture contains one literal physical socket');
const socket = ws[0];
const url = new URL(socket.path, `wss://${socket.host}`).href;
const framesBytes = readFileSync(socket.fixture);
const legacy = JSON.parse(framesBytes);
assert(Array.isArray(legacy) && legacy.length);
const tape = {
  connections:[{id:0, key:streamKey(url), occurrence:0, url, openedAtMs:0}],
  frames:legacy.map(f => {
    assert(typeof f.base64 === 'string' && typeof f.binary === 'boolean');
    assert(Number.isFinite(f.afterMs) && f.afterMs >= 0);
    return {connection:0, atMs:f.afterMs, kind:'message', base64:f.base64, binary:f.binary};
  }),
  durationMs:Math.max(...legacy.map(f => f.afterMs)),
};
const provenance = [];
const rest = requests.filter(r => !r.fixture.endsWith('.ws.json')).map(r => {
  const bytes = readFileSync(r.fixture);
  assert.equal(hash(bytes), r.responseHash ?? r.liveResponseHash, 'exact recorded HTTP bytes');
  provenance.push({method:r.method,host:r.host,path:r.path,fixture:r.fixture,hash:hash(bytes)});
  return [readKey(r.method, new URL(r.path, `https://${r.host}`), ''), {
    status:r.status, headers:{'content-type':'application/json'}, bytes,
  }];
});
assert.equal(new Set(rest.map(([key]) => key)).size, rest.length);
const bundle = {
  exchange:report.exchange, tape, symbol:'BTC', candleTo:Date.parse(report.at), rest,
  capture:{windowMs:tape.durationMs, frames:legacy.length, connections:1,
    legacyReport:resolve(reportPath), legacyFrames:socket.fixture, legacyFramesHash:hash(framesBytes),
    rest:provenance, timing:'Recorded relative frame times preserved; single captured socket only; no live-window claim.'},
};
writeFileSync(outputPath, serialize(bundle), {mode:0o600});
console.log(JSON.stringify({output:resolve(outputPath), exchange:report.exchange, frames:legacy.length, rest:rest.length, hash:hash(readFileSync(outputPath))}));
