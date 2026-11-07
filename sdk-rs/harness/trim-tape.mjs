// Cut a recorded tape to its first seconds and gzip it, for a fixture small
// enough to commit. Keep every HTTP anchor inside the selected window.
//
//   corepack yarn rust:trim-tape --tape <in.jsonl> --seconds 6 --out <out.jsonl.gz>
import { readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gzipSync } from 'node:zlib';

const { values } = parseArgs({
  options: { tape: { type: 'string' }, seconds: { type: 'string', default: '6' }, 'http-limit': { type: 'string', default: '0' }, out: { type: 'string' } },
});
const limit = Number(values.seconds) * 1000;
// Sequence recovery tapes may need every cached snapshot, including stale ones.
const httpLimit = Number(values['http-limit']);
if (!Number.isInteger(httpLimit) || httpLimit < 0) throw new Error('http-limit must be a nonnegative integer (0 keeps every response)');
const lines = readFileSync(values.tape, 'utf8').split('\n').filter(Boolean);
const served = new Map();
const kept = lines.filter((line) => {
  const record = JSON.parse(line);
  if (record.type === 'meta' || record.type === 'config') return true;
  if (record.t > limit) return false;
  if (record.type !== 'http' || httpLimit === 0) return true;
  const key = `${record.method} ${record.url}`;
  served.set(key, (served.get(key) ?? 0) + 1);
  return served.get(key) <= httpLimit;
});
writeFileSync(values.out, gzipSync(`${kept.join('\n')}\n`, { level: 9 }));
console.log(JSON.stringify({ out: values.out, records: kept.length, of: lines.length }));
