import assert from 'node:assert/strict';
import { test } from 'node:test';
import { coverageExitCode } from '../src/cli.ts';
import type { ReconcileResult } from '../src/manifest.ts';

test('coverage rejects added symbols even when no serialized files change', () => {
  const result: ReconcileResult = {
    added: ['packages/safe-cex/src/control.ts::mustHaveManifestRow (function)'],
    orphaned: [],
    changed: [],
  };
  assert.equal(coverageExitCode(true, result), 1, 'an added symbol must independently fail check mode');
  assert.equal(coverageExitCode(false, result), 0, 'write mode must still succeed');
});

test('coverage keeps orphan and changed-file failures independent and accepts a clean check', () => {
  assert.equal(coverageExitCode(true, { added: [], orphaned: ['removed'], changed: [] }), 1);
  assert.equal(coverageExitCode(true, { added: [], orphaned: [], changed: ['format-only'] }), 1);
  assert.equal(coverageExitCode(true, { added: [], orphaned: [], changed: [] }), 0);
});
