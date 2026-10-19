// Cut a recorded tape to its first seconds and gzip it, for a fixture small
// enough to commit. Keep every HTTP anchor inside the selected window.
//
//   corepack yarn rust:trim-tape --tape <in.jsonl> --seconds 6 --out <out.jsonl.gz>
import { readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { gzipSync } from 'node:zlib';

const { values } = parseArgs({
  options: { tape: { type: 'string' }, seconds: { type: 'string', default: '6' }, out: { type: 'string' } },
});
const limit = Number(values.seconds) * 1000;
const lines = readFileSync(values.tape, 'utf8').split('\n').filter(Boolean);
const kept = lines.filter((line) => {
  const record = JSON.parse(line);
  return record.type === 'meta' || record.t <= limit;
});
writeFileSync(values.out, gzipSync(`${kept.join('\n')}\n`, { level: 9 }));
console.log(JSON.stringify({ out: values.out, records: kept.length, of: lines.length }));
