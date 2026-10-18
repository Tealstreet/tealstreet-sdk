// The seventeen compiler-selected non-socket inverse owners only.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {inventory} from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import {loadPolicy} from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
const root=fileURLToPath(new URL('../..', import.meta.url));
const selected=[
 'binanceinverse_types::BinanceIncomeRecord','binanceinverse_types::BinanceInversePositionRisk',
 'inverse::BinanceInverseExchange','inverse::api::optional','inverse::globals::table',
 ...['contains','enum_value','global','install_async','install_sync','optional','request','store','v'].map(n=>'inverse::ops::'+n),
 'inverse::orders::equal','inverse::orders::inverse',
].map(n=>'safe_cex_venue_binance::'+n).sort();
const aliasName='safe_cex_venue_binance::binanceinverse_types::GetPositionRiskResponse';
assert.equal(selected.length,16);
const args=process.argv.slice(2);
assert.ok(args.length<=1 && args.every(a=>a==='--expect-unclassified' || a.startsWith('--only=')));
const allNames=[...selected,aliasName].sort();
const chosen=args[0]?.startsWith('--only=')?args[0].slice(7).split(','):allNames;
assert.ok(chosen.length && new Set(chosen).size===chosen.length && chosen.every(n=>allNames.includes(n)));
function cargo(args, env={}) {
 const r=spawnSync('cargo',args,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1',...env}});
 assert.equal(r.status,0,args.join(' ')+'\n'+r.stdout+'\n'+r.stderr);return r.stdout;
}
const metadata=JSON.parse(cargo(['metadata','--locked','--format-version=1']));
const pkg=metadata.packages.find(p=>p.name==='safe-cex-venue-binance');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target=pkg.targets.find(t=>t.kind.includes('lib'));
const discovery=path.join(metadata.target_directory,'accepted-inventory');
cargo(['rustdoc','--locked','-p',pkg.name,'--jobs=1','--lib','--','-Z','unstable-options','--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],{RUSTC_BOOTSTRAP:'1',CARGO_TARGET_DIR:discovery});
const found=inventory(JSON.parse(fs.readFileSync(path.join(discovery,'doc',target.name+'.json'))),{defaultTraits:loadPolicy('default-traits'),receiverTraits:loadPolicy('receiver-traits'),readSource:f=>fs.readFileSync(path.join(root,f))});
const types=found.types.filter(t=>selected.includes(t.name));
const aliases=found.publicAliases.filter(t=>t.name===aliasName);
assert.deepEqual(types.map(t=>t.name).sort(),selected);assert.equal(aliases.length,1);
fs.writeFileSync(path.join(metadata.target_directory,'inverse-accepted-inventory.json'),JSON.stringify({types,aliases},null,2)+'\n');
const results=runProbes(root,metadata,[{package:pkg.name,packageId:pkg.id,target,types:types.filter(t=>chosen.includes(t.name)),publicAliases:aliases.filter(t=>chosen.includes(t.name)),defaultWitnesses:[],locals:[],delegatedLocals:[],macroOrigins:[],externalTypeAliases:[]}],loadContracts(),{target:'native',features:[],jobs:1});
fs.writeFileSync(path.join(metadata.target_directory,'inverse-accepted-results.json'),JSON.stringify(results,null,2)+'\n');
assert.deepEqual(results.map(r=>r.name).sort(),chosen.slice().sort());
if(process.argv[2]==='--expect-unclassified') {
 assert.ok(results.every(r=>r.failures.length && r.failures.some(f=>/unclassified|needs an accepted-set contract/.test(f))));
 console.log('All 17 exact production owners fail missing-classification checks by name.');
} else {
 for(const r of results){assert.deepEqual(r.failures,[],r.name);assert.ok(!r.classificationOnly,r.name+': native runtime execution required');}
 console.log(`Inverse accepted cohort: ${results.length} actual native owners pass, zero classification-only receipts.`);
}
