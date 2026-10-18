// Bounded native/default retained JSON owner proofs; broader inventory remains separate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {inventory} from './accepted-inventory.mjs';
import {loadContracts} from './accepted-contract-files.mjs';
import {contractFor} from './accepted-contracts.mjs';
import {runProbes} from './accepted-probes.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const args=process.argv.slice(2);
assert.ok(args.every(v=>v==='--inventory'||/^--package=safe-cex-(types|num)$/.test(v)), 'only --inventory and --package=safe-cex-{types,num} are supported');
const selected=args.find(v=>v.startsWith('--package='))?.slice(10)??'safe-cex-types';
const cargo = args => {
  const r=spawnSync('cargo',args,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RUSTC_BOOTSTRAP:'1',CARGO_BUILD_JOBS:'1'}});
  assert.equal(r.status,0,r.stdout+'\n'+r.stderr);return r.stdout;
};
const metadata=JSON.parse(cargo(['metadata','--locked','--format-version','1']));
const pkg=metadata.packages.find(p=>p.name===selected);
const target=pkg.targets.find(t=>t.kind.includes('lib'));
cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs','1','--','-Z','unstable-options','--output-format','json','--document-private-items','--document-hidden-items','-A','rustdoc::all']);
const doc=JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc',selected.replaceAll('-','_')+'.json'),'utf8'));
const typeOwners=['RetainedJsonInput','Capture','BufferVisitor','raw_node','from_value_at_depth','into_value','deserialize_retained_json','deserialize_present_retained_json'].map(name=>'safe_cex_types::retained_json::'+name);
typeOwners.push('safe_cex_types::retained_json::parser::parse');
const numOwners=['JsJsonValue','quoted','quoted_utf16','number','write','write_retained'].map(name=>'safe_cex_num::json_stringify::'+name);
const onlyOwners=selected==='safe-cex-types'?typeOwners:numOwners;
const observed=inventory(doc,{onlyOwners});
const configuration={target:'native',features:[],jobs:1};
const receipt={...observed,package:pkg.name,packageId:pkg.id,target,features:[]};
fs.writeFileSync(path.join(metadata.target_directory,'retained-owner-inventory-'+selected+'.json'),JSON.stringify(receipt,null,2)+'\n');
if (!process.argv.includes('--inventory')) {
  const contracts=loadContracts(path.join(root,'safe-cex-parity/accepted-set/contracts/'+selected+'.json'));
  for(const owner of observed.types)contractFor(owner,contracts);
  const results=runProbes(root,metadata,[receipt],contracts,configuration);
  assert.deepEqual(results.filter(r=>r.failures.length),[],'retained native owner probes failed');
}
console.log('retained JSON: '+onlyOwners.length+' compiler-owned native/default owners; full model/local/target inventories remain separate');
