// Exact four-owner scope on the maintained compiler/discovery/probe path.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=fileURLToPath(new URL('../..',import.meta.url));
const names=['BinanceExchange','BinancePMExchange','BinanceSpotExchange','ExchangeState'];
const args=process.argv.slice(2);
assert.ok(args.every(a=>['--expect-unclassified','--expect-mismatch','--default'].includes(a)||a.startsWith('--only=')));
const only=args.find(a=>a.startsWith('--only='))?.slice(7).split(',')??names;
assert.ok(only.length && new Set(only).size===only.length && only.every(n=>names.includes(n)));
const owners=only.map(n=>'safe_cex_venue_binance::exchange_classes::'+n).sort();
const target=path.resolve(root,process.env.CARGO_TARGET_DIR??'target/accepted-inventory');
const run=spawnSync(process.execPath,['safe-cex-parity/scripts/accepted-set.mjs','--package=safe-cex-venue-binance','--jobs=1','--measure',...(args.includes('--default')?[]:['--features=native']),...owners.map(n=>'--owner='+n)],{cwd:root,stdio:'inherit',env:{...process.env,CARGO_INCREMENTAL:'0',CARGO_BUILD_JOBS:'1',RUSTC_BOOTSTRAP:'1',CARGO_TARGET_DIR:target}});
assert.equal(run.status,0,run.error??'canonical discovery/measurement failed');
const rows=JSON.parse(fs.readFileSync(path.join(target,'accepted-set-results.json')));
assert.deepEqual([...new Set(rows.map(r=>r.name))].sort(),owners);
for(const owner of owners){
 const native=rows.filter(r=>r.name===owner&&r.configuration.target==='native'&&r.compilerFeatures.includes('native'));
 assert.equal(native.length,1,owner+': exactly one native receipt');
 const row=native[0];assert.ok(!row.classificationOnly,owner+': executed raw owner required');
 if(args.includes('--expect-unclassified')) assert.ok(row.failures.some(f=>f.includes('unclassified deserializable type')),owner);
 else if(args.includes('--expect-mismatch')) assert.ok(row.failures.some(f=>f.includes('function output mismatch')),owner);
 else assert.deepEqual(row.failures,[],owner);
 if(args.includes('--default')) assert.ok(rows.some(r=>r.name===owner&&r.status==='native-gated'&&r.observed===false),owner+': retain explicit unexecuted default profile');
}
console.log(`${owners.length} class/State owners: ${args.includes('--expect-unclassified')?'named missing-contract RED':args.includes('--expect-mismatch')?'compiled named mismatch RED':'executed native GREEN'}.`);
