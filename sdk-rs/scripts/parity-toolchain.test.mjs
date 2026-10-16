import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import test from 'node:test';

// Reads whichever workflow carries the tier; see rust-ci-wiring.test.mjs.
const CANDIDATES = [
  '../../mirrors/tealstreet-sdk/.github/workflows/rust.yml',
  '../../.github/workflows/rust.yml',
];
const found = CANDIDATES.map((rel) => new URL(rel, import.meta.url)).find((url) => existsSync(url));
assert.ok(found, 'no mirror workflow found');
const workflow = readFileSync(found, 'utf8');
// The mirror's Actions policy allows GitHub-owned actions only, so stable and
// rustfmt arrive through the image's own rustup.
const setup = '      - name: Bootstrap Rust\n        run: rustup toolchain install stable --profile minimal --component rustfmt --no-self-update\n';
// The same step without rustfmt: what the mutation controls below swap in.
const plain = '      - name: Bootstrap Rust\n        run: rustup toolchain install stable --profile minimal --no-self-update\n';

function audit(source) {
  const jobs = [...source.matchAll(/^  types:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/gm)];
  assert.equal(jobs.length, 1, 'exactly one TypeScript parity job');
  const body = jobs[0][1];
  assert.doesNotMatch(body, /^    (?:if|continue-on-error):/m, 'parity job must be unconditional and fail closed');
  const steps = [...body.matchAll(/^      - name: Bootstrap Rust\n([\s\S]*?)(?=^      - |$(?![\s\S]))/gm)];
  assert.equal(steps.length, 1, 'exactly one parity stable setup');
  const block = steps[0][1];
  assert.match(block, /^        run: rustup toolchain install stable /m, 'parity generator installs stable through rustup');
  assert.doesNotMatch(block, /^        (?:if|continue-on-error):/m, 'parity stable setup must be unconditional and fail closed');
  assert.match(block, /--component rustfmt\b/, 'parity default stable must explicitly install rustfmt');
  const consumer = body.match(/^      - name: Check venue oracle binance\n([\s\S]*?)(?=^      - |$(?![\s\S]))/m);
  assert.ok(consumer, 'root Binance generator must remain present');
  assert.match(consumer[1], /^        working-directory: \.$/m, 'Binance generator runs at repository root');
  assert.ok(steps[0].index < consumer.index, 'parity stable rustfmt must be installed before the root Binance generator');
}

test('parity root generators have unconditional stable rustfmt before use', () => audit(workflow));

test('missing stable rustfmt is rejected', () => {
  assert.ok(workflow.includes(setup), 'control must change actual parity setup');
  assert.throws(() => audit(workflow.replace(setup, plain)), /default stable must explicitly install rustfmt/);
});

test('clippy alone does not satisfy the root rustfmt prerequisite', () => {
  assert.throws(() => audit(workflow.replace(setup, setup.replace('--component rustfmt', '--component clippy'))), /default stable must explicitly install rustfmt/);
});

test('rustfmt installed only in the native job is rejected', () => {
  const withoutParity = workflow.replace(setup, plain);
  assert.ok(withoutParity.indexOf(setup) < withoutParity.indexOf('\n  types:\n'), 'control targets earlier native setup');
  assert.throws(() => audit(withoutParity.replace(setup, setup)), /default stable must explicitly install rustfmt/);
});

test('rustfmt setup after its consumer is rejected', () => {
  // MOVE it, rather than leaving a second bootstrap behind: two would trip the
  // "exactly one" assertion instead of the ordering one this control is for.
  const moved = workflow
    .replace(setup, '')
    .replace('      - name: Check live Asterdex TypeScript equivalence\n', setup + '      - name: Check live Asterdex TypeScript equivalence\n');
  assert.throws(() => audit(moved), /installed before the root Binance generator/);
});

test('a conditional stable setup is rejected', () => {
  assert.throws(() => audit(workflow.replace(setup, setup.replace('        run:', '        if: false\n        run:'))), /setup must be unconditional and fail closed/);
});

test('a setup that ignores failure is rejected', () => {
  assert.throws(() => audit(workflow.replace(setup, setup.replace('        run:', '        continue-on-error: true\n        run:'))), /setup must be unconditional and fail closed/);
});
