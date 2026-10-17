import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import test from 'node:test';
import {inventory} from './accepted-inventory.mjs';
import {loadContracts} from './accepted-contract-files.mjs';
import {contractFor} from './accepted-contracts.mjs';
import {runProbes} from './accepted-probes.mjs';

const root=fileURLToPath(new URL('../..',import.meta.url));
const decoders=['decode','native_open_positions','format_open_positions','format_balance','format_positions','format_orders']
  .map(name=>`safe_cex_venue_breakoutprop::portfolio::${name}`);
const wrappers=['fetch_balance','fetch_positions','fetch_open_positions','fetch_orders']
  .map(name=>`safe_cex_venue_breakoutprop::portfolio_reads::${name}`);
function cargo(args,env={}) {
  const result=spawnSync('cargo',args,{cwd:root,encoding:'utf8',timeout:180000,
    maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1',...env}});
  assert.equal(result.status,0,result.stderr);
  return result.stdout;
}
test('Breakout portfolio contracts execute the production native readers',{timeout:300000},()=>{
  const metadata=JSON.parse(cargo(['metadata','--locked','--no-deps','--format-version=1']));
  const pkg=metadata.packages.find(row=>row.name==='safe-cex-venue-breakoutprop');
  const target=pkg.targets.find(row=>row.kind.includes('lib'));
  cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs','1','--','-Z','unstable-options',
    '--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],{RUSTC_BOOTSTRAP:'1'});
  const doc=JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc',`${target.name}.json`)));
  const found=inventory(doc,{onlyOwners:[...decoders,...wrappers]});
  assert.deepEqual(found.types.map(owner=>owner.name).sort(),[...decoders,...wrappers].sort());
  const contracts=loadContracts(fileURLToPath(new URL('../accepted-set/contracts/safe-cex-venue-breakoutprop.json',import.meta.url)));
  for(const owner of found.types) contractFor(owner,contracts);
  const selected={...found,types:found.types.filter(owner=>decoders.includes(owner.name)),
    package:pkg.name,packageId:pkg.id,target,features:[],publicAliases:[],defaultWitnesses:[],locals:[],delegatedLocals:[]};
  const results=runProbes(root,metadata,[selected],contracts,{target:'native',features:[],jobs:1});
  assert.deepEqual(results.map(result=>result.name).sort(),[...decoders].sort());
  for(const result of results) assert.deepEqual(result.failures,[],result.name);
});
