import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { parseCohortJson } from './cpu-cohort.mjs';

export class BitgetOracleChanged extends Error {
  constructor() {
    super('NOT-CERTIFIED: Bitget TS oracle output changed between baseline and candidate; arm-matched correctness is not available');
    this.disposition = 'NOT-CERTIFIED';
    this.certificate = null;
  }
}

export function assertBitgetOracleUnchanged(outputs) {
  const published = Object.fromEntries(['baseline', 'candidate'].map(arm => {
    const value = parseCohortJson(outputs[arm]);
    if (value.impl !== 'ts' || !Array.isArray(value.markets) || !Array.isArray(value.checkpoints) ||
        !Array.isArray(value.commands) || !value.metrics) throw new Error(`invalid ${arm} TS oracle replay`);
    const { metrics, ...result } = value;
    return [arm, result];
  }));
  if (!isDeepStrictEqual(published.baseline, published.candidate)) throw new BitgetOracleChanged();
}

const insist = (ok, message) => { if (!ok) throw new Error(`Bitget oracle collection: ${message}`); };
const sha = bytes => createHash('sha256').update(bytes).digest('hex');

// Observe each source's TS output before a common-oracle Rust comparison can certify.
// This is a refusal guard; it never supplies arm-matched correctness or CPU authority.
export function collectBitgetOracle({ root, directory, authority }) {
  const git = (...args) => {
    const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
    insist(result.status === 0 && !result.error && !result.signal, `git ${args[0]} failed: ${result.stderr}`);
    return result.stdout.trim();
  };
  insist(git('rev-parse', 'HEAD') === authority.candidate && !git('status', '--porcelain'),
    'clean authenticated candidate checkout required');
  const tape = 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz';
  const tapeBytes = readFileSync(resolve(root, tape));
  const tapePath = resolve(directory, 'oracle-tape.jsonl.gz');
  writeFileSync(tapePath, tapeBytes, { flag: 'wx' });
  const outputs = {}, sources = {};
  try {
    for (const arm of ['baseline', 'candidate']) {
      const source = authority[arm];
      git('checkout', '--detach', source);
      insist(git('rev-parse', 'HEAD') === source && !git('status', '--porcelain'), `dirty ${arm} oracle source`);
      const out = resolve(directory, `oracle-${arm}.json`);
      const result = spawnSync(process.execPath, ['--expose-gc', '--import', 'tsx',
        'sdk-rs/harness/replay.mjs', '--impl', 'ts', '--venue', 'bitget', '--tape', tapePath,
        '--books', 'BTCUSDT,ETHUSDT', '--out', out], {
        cwd: root, encoding: 'utf8', timeout: 125_000,
        env: { ...process.env, ESBUILD_WORKER_THREADS: '0' }, maxBuffer: 8 * 1024 * 1024,
      });
      writeFileSync(resolve(directory, `oracle-${arm}.log`), (result.stdout ?? '') + (result.stderr ?? ''), { flag: 'wx' });
      insist(result.status === 0 && !result.error && !result.signal, `${arm} TS replay failed: ${result.error ?? result.signal ?? result.status}`);
      outputs[arm] = readFileSync(out, 'utf8');
      sources[arm] = { source, tree: git('rev-parse', 'HEAD^{tree}'), outputSha256: sha(outputs[arm]) };
      insist(!git('status', '--porcelain') && git('rev-parse', 'HEAD') === source, `${arm} TS replay changed source`);
    }
  } finally {
    git('checkout', '--detach', authority.candidate);
  }
  const receipt = { runId: authority.runId, jobId: authority.jobId, runnerId: authority.runnerId,
    witness: authority.witness, sources, tape, tapeSha256: sha(tapeBytes), books: 'BTCUSDT,ETHUSDT',
    nodeSha256: sha(readFileSync(process.execPath)) };
  writeFileSync(resolve(directory, 'oracle-receipt.json'), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx' });
  try { assertBitgetOracleUnchanged(outputs); }
  catch (error) {
    if (error instanceof BitgetOracleChanged) error.receipt = receipt;
    throw error;
  }
  return receipt;
}
