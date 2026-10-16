import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { registerHooks } from 'node:module';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { verifiedMacroOrigins } from './accepted-macro-origins.mjs';

const name = 'safe_cex_venue_weex::response_types::WeexKlineRow';
const number = 'safe_cex_venue_weex::response_types::WeexNumberOrString';
const string = 'alloc::string::String';
// Synthetic AST/artifact fixtures exercise policy refusals only. The native test
// below independently discovers and authenticates the real SDK compiler inputs.
const target = { tuple: [number,string,string,string,string,string,number,string,number,string,string].map(path => ({ resolved_path: { path, args: null } })) };
const fixtureAlias = { name, ident: 'WeexKlineRow', span: { filename: 'safe-cex-venue-weex/src/response_types.rs' }, generics: { params: [], where_predicates: [] }, rawTarget: structuredClone(target), target: structuredClone(target) };
const fixtureContext = { macroOrigins: ['serde_json'], externalTypeAliases: Array.from({length:8}, () => ({ name, canonical:string, kind:'struct', artifact:{kind:'sysroot'} })) };
function contract(alias) {
  return {
    category:'decoder', shape:'union', evidence:'Reviewed actual eleven-slot protocol tuple; every slot is observed directly before owner serialization.',
    generics:alias.generics, rawTarget:alias.rawTarget, target:alias.target, entrypoints:[],
    positive:[[3.25,'open-a','high-b','low-c','close-d','volume-e',-2.5,'quote-f','9','buy-g','sell-h'],['7','open-j','high-k','low-l','close-m','volume-n',11,'quote-p',-0.5,'buy-q','sell-r']],
    negative:[null,{},[],[3.25,'open-a','high-b','low-c','close-d','volume-e',-2.5,'quote-f','9','buy-g'],[3.25,'open-a','high-b','low-c','close-d','volume-e',-2.5,'quote-f','9','buy-g','sell-h','extra'],[3.25,17,'high-b','low-c','close-d','volume-e',-2.5,'quote-f','9','buy-g','sell-h']],
    observation:{
      call:'{ let decoded=serde_json::from_value::<$type>($value.clone()).expect("reviewed tuple");let scalar=|value:crate::response_types::WeexNumberOrString|match value{crate::response_types::WeexNumberOrString::Number(n)=>serde_json::json!({"numberBits":format!("{:016x}",n.to_bits())}),crate::response_types::WeexNumberOrString::String(s)=>serde_json::json!({"string":s})};serde_json::json!([scalar(decoded.0),decoded.1,decoded.2,decoded.3,decoded.4,decoded.5,scalar(decoded.6),decoded.7,scalar(decoded.8),decoded.9,decoded.10]) }',
      cases:[
        { input:[3.25,'open-a','high-b','low-c','close-d','volume-e',-2.5,'quote-f','9','buy-g','sell-h'], output:[{numberBits:'400a000000000000'},'open-a','high-b','low-c','close-d','volume-e',{numberBits:'c004000000000000'},'quote-f',{string:'9'},'buy-g','sell-h'] },
        { input:['7','open-j','high-k','low-l','close-m','volume-n',11,'quote-p',-0.5,'buy-q','sell-r'], output:[{string:'7'},'open-j','high-k','low-l','close-m','volume-n',{numberBits:'4026000000000000'},'quote-p',{numberBits:'bfe0000000000000'},'buy-q','sell-r'] },
      ],
    },
  };
}

async function pureControls() {
  const { genericAliasProbe } = await import('./accepted-callable-alias.mjs');
  const { aliasContract } = await import('./accepted-aliases.mjs');
  const rule=contract(fixtureAlias);
  const probe=genericAliasProbe(fixtureAlias,rule,fixtureContext);
  assert.equal(probe.probeKind,'reviewed-weex-kline-tuple');
  assert.equal(probe.suffix,'');
  assert.match(probe.expectedType,/^\(crate::response_types::WeexNumberOrString,/);
  for(const mutate of [
    a=>{a.ident='Other';},a=>{a.span.filename='other.rs';},a=>{a.generics.params=[{name:'T'}];},
    a=>{a.rawTarget.tuple[1]={primitive:'f64'};},a=>{a.target.tuple.reverse();},a=>{a.target.tuple.pop();},
  ]) { const a=structuredClone(fixtureAlias);mutate(a);assert.throws(()=>genericAliasProbe(a,contract(a),fixtureContext),/reviewed Weex kline tuple/); }
  for(const key of ['instantiation','aliasProbeExpected','aliasProbeKind','aliasProbeSuffix','nativeCanonical','expectedType','suffix','probeKind','unsized']) assert.throws(()=>genericAliasProbe(fixtureAlias,{...rule,[key]:'redirect'},fixtureContext),/caller cannot supply/);
  for(let i=0;i<8;i++) for(const mutate of [rows=>rows.splice(i,1),rows=>{rows[i].artifact.kind='workspace';},rows=>{rows[i].canonical='other::String';}]) {
    const context=structuredClone(fixtureContext);mutate(context.externalTypeAliases);assert.throws(()=>genericAliasProbe(fixtureAlias,rule,context),/every repeated compiler root/);
  }
  assert.throws(()=>genericAliasProbe(fixtureAlias,rule,{...fixtureContext,macroOrigins:[]}),/actual compiler context/);
  assert.throws(()=>genericAliasProbe(fixtureAlias,{...rule,category:'non-decoder'},fixtureContext),/explicit tuple decoder/);
  assert.equal(genericAliasProbe({...fixtureAlias,name:'other::Tuple'},rule,fixtureContext),undefined);
  const unknown={...fixtureAlias,name:'other::Tuple'};
  assert.throws(()=>aliasContract(unknown,{[unknown.name]:rule},fixtureContext),/alias target requires explicit compiler root identity coverage/);
  const url=new URL('./accepted-weex-kline-alias.mjs',import.meta.url).href;
  const changed=structuredClone(fixtureAlias);changed.target.tuple[0]={primitive:'f64'};
  const anchor='  assert.deepEqual(alias.target, target, `${label}: ordered expanded slots changed`);';
  const child=mutate=>spawnSync(process.execPath,['--input-type=module','--eval',`import assert from 'node:assert/strict';${mutate?`import {registerHooks} from 'node:module';registerHooks({load(url,context,next){const r=next(url,context);if(url!==${JSON.stringify(url)})return r;const s=String(r.source);const anchor=${JSON.stringify(anchor)};assert.equal(s.split(anchor).length,2);return {...r,source:s.replace(anchor,'')};}});`:''}const {weexKlineTupleProbe}=await import(${JSON.stringify(url)});assert.throws(()=>weexKlineTupleProbe(${JSON.stringify(changed)},${JSON.stringify(contract(changed))},${JSON.stringify(fixtureContext)}),/ordered expanded slots changed/,'WEEX-KLINE expanded slots must refuse');`],{encoding:'utf8'});
  assert.equal(child(false).status,0);
  const mutant=child(true);assert.equal(mutant.status,1);assert.match(mutant.stderr,/WEEX-KLINE expanded slots must refuse/);
  console.log('Weex kline expanded slots: named assertion; process exit 1');
  assert.equal(child(false).status,0);
}

const root=fileURLToPath(new URL('../..',import.meta.url));
function cargo(args) {
  const result=spawnSync('cargo',['--color','never',...args],{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1'}});
  assert.equal(result.status,0,`tuple preparation compiler failures are not controls\n${result.stderr}`);
  return result.stdout;
}
async function nativeProbe(mode) {
  const policyUrl=new URL('./accepted-callable-alias.mjs',import.meta.url).href;
  const routeHook=registerHooks({load(url,context,next){
    const result=next(url,context);
    if(url!==policyUrl || mode!=='route') return result;
    const anchor='  const weex = weexKlineTupleProbe(alias, contract, context);\n  if (weex) return weex;';
    const source=String(result.source);assert.equal(source.split(anchor).length,2);
    return {...result,source:source.replace(anchor,'')};
  }});
  const { aliasTargetIdentity } = await import('./accepted-aliases.mjs');
  const metadata=JSON.parse(cargo(['metadata','--locked','--format-version=1']));
  const pkg=metadata.packages.find(pkg=>pkg.name==='safe-cex-venue-weex');
  assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
  const target=pkg.targets.find(target=>target.kind.includes('lib'));
  cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs=1','--','-Z','unstable-options','--output-format=json','--document-private-items','--document-hidden-items','-A','rustdoc::all']);
  const doc=JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc',target.name+'.json')));
  const artifacts=cargo(['check','--locked','-p',pkg.name,'--lib','--jobs=1','--message-format=json']).trim().split('\n').map(line=>JSON.parse(line)).filter(row=>row.reason==='compiler-artifact');
  const sysroot=spawnSync('rustc',['--print','sysroot'],{cwd:root,encoding:'utf8'});assert.equal(sysroot.status,0);
  const found=inventory(doc,{defaultTraits:loadPolicy('default-traits'),receiverTraits:loadPolicy('receiver-traits'),readSource:file=>fs.readFileSync(path.resolve(root,file))});
  const aliases=found.publicAliases.filter(alias=>alias.name===name);assert.equal(aliases.length,1);
  const members=metadata.packages.filter(pkg=>metadata.workspace_members.includes(pkg.id));
  const externalTypeAliases=found.externalTypeAliases.filter(row=>row.name===name).map(row=>({...row,artifact:aliasTargetIdentity(row,artifacts,members,sysroot.stdout.trim())}));
  const macroOrigins=verifiedMacroOrigins(doc,pkg,metadata,artifacts,sysroot.stdout.trim());
  const state={count:0};globalThis.__weexKlineNative=state;
  const probesUrl=new URL('./accepted-probes.mjs',import.meta.url).href;
  const hook=registerHooks({load(url,context,next){
    const result=next(url,context);if(url!==probesUrl) return result;
    let source=String(result.source);
    const anchor='  const alternate = `|value|';assert.equal(source.split(anchor).length,2);
    const injection=`  if(type.name===${JSON.stringify(name)}){assert.equal(contract.aliasProbeKind,'reviewed-weex-kline-tuple');globalThis.__weexKlineNative.count++;${mode==='identity'?`canonical=literal('wrong-native-weex-tuple');`:''}}\n`;
    source=source.replace('  const canonical = unsized','  let canonical = unsized').replace(anchor,injection+anchor);
    return {...result,source};
  }});
  try {
    const {runProbes}=await import(pathToFileURL(path.join(root,'safe-cex-parity/scripts/accepted-probes.mjs')));
    const rows=runProbes(root,metadata,[{package:pkg.name,target,types:[],publicAliases:aliases,externalTypeAliases,macroOrigins,locals:[]}],{[name]:contract(aliases[0])},{target:'native',features:[],jobs:1});
    console.log('WEEX_KLINE_NATIVE '+JSON.stringify({count:state.count,rows}));
    assert.equal(rows.length,1);assert.equal(rows[0].name,name);
    assert.deepEqual(rows[0].failures,[],'actual Weex tuple native identity slots and traits');
    assert.equal(state.count,1);assert.ok(!rows[0].classificationOnly);
  } finally {hook.deregister();routeHook.deregister();delete globalThis.__weexKlineNative;}
}
function nativeChild(mode) {
  const result=spawnSync(process.execPath,[fileURLToPath(import.meta.url),'--weex-kline-native',mode],{encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RUSTC_BOOTSTRAP:'1'}});
  assert.ok(!result.error,String(result.error));const output=result.stdout+'\n'+result.stderr;
  assert.doesNotMatch(output,/compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  const receipts=output.split('\n').filter(line=>line.startsWith('WEEX_KLINE_NATIVE ')).map(line=>JSON.parse(line.slice('WEEX_KLINE_NATIVE '.length)));
  assert.equal(receipts.length,1,output);assert.deepEqual(receipts[0].rows.map(row=>row.name),[name]);
  assert.equal(receipts[0].count,mode==='route'?0:1);
  return {...result,output,rows:receipts[0].rows};
}
if(process.argv[2]==='--weex-kline-native') await nativeProbe(process.argv[3]);
else {
  test('exact Weex kline tuple policy refuses slot root and caller redirects',pureControls);
  test('Weex kline tuple executes actual native slots identity and trait probes',{timeout:1800000},()=>{
    let result=nativeChild('baseline');assert.equal(result.status,0,result.output);
    for(const mode of ['identity','route']) {
      result=nativeChild(mode);assert.equal(result.status,1,result.output);assert.match(result.output,/actual Weex tuple native identity slots and traits/);
      assert.match(result.rows[0].failures.join(';'),mode==='identity'?/compiler owner identity mismatch/:/alias target requires explicit compiler root identity coverage/);
      console.log(`Weex kline ${mode==='identity'?'native identity':'policy routing'}: named assertion; process exit 1`);
      result=nativeChild('baseline');assert.equal(result.status,0,result.output);
    }
  });
}
