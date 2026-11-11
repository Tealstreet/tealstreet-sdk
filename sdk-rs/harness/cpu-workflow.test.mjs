import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { GATE_SUITES } from './ci-gate-suites.mjs';

const source = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const job = (text, name) => {
  const match = new RegExp(`^  ${name}:\\n([\\s\\S]*?)(?=^  [a-z][a-z-]*:\\n|$(?![\\s\\S]))`, 'm').exec(text);
  assert(match, `missing ${name}`);
  return match[1];
};

test('Bitget fixed tip and anchor collection leaves time to finish its certificate', () => {
  // #3317, run 42778780656/job 129707396005: all sixteen cohorts finished,
  // but the 45-minute deadline canceled the original producer before its
  // certificate finished. Keep the observed complete trace plus the same
  // twelve-minute reserve used by the producer-binding coordinator jobs.
  const started = Date.parse('2026-11-10T08:05:55Z');
  const completed = Date.parse('2026-11-10T08:51:20Z');
  const completeTraceMs = completed - started;
  const proofReserveMs = 12 * 60_000;
  const verify = text => {
    const paired = job(text, 'bitget-paired');
    const timeout = /^    timeout-minutes: (\d+)$/m.exec(paired);
    assert.ok(timeout, 'the physical producer needs a bounded job window');
    assert.ok(Number(timeout[1]) * 60_000 >= completeTraceMs + proofReserveMs,
      'the fixed two-witness trace and proof reserve must fit before the job deadline');
    assert.match(paired, /run: node sdk-rs\/harness\/bitget-paired-ci\.mjs --out/);
    assert.doesNotMatch(paired, /continue-on-error|gh run rerun|retries|retry:/);
  };
  verify(source);
  const deadlineRegression = source.replace(job(source, 'bitget-paired'),
    job(source, 'bitget-paired').replace(/timeout-minutes: \d+/, 'timeout-minutes: 45'));
  assert.throws(() => verify(deadlineRegression), /fixed two-witness trace/);
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

test('Bitget paired producer is mandatory, bounded, source-selected and independently certified', () => {
  const paired = job(source, 'bitget-paired');
  assert.match(paired, /name: Rust \/ Bitget counterbalanced steady CPU/);
  verifyWorkflow(source);
  assert.match(paired, /CARGO_BUILD_JOBS: 4/);
  assert.match(paired, /node-version: 22/);
  assert.match(paired, /Public counterbalanced Bitget steady CPU/);
  assert.match(paired, /bitget-paired-ci\.mjs --out/);
  assert.match(paired, /name: bitget-paired-steady/);
  assert.match(paired, /name: Retain counterbalanced Bitget steady evidence\n        if: always\(\)/);
  assert.doesNotMatch(paired, /--baseline|--candidate|--tolerance|--threshold|continue-on-error|workflow_dispatch:/);
  assert.match(job(source, 'cpu-plan'), /needs: \[selection, gate, bitget-paired\]/);
  assert.match(job(source, 'cpu-certify'), /needs: \[selection, gate, bitget-paired, cpu-plan\]/);
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
