// Exact typed constructor/Deref owner; execution proof is the native private_owner_binding.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {inventory} from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import {loadPolicy} from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
const workspace=fileURLToPath(new URL('../..',import.meta.url));
const selected=['safe_cex_venue_binance::private_classes::PrivateSocket'];
const args=process.argv.slice(2);
assert.ok(args.length<=1&&args.every(a=>a==='--expect-unclassified'));
const chosen=selected;
function cargo(args,env={}) {
 const r=spawnSync('cargo',args,{cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1',...env}});
 assert.equal(r.status,0,`${args.join(' ')}\n${r.error??''}\n${r.stderr}`);return r.stdout;
}
process.env.RUSTC_BOOTSTRAP='1';
const metadata=JSON.parse(cargo(['metadata','--locked','--format-version=1']));
const pkg=metadata.packages.find(p=>p.name==='safe-cex-venue-binance');
assert.ok(pkg&&metadata.workspace_members.includes(pkg.id));
const target=pkg.targets.find(t=>t.kind.includes('lib'));assert.ok(target);
const isolated=path.join(metadata.target_directory,'accepted-inventory');
cargo(['rustdoc','--locked','-p',pkg.name,'--jobs=1','--lib','--','-Z','unstable-options','--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],{CARGO_TARGET_DIR:isolated});
const doc=JSON.parse(fs.readFileSync(path.join(isolated,'doc',`${target.name}.json`)));
assert.equal(doc.index[String(doc.root)].name,target.name);
const policy=path.join(workspace,'safe-cex-parity/accepted-set');
const found=inventory(doc,{onlyOwners:selected,defaultTraits:loadPolicy('default-traits',policy),receiverTraits:loadPolicy('receiver-traits',policy),readSource:f=>fs.readFileSync(path.resolve(workspace,f))});
assert.deepEqual(found.types.map(t=>t.name).sort(),selected);
assert.ok(found.types.every(t=>t.deserialize===false));
assert.deepEqual(found.types[0].rawEntrypoints,[false,false]);
assert.deepEqual(found.types[0].methods.map(m=>m.name),['new']);
assert.deepEqual(found.types[0].traitMethods.map(m=>m.name),['deref']);
fs.writeFileSync(path.join(metadata.target_directory,'private-socket-inventory.json'),JSON.stringify(found.types,null,2)+'\n');
const results=runProbes(workspace,metadata,[{...found,types:found.types.filter(t=>chosen.includes(t.name)),package:pkg.name,packageId:pkg.id,target,features:[],publicAliases:[],defaultWitnesses:[],locals:[],delegatedLocals:[]}],loadContracts(),{target:'native',features:[],jobs:1});
fs.writeFileSync(path.join(metadata.target_directory,'private-socket-results.json'),JSON.stringify(results,null,2)+'\n');
assert.deepEqual(results.map(r=>r.name).sort(),chosen.slice().sort());
for(const result of results) {
 if(args[0]==='--expect-unclassified')assert.ok(result.failures.some(f=>f.includes('unclassified deserializable type')),result.name);
 else assert.deepEqual(result.failures,[],result.name);
}
console.log(`${results.length} actual native private owners: ${args[0]==='--expect-unclassified'?'named missing-contract RED':'receipts GREEN'}. Scope excludes other owners/features.`);
