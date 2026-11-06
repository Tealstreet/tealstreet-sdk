// Refusal authority is separate from every CPU certificate path.
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { parseCohortJson } from './cpu-cohort.mjs';
import { bitgetAnchorAuthority } from './bitget-paired-steady.mjs';
import { assertBitgetOracleUnchanged, BitgetOracleChanged } from './bitget-paired-oracle.mjs';

const refusals = new WeakSet();
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const insist = (ok, message) => { if (!ok) throw new Error(`Bitget oracle refusal: ${message}`); };
const read = (entries, name) => { insist(entries.has(name), `missing ${name}`); return parseCohortJson(entries.get(name)); };

export function readBitgetOracleRefusal({ entries, authority, sourceTrees, tapeSha256 }) {
  const anchor = bitgetAnchorAuthority(authority);
  const result = read(entries, 'steady-assessment.json');
  const message = new BitgetOracleChanged().message;
  insist(result.valid === false && result.disposition === 'NOT-CERTIFIED' && result.certified === false &&
    result.certificate === null && isDeepStrictEqual(result.errors, [message]), 'not an exact non-certifying refusal');
  const receipt = result.oracle, witness = receipt?.witness;
  insist(witness === 'tip' || witness === 'anchor', 'unknown witness');
  insist(![...entries.keys()].some(name => name.startsWith(`${witness}/records/`) ||
    (witness === 'tip' && name.startsWith('anchor/'))), 'refused witness contains measurement records');
  const selected = witness === 'anchor' ? anchor : authority;
  insist(receipt.runId === authority.runId && receipt.jobId === authority.jobId && receipt.runnerId === authority.runnerId &&
    receipt.tape === 'sdk-rs/harness/fixtures/bitget-public.jsonl.gz' && receipt.tapeSha256 === tapeSha256 &&
    /^[a-f0-9]{64}$/.test(tapeSha256 ?? '') && receipt.books === 'BTCUSDT,ETHUSDT' &&
    /^[a-f0-9]{64}$/.test(receipt.nodeSha256 ?? ''), 'case or physical producer differs');
  insist(isDeepStrictEqual(receipt, read(entries, `${witness}/oracle-receipt.json`)), 'receipt differs');
  const outputs = {};
  for (const arm of ['baseline', 'candidate']) {
    const raw = entries.get(`${witness}/oracle-${arm}.json`), source = receipt.sources?.[arm];
    insist(raw && source?.source === selected[arm] && /^[a-f0-9]{40}$/.test(sourceTrees?.[selected[arm]] ?? '') &&
      source.tree === sourceTrees[selected[arm]] && source.outputSha256 === sha(raw), 'source tree or raw oracle bytes differ');
    outputs[arm] = raw;
  }
  let changed = false;
  try { assertBitgetOracleUnchanged(outputs); }
  catch (error) { if (!(error instanceof BitgetOracleChanged)) throw error; changed = true; }
  insist(changed, 'no actual published oracle difference');
  const refusal = Object.freeze({ disposition: 'NOT-CERTIFIED', certified: false, certificate: null,
    reason: message, witness, authority: selected });
  refusals.add(refusal);
  return refusal;
}

export const isBitgetOracleRefusal = value => refusals.has(value);
