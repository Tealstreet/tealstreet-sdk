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
function verifyWorkflow(text) {
  const first = job(text, 'gate'), plan = job(text, 'cpu-plan'), second = job(text, 'cpu-second'), final = job(text, 'cpu-final');
  assert.match(first, /^    name: Rust \/ replay measurement \(\$\{\{ matrix\.venue \}\}\)$/m);
  const prepare = first.slice(first.indexOf('      - name: Seal approved CPU measurement inputs'), first.indexOf('      - name: Public data'));
  assert.match(prepare, /--command prepare --suite '\$\{\{ matrix\.venue \}\}'/);
  assert.doesNotMatch(prepare, /\n        if:/, 'every first suite must seal inputs');
  const collect = first.slice(first.indexOf('      - name: Verify strict replay results'), first.indexOf('      - name: Retain CPU runtime capsule'));
  assert.match(collect, /if: always\(\)/);
  assert.match(collect, /--command collect --suite '\$\{\{ matrix\.venue \}\}'/);
  assert.doesNotMatch(collect, /^          (?:if|else)\b|rust:gate:verify/m, 'no suite bypass of shared collection');
  assert.match(first, /name: Retain the eligible original runtime bytes\n        if: steps\.cpu_collect\.outputs\.has_eligible == 'true'/);
  assert.match(plan, /needs: gate\n    if: always\(\)/);
  assert.match(plan, /matrix: \$\{\{ steps\.plan\.outputs\.matrix \}\}/);
  assert.match(plan, /--command plan --out /);
  assert.doesNotMatch(plan, /--suite /, 'planner covers the entire registered suite set');
  assert.match(second, /needs: cpu-plan\n    if: needs\.cpu-plan\.result == 'success' && needs\.cpu-plan\.outputs\.has_eligible == 'true'/);
  assert.match(second, /matrix: \$\{\{ fromJSON\(needs\.cpu-plan\.outputs\.matrix\) \}\}/);
  assert.match(second, /name: Rust \/ CPU remeasurement \(\$\{\{ matrix\.suite \}\}\)/);
  assert.match(second, /--command second --suite '\$\{\{ matrix\.suite \}\}'/);
  assert.match(second, /name: cpu-second-\$\{\{ matrix\.suite \}\}/);
  assert.match(final, /name: Rust \/ replay gate \(\$\{\{ matrix\.suite \}\}\)/);
  assert.match(final, /needs: \[gate, cpu-plan, cpu-second\]\n    if: always\(\)/);
  const suites = /suite: \[([^\]]+)\]/.exec(final);
  assert(suites);
  assert.deepEqual(suites[1].split(',').map(suite => suite.trim()).sort(), [...GATE_SUITES].sort());
  assert.match(final, /--command certify --suite '\$\{\{ matrix\.suite \}\}'/);
  for (const supplemental of [plan, second, final]) {
    assert.doesNotMatch(supplemental, /(?:yarn|npm|cargo|rustup) (?:install|build|add)|rust:build|--command prepare/);
    assert.doesNotMatch(supplemental, /continue-on-error|gh run rerun|retries|retry:/);
  }
  assert.doesNotMatch(first, /continue-on-error/);
}

test('all required suite names certify unconditionally, with an eligible-only separate runner matrix', () => verifyWorkflow(source));

test('workflow controls reject skipped final checks, unconditional second jobs and suite/local bypasses', () => {
  const changes = [
    text => text.replace("needs: cpu-plan\n    if: needs.cpu-plan.result == 'success' && needs.cpu-plan.outputs.has_eligible == 'true'", 'needs: cpu-plan\n    if: always()'),
    text => text.replace('needs: [gate, cpu-plan, cpu-second]\n    if: always()', 'needs: [gate, cpu-plan, cpu-second]\n    if: success()'),
    text => text.replace('suite: [breakoutprop,', 'suite: ['),
    text => text.replace('--command plan --out', '--command plan --suite bitget --out'),
    text => text.replace("--command collect --suite '${{ matrix.venue }}'", "rust:gate:verify --suite '${{ matrix.venue }}'"),
    text => text.replace('matrix: ${{ fromJSON(needs.cpu-plan.outputs.matrix) }}', 'matrix: { suite: [bitget] }'),
    text => text.replace('      # No install, compilation, module build, or measurement fallback here.', '      - run: cargo build\n      # No install, compilation, module build, or measurement fallback here.'),
  ];
  for (const change of changes) { const changed = change(source); assert.notEqual(changed, source); assert.throws(() => verifyWorkflow(changed)); }
});
