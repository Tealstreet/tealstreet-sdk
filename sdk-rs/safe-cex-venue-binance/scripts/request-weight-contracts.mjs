import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const workspace = fileURLToPath(new URL('../..', import.meta.url));
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
const target = path.resolve(process.env.CARGO_TARGET_DIR ?? path.join(workspace,'target/accepted-inventory'));
const run = spawnSync(process.execPath,[path.join(workspace,'safe-cex-parity/scripts/accepted-set.mjs'),
  '--package=safe-cex-venue-binance','--jobs=2','--measure','--module=request_weight'],
  {cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,
    env:{...process.env,RUSTC_BOOTSTRAP:'1',CARGO_BUILD_JOBS:'2',CARGO_TARGET_DIR:target}});
assert.equal(run.status,0,`${run.error??''}\n${run.stdout}\n${run.stderr}`);
process.stdout.write(run.stdout);
const rows=JSON.parse(fs.readFileSync(path.join(target,'accepted-set-results.json'),'utf8'));
const prefix='safe_cex_venue_binance::request_weight::';
const missing=['get_response_header','observe_request_weight','type_error'].map(n=>prefix+n);
assert.equal(rows.length,10);
for(const row of rows) {
  if(args.length && missing.includes(row.name)) assert.ok(row.failures.some(s=>s.includes('unclassified deserializable type')),row.name);
  else assert.deepEqual(row.failures,[],row.name);
  if(!args.length && missing.includes(row.name)) assert.ok(!row.classificationOnly,row.name);
}
assert.deepEqual(rows.filter(r=>missing.includes(r.name)).map(r=>r.name).sort(),missing.sort());
console.log(args.length?'Three named missing-contract RED; seven existing aliases pass.':'Three executed request-weight outputs and seven preserved aliases GREEN.');
