import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { revalidateRecordedEvidence } from './revalidate-gate.mjs';

test('an earlier valid CPU FAIL report cannot mask a later crashed revalidation', () => {
  const root = mkdtempSync(join(tmpdir(), 'revalidate-gate-'));
  mkdirSync(join(root, 'sdk-rs/harness'), { recursive: true });
  // Unit fixture controls only the validator process's I/O; the real offline
  // gate and its no-replay route are exercised in gate.test.mjs.
  writeFileSync(join(root, 'sdk-rs/harness/gate.mjs'), `
    import { readFileSync, writeFileSync, existsSync } from 'node:fs';
    if (existsSync('crash')) throw new Error('validator crashed before report');
    const report = JSON.parse(readFileSync(process.argv[process.argv.indexOf('--recorded-evidence')+1], 'utf8')).report;
    writeFileSync(process.argv[process.argv.indexOf('--report')+1], JSON.stringify(report));
    process.exitCode = 1;
  `);
  const evidence = { report: { private: false, venue: 'mexc', tape: 'public.jsonl', books: 'BTC', status: 'FAIL', failures: ['CPU miss'] } };
  try {
    const good = revalidateRecordedEvidence(evidence, { root });
    assert.equal(JSON.parse(readFileSync(good.strictPath, 'utf8')).status, 'FAIL');
    writeFileSync(join(root, 'crash'), '1');
    assert.throws(() => revalidateRecordedEvidence(evidence, { root }), /missing or invalid/);
    assert.equal(JSON.parse(readFileSync(good.strictPath, 'utf8')).status, 'FAIL', 'earlier evidence stays immutable');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
