import { loadPolicy } from './accepted-policy-files.mjs';
// Production WS native controls mutate ws_auth.rs sequentially and restore it in finally.
import assert from 'node:assert/strict';
import { loadContracts } from './accepted-contract-files.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { runProbes } from './accepted-probes.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const sourcePath = path.join(root, 'safe-cex-types/src/ws_auth.rs');
const original = fs.readFileSync(sourcePath, 'utf8');
const contracts = loadContracts();
const visitorPins = loadPolicy('local-functions');
// RequestVisitor and ResponseVisitor are delegated to their enclosing
// Deserialize owners by the scanner. The free payload helper retains the
// explicit whole-file pin, which also detects mutations in those two bodies.
const expectedVisitorLabels = ['PayloadVisitor'].map(name => `safe-cex-types/src/ws_auth.rs::LOCAL_TRAIT_IMPL:serde::de::Visitor:${name}`);
function fingerprint(source, rules = visitorPins) {
  for (const label of expectedVisitorLabels) {
    assert.equal(rules[label]?.sourceSha256, createHash('sha256').update(source).digest('hex'), `${label}: accepted-set assertion: reviewed nested-function source changed`);
  }
}
function cargo(args, extraEnv = {}) {
  const result = spawnSync('cargo', ['--color','never',...args], {cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'2',...extraEnv}});
  assert.equal(result.status,0,`WS native preparation failed; compiler failures are not controls\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
function probe(ownerName, reviewedSourceSha) {
  const reviewed = structuredClone(visitorPins);
  if (reviewedSourceSha) for (const label of expectedVisitorLabels) reviewed[label].sourceSha256 = reviewedSourceSha;
  fingerprint(fs.readFileSync(sourcePath), reviewed); // temporary in-memory overlay only
  const metadata = JSON.parse(cargo(['metadata','--no-deps','--locked','--format-version','1']));
  const member = metadata.packages.find(item => item.name === 'safe-cex-types');
  assert.ok(member && metadata.workspace_members.includes(member.id));
  const target = member.targets.find(item => item.kind.includes('lib'));
  cargo(['rustdoc','--locked','-p',member.name,'--lib','--jobs','2','--','-Z','unstable-options','--output-format','json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],{RUSTC_BOOTSTRAP:'1'});
  const found = inventory(JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc','safe_cex_types.json'))),{onlyOwners:[ownerName]});
  assert.deepEqual(found.types.map(owner=>owner.name),[ownerName]); // independent, exact single-owner floor
  assert.equal(path.resolve(root,found.types[0].span.filename),sourcePath);
  const receipts = runProbes(root,metadata,[{package:member.name,target,features:[],types:found.types,locals:[]}],contracts);
  assert.deepEqual(receipts.map(row=>row.name),[ownerName]);
  assert.ok(receipts.every(row=>row.failures.length===0),JSON.stringify(receipts));
}
function child(ownerName,reviewedSha) {
  const result=spawnSync(process.execPath,[fileURLToPath(import.meta.url),'--probe',ownerName,...(reviewedSha?[reviewedSha]:[])],{encoding:'utf8',maxBuffer:64*1024*1024});
  assert.ok(!result.error,String(result.error));
  const output=`${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output,/compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  return {...result,output};
}
function replaceOnce(source,before,after) {
  assert.equal(source.split(before).length-1,1,'mutation must match exactly one production site');
  return source.replace(before,after);
}
const type=name=>`safe_cex_types::ws_auth::${name}`;
const toSign = source => replaceOnce(source,'.as_str().map(str::to_owned)','.as_str().map(|_| "MUTATED_NATIVE".to_owned())');
const extraNull = source => replaceOnce(source,'"extraData" => auth.extra_data = Some(map.next_value()?),','"extraData" => { let raw: Box<serde_json::value::RawValue> = map.next_value()?; auth.extra_data = (raw.get() != "null").then_some(raw); },');
const controls = [
  ['request explicit null id native presence',type('WsAuthRequest'),source=>replaceOnce(source,'"id" => request.id = Some(map.next_value()?),','"id" => { let id: Value = map.next_value()?; request.id = (!id.is_null()).then_some(id); },')],
  ['request raw null type native presence',type('WsAuthRequest'),source=>replaceOnce(source,'"type" => request.request_type = Some(map.next_value()?),','"type" => { let raw: Box<serde_json::value::RawValue> = map.next_value()?; request.request_type = (raw.get() != "null").then_some(raw); },')],
  ['request nested native payload',type('WsAuthRequest'),toSign],
  ['auth native payload',type('WsAuth'),toSign],
  ['auth native raw null metadata',type('WsAuth'),extraNull],
  ['payload native payload',type('ws_auth_payload'),toSign],
  ['payload native raw null metadata',type('ws_auth_payload'),extraNull],
  ['seed native payload',type('WsAuthSeed'),toSign],
  ['seed native raw null metadata',type('WsAuthSeed'),extraNull],
  ['seed native Some outcome',type('WsAuthSeed'),source=>replaceOnce(source,') -> Result<Self::Value, D::Error> {\n        ws_auth_payload(deserializer)\n    }',') -> Result<Self::Value, D::Error> {\n        ws_auth_payload(deserializer).map(|_| None)\n    }')],
];
if(process.argv[2]==='--probe') {
  probe(process.argv[3],process.argv[4]);
} else {
  test('WS four native families reject field and presence mutations before wire assertions',{timeout:3600000},()=>{
    assert.equal(controls.length,10); // independent planned floor; update consciously with registry
    for(const owner of ['WsAuthRequest','WsAuth','ws_auth_payload','WsAuthSeed']) {
      const result=child(type(owner));assert.equal(result.status,0,result.output);
    }
    for(const [name,owner,mutate] of controls) {
      try {
        const changed=mutate(original);assert.notEqual(changed,original,`${name}: applied`);
        fs.writeFileSync(sourcePath,changed);
        assert.throws(()=>fingerprint(Buffer.from(changed)),/reviewed nested-function source changed/);
        const result=child(owner,createHash('sha256').update(changed).digest('hex'));
        assert.equal(result.status,1,`${name}: ${result.output}`);
        assert.match(result.output,/AssertionError/);
        assert.match(result.output,/native observation mismatch/);
        assert.match(result.output,/"nativeOnly":true/);
        assert.doesNotMatch(result.output,/Deserialize output mismatch|function output mismatch/);
        console.log(`${name}: named assertion; process exit ${result.status}`);
      } finally {fs.writeFileSync(sourcePath,original);}
      const restored=child(owner);assert.equal(restored.status,0,restored.output);
    }
  });
}
