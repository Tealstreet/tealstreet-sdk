import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

test('invalid or undeclared reports are never labelled as known failures or certified passes', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'gate-verifier-'));
  context.after(() => rmSync(directory, { recursive: true, force: true }));
  const ledger = join(directory, 'sdk-rs/harness/out/ci');
  mkdirSync(ledger, { recursive: true });
  const reportPath = join(ledger, 'report.json');
  for (const status of ['FAIL', 'PASS']) {
    writeFileSync(reportPath, JSON.stringify({
      schema: 1, venue: 'unregistered-test-venue', tape: 'unknown.jsonl', books: '',
      tapeSha256: 'a'.repeat(64), private: true, status,
      failures: status === 'FAIL' ? ['unlisted divergence'] : [],
      correctnessRuns: 1, runs: 0, samples: { ts: [], rust: [] },
    }));
    writeFileSync(join(ledger, 'ledger.jsonl'), JSON.stringify({ reportPath, exitCode: status === 'PASS' ? 0 : 1 }) + '\n');
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./verify-gates.mjs', import.meta.url)), '--suite', 'binance'], {
      cwd: directory, encoding: 'utf8', timeout: 10000,
    });
    assert.ifError(result.error);
    assert.equal(result.status, 1);
    assert.match(result.stdout, /UNEXPECTED \/ INVALID RESULT unregistered-test-venue/);
    assert.match(result.stdout, /0 strict passes/);
    assert.match(result.stdout, /0 declared known-failure gates/);
    assert.doesNotMatch(result.stdout, /KNOWN FAILURE \(not parity\)/);
  }
});
