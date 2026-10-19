// Reproducible fixture diagnostics, separate from the uninstrumented gates:
//   corepack yarn rust:build-profile okx bybit
//   corepack yarn rust:profile --venue okx --delivery single
//   corepack yarn rust:profile --venue bybit --delivery batch
// No-op probes estimate the string/byte crossing floor. IgnoredAny validates JSON
// without building venue state: its cost is NOT the full typed venue parser.
import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { parseArgs } from 'node:util';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const { values } = parseArgs({ options: {
  venue: { type: 'string', default: 'okx' },
  tape: { type: 'string' },
  books: { type: 'string' },
  delivery: { type: 'string', default: 'single' },
  cadence: { type: 'string', default: 'recorded' },
  runs: { type: 'string', default: '5' },
  'probe-rounds': { type: 'string', default: '5' },
  'batch-size': { type: 'string', default: '64' },
} });
const family = venueFamily(values.venue);
const positive = (name, value) => {
  const n = Number(value);
  if (!Number.isSafeInteger(n) || n < 1) throw new Error(`${name} must be a positive integer`);
  return n;
};
const runs = positive('runs', values.runs);
const rounds = positive('probe-rounds', values['probe-rounds']);
const batchSize = positive('batch-size', values['batch-size']);
if (!['single', 'batch'].includes(values.delivery)) throw new Error('delivery must be single or batch');
if (!['frame', 'recorded'].includes(values.cadence)) throw new Error('cadence must be frame or recorded');
const dir = fileURLToPath(new URL('.', import.meta.url));
const tape = resolve(values.tape ?? join(dir, 'fixtures', `${family}-public.jsonl.gz`));
const defaultBooks = {
  binance: 'BTCUSDT,ETHUSDT,SOLUSDT',
  bybit: 'BTCUSDT,ETHUSDT',
  okx: 'BTCUSDT,ETHUSDT',
  cryptocom: 'BTCUSD-PERP,ETHUSD-PERP',
};
const books = values.books ?? defaultBooks[family];
if (books === undefined) throw new Error(`Specify --books for ${family}`);
const compressed = readFileSync(tape);
const records = (tape.endsWith('.gz') ? gunzipSync(compressed) : compressed).toString('utf8').split('\n').filter(Boolean).map(JSON.parse);
const texts = records.filter((row) => row.type === 'ws').map((row) => row.text);
if (!texts.length) throw new Error('No websocket messages in fixture');
const require = createRequire(import.meta.url);
const probe = require(`../hosts/wasm/pkg/profile/${family}/safe_cex_wasm.js`);
if (!probe.profileAcceptText || !probe.profileAcceptBytes || !probe.profileValidateJson) {
  throw new Error('Build optional profiling exports with harness/build-profile.sh');
}
const median = (rows) => [...rows].sort((a, b) => a - b)[Math.floor(rows.length / 2)];
let sink = 0;
const measure = (fn) => {
  const samples = [];
  // Warm the same complete fixture once before collecting samples.
  fn();
  for (let run = 0; run < runs; run++) {
    globalThis.gc?.();
    const cpu = process.threadCpuUsage();
    const start = performance.now();
    for (let round = 0; round < rounds; round++) fn();
    const usage = process.threadCpuUsage(cpu);
    samples.push({ wallMs: (performance.now() - start) / rounds, cpuMs: (usage.user + usage.system) / 1000 / rounds });
  }
  return { medianWallMs: median(samples.map((s) => s.wallMs)), medianCpuMs: median(samples.map((s) => s.cpuMs)), samples };
};
const encoded = texts.map((text) => new TextEncoder().encode(text));
const batches = [];
for (let at = 0; at < encoded.length; at += batchSize) {
  const rows = encoded.slice(at, at + batchSize);
  // The production batch wire: socket u32, UTF-8 byte length u32, now f64 LE.
  const bytes = new Uint8Array(rows.reduce((n, row) => n + 16 + row.length, 0));
  const view = new DataView(bytes.buffer);
  let offset = 0;
  for (const row of rows) {
    view.setUint32(offset, 1, true);
    view.setUint32(offset + 4, row.length, true);
    view.setFloat64(offset + 8, 1_700_000_000_000, true);
    bytes.set(row, offset + 16);
    offset += 16 + row.length;
  }
  batches.push(bytes);
}
const probes = {
  textCrossing: measure(() => { for (const text of texts) sink += probe.profileAcceptText(text); }),
  textCrossingAndJsonValidation: measure(() => { for (const text of texts) sink += Number(probe.profileValidateJson(text)); }),
  // Prepared buffers isolate crossing only; production host packing is additional.
  preparedBatchCrossing: measure(() => { for (const bytes of batches) sink += probe.profileAcceptBytes(bytes); }),
};
probes.jsonValidationDifferential = {
  medianCpuMs: probes.textCrossingAndJsonValidation.medianCpuMs - probes.textCrossing.medianCpuMs,
  interpretation: 'Difference of independent probe medians; rough syntax-validation cost only, not an exact decomposition of venue parsing.',
};
const replaySamples = [];
const scratch = mkdtempSync(join(tmpdir(), 'rust-engine-profile-'));
try {
  for (let run = 0; run < runs; run++) {
    const out = join(scratch, `${run}.json`);
    const child = spawnSync(process.execPath, ['--expose-gc', join(dir, 'replay.mjs'), '--impl', 'rust', '--venue', values.venue,
      '--tape', tape, '--books', books, '--delivery', values.delivery, '--cadence', values.cadence, '--profile', '--out', out], { encoding: 'utf8', timeout: 150_000 });
    if (child.status !== 0) throw new Error(`Replay failed (${child.status}): ${child.stderr}\n${child.stdout}`);
    const result = JSON.parse(readFileSync(out, 'utf8'));
    replaySamples.push({ metrics: result.metrics, frames: result.frames });
  }
} finally { rmSync(scratch, { recursive: true, force: true }); }
const methods = {};
for (const name of new Set(replaySamples.flatMap((row) => Object.keys(row.metrics.profile)))) {
  const samples = replaySamples.map((row) => row.metrics.profile[name] ?? { calls: 0, wallMs: 0 });
  methods[name] = { calls: median(samples.map((s) => s.calls)), medianWallMs: median(samples.map((s) => s.wallMs)) };
}
console.log(JSON.stringify({ venue: values.venue, delivery: values.delivery, cadence: values.cadence, tape, node: process.version, runs, rounds,
  messages: texts.length, utf8Bytes: encoded.reduce((n, row) => n + row.length, 0), batchSize, preparedBatchCalls: batches.length,
  probes, replay: { methods, samples: replaySamples }, sink,
  interpretation: 'wsMessage/wsMessages includes crossing, venue parsing/state and returned-action serialization. takeFrame includes binary frame writing and wasm-bindgen return copy. JSON validation is a parser floor, not typed venue parsing. Prepared batch probes exclude host packing. Instrumented wall timings are diagnostics, not gate CPU metrics.',
}, null, 2));
