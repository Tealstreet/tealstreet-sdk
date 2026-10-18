import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {acceptedAliasContext} from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const args=process.argv.slice(2);
assert.ok(args.length<=1 && args.every(arg=>arg==='--expect-unclassified'||arg.startsWith('--expect-mismatch=')));
const mismatch=args.find(arg=>arg.startsWith('--expect-mismatch='))?.slice('--expect-mismatch='.length);
const workspace=fileURLToPath(new URL('../..',import.meta.url));
const names=['position_metadata::PositionMetadataHandle','wire::FetchContractsResponse','wire::FetchKlineResponse','public_bodies::NativeCallback','collection_reads::Callback','trading_rest::Body','trading_rest::AsyncCallback'].map(n=>'safe_cex_venue_bitrue::'+n).sort();
if(mismatch)assert.ok(names.includes('safe_cex_venue_bitrue::'+mismatch),'unknown mismatch owner');
const c=acceptedAliasContext({workspace,packageName:'safe-cex-venue-bitrue',aliasNames:names,jobs:1});
const own=c.artifacts.filter(a=>a.package_id===c.pkg.id&&a.target.name===c.target.name);
assert.equal(own.length,1);
fs.writeFileSync(path.join(c.metadata.target_directory,'bitrue-public-alias-inventory.json'),JSON.stringify(c.aliases,null,2)+'\n');
fs.writeFileSync(path.join(c.metadata.target_directory,'bitrue-public-alias-context.json'),JSON.stringify({packageId:c.pkg.id,features:own[0].features,macroOrigins:c.macroOrigins,claims:c.aliases.map(a=>({name:a.name,claim:a.claim})),externalRoots:c.found.externalTypeAliases},null,2)+'\n');
const results=runProbes(c.root,c.metadata,[{package:c.pkg.name,packageId:c.pkg.id,features:own[0].features,target:c.target,types:[],publicAliases:c.aliases,defaultWitnesses:[],locals:[],delegatedLocals:[],macroOrigins:c.macroOrigins,externalTypeAliases:c.found.externalTypeAliases}],loadContracts(),{target:'native',features:own[0].features,jobs:1});
fs.writeFileSync(path.join(c.metadata.target_directory,'bitrue-public-alias-results.json'),JSON.stringify(results,null,2)+'\n');
assert.deepEqual(results.map(r=>r.name).sort(),names);
if(args.includes('--expect-unclassified')){
 for(const r of results)assert.ok(r.failures.some(f=>f.includes('public alias needs an accepted-set contract')),r.name);
 console.log('Seven actual compiler-discovered Bitrue aliases fail missing classification by name.');
}else if(mismatch){
 assert.deepEqual(results.filter(r=>r.failures.length).map(r=>r.name),['safe_cex_venue_bitrue::'+mismatch]);
 console.log('Compiled native mismatch detected for '+mismatch);
}else{
 for(const r of results){
  assert.deepEqual(r.failures,[],r.name);
  if(!r.name.includes('::wire::')){
   assert.equal(r.classificationOnly,true,r.name);
   assert.equal(r.category,'non-decoder-alias',r.name);
   assert.equal(r.deserialize,false,r.name);
  }else assert.equal(r.classificationOnly,undefined,r.name);
 }
 assert.equal(results.filter(r=>r.classificationOnly).length,5);
 console.log('Seven actual compiler-discovered Bitrue aliases: native decoder2, specific classification5, zero failed receipts.');
}
