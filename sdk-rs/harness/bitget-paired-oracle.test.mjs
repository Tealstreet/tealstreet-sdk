import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { collectBitgetOracle, BitgetOracleChanged, assertBitgetOracleUnchanged } from './bitget-paired-oracle.mjs';

const output = (max = 1100000, cpuMs = 100) => JSON.stringify({ impl: 'ts',
  markets: [{ symbol: 'BTCUSDT', limits: { amount: { max } } }], tickers: [], books: {},
  checkpoints: [], commands: [], metrics: { cpuMs } });

test('published oracle changes refuse certification, including an unchanged Rust build', () => {
  assert.throws(() => assertBitgetOracleUnchanged({ baseline: output(), candidate: output(160000) }), error => {
    assert.ok(error instanceof BitgetOracleChanged);
    assert.equal(error.disposition, 'NOT-CERTIFIED');
    assert.equal(error.certificate, null);
    assert.match(error.message, /oracle output changed/);
    return true;
  });
});

test('identical published output ignores CPU observations and object key order', () => {
  const candidate = JSON.parse(output(1100000, 999));
  candidate.markets[0] = { limits: candidate.markets[0].limits, symbol: 'BTCUSDT' };
  assert.doesNotThrow(() => assertBitgetOracleUnchanged({ baseline: output(), candidate: JSON.stringify(candidate) }));
});

test('incomplete, ambiguous or non-TS outputs fail as collection errors, not oracle drift', () => {
  for (const candidate of ['{}', '{"impl":"ts","impl":"ts"}', output().replace('"ts"', '"rust"')]) {
    assert.throws(() => assertBitgetOracleUnchanged({ baseline: output(), candidate }), error => {
      assert.ok(!(error instanceof BitgetOracleChanged));
      return true;
    });
  }
});

function repository(t, candidateMax, failBaseline = false) {
  const parent = resolve('sdk-rs/harness/out'); mkdirSync(parent, { recursive: true });
  const directory = mkdtempSync(resolve(parent, 'oracle-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const root = resolve(directory, 'source'); mkdirSync(root);
  const evidence = resolve(directory, 'evidence'); mkdirSync(evidence);
  const git = (...args) => {
    const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git('init', '-b', 'master'); git('config', 'user.name', 'Harness test'); git('config', 'user.email', 'test@example.invalid');
  mkdirSync(resolve(root, 'sdk-rs/harness/fixtures'), { recursive: true });
  writeFileSync(resolve(root, 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz'), 'fixed tape');
  let revision = 0;
  const commit = (max, fail) => {
    writeFileSync(resolve(root, 'sdk-rs/harness/replay.mjs'),
      `// source revision ${++revision}\nimport {writeFileSync} from 'node:fs';\n${fail ? 'process.exit(7);' : ''}\n` +
      `writeFileSync(process.argv[process.argv.indexOf('--out')+1], ${JSON.stringify(output(max))});`);
    git('add', 'sdk-rs'); git('commit', '-m', String(max));
    return git('rev-parse', 'HEAD');
  };
  const baseline = commit(1100000, failBaseline);
  const candidate = commit(candidateMax, false);
  return { root, directory: evidence, git, authority: { baseline, candidate, runId: '1', jobId: '2', runnerId: '3', witness: 'tip' } };
}

test('real source checkouts retain both TS observations and restore candidate on oracle drift', t => {
  const x = repository(t, 160000);
  assert.throws(() => collectBitgetOracle(x), BitgetOracleChanged);
  assert.equal(x.git('rev-parse', 'HEAD'), x.authority.candidate);
  const receipt = JSON.parse(readFileSync(resolve(x.directory, 'oracle-receipt.json')));
  assert.equal(receipt.sources.baseline.source, x.authority.baseline);
  assert.equal(receipt.sources.candidate.source, x.authority.candidate);
  assert.notEqual(receipt.sources.baseline.outputSha256, receipt.sources.candidate.outputSha256);
});

test('an oracle replay failure restores candidate and never becomes NOT-CERTIFIED drift', t => {
  const x = repository(t, 160000, true);
  assert.throws(() => collectBitgetOracle(x), error => {
    assert.ok(!(error instanceof BitgetOracleChanged));
    assert.match(error.message, /baseline TS replay failed: 7/);
    return true;
  });
  assert.equal(x.git('rev-parse', 'HEAD'), x.authority.candidate);
});

test('equal published output from different source commits continues without refusal', t => {
  const x = repository(t, 1100000);
  const receipt = collectBitgetOracle(x);
  assert.equal(receipt.sources.baseline.outputSha256, receipt.sources.candidate.outputSha256);
  assert.notEqual(receipt.sources.baseline.source, receipt.sources.candidate.source);
  assert.equal(x.git('rev-parse', 'HEAD'), x.authority.candidate);
});
