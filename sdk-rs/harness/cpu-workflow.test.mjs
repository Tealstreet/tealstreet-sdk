import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const job = (text, name) => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z-]*:\\n|$(?![\\s\\S]))`, 'm').exec(text);
  assert(match, `missing ${name}`);
  return match[1];
};

function verifyNoCpuGates(text) {
  assert.doesNotMatch(text, /^  (?:bitget-paired|cpu-(?:plan|certify|final)):|^    name: Rust \/ (?:CPU gate|shared CPU certification|CPU remeasurement plan|Bitget counterbalanced steady CPU)/m);
  assert.doesNotMatch(text, /^    needs:.*\bcpu-|needs\.["']?cpu-(?:plan|certify|final)|--command (?:plan|certify|status)\b|CPU_CERT_RESULTS?/m);
}

test('Sam 2026-11-17: CPU gate jobs and their nightly monitor are removed', () => {
  verifyNoCpuGates(source);
  for (const path of ['../../.github/workflows/rust-cpu-nightly.yml', './cpu-nightly.mjs', './cpu-nightly.test.mjs',
    './bitget-paired-run.mjs', './bitget-paired-run.test.mjs'])
    assert.equal(existsSync(new URL(path, import.meta.url)), false, path);
});

test('Sam 2026-11-17: reintroduced CPU gate jobs or certification calls are refused', () => {
  for (const addition of ['  cpu-final:\n', '    name: Rust / CPU gate (binance)\n',
    '    name: Rust / shared CPU certification\n', '    run: node cpu-ci.mjs --command certify\n',
    '    env: CPU_CERT_RESULTS\n', '    needs: [selection, cpu-final]\n', '  bitget-paired:\n']) assert.throws(() => verifyNoCpuGates(source + addition));
});

function verifyWorkflow(text) {
  const first = job(text, 'gate');
  verifyNoCpuGates(text);
  // One in-job re-measure (Sam, 2026-11-07): no separate remeasurement runner or runtime handoff.
  assert.doesNotMatch(text, /^  cpu-second:$|Retain the eligible original runtime bytes|--command second|cpu-runtime-\$/m);
  assert.match(first, /^    name: Rust \/ replay measurement \(\$\{\{ matrix\.venue \}\}\)$/m);
  const prepare = first.slice(first.indexOf('      - name: Seal approved CPU measurement inputs'), first.indexOf('      - name: Public data'));
  assert.match(prepare, /--command prepare --suite '\$\{\{ matrix\.venue \}\}'/);
  assert.doesNotMatch(prepare, /\n        if:/, 'every first suite must seal inputs');
  const collect = first.slice(first.indexOf('      - name: Verify strict replay results'), first.indexOf('      - name: Retain CPU runtime capsule'));
  assert.match(collect, /if: always\(\)/);
  assert.match(collect, /--command collect --suite '\$\{\{ matrix\.venue \}\}'/);
  assert.doesNotMatch(collect, /^          (?:if|else)\b|rust:gate:verify/m, 'no suite bypass of shared collection');
  for (const authorityConsumer of [first])
    assert.match(authorityConsumer, /uses: actions\/checkout@v7\n        with:\n          ref: \$\{\{ github\.sha \}\}\n          fetch-depth: 1\n          fetch-tags: false\n      - uses: \.\/\.github\/actions\/fetch-rust-history\n(?=      - )/,
      'independent composition proof needs exact source and unconditional complete targeted history');
  assert.doesNotMatch(first, /continue-on-error/);
}

test('replay measurement retains sealed collection without CPU certification jobs', () => verifyWorkflow(source));

test('workflow controls reject skipped collection, a revived second runner and local bypasses', () => {
  const changes = [
    ...['gate'].flatMap(name => [
      ['fetch-depth: 1', 'fetch-depth: 0'],
      ['ref: ${{ github.sha }}', 'ref: master'],
      ['fetch-tags: false', 'fetch-tags: true'],
      ['      - uses: ./.github/actions/fetch-rust-history\n', ''],
      ['      - uses: ./.github/actions/fetch-rust-history\n',
        '      - uses: ./.github/actions/fetch-rust-history\n        if: false\n'],
    ].map(([before, after]) => text => text.replace(job(text, name), job(text, name).replace(before, after)))),
    text => text.replace('\n  result:\n', '\n  cpu-second:\n    name: Rust / CPU remeasurement (bitget)\n\n  result:\n'),
    text => text.replace("--command collect --suite '${{ matrix.venue }}'", "rust:gate:verify --suite '${{ matrix.venue }}'"),
  ];
  for (const change of changes) { const changed = change(source); assert.notEqual(changed, source); assert.throws(() => verifyWorkflow(changed)); }
});


test('A/A diagnostics are explicit dispatch-only and cannot supply certification evidence', () => {
  const diagnostic = job(source, 'cpu-budget-diagnostic');
  assert.match(diagnostic, /if: github.event_name == 'workflow_dispatch' && inputs.cpu_budget_diagnostic/);
  assert.match(diagnostic, /fail-fast: false/);
  assert.match(diagnostic, /slot: \[1, 2, 3, 4\]/);
  assert.match(diagnostic, /cpu-budget-diagnostic\.mjs --module/);
  assert.doesNotMatch(diagnostic, /--command (?:collect|certify|second)|rust:gate:record|gh run rerun|continue-on-error/);
  assert.match(diagnostic, /name: cpu-budget-diagnostic-\$\{\{ matrix\.slot \}\}/);
  const scope = job(source, 'scope');
  assert.match(scope, /if \[ '\$\{\{ github\.event_name \}\}' = workflow_dispatch \] && \[ '\$\{\{ inputs\.cpu_budget_diagnostic \}\}' = true \]/);
  assert.match(scope, /else\n\s+node sdk-rs\/harness\/ci-scope\.mjs/);
});

test('Binance replay measurement has 45 minutes while other matrix venues retain 30', () => {
  const first = job(source, 'gate');
  assert.match(first, /^    timeout-minutes: \$\{\{ matrix\.venue == 'binance' && 45 \|\| 30 \}\}$/m);
});

function measurementNodeCheck(text) {
  const first = job(text, 'gate');
  const marker = '      - name: Assert Node 22 before sealing CPU inputs\n';
  const start = first.indexOf(marker);
  assert(start >= 0, 'CPU seal needs an explicit Node version check');
  const end = first.indexOf('      - name:', start + marker.length);
  assert.equal(first.slice(end).split('\n')[0], '      - name: Seal approved CPU measurement inputs');
  const step = first.slice(start, end);
  assert.doesNotMatch(step, /\n        if:/, 'every measurement runner must check its Node version');
  return step.split('        run: |\n')[1].split('\n').filter(Boolean).map(line => line.slice(10)).join('\n');
}

test('measurement seal and collection use the same Yarn Node as replay, after explicit diagnostics', () => {
  const first = job(source, 'gate');
  const script = measurementNodeCheck(source);
  for (const diagnostic of ['node --version', 'which -a node', 'command -v node', 'corepack yarn -s node --version'])
    assert(script.includes(diagnostic), `missing diagnostic: ${diagnostic}`);
  for (const command of ['prepare', 'collect']) {
    assert.match(first, new RegExp(`corepack yarn -s node sdk-rs/harness/cpu-ci\\.mjs --command ${command} --suite`));
    assert.doesNotMatch(first, new RegExp(`^ +node sdk-rs/harness/cpu-ci\\.mjs --command ${command}`, 'm'));
  }
});

for (const version of ['v22.23.4', 'v24.22.2', '']) {
  test(`measurement Node assertion checks Yarn's runtime (${version || 'empty'}), not the bare Node`, t => {
    const directory = mkdtempSync(join(tmpdir(), 'cpu-node-pin-'));
    t.after(() => rmSync(directory, { recursive: true, force: true }));
    writeFileSync(join(directory, 'node'), '#!/bin/sh\nprintf "v24.22.2\\n"\n');
    writeFileSync(join(directory, 'corepack'), '#!/bin/sh\n[ "$*" = "yarn -s node --version" ] || exit 99\nprintf "%s\\n" "$CPU_TEST_NODE_VERSION"\n');
    for (const executable of ['node', 'corepack']) chmodSync(join(directory, executable), 0o755);
    const run = spawnSync('bash', ['-e', '-c', measurementNodeCheck(source)], {
      encoding: 'utf8', env: { ...process.env, PATH: `${directory}:${process.env.PATH}`, CPU_TEST_NODE_VERSION: version },
    });
    assert.equal(run.status, version.startsWith('v22.') ? 0 : 1, run.stderr);
    assert(run.stdout.includes('v24.22.2'), 'bare Node diagnostic is retained even when Yarn uses 22');
    assert(run.stdout.includes(join(directory, 'node')), 'resolved bare Node paths are visible');
    assert(run.stdout.includes(`CPU measurement Node: ${version}`));
    if (!version.startsWith('v22.')) assert(run.stdout.includes(`::error::CPU measurement requires Node v22; found ${version || '<empty>'}`));
  });
}

test('Bun PATH export preserves setup Node for the actual sealing assertion', t => {
  const first = job(source, 'gate');
  const start = first.indexOf('      - name: Install pinned Bun for standalone CLI loader proof\n');
  assert(start >= 0);
  const end = first.indexOf('      - name:', start + 1);
  const install = first.slice(start, end).split('        run: |\n')[1]
    .split('\n').filter(Boolean).map(line => line.slice(10)).join('\n');
  const directory = mkdtempSync(join(tmpdir(), 'cpu-bun-node-path-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const pinned = join(directory, 'node22'), globalBin = join(directory, 'global');
  for (const path of [pinned, globalBin]) mkdirSync(path);
  const executables = [
    [join(pinned, 'node'), '#!/bin/sh\ncase "$1" in --version) printf "v22.23.4\\n";; -p) printf "%s\\n" "$CPU_TEST_PINNED_NODE";; *) exit 99;; esac\n'],
    [join(globalBin, 'node'), '#!/bin/sh\nprintf "v24.22.2\\n"\n'],
    [join(globalBin, 'bun'), '#!/bin/sh\nprintf "1.4.3\\n"\n'],
    [join(pinned, 'corepack'), `#!/bin/sh
case "$*" in
  'yarn -s global add bun@1.4.3') exit 0;;
  'yarn -s global bin') printf '%s\\n' "$CPU_TEST_GLOBAL_BIN";;
  'yarn -s node --version') exec node --version;;
  *) exit 99;;
esac
`],
  ];
  for (const [path, contents] of executables) { writeFileSync(path, contents); chmodSync(path, 0o755); }
  const env = { ...process.env, PATH: `${pinned}:${process.env.PATH}`,
    GITHUB_PATH: join(directory, 'github-path'), CPU_TEST_GLOBAL_BIN: globalBin, CPU_TEST_PINNED_NODE: join(pinned, 'node') };
  const installed = spawnSync('bash', ['-e', '-c', install], { encoding: 'utf8', env });
  assert.equal(installed.status, 0, installed.stderr);
  const paths = readFileSync(env.GITHUB_PATH, 'utf8').trim().split('\n');
  assert(paths.includes(globalBin), 'Bun global bin remains available to CLI loader checks');
  // The runner prepends GITHUB_PATH entries to each following step, last entry first.
  const nextEnv = entries => ({ ...env, PATH: `${[...entries].reverse().join(':')}:${env.PATH}` });
  const check = entries => spawnSync('bash', ['-e', '-c', measurementNodeCheck(source)], {
    encoding: 'utf8', env: nextEnv(entries),
  });
  const bun = spawnSync('bun', ['--version'], { encoding: 'utf8', env: nextEnv(paths) });
  assert.equal(bun.status, 0, bun.stderr);
  assert.equal(bun.stdout.trim(), '1.4.3');
  const replay = check(paths);
  assert.equal(replay.status, 0, replay.stdout + replay.stderr);
  assert.match(replay.stdout, /CPU measurement Node: v22\.23\.4/);
  const withoutPin = check(paths.filter(path => path !== pinned));
  assert.equal(withoutPin.status, 1, withoutPin.stdout + withoutPin.stderr);
  assert.match(withoutPin.stdout, /requires Node v22; found v24\.22\.2/);
});

test('both native consumer jobs execute the wasm-free mobile suite through an existing script', () => {
  const pkg = JSON.parse(readFileSync(new URL('../../packages/safe-cex-react-native/package.json', import.meta.url)));
  const commands = [...source.matchAll(/corepack yarn workspace @tealstreet\/safe-cex-react-native (\S+)/g)];
  assert.equal(commands.length, 2, 'retain mobile verification in both consumer jobs');
  for (const [, command] of commands) {
    assert.equal(command, 'test-unit', 'mobile consumes delegated frames with WebAssembly unavailable');
    assert.ok(Object.hasOwn(pkg.scripts, command), `missing mobile test script ${command}`);
  }
});
