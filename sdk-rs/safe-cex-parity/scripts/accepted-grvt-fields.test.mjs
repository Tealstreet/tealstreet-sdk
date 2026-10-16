import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import {spawnSync} from 'node:child_process';
import {registerHooks} from 'node:module';
import {fileURLToPath} from 'node:url';
import {loadPolicy} from './accepted-policy-files.mjs';
import {inventory} from './accepted-inventory.mjs';
import {verifiedMacroOrigins} from './accepted-macro-origins.mjs';

const name='safe_cex_venue_grvt::composition::Fields';
const root=fileURLToPath(new URL('../..',import.meta.url));
const spec=loadPolicy('generic-aliases')[name];
// Synthetic AST/artifact fixtures exercise policy refusals only. nativeProbe
// independently discovers and authenticates the selected SDK compiler inputs.
const alias={name,ident:'Fields',span:{filename:spec.file,begin:[12,1],end:[12,61]},generics:spec.generics,rawTarget:spec.rawTarget,target:spec.target};
function contract(actual,witnesses){return {category:'decoder',shape:'borrowed-map',evidence:'Concrete bool instantiation retains static key borrowing; actual text, retained borrowed Value and owned Value routes have separate admission and native key/value/pointer witnesses.',generics:actual.generics,rawTarget:actual.rawTarget,target:actual.target,entrypoints:[],borrowedRoutes:witnesses()};}
const identity={expectedType:spec.expectedType,suffix:spec.suffix,canonical:spec.canonical,monomorphization:'one reviewed bool instantiation'};
const context={packageMetadata:{name:'safe-cex-venue-grvt',manifest_path:path.join(root,'safe-cex-venue-grvt/Cargo.toml')},metadata:{packages:[{id:'fixture-indexmap',name:'indexmap'}]},macroOrigins:['serde_json'],externalTypeAliases:[['alloc::rc::Rc','sysroot'],['core::cell::RefCell','sysroot'],['indexmap::map::IndexMap','registry']].map(([canonical,kind])=>({name,canonical,kind:'struct',artifact:{kind,packageId:'fixture-indexmap'}}))};

async function pureControls(){
 const {aliasContract}=await import('./accepted-aliases.mjs');
 const {grvtFieldsContract,grvtFieldsWitnesses}=await import('./accepted-grvt-fields.mjs');
 const rules=contract(alias,grvtFieldsWitnesses);assert.equal(grvtFieldsContract(alias,rules,context,identity).aliasProbeKind,'reviewed-grvt-fields');
 assert.equal(aliasContract(alias,{[name]:rules},context).aliasProbeKind,'reviewed-grvt-fields');
 for(const field of ['instantiation','aliasProbeExpected','aliasProbeKind','aliasProbeSuffix','nativeCanonical'])assert.throws(()=>aliasContract(alias,{[name]:{...rules,[field]:'redirect'}},context),/cannot be supplied|cannot supply/);
 assert.equal(rules.borrowedRoutes.length,36);
 assert.deepEqual(rules.borrowedRoutes.filter(row=>row.output.accepted).map(row=>[row.input.route,row.input.fixture]),[['static-text',0],['static-text',1],['static-text',2],['static-borrowed-value',0],['static-borrowed-value',1],['static-borrowed-value',2],['static-borrowed-value',3],['owned-value',2]]);
 for(const change of [a=>a.ident='Other',a=>a.span.end[1]++,a=>a.generics.params[0].name='Other',a=>a.rawTarget.resolved_path.path='Other',a=>a.target.resolved_path.path='Other']){const a=structuredClone(alias);change(a);assert.throws(()=>grvtFieldsContract(a,rules,context,identity),/reviewed borrowed Fields/);}
 for(const change of [c=>c.category='non-decoder',c=>c.borrowedRoutes.pop(),c=>c.borrowedRoutes[0].output.members.reverse(),c=>c.observation={call:'redirect'},c=>c.entrypoints=[{}]]){const c=structuredClone(rules);change(c);assert.throws(()=>grvtFieldsContract(alias,c,context,identity),/reviewed borrowed Fields/);}
 for(const key of ['expectedType','suffix','canonical'])assert.throws(()=>grvtFieldsContract(alias,rules,context,{...identity,[key]:'redirect'}),/reviewed borrowed Fields/);
 for(let i=0;i<3;i++){const c=structuredClone(context);c.externalTypeAliases.splice(i,1);assert.throws(()=>grvtFieldsContract(alias,rules,c,identity),/authenticated .* root/);}
 assert.throws(()=>grvtFieldsContract(alias,rules,{...context,packageMetadata:{...context.packageMetadata,name:'other'}},identity),/actual compiler package/);
 assert.equal(grvtFieldsContract({...alias,name:'other::Fields'},rules,context,identity),undefined);
 assert.throws(()=>aliasContract({...alias,name:'other::Fields'},{'other::Fields':rules},context),/source must directly name its type parameter/);
 assert.equal(grvtFieldsContract(alias,{...rules,shape:'map'},context,identity),undefined);
}
function cargo(args){const result=spawnSync('cargo',['--color','never',...args],{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RUSTC_BOOTSTRAP:'1',CARGO_BUILD_JOBS:'1'}});assert.equal(result.status,0,`Fields preparation compiler failure is not a control\n${result.stderr}`);return result.stdout;}
async function nativeProbe(mode){
 const probesUrl=new URL('./accepted-probes.mjs',import.meta.url).href;
 const helperUrl=new URL('./accepted-grvt-fields.mjs',import.meta.url).href;
 const aliasUrl=new URL('./accepted-aliases.mjs',import.meta.url).href;
 let injected=0;
 const hook=registerHooks({load(url,ctx,next){const result=next(url,ctx);if(![probesUrl,helperUrl,aliasUrl].includes(url))return result;let source=String(result.source);
  if(url===probesUrl && mode==='identity'){const anchor="  if (contract.aliasProbeKind === 'reviewed-grvt-fields') return";assert.equal(source.split(anchor).length,2);source=source.replace(anchor,"  if (contract.aliasProbeKind === 'reviewed-grvt-fields') contract.canonical = 'wrong-native-fields';\n"+anchor);injected++;}
  if(url===helperUrl && mode==='members'){const anchor='fields.iter().map(|(key, value)|';assert.equal(source.split(anchor).length,2);source=source.replace(anchor,'fields.iter().rev().map(|(key, value)|');injected++;}
  if(url===aliasUrl && mode==='route'){const anchor='  if (borrowed) return borrowed;';assert.equal(source.split(anchor).length,2);source=source.replace(anchor,'');injected++;}
  return {...result,source};
 }});
 try{
  const {aliasTargetIdentity}=await import('./accepted-aliases.mjs');
  const {grvtFieldsWitnesses}=await import('./accepted-grvt-fields.mjs');
  const metadata=JSON.parse(cargo(['metadata','--locked','--format-version=1']));
  const pkg=metadata.packages.find(pkg=>pkg.name==='safe-cex-venue-grvt');assert.ok(pkg&&metadata.workspace_members.includes(pkg.id));
  const target=pkg.targets.find(target=>target.kind.includes('lib'));
  const selectedTarget=process.env.FIELDS_TEST_TARGET??'native';assert.ok(['native','wasm32-unknown-unknown'].includes(selectedTarget));
  const targetArgs=selectedTarget==='native'?[]:['--target',selectedTarget];
  cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs=1',...targetArgs,'--','-Z','unstable-options','--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all']);
  const doc=JSON.parse(fs.readFileSync(path.join(metadata.target_directory,...(targetArgs.length?[selectedTarget]:[]),'doc',target.name+'.json')));
  const artifacts=cargo(['check','--locked','-p',pkg.name,'--lib','--jobs=1',...targetArgs,'--message-format=json']).trim().split('\n').map(line=>JSON.parse(line)).filter(row=>row.reason==='compiler-artifact');
  const sysroot=spawnSync('rustc',['--print','sysroot'],{cwd:root,encoding:'utf8'});assert.equal(sysroot.status,0);
  const found=inventory(doc,{defaultTraits:loadPolicy('default-traits'),receiverTraits:loadPolicy('receiver-traits'),readSource:file=>fs.readFileSync(path.resolve(root,file))});
  const aliases=found.publicAliases.filter(alias=>alias.name===name);assert.equal(aliases.length,1);
  const members=metadata.packages.filter(pkg=>metadata.workspace_members.includes(pkg.id));
  const externalTypeAliases=found.externalTypeAliases.filter(row=>row.name===name).map(row=>({...row,artifact:aliasTargetIdentity(row,artifacts,members,sysroot.stdout.trim())}));
  const macroOrigins=verifiedMacroOrigins(doc,pkg,metadata,artifacts,sysroot.stdout.trim());
  const {runProbes}=await import('./accepted-probes.mjs');
  assert.equal(injected,mode==='baseline'?0:1,'Fields mutation hook must execute before production compilation');
  const rows=runProbes(root,metadata,[{package:pkg.name,target,types:[],publicAliases:aliases,externalTypeAliases,macroOrigins,locals:[]}],{[name]:contract(aliases[0],grvtFieldsWitnesses)},{target:selectedTarget,features:[],jobs:1});
  console.log('GRVT_FIELDS_NATIVE '+JSON.stringify({mode,rows}));
  assert.equal(rows.length,1);assert.equal(rows[0].name,name);assert.ok(!rows[0].classificationOnly);
  assert.deepEqual(rows[0].failures,[],'actual Fields routes native keys identity and trait observations');
 }finally{hook.deregister();}
}
function nativeChild(mode){const run=spawnSync(process.execPath,[fileURLToPath(import.meta.url),'--fields-native',mode],{encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RUSTC_BOOTSTRAP:'1'}});const output=run.stdout+'\n'+run.stderr;assert.ok(!run.error,String(run.error));assert.doesNotMatch(output,/compiler failure is not a control|compile failure is not a control|error\[E\d+\]|could not compile/);const receipts=output.split('\n').filter(line=>line.startsWith('GRVT_FIELDS_NATIVE ')).map(line=>JSON.parse(line.slice('GRVT_FIELDS_NATIVE '.length)));assert.equal(receipts.length,1,output);return {...run,output,rows:receipts[0].rows};}
if(process.argv[2]==='--fields-native')await nativeProbe(process.argv[3]);
else{
 test('exact borrowed Fields policy retains every route and refuses compiler or witness redirects',pureControls);
 test('borrowed Fields executes actual static and owned routes and rejects native drift',{timeout:1800000},()=>{
  let run=nativeChild('baseline');assert.equal(run.status,0,run.output);
  for(const mode of ['identity','members','route']){
   run=nativeChild(mode);assert.equal(run.status,1,run.output);assert.match(run.output,/actual Fields routes native keys identity and trait observations/);
   assert.match(run.rows[0].failures.join(';'),mode==='route'?/borrowed routes require the exact reviewed alias/:/output mismatch/);
   console.log(`Fields ${mode}: named assertion; process exit 1`);
   run=nativeChild('baseline');assert.equal(run.status,0,run.output);
  }
 });
}
