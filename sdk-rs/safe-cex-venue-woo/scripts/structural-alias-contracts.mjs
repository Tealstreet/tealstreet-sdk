import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {acceptedAliasContext} from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

assert.deepEqual(process.argv.slice(2),[]);
const workspace=fileURLToPath(new URL('../..',import.meta.url));
const names=['legacy_metadata::JsString','legacy_metadata::Rows','legacy_metadata::Fields','v3_contracts::WooV3Decimal','v3_ws_public::WooV3WsBookLevel'].map(n=>'safe_cex_venue_woo::'+n).sort();
const c=acceptedAliasContext({workspace,packageName:'safe-cex-venue-woo',aliasNames:names,jobs:1});
const own=c.artifacts.filter(a=>a.package_id===c.pkg.id&&a.target.name===c.target.name);
assert.equal(own.length,1);
const results=runProbes(c.root,c.metadata,[{package:c.pkg.name,packageId:c.pkg.id,features:own[0].features,target:c.target,types:[],publicAliases:c.aliases,defaultWitnesses:[],locals:[],delegatedLocals:[],macroOrigins:c.macroOrigins,externalTypeAliases:c.found.externalTypeAliases}],loadContracts(),{target:'native',features:[],jobs:1});
fs.writeFileSync(path.join(c.metadata.target_directory,'woo-structural-alias-results.json'),JSON.stringify(results,null,2)+'\n');
assert.deepEqual(results.map(r=>r.name).sort(),names);
const decoders=new Set(['legacy_metadata::JsString','v3_contracts::WooV3Decimal','v3_ws_public::WooV3WsBookLevel'].map(n=>'safe_cex_venue_woo::'+n));
for(const result of results){
  assert.deepEqual(result.failures,[],result.name);
  if(decoders.has(result.name)){
    assert.equal(Object.hasOwn(result,'sample'),true,result.name);
    assert.equal(Object.hasOwn(result,'variants'),true,result.name);
    assert.notEqual(result.classificationOnly,true,result.name);
  }else{
    assert.equal(result.classificationOnly,true,result.name);
    assert.equal(result.category,'non-decoder-alias',result.name);
    assert.equal(result.deserialize,false,result.name);
  }
}
console.log('WOO aliases: 3 runtime decoders; 2 specific classifications; 0 failed');
