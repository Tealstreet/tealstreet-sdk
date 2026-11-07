// Temporary diagnostic only; remove before the performance PR is ready.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import os from 'node:os';

const out = resolve(process.argv[2]);
mkdirSync(out, { recursive: true });
const run = (command, args, output) => {
  const result = execFileSync(command, args, { maxBuffer: 64 * 1024 * 1024, encoding: 'utf8' });
  if (output) writeFileSync(resolve(out, output), result);
  return result;
};
const hash = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');
writeFileSync(resolve(out, 'custody.json'), JSON.stringify({
  source: run('git', ['rev-parse', 'HEAD']).trim(), node: process.version,
  cpu: os.cpus()[0], platform: process.platform, arch: process.arch,
  rustc: run('rustc', ['--version']), bindgen: run('wasm-bindgen', ['--version']),
  binaryen: run('wasm-opt', ['--version']),
  moduleSha256: hash('sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm_bg.wasm'),
}, null, 2));
run('corepack', ['yarn', '-s', 'rust:gate:record', '--venue', 'asterdex',
  '--tape', 'sdk-rs/harness/fixtures/asterdex-public.jsonl.gz', '--books', 'BTCUSDT,ETHUSDT', '--runs', '16'], 'paired.log');
cpSync('sdk-rs/hosts/wasm/pkg/node', resolve(out, 'shipped-node'), { recursive: true });

const named = resolve(out, 'named-node');
run('wasm-bindgen', ['--remove-producers-section', '--target', 'nodejs', '--out-dir', named,
  'sdk-rs/target/wasm32-unknown-unknown/release/safe_cex_wasm.wasm']);
const moduleFile = resolve(named, 'safe_cex_wasm_bg.wasm');
for (let pass = 0; pass < 2; pass++) run('wasm-opt', ['-Oz', '-g', '--enable-bulk-memory', '--enable-bulk-memory-opt',
  '--enable-mutable-globals', '--enable-sign-ext', '--enable-nontrapping-float-to-int',
  '--enable-reference-types', '--enable-multivalue', moduleFile, '-o', moduleFile]);
writeFileSync(resolve(named, 'package.json'), '{"type":"commonjs"}');
run('wasm-opt', [moduleFile, '--print-function-map', '-o', '/dev/null'], 'function-map.txt');
const standard = (file) => {
  const bytes = readFileSync(file), parts = [bytes.subarray(0, 8)];
  for (let offset = 8; offset < bytes.length;) {
    const start = offset, id = bytes[offset++];
    let length = 0, shift = 0, byte;
    do { byte = bytes[offset++]; length |= (byte & 127) << shift; shift += 7; } while (byte & 128);
    offset += length;
    if (id !== 0) parts.push(bytes.subarray(start, offset));
  }
  return Buffer.concat(parts);
};
const namedIdentical = standard(moduleFile).equals(standard('sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm_bg.wasm'));
writeFileSync(resolve(out, 'named-executable-comparison.json'), JSON.stringify({ namedIdentical }));
// Exact shipped module remains the profile source if retaining names changed code.
const wasm = resolve(namedIdentical ? named : 'sdk-rs/hosts/wasm/pkg/node', 'safe_cex_wasm.js');
const tapePath = 'sdk-rs/harness/tape.mjs', original = readFileSync(tapePath, 'utf8');
const warm = 'if (delivered === warm) this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };';
assert.equal(original.split(warm).length, 2);
const finish = 'if (this.selectedPollingResponses) await this.finishPolling(clock);\n    return delivered;';
assert.equal(original.split(finish).length, 2);
const instrumented = `import { Session } from 'node:inspector';\nimport { writeFileSync } from 'node:fs';\n` + original
  .replace(warm, `if (delivered === warm) {
        this.warmMark = { thread: process.threadCpuUsage(), wall: performance.now() };
        this.steadyProfile = new Session(); this.steadyProfile.connect();
        await new Promise((resolve, reject) => this.steadyProfile.post('Profiler.enable', error => error ? reject(error) : resolve()));
        await new Promise((resolve, reject) => this.steadyProfile.post('Profiler.setSamplingInterval', { interval: 100 }, error => error ? reject(error) : resolve()));
        await new Promise((resolve, reject) => this.steadyProfile.post('Profiler.start', error => error ? reject(error) : resolve()));
      }`)
  .replace(finish, `if (this.selectedPollingResponses) await this.finishPolling(clock);
    const { profile } = await new Promise((resolve, reject) => this.steadyProfile.post('Profiler.stop', (error, result) => error ? reject(error) : resolve(result)));
    writeFileSync(process.env.ASTERDEX_PROFILE_PATH, JSON.stringify(profile));
    this.steadyProfile.disconnect();
    return delivered;`);
try {
  writeFileSync(tapePath, instrumented);
  for (let pair = 0; pair < 5; pair++) for (const impl of pair % 2 ? ['ts', 'rust'] : ['rust', 'ts']) {
    process.env.ASTERDEX_PROFILE_PATH = resolve(out, `${impl}-${pair}.cpuprofile`);
    run(process.execPath, ['--expose-gc', ...(impl === 'ts' ? ['--import', 'tsx'] : []),
      'sdk-rs/harness/replay.mjs', '--impl', impl, '--venue', 'asterdex', '--tape',
      'sdk-rs/harness/fixtures/asterdex-public.jsonl.gz', '--books', 'BTCUSDT,ETHUSDT',
      '--performance', '--wasm', wasm, '--out', resolve(out, `${impl}-${pair}.json`)], `${impl}-${pair}.log`);
  }
} finally {
  writeFileSync(tapePath, original);
}
