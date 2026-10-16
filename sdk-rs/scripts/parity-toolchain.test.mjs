import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const workflow = readFileSync(new URL('../../.github/workflows/rust.yml', import.meta.url), 'utf8');
const bootstrap = '      - name: Bootstrap Rust\n        uses: dtolnay/rust-toolchain@stable\n';
const setup = bootstrap + '        with:\n          components: rustfmt\n';

function audit(source) {
  const jobs = [...source.matchAll(/^  types:\n([\s\S]*?)(?=^  [\w-]+:|$(?![\s\S]))/gm)];
  assert.equal(jobs.length, 1, 'exactly one TypeScript parity job');
  const body = jobs[0][1];
  assert.doesNotMatch(body, /^    (?:if|continue-on-error):/m, 'parity job must be unconditional and fail closed');
  const steps = [...body.matchAll(/^      - name: Bootstrap Rust\n([\s\S]*?)(?=^      - |$(?![\s\S]))/gm)];
  assert.equal(steps.length, 1, 'exactly one parity stable bootstrap');
  const block = steps[0][1];
  assert.match(block, /^        uses: dtolnay\/rust-toolchain@stable$/m, 'parity generator uses default stable');
  assert.doesNotMatch(block, /^        (?:if|continue-on-error):/m, 'parity stable bootstrap must be unconditional and fail closed');
  const components = block.match(/^        with:\n          components: ([a-z, -]+)$/m)?.[1].split(/[, ]+/);
  assert.ok(components?.includes('rustfmt'), 'parity default stable must explicitly install rustfmt');
  const consumer = body.match(/^      - name: Check venue oracle binance\n([\s\S]*?)(?=^      - |$(?![\s\S]))/m);
  assert.ok(consumer, 'root Binance generator must remain present');
  assert.match(consumer[1], /^        working-directory: \.$/m, 'Binance generator runs at repository root');
  assert.ok(steps[0].index < consumer.index, 'parity stable rustfmt must be installed before the root Binance generator');
}

test('parity root generators have unconditional stable rustfmt before use', () => audit(workflow));

test('missing stable rustfmt is rejected', () => {
  assert.ok(workflow.includes(setup), 'control must change actual parity setup');
  assert.throws(() => audit(workflow.replace(setup, bootstrap)), /default stable must explicitly install rustfmt/);
});

test('clippy alone does not satisfy the root rustfmt prerequisite', () => {
  assert.throws(() => audit(workflow.replace(setup, setup.replace('components: rustfmt', 'components: clippy'))), /default stable must explicitly install rustfmt/);
});

test('rustfmt installed only in the native job is rejected', () => {
  const withoutParity = workflow.replace(setup, bootstrap);
  assert.ok(withoutParity.indexOf(bootstrap) < withoutParity.indexOf('\n  types:\n'), 'control targets earlier native setup');
  assert.throws(() => audit(withoutParity.replace(bootstrap, setup)), /default stable must explicitly install rustfmt/);
});

test('rustfmt setup after its consumer is rejected', () => {
  const moved = workflow.replace(setup, '').replace('      - name: Check live Asterdex TypeScript equivalence\n', setup + '      - name: Check live Asterdex TypeScript equivalence\n');
  assert.throws(() => audit(moved), /installed before the root Binance generator/);
});

test('a conditional stable bootstrap is rejected', () => {
  assert.throws(() => audit(workflow.replace(setup, setup.replace('        uses:', '        if: false\n        uses:'))), /bootstrap must be unconditional and fail closed/);
});

test('a bootstrap that ignores failure is rejected', () => {
  assert.throws(() => audit(workflow.replace(setup, setup.replace('        uses:', '        continue-on-error: true\n        uses:'))), /bootstrap must be unconditional and fail closed/);
});
