// Measures one venue's family wasm package against the combined engine:
// shipped bytes, cold module load and public replay CPU/memory, in
// alternating pairs within one process tree. Outputs must match exactly.
//
//   corepack yarn rust:measure-family --venue bybit --tape sdk-rs/harness/fixtures/bybit-public.jsonl.gz \
//     --books BTCUSDT,ETHUSDT --combined /tmp/pkg-combined --family /tmp/pkg-bybit --out /tmp/bybit.json
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { brotliCompressSync, gzipSync } from 'node:zlib';
import { venueOf } from './venues.mjs';
import { venueFamily } from '../hosts/wasm/js/venues.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string' },
    tape: { type: 'string' },
    books: { type: 'string' },
    combined: { type: 'string' },
    family: { type: 'string' },
    pairs: { type: 'string', default: '7' },
    out: { type: 'string' },
  },
});
for (const name of ['venue', 'tape', 'combined', 'family', 'out'])
  if (!values[name]) throw new Error(`--${name} is required`);
// Required but may be empty: '' replays no books, like rust:gate on private tapes.
if (values.books === undefined) throw new Error('--books is required');
const pairs = Number(values.pairs);
if (!Number.isInteger(pairs) || pairs < 1) throw new Error('--pairs must be a positive integer');
const venue = venueOf(values.venue);
const arms = { combined: resolve(values.combined), family: resolve(values.family) };
const scratch = `sdk-rs/harness/out/family-${values.venue}-${process.pid}`;
mkdirSync(scratch, { recursive: true });

const median = (xs) => {
  const sorted = [...xs].sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};

const bytes = (dir) => {
  const web = readFileSync(join(dir, 'web/safe_cex_wasm_bg.wasm'));
  const glue = readFileSync(join(dir, 'web/safe_cex_wasm.js'));
  return {
    webWasm: web.length,
    webWasmGzip: gzipSync(web, { level: 9 }).length,
    webWasmBrotli: brotliCompressSync(web).length,
    webGlue: glue.length,
    nodeWasm: statSync(join(dir, 'node/safe_cex_wasm_bg.wasm')).size,
  };
};

// A fresh process loads (reads and compiles) the Node package only.
const coldLoad = (dir) => {
  const code = `
    const rss = process.memoryUsage().rss;
    const cpu = process.cpuUsage();
    const thread = process.threadCpuUsage();
    const wall = performance.now();
    require(${JSON.stringify(join(dir, 'node/safe_cex_wasm.js'))});
    const p = process.cpuUsage(cpu);
    const t = process.threadCpuUsage(thread);
    console.log(JSON.stringify({
      wallMs: performance.now() - wall,
      threadCpuMs: (t.user + t.system) / 1000,
      processCpuMs: (p.user + p.system) / 1000,
      rssBytes: process.memoryUsage().rss - rss,
    }));`;
  const result = spawnSync(process.execPath, ['-e', code], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`cold load failed: ${result.stderr}`);
  return JSON.parse(result.stdout);
};

const replay = (dir, label) => {
  const out = `${scratch}/${label}.json`;
  const args = ['--expose-gc', 'sdk-rs/harness/replay.mjs', '--impl', 'rust', '--venue', values.venue,
    '--tape', values.tape, '--books', values.books, '--out', out, '--wasm', join(dir, 'node/safe_cex_wasm.js')];
  const result = spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 125000 });
  if (result.status !== 0) throw new Error(`${label} replay failed: ${result.stderr}`);
  const output = JSON.parse(readFileSync(out, 'utf8'));
  const { metrics, ...state } = output;
  return { metrics, state };
};

// Leaf paths where two outputs differ, for diagnosing a build difference.
const differingPaths = (a, b, path = '', found = []) => {
  if (found.length >= 20) return found;
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    for (const key of new Set([...Object.keys(a), ...Object.keys(b)])) differingPaths(a[key], b[key], `${path}/${key}`, found);
  } else if (JSON.stringify(a) !== JSON.stringify(b)) {
    found.push(`${path || '/'}: combined=${JSON.stringify(a)?.slice(0, 160)} family=${JSON.stringify(b)?.slice(0, 160)}`);
  }
  return found;
};

const samples = { combined: { cold: [], replay: [] }, family: { cold: [], replay: [] } };
let reference;
const differences = [];
for (let pair = 0; pair < pairs; pair++) {
  // Alternate which arm runs first so drift is shared.
  const order = pair % 2 ? ['family', 'combined'] : ['combined', 'family'];
  for (const arm of order) {
    samples[arm].cold.push(coldLoad(arms[arm]));
    const { metrics, state } = replay(arms[arm], `${arm}-${pair}`);
    samples[arm].replay.push(metrics);
    reference ??= state;
    const paths = differingPaths(reference, state);
    if (paths.length) differences.push(`${arm} pair ${pair + 1} output differs:\n    ${paths.join('\n    ')}`);
  }
}

const summary = (arm) => ({
  bytes: bytes(arms[arm]),
  cold: Object.fromEntries(
    ['wallMs', 'threadCpuMs', 'processCpuMs', 'rssBytes'].map((key) => [key, median(samples[arm].cold.map((s) => s[key]))]),
  ),
  replay: Object.fromEntries(
    ['cpuMs', 'steadyCpuMs', 'processCpuMs', 'retainedBytes', 'rssBytes'].map((key) => [
      key,
      median(samples[arm].replay.map((s) => s[key])),
    ]),
  ),
});
const report = {
  venue: values.venue,
  familyName: venueFamily(venue.rust),
  pairs,
  differences,
  combined: summary('combined'),
  family: summary('family'),
  samples,
};
writeFileSync(values.out, JSON.stringify(report, null, 2));

const line = (label, a, b, unit, scale = 1) =>
  console.log(
    `${label.padEnd(22)} combined ${(a / scale).toFixed(1).padStart(9)} ${unit}   family ${(b / scale).toFixed(1).padStart(9)} ${unit}   ${(b / a).toFixed(3)}x`,
  );
const [c, f] = [report.combined, report.family];
console.log(`${values.venue} (${pairs} alternating pairs)`);
line('web wasm', c.bytes.webWasm, f.bytes.webWasm, 'KB', 1024);
line('web wasm brotli', c.bytes.webWasmBrotli, f.bytes.webWasmBrotli, 'KB', 1024);
line('node wasm', c.bytes.nodeWasm, f.bytes.nodeWasm, 'KB', 1024);
line('cold load wall', c.cold.wallMs, f.cold.wallMs, 'ms');
line('cold load process cpu', c.cold.processCpuMs, f.cold.processCpuMs, 'ms');
line('cold load rss', c.cold.rssBytes, f.cold.rssBytes, 'MB', 1e6);
line('main-thread cpu', c.replay.cpuMs, f.replay.cpuMs, 'ms');
line('steady cpu', c.replay.steadyCpuMs, f.replay.steadyCpuMs, 'ms');
line('process cpu', c.replay.processCpuMs, f.replay.processCpuMs, 'ms');
line('retained memory', c.replay.retainedBytes, f.replay.retainedBytes, 'MB', 1e6);
line('rss', c.replay.rssBytes, f.replay.rssBytes, 'MB', 1e6);
if (differences.length) {
  console.log(`OUTPUT DIFFERS (${differences.length})`);
  for (const difference of differences) console.log(`  ${difference}`);
  process.exit(1);
}
console.log('outputs identical across arms and pairs');
