import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const workspace=fileURLToPath(new URL('../..',import.meta.url));
const args=process.argv.slice(2);
assert.ok(args.length<=1 && args.every(arg=>arg==='--expect-unclassified'));
const target=path.resolve(process.env.CARGO_TARGET_DIR??path.join(workspace,'target/accepted-inventory'));
const result=spawnSync(process.execPath,[path.join(workspace,'safe-cex-parity/scripts/accepted-set.mjs'),
 '--package=safe-cex-venue-binance','--jobs=2','--measure','--module=private_event_value'],
 {cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,
 env:{...process.env,RUSTC_BOOTSTRAP:'1',CARGO_BUILD_JOBS:'2',CARGO_TARGET_DIR:target}});
assert.equal(result.status,0,`${result.error??''}\n${result.stdout}\n${result.stderr}`);
process.stdout.write(result.stdout);
const rows=JSON.parse(fs.readFileSync(path.join(target,'accepted-set-results.json'),'utf8'));
const prefix='safe_cex_venue_binance::private_event_value::';
const names=['PrivateEnum','array_for_each','record','stock','string','table'].map(n=>prefix+n);
assert.equal(rows.length,7);
assert.deepEqual(rows.map(r=>r.name).sort(),[...names,prefix+'EventArray'].sort());
for(const row of rows) {
 if(args.length && names.includes(row.name)) assert.ok(row.failures.some(s=>s.includes('unclassified deserializable type')),row.name);
 else assert.deepEqual(row.failures,[],row.name);
 if(!args.length) assert.ok(row.classificationOnly,row.name);
}
console.log(args.length?'Six named missing-contract RED; existing EventArray alias passes.':'Six individual typed value/iteration/property roles and preserved EventArray alias GREEN; classifications do not execute decoders.');
