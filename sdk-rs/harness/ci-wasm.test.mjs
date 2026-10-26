import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { archiveCiWasm, assertBuildEnvironment, buildInputFingerprint, validateCiWasm, validatePairedCiWasm, validateSameRunCiWasm } from './ci-wasm.mjs';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fixture = (t) => {
  const root = mkdtempSync(join(tmpdir(), 'ci-wasm-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const git = (...args) => {
    const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git('init', '--quiet');
  git('config', 'user.name', 'CI wasm test');
  git('config', 'user.email', 'ci-wasm@example.invalid');
  const source = join(root, 'sdk-rs/engine/src/lib.rs');
  mkdirSync(join(root, 'sdk-rs/engine/src'), { recursive: true });
  writeFileSync(source, '// original\n');
  for (const [name, bytes] of [
    ['sdk-rs/rust-toolchain.toml', '[toolchain]\nchannel = "1.99.0"\n'],
    ['sdk-rs/harness/install-binaryen.sh', 'version=132\n'],
    ['.github/workflows/rust.yml', 'WASM_BINDGEN_VERSION: 0.2.129\n'],
  ]) {
    mkdirSync(join(root, name, '..'), { recursive: true });
    writeFileSync(join(root, name), bytes);
    git('add', name);
  }
  git('add', 'sdk-rs/engine/src/lib.rs');
  git('commit', '--quiet', '-m', 'initial');
  const directory = join(root, 'artifact');
  mkdirSync(directory);
  const hashes = {};
  for (const name of ['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json']) {
    const bytes = name === 'package.json' ? '{"type":"commonjs"}' : `fixture ${name}`;
    writeFileSync(join(directory, name), bytes);
    hashes[name] = sha256(bytes);
  }
  const metadata = {
    schema: 1, sourceHead: git('rev-parse', 'HEAD'), runId: '123',
    family: 'binance', venue: 'binance', target: 'nodejs', profile: 'release',
    buildFlags: { rustFlagsSha256: sha256(''), encodedRustFlagsSha256: sha256(''), wasmOptLevel: '-Oz', wasmOpt: 'required' },
    tools: { rustc: 'rustc 1.99.0 (fixture)', 'wasm-bindgen': 'wasm-bindgen 0.2.129', 'wasm-opt': 'wasm-opt version 132 (fixture)' },
    inputsSha256: buildInputFingerprint(root), hashes,
  };
  const save = () => writeFileSync(join(directory, 'build.json'), JSON.stringify(metadata));
  save();
  return { root, source, directory, metadata, git, save,
    validate: () => validateCiWasm({ root, directory, venue: 'binancepm' }) };
};

test('matching clean build tree accepts a family alias without replacing installed wasm', (t) => {
  const f = fixture(t);
  assert.equal(f.validate().path, join(f.directory, 'safe_cex_wasm.js'));
  assert.deepEqual(f.validate().metadata, f.metadata);
});

test('cross-job family reuse requires both exact checkout and same workflow run', t => {
  const f = fixture(t), env = { GITHUB_ACTIONS: 'true', GITHUB_RUN_ID: '123' };
  const validate = () => validateSameRunCiWasm({ root: f.root, directory: f.directory, venue: 'binancepm', env });
  assert.doesNotThrow(validate);
  f.metadata.runId = '124'; f.save(); assert.throws(validate, /source\/run/);
  f.metadata.runId = '123'; f.metadata.sourceHead = 'a'.repeat(40); f.save(); assert.throws(validate, /source\/run/);
  f.metadata.sourceHead = f.git('rev-parse', 'HEAD'); f.save();
  writeFileSync(join(f.directory, 'safe_cex_wasm.js'), '// substituted'); assert.throws(validate, /hash/);
});
for (const mode of ['unstaged', 'staged', 'untracked', 'committed']) {
  test(`refuses ${mode} build input changes with no fallback`, (t) => {
    const f = fixture(t);
    writeFileSync(mode === 'untracked' ? join(f.root, 'sdk-rs/engine/src/new.rs') : f.source, '// changed\n');
    if (mode === 'staged' || mode === 'committed') f.git('add', 'sdk-rs/engine/src/lib.rs');
    if (mode === 'committed') f.git('commit', '--quiet', '-m', 'change input');
    assert.throws(f.validate, /dirty or untracked|fingerprint differs/);
  });
}
test('harness-only edits and commits can reuse the same compiled engine', (t) => {
  const f = fixture(t);
  mkdirSync(join(f.root, 'sdk-rs/harness'), { recursive: true });
  writeFileSync(join(f.root, 'sdk-rs/harness/venues.mjs'), '// clock change\n');
  assert.doesNotThrow(f.validate);
  f.git('add', 'sdk-rs/harness/venues.mjs');
  f.git('commit', '--quiet', '-m', 'harness change');
  assert.doesNotThrow(f.validate);
});
for (const committed of [false, true]) {
  test(`shared construction rules invalidate reuse (${committed ? 'committed' : 'dirty'})`, (t) => {
    const f = fixture(t);
    const path = 'packages/safe-cex/src/exchanges/rust/construction-rules.json';
    mkdirSync(join(f.root, path, '..'), { recursive: true });
    writeFileSync(join(f.root, path), '{"default":{},"venues":{}}');
    if (committed) {
      f.git('add', path);
      f.git('commit', '--quiet', '-m', 'change shared rules');
    }
    assert.throws(f.validate, /dirty or untracked|fingerprint differs/);
  });
}
test('artifact cannot redefine the fixed fingerprint input list', (t) => {
  const f = fixture(t);
  f.metadata.inputs = [];
  f.save();
  writeFileSync(f.source, '// dirty\n');
  assert.throws(f.validate, /dirty or untracked/);
});
for (const name of ['safe_cex_wasm.js', 'safe_cex_wasm_bg.wasm', 'package.json']) {
  for (const mode of ['missing', 'corrupt']) {
    test(`refuses ${mode} ${name}`, (t) => {
      const f = fixture(t);
      if (mode === 'missing') rmSync(join(f.directory, name));
      else writeFileSync(join(f.directory, name), 'changed');
      assert.throws(f.validate, /ENOENT|hash mismatch/);
    });
  }
}
for (const mutation of [
  (m) => { m.family = 'bybit'; },
  (m) => { m.venue = 'bybit'; },
  (m) => { m.tools = undefined; },
  (m) => { m.tools.rustc = 'rustc 1.98.0 (old)'; },
  (m) => { m.tools['wasm-bindgen'] = 'wasm-bindgen 0.2.128'; },
  (m) => { m.tools['wasm-opt'] = 'wasm-opt version 131'; },
  (m) => { m.schema = 2; },
  (m) => { m.target = 'web'; },
  (m) => { m.buildFlags.wasmOptLevel = '-O3'; },
  (m) => { m.buildFlags.rustFlagsSha256 = sha256('-C opt-level=1'); },
]) {
  test('refuses a changed family/build contract', (t) => {
    const f = fixture(t);
    mutation(f.metadata);
    f.save();
    assert.throws(f.validate, /contract|family/);
  });
}

for (const name of ['CARGO_PROFILE_RELEASE_OPT_LEVEL', 'CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS',
  'CARGO_BUILD_RUSTFLAGS', 'RUSTUP_TOOLCHAIN', 'RUSTC_WRAPPER']) {
  test(`refuses build override ${name}`, () => {
    assert.throws(() => assertBuildEnvironment({ [name]: 'override' }), /Unsupported.*overrides/);
  });
}
test('allows a CI target cache directory without changing build semantics', () => {
  assert.doesNotThrow(() => assertBuildEnvironment({ CARGO_TARGET_DIR: '/tmp/cache', CARGO_BUILD_JOBS: '4' }));
});

const archiveFixture = (t) => {
  const f = fixture(t);
  writeFileSync(join(f.root, '.gitignore'), 'sdk-rs/hosts/wasm/pkg/\n');
  f.git('add', '.gitignore');
  f.git('commit', '--quiet', '-m', 'ignore generated bindings');
  const pkg = join(f.root, 'sdk-rs/hosts/wasm/pkg/node');
  mkdirSync(pkg, { recursive: true });
  for (const name of Object.keys(f.metadata.hashes)) copyFileSync(join(f.directory, name), join(pkg, name));
  const bin = join(f.root, 'bin');
  mkdirSync(bin);
  for (const [tool, version] of Object.entries(f.metadata.tools)) {
    const executable = join(bin, tool);
    writeFileSync(executable, `#!/bin/sh\nprintf '%s\\n' '${version}'\n`);
    chmodSync(executable, 0o755);
  }
  const destination = join(f.root, 'archived');
  const archive = () => archiveCiWasm({ root: f.root, venue: 'binance', destination,
    env: { PATH: bin, GITHUB_ACTIONS: 'true', GITHUB_RUN_ID: '123', SAFE_CEX_WASM_OPT: 'required' } });
  return { ...f, pkg, destination, archive };
};
test('archives the three CI-built files and validates their recorded provenance', (t) => {
  const f = archiveFixture(t);
  const metadata = f.archive();
  assert.equal(metadata.sourceHead, f.git('rev-parse', 'HEAD'));
  assert.deepEqual(metadata.hashes, f.metadata.hashes);
  assert.doesNotThrow(() => validateCiWasm({ root: f.root, directory: f.destination, venue: 'binancespot' }));
});
test('archive refuses an incomplete generated package', (t) => {
  const f = archiveFixture(t);
  rmSync(join(f.pkg, 'safe_cex_wasm_bg.wasm'));
  assert.throws(f.archive, /ENOENT/);
});
test('archive refuses local builds and dirty covered inputs', (t) => {
  const f = archiveFixture(t);
  assert.throws(() => archiveCiWasm({ root: f.root, venue: 'binance', destination: f.destination, env: {} }), /GitHub Actions/);
  writeFileSync(f.source, '// dirty\n');
  assert.throws(f.archive, /dirty or untracked/);
});

test('refuses new repository-root Cargo configuration', (t) => {
  const f = fixture(t);
  mkdirSync(join(f.root, '.cargo'));
  writeFileSync(join(f.root, '.cargo/config.toml'), '[build]\nrustflags = ["-C", "opt-level=1"]\n');
  assert.throws(f.validate, /dirty or untracked/);
});

test('gate rejects a missing CI artifact before starting either replay', (t) => {
  const f = fixture(t);
  const gate = new URL('./gate.mjs', import.meta.url);
  const result = spawnSync(process.execPath, [gate.pathname, '--ci-wasm', join(f.root, 'missing'), '--venue', 'binance'], {
    encoding: 'utf8', timeout: 10000,
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /ENOENT.*build\.json/);
  assert.doesNotMatch(result.stdout, /Replay output:/);
});

test('CI retains wasm separately for three days without extending report storage', () => {
  const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
  const archive = workflow.indexOf('name: Archive the matching CI replay engine');
  assert.ok(archive > workflow.indexOf('name: Build the venue wasm engine'));
  assert.ok(archive < workflow.indexOf('name: Check the wasm size budget'));
  const upload = workflow.slice(workflow.indexOf('name: Retain the matching CI replay engine briefly'));
  assert.match(upload, /name: ci-wasm-\$\{\{ matrix\.venue \}\}/);
  assert.match(upload, /path: sdk-rs\/hosts\/wasm\/pkg\/ci-replay\//);
  assert.match(upload, /retention-days: 3/);
  const reports = workflow.slice(workflow.indexOf('name: Retain strict gate reports'), workflow.indexOf('name: Retain the matching CI replay engine briefly'));
  assert.match(reports, /sdk-rs\/harness\/out\//);
  assert.match(reports, /retention-days: 14/);
  assert.doesNotMatch(reports, /pkg\//);
});

for (const directory of ['', '   ']) {
  test('gate refuses an explicitly empty CI artifact directory without fallback', () => {
    const result = spawnSync(process.execPath, [new URL('./gate.mjs', import.meta.url).pathname,
      '--ci-wasm', directory, '--venue', 'binance'], { encoding: 'utf8', timeout: 10000 });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /CI wasm directory must be nonempty/);
    assert.doesNotMatch(result.stdout, /Replay output:/);
  });
}

test('gate passes validated wasm only to Rust and records its provenance', (t) => {
  const f = fixture(t);
  const tape = join(f.root, 'tape.jsonl');
  const report = join(f.root, 'gate.json');
  writeFileSync(tape, JSON.stringify({ type: 'meta', private: true }) + '\n');
  const script = `
    import assert from 'node:assert/strict';
    import childProcess from 'node:child_process';
    import { readFileSync, writeFileSync } from 'node:fs';
    import { syncBuiltinESMExports } from 'node:module';
    import { expectedCheckpointPlan } from ${JSON.stringify(new URL('./tape.mjs', import.meta.url).href)};
    import { venueOf } from ${JSON.stringify(new URL('./venues.mjs', import.meta.url).href)};
    const originalSpawn = childProcess.spawnSync;
    const calls = [];
    process.argv = ['node', 'gate.mjs', '--venue', 'binancepm', '--tape', ${JSON.stringify(tape)},
      '--report', ${JSON.stringify(report)}, '--books', '', '--runs', '1', '--ci-wasm', ${JSON.stringify(f.directory)}];
    childProcess.spawnSync = (executable, args, options) => {
      if (executable === 'git') return originalSpawn(executable, args, options);
      const impl = args[args.indexOf('--impl') + 1];
      calls.push(impl);
      if (impl === 'rust') assert.equal(args[args.indexOf('--wasm') + 1], ${JSON.stringify(join(f.directory, 'safe_cex_wasm.js'))});
      else assert.ok(!args.includes('--wasm'));
      const cpu = impl === 'rust' ? 1 : 2;
      const retained = impl === 'rust' ? 10 : 20;
      const account = { hedged: false, balance: { used: 0, free: 100, total: 100, upnl: 0 },
        orders: [], positions: [], settings: [], fills: [], flatPositionPnl: [] };
      const output = {
        frames: 1, markets: [{ symbol: 'BTCUSDT', active: true, precision: { amount: 1, price: 0.1 } }],
        tickers: [{ symbol: 'BTCUSDT', bid: 99, ask: 101, last: 100 }], books: {}, commands: [], account,
        unserved: 0, missingWrites: 0, unusedWrites: 0, invalidSignatures: 0,
        protocolErrors: [], wsRequests: [], subscriptions: [], requests: [], writes: [], writeCount: 0,
        checkpoints: expectedCheckpointPlan([{ type: 'meta', private: true }], venueOf('binancepm'), impl)
          .map((point) => ({ ...point, markets: [], tickers: [], books: {}, account })),
        transportReceipt: {},
        metrics: { baselineKind: 'idle-loaded', measuredMode: 'correctness',
          cpuMs: cpu, steadyCpuMs: cpu, processCpuMs: cpu, runtimeCpuMs: cpu, runtimeProcessCpuMs: cpu,
          idleLoadCpuMs: 0, idleLoadProcessCpuMs: 0, idleLoadWallMs: 0, runtimeWallMs: 1, wallMs: 1,
          heapBeforeBytes: 100, heapUsedBytes: 100 + retained, buffersBefore: 100, buffersAfter: 100,
          retainedBytes: retained, heapRetainedBytes: retained, bufferRetainedBytes: 0,
          wasmBytes: 0, wasmBeforeBytes: 0, wasmRetainedBytes: 0 },
      };
      writeFileSync(args[args.indexOf('--out') + 1], JSON.stringify(output));
      return { status: 0, stderr: '' };
    };
    syncBuiltinESMExports();
    await import(${JSON.stringify(new URL('./gate.mjs', import.meta.url).href)});
    assert.deepEqual(calls, ['ts', 'rust']);
    assert.deepEqual(JSON.parse(readFileSync(${JSON.stringify(report)}, 'utf8')).ciWasm, ${JSON.stringify(f.metadata)});
  `;
  const runner = join(f.root, 'run.mjs');
  writeFileSync(runner, script);
  const result = spawnSync(process.execPath, [runner], { cwd: f.root, encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr + result.stdout);
  assert.match(result.stdout, /\nPASS\n$/);
});

test('paired Bitget validates the sealed old source while executing from clean common harness HEAD', t => {
  const f = fixture(t);
  f.metadata.family = 'bitget'; f.metadata.venue = 'bitget'; f.save();
  const sourceHead = f.metadata.sourceHead;
  writeFileSync(f.source, '// candidate runtime\n'); f.git('add', 'sdk-rs/engine/src/lib.rs');
  f.git('commit', '--quiet', '-m', 'candidate runtime');
  const env = { GITHUB_ACTIONS: 'true', GITHUB_RUN_ATTEMPT: '1', GITHUB_RUN_ID: '123' };
  const args = { root: f.root, directory: f.directory, venue: 'bitget', sourceHead, env };
  assert.throws(() => validateCiWasm(args), /fingerprint differs/);
  assert.equal(validatePairedCiWasm(args).metadata.sourceHead, sourceHead);
  assert.notEqual(f.git('rev-parse', 'HEAD'), sourceHead);
  for (const mutate of [x => { x.venue = 'bitgetusdc'; }, x => { x.sourceHead = f.git('rev-parse', 'HEAD'); },
    x => { x.sourceHead = 'f'.repeat(40); }, x => { x.env.GITHUB_RUN_ATTEMPT = '2'; },
    x => { x.env.GITHUB_RUN_ID = '124'; }, x => { x.env.GITHUB_ACTIONS = 'false'; }]) {
    const x = structuredClone(args); mutate(x); assert.throws(() => validatePairedCiWasm(x));
  }
  writeFileSync(f.source, '// dirty measurement checkout\n');
  assert.throws(() => validatePairedCiWasm(args), /dirty or untracked/);
});

test('paired source cannot be selected outside the dedicated nine-pair Bitget CI measurement', t => {
  const f = fixture(t), gate = new URL('./gate.mjs', import.meta.url);
  for (const args of [[], ['--ci-wasm', f.directory, '--venue', 'binance', '--runs', '9'],
    ['--ci-wasm', f.directory, '--venue', 'bitget', '--runs', '1'],
    ['--ci-wasm', f.directory, '--venue', 'bitget', '--runs', '9']]) {
    const result = spawnSync(process.execPath, [gate.pathname, '--paired-source', f.metadata.sourceHead, ...args],
      { cwd: f.root, encoding: 'utf8', env: { ...process.env, GITHUB_ACTIONS: 'false' } });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Paired source|Paired module selection/);
  }
});
