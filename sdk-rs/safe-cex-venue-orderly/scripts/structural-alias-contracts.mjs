import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {acceptedAliasContext} from '../../safe-cex-parity/scripts/accepted-package-context.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

assert.deepEqual(process.argv.slice(2),[]);
const workspace=fileURLToPath(new URL('../..',import.meta.url));
const names=['ws_controls::Value','ws_controls::Result','ws_controls::Entries'].map(n=>'safe_cex_venue_orderly::'+n).sort();
const c=acceptedAliasContext({workspace,packageName:'safe-cex-venue-orderly',aliasNames:names,jobs:1});
const own=c.artifacts.filter(a=>a.package_id===c.pkg.id&&a.target.name===c.target.name);
assert.equal(own.length,1);
const results=runProbes(c.root,c.metadata,[{package:c.pkg.name,packageId:c.pkg.id,features:own[0].features,target:c.target,types:[],publicAliases:c.aliases,defaultWitnesses:[],locals:[],delegatedLocals:[],macroOrigins:c.macroOrigins,externalTypeAliases:c.found.externalTypeAliases}],loadContracts(),{target:'native',features:[],jobs:1});
fs.writeFileSync(path.join(c.metadata.target_directory,'orderly-structural-alias-results.json'),JSON.stringify(results,null,2)+'\n');
assert.deepEqual(results.map(r=>r.name).sort(),names);
for(const result of results){
  assert.deepEqual(result.failures,[],result.name);
  assert.equal(result.classificationOnly,true,result.name);
  assert.equal(result.category,'non-decoder-alias',result.name);
  assert.equal(result.deserialize,false,result.name);
  assert.match(result.monomorphization,/one reviewed .*binding::Host.* instantiation/,result.name);
}
console.log('Orderly aliases: 3 specific native-host classifications; 0 failed');
