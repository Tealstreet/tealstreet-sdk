import assert from 'node:assert/strict';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { GATE_SUITES } from './ci-gate-suites.mjs';

const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const job = (text, name) => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z-]*:\\n|$(?![\\s\\S]))`, 'm').exec(text);
  assert(match, `missing ${name}`);
  return match[1];
};

test('Bitget bounded collection leaves time to publish original-attempt evidence', () => {
  // #3317: master 93ce5a61df moved collection into bitget-paired-run.mjs.
  // Master 589038b822 adds two measured witnesses within an 80/90-minute bound;
  // retain its exact deadline and ten-minute evidence-publication reserve.
  const runner = readFileSync(new URL('./bitget-paired-run.mjs', import.meta.url), 'utf8');
  const deadline = /timeoutMs = (\d+) \* 60_000/.exec(runner);
  assert.ok(deadline, 'the collector needs an explicit bounded deadline');
  const collectionMinutes = Number(deadline[1]);
  assert.equal(collectionMinutes, 80);
  const verify = text => {
    const paired = job(text, 'bitget-paired');
    const timeout = /^    timeout-minutes: (\d+)$/m.exec(paired);
    assert.ok(timeout, 'the physical producer needs a bounded job window');
    assert.equal(Number(timeout[1]), 90);
    assert.ok(Number(timeout[1]) - collectionMinutes >= 10,
      'the collector must leave an evidence-publication reserve');
    assert.match(paired, /run: node sdk-rs\/harness\/bitget-paired-run\.mjs --out/);
    assert.doesNotMatch(paired, /continue-on-error|gh run rerun|retries|retry:/);
  };
  verify(source);
  const deadlineRegression = source.replace(job(source, 'bitget-paired'),
    job(source, 'bitget-paired').replace(/timeout-minutes: \d+/, 'timeout-minutes: 80'));
  assert.throws(() => verify(deadlineRegression));
});
function verifyWorkflow(text) {
  const first = job(text, 'gate'), plan = job(text, 'cpu-plan'), certify = job(text, 'cpu-certify'), final = job(text, 'cpu-final');
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
  assert.match(plan, /needs: \[selection, gate, bitget-paired\]\n    if: always\(\)/);
  assert.doesNotMatch(plan, /has_eligible|outputs\.matrix/);
  assert.match(plan, /--command plan --out /);
  assert.doesNotMatch(plan, /--suite /, 'planner covers the entire registered suite set');
  assert.match(final, /name: Rust \/ CPU gate \(\$\{\{ matrix\.suite \}\}\)/);
  assert.match(certify, /needs: \[selection, gate, bitget-paired, cpu-plan\]\n    if: always\(\)/);
  assert.match(final, /needs: \[selection, cpu-certify\]\n    if: always\(\)/);
  const suites = /suite: \[([^\]]+)\]/.exec(final);
  assert(suites);
  assert.deepEqual(suites[1].split(',').map(suite => suite.trim()).sort(), [...GATE_SUITES].sort());
  assert.match(certify, /--command certify --out/);
  assert.match(final, /--command status --suite '\$\{\{ matrix\.suite \}\}'/);
  assert.match(final, /CPU_CERT_RESULT: \$\{\{ needs\.cpu-certify\.result \}\}/);
  assert.match(final, /CPU_CERT_RESULTS: \$\{\{ needs\.cpu-certify\.outputs\.results \}\}/);
  assert.doesNotMatch(final, /GITHUB_TOKEN|--command certify|upload-artifact/);
  for (const supplemental of [plan, certify, final]) {
    assert.match(supplemental, /defaults:\n      run:\n        working-directory: \.\n/);
    assert.doesNotMatch(supplemental, /(?:yarn|npm|cargo|rustup) (?:install|build|add)|rust:build|--command prepare/);
    assert.doesNotMatch(supplemental, /continue-on-error|gh run rerun|retries|retry:/);
  }
  for (const coordinator of [plan, certify]) {
    const prepare = coordinator.indexOf('uses: ./.github/actions/prepare-yarn');
    const install = coordinator.indexOf('uses: ./.github/actions/install-yarn');
    const execute = coordinator.indexOf('run: node sdk-rs/harness/cpu-ci.mjs');
    assert(prepare >= 0 && install > prepare && execute > install, 'coordinator needs locked dependencies before verifier imports');
    assert.match(coordinator, /cache-dir: \$\{\{ steps\.yarn\.outputs\.cache-dir \}\}/);
    assert.match(coordinator, /cache-key: \$\{\{ steps\.yarn\.outputs\.cache-key \}\}/);
    assert.match(coordinator, /cache-hit: \$\{\{ steps\.yarn\.outputs\.cache-hit \}\}/);
  }
  for (const authorityConsumer of [first, job(text, 'bitget-paired'), plan, certify])
    assert.match(authorityConsumer, /uses: actions\/checkout@v7\n        with:\n          ref: \$\{\{ github\.sha \}\}\n          fetch-depth: 1\n          fetch-tags: false\n      - uses: \.\/\.github\/actions\/fetch-rust-history\n(?=      - )/,
      'independent composition proof needs exact source and unconditional complete targeted history');
  assert.doesNotMatch(final, /uses: \.\/\.github\/actions\/(?:prepare-yarn|install-yarn)/);
  assert.doesNotMatch(first, /continue-on-error/);
}

test('all required suite names certify unconditionally, with no separate remeasurement runner', () => verifyWorkflow(source));

function verifyBitgetProducer(text) {
  const paired = job(text, 'bitget-paired');
  assert.match(paired, /name: Rust \/ Bitget counterbalanced steady CPU/);
  verifyWorkflow(text);
  // Two measured witnesses now include identical master builds; retain a hard
  // deadline with ten minutes beyond the collector's eighty-minute budget.
  assert.match(paired, /^    timeout-minutes: 90$/m);
  assert.match(paired, /CARGO_BUILD_JOBS: 4/);
  assert.match(paired, /node-version: 22/);
  assert.match(paired, /Public counterbalanced Bitget steady CPU/);
  assert.match(paired, /run: node sdk-rs\/harness\/bitget-paired-run\.mjs --out/);
  assert.match(paired, /name: bitget-paired-steady/);
  assert.match(paired, /name: Retain counterbalanced Bitget steady evidence\n        if: success\(\) \|\| failure\(\)/);
  assert.doesNotMatch(paired, /--baseline|--candidate|--tolerance|--threshold|continue-on-error|workflow_dispatch:/);
  assert.match(job(text, 'cpu-plan'), /needs: \[selection, gate, bitget-paired\]/);
  assert.match(job(text, 'cpu-certify'), /needs: \[selection, gate, bitget-paired, cpu-plan\]/);
}

test('Bitget paired producer is mandatory, bounded, source-selected and independently certified', () => verifyBitgetProducer(source));

test('Bitget workflow guard rejects a bypassed deadline or canceled/missing failure evidence', () => {
  verifyBitgetProducer(source);
  for (const [before, after] of [
    ['bitget-paired-run.mjs --out', 'bitget-paired-ci.mjs --out'],
    ['if: success() || failure()', 'if: always()'],
    ['if: success() || failure()', 'if: success()'],
    ['timeout-minutes: 90', 'timeout-minutes: 120'],
    ['run: node sdk-rs/harness/bitget-paired-run.mjs', 'continue-on-error: true\n        run: node sdk-rs/harness/bitget-paired-run.mjs'],
  ]) {
    const paired = job(source, 'bitget-paired'), changed = paired.replace(before, after);
    assert.notEqual(changed, paired, `mutation must apply: ${before}`);
    assert.throws(() => verifyBitgetProducer(source.replace(paired, changed)), before);
  }
});

test('workflow controls reject skipped final checks, a revived second runner and suite/local bypasses', () => {
  const changes = [
    ...['gate', 'bitget-paired', 'cpu-plan', 'cpu-certify'].flatMap(name => [
      ['fetch-depth: 1', 'fetch-depth: 0'],
      ['ref: ${{ github.sha }}', 'ref: master'],
      ['fetch-tags: false', 'fetch-tags: true'],
      ['      - uses: ./.github/actions/fetch-rust-history\n', ''],
      ['      - uses: ./.github/actions/fetch-rust-history\n',
        '      - uses: ./.github/actions/fetch-rust-history\n        if: false\n'],
    ].map(([before, after]) => text => text.replace(job(text, name), job(text, name).replace(before, after)))),
    text => text.replace(job(text, 'cpu-plan'), job(text, 'cpu-plan').replace('uses: ./.github/actions/install-yarn', 'uses: actions/checkout@v7')),
    text => text.replace(job(text, 'cpu-certify'), job(text, 'cpu-certify').replace('uses: ./.github/actions/prepare-yarn', 'uses: actions/checkout@v7')),
    text => text.replace(job(text, 'cpu-plan'), job(text, 'cpu-plan').replace('cache-key: ${{ steps.yarn.outputs.cache-key }}', 'cache-key: unbound')),
    text => text.replace('\n  cpu-certify:\n', '\n  cpu-second:\n    name: Rust / CPU remeasurement (bitget)\n\n  cpu-certify:\n'),
    text => text.replace(job(text, 'cpu-final'), job(text, 'cpu-final') + '      - uses: ./.github/actions/prepare-yarn\n'),
    text => text.replace('needs: [selection, gate, bitget-paired, cpu-plan]\n    if: always()', 'needs: [selection, gate, bitget-paired, cpu-plan]\n    if: success()'),
    text => text.replace('needs: [selection, cpu-certify]\n    if: always()', 'needs: cpu-certify\n    if: success()'),
    text => text.replace('timeout-minutes: 5\n    defaults:\n      run:\n        working-directory: .', 'timeout-minutes: 5'),
    text => text.replace('suite: [apexomni,', 'suite: ['),
    text => text.replace('--command plan --out', '--command plan --suite bitget --out'),
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
