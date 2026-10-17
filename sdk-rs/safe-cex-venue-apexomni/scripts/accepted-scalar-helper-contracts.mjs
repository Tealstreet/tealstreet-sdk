// Individually reviewed scalar/validation bodies; no whole-venue coverage claim.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {inventory} from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import {contractFor} from '../../safe-cex-parity/scripts/accepted-contracts.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
const workspace=fileURLToPath(new URL('../../',import.meta.url));
function cargo(args,env={}) {
 const r=spawnSync('cargo',args,{cwd:workspace,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1',...env}});
 assert.equal(r.status,0,`${args.join(' ')} failed\n${r.error??''}\n${r.stderr}`);
 return r.stdout;
}
const selected=[
  "safe_cex_venue_apexomni::exchange_utils::calculate_tp_sl_price",
  "safe_cex_venue_apexomni::exchange_utils::character_at",
  "safe_cex_venue_apexomni::exchange_utils::d8",
  "safe_cex_venue_apexomni::exchange_utils::fee_helper",
  "safe_cex_venue_apexomni::exchange_utils::generate_apex_random_client_id",
  "safe_cex_venue_apexomni::exchange_utils::iso_timestamp_to_epoch_hours",
  "safe_cex_venue_apexomni::signer_utils::assert",
  "safe_cex_venue_apexomni::signer_utils::assert_account_id",
  "safe_cex_venue_apexomni::signer_utils::assert_address",
  "safe_cex_venue_apexomni::signer_utils::assert_chain_id",
  "safe_cex_venue_apexomni::signer_utils::assert_nonce",
  "safe_cex_venue_apexomni::signer_utils::assert_pub_key_hash",
  "safe_cex_venue_apexomni::signer_utils::assert_sub_account_id",
  "safe_cex_venue_apexomni::signer_utils::assert_token_id",
  "safe_cex_venue_apexomni::signer_utils::assert_trading_fee_ratio",
  "safe_cex_venue_apexomni::signer_utils::assert_ts",
  "safe_cex_venue_apexomni::signer_utils::assert_type_array",
  "safe_cex_venue_apexomni::signer_utils::assert_type_boolean",
  "safe_cex_venue_apexomni::signer_utils::assert_type_number",
  "safe_cex_venue_apexomni::signer_utils::assert_type_string",
  "safe_cex_venue_apexomni::signer_utils::assert_withdraw_fee_ratio",
  "safe_cex_venue_apexomni::signer_utils::display_value",
  "safe_cex_venue_apexomni::utils::drop_prefix"
];
const prefix='--expect-output-failure=';
assert.ok(process.argv.length===2||(process.argv.length===3&&process.argv[2].startsWith(prefix)),'unsupported arguments');
const control=process.argv[2]?.slice(prefix.length);
const contracts=loadContracts();
assert.ok(control===undefined||(selected.includes(control)&&contracts[control].category==='decoder'),'control must name one of the16 actual runtime decoders');
const metadata=JSON.parse(cargo(['metadata','--locked','--format-version=1']));
const pkg=metadata.packages.find(p=>p.name==='safe-cex-venue-apexomni');
assert.ok(pkg&&metadata.workspace_members.includes(pkg.id),'actual compiler package required');
const target=pkg.targets.find(t=>t.kind.includes('lib')); assert.ok(target);
cargo(['rustdoc','--locked','-p',pkg.name,'--jobs=1','--lib','--','-Z','unstable-options','--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],{RUSTC_BOOTSTRAP:'1'});
const doc=JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc/safe_cex_venue_apexomni.json'),'utf8'));
const types=inventory(doc,{onlyOwners:selected,readSource:file=>fs.readFileSync(path.join(workspace,file))}).types;
assert.deepEqual(types.map(t=>t.name).sort(),selected);
assert.ok(types.every(t=>t.kind==='function'&&t.deserialize===false));
for(const type of types) contractFor(type,contracts);
console.log('Fresh native/default inventory:23 actual helper bodies; individually reviewed16 decoder/7 typed non-decoder roles. Classification receipts are not runtime coverage.');
const observed=control?types.filter(t=>t.name===control):types;
const results=runProbes(workspace,metadata,[{package:pkg.name,packageId:pkg.id,target,types:observed,publicAliases:[],defaultWitnesses:[],locals:[],delegatedLocals:[]}],contracts,{target:'native',features:[],jobs:1});
assert.equal(results.length,observed.length);
if(control) {
 assert.equal(results.length,1);
 assert.equal(results[0].classificationOnly,false);
 assert.ok(results[0].failures.length>0&&results[0].failures.every(f=>f.startsWith('function output mismatch for ')),'compiled selected native output mismatch required; tooling/compile failures do not count');
 console.log(`NATIVE_OUTPUT_FAILURE: ${control}`);
} else {
 assert.equal(results.filter(r=>!r.classificationOnly&&r.category==='decoder').length,16);
 assert.equal(results.filter(r=>r.classificationOnly&&r.category==='non-decoder').length,7);
 for(const result of results) assert.equal(result.failures.length,0,`${result.name}: ${result.failures.join('; ')}`);
 console.log('23 helper receipts passed:16 actual runtime decoders/7 explicit classifications/zero failures. Other owners, aliases, receivers, locals, construction and feature/wasm accepted behavior remain open.');
}
