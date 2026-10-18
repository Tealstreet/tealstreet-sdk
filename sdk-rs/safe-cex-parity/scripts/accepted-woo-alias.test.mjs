import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import {wooBookLevelArrayProbe} from './accepted-woo-alias.mjs';
import {aliasContract} from './accepted-aliases.mjs';

const name='safe_cex_venue_woo::v3_ws_public::WooV3WsBookLevel';
const target={array:{type:{resolved_path:{path:'alloc::string::String',args:null}},len:'2'}};
const alias={name,ident:'WooV3WsBookLevel',span:{filename:'safe-cex-venue-woo/src/v3_ws_public.rs'},generics:{params:[],where_predicates:[]},rawTarget:target,target};
const contract={positive:[['a','b'],['c','d']],negative:[null,[]],observation:{call:'{ let decoded: $type = serde_json::from_value($value.clone()).unwrap();serde_json::json!([decoded[0].len(),decoded[1].len()]) }',cases:[{input:['a','b'],output:[1,1]},{input:['c','d'],output:[1,1]}]},category:'decoder',shape:'union',evidence:'Actual two-slot String wire decoder, not a tuple or generic family permission.',generics:alias.generics,rawTarget:target,target};
const context={macroOrigins:['serde_json'],externalTypeAliases:[{name,canonical:'alloc::string::String',kind:'struct',artifact:{kind:'sysroot'}}]};

test('exact WOO book array refuses slot root and caller redirects',()=>{
  const result=wooBookLevelArrayProbe(alias,contract,context);
  assert.equal(result.expectedType,'[::std::string::String;2]');
  assert.equal(result.probeKind,'reviewed-woo-book-level-array');
  for(const mutate of [a=>a.ident='Other',a=>a.span.filename='other.rs',a=>a.generics.params.push({name:'T'}),a=>a.rawTarget.array.len='3',a=>a.target.array.type={primitive:'f64'}]){
    const changed=structuredClone(alias);mutate(changed);
    assert.throws(()=>wooBookLevelArrayProbe(changed,contract,context),/reviewed WOO book level array/);
  }
  for(const key of ['instantiation','aliasProbeExpected','aliasProbeKind','aliasProbeSuffix','nativeCanonical','expectedType','suffix','probeKind','unsized'])assert.throws(()=>wooBookLevelArrayProbe(alias,{...contract,[key]:'redirect'},context),/caller cannot supply/);
  for(const mutate of [c=>c.macroOrigins=[],c=>c.externalTypeAliases=[],c=>c.externalTypeAliases[0].artifact.kind='workspace',c=>c.externalTypeAliases[0].canonical='other::String']){
    const changed=structuredClone(context);mutate(changed);assert.throws(()=>wooBookLevelArrayProbe(alias,contract,changed),/compiler/);
  }
  assert.throws(()=>wooBookLevelArrayProbe(alias,{...contract,category:'non-decoder'},context),/explicit array decoder/);
  const unknown={...alias,name:'other::Array'};
  assert.equal(wooBookLevelArrayProbe(unknown,contract,context),undefined);
  assert.throws(()=>aliasContract(unknown,{[unknown.name]:contract},context),/alias target requires explicit compiler root identity coverage/);
  const url=new URL('./accepted-woo-alias.mjs',import.meta.url).href;
  const changed=structuredClone(alias);changed.target={array:{type:target.array.type,len:'3'}};
  const anchor='  assert.deepEqual(alias.target, target, `${label}: ordered expanded slots changed`);';
  function child(fault) {
    const hook = fault ? `import {registerHooks} from 'node:module';registerHooks({load(url,context,next){const r=next(url,context);if(url!==${JSON.stringify(url)})return r;const s=String(r.source);const a=${JSON.stringify(anchor)};assert.equal(s.split(a).length,2);return {...r,source:s.replace(a,'')};}});` : '';
    const body = `import assert from 'node:assert/strict';${hook}const {wooBookLevelArrayProbe}=await import(${JSON.stringify(url)});assert.throws(()=>wooBookLevelArrayProbe(${JSON.stringify(changed)},${JSON.stringify({...contract,target:changed.target})},${JSON.stringify(context)}),/ordered expanded slots changed/,'WOO-ARRAY expanded slots must refuse');`;
    return spawnSync(process.execPath,['--input-type=module','--eval',body],{encoding:'utf8'});
  }
  assert.equal(child(false).status,0);const fault=child(true);assert.equal(fault.status,1);assert.match(fault.stderr,/WOO-ARRAY expanded slots must refuse/);assert.equal(child(false).status,0);
  console.log('WOO array expanded slots: named assertion; process exit 1');
});

test('WOO five structural aliases execute actual native traits and decoded values',()=>{
  const result=spawnSync(process.execPath,[fileURLToPath(new URL('../../safe-cex-venue-woo/scripts/structural-alias-contracts.mjs',import.meta.url))],{cwd:fileURLToPath(new URL('../..',import.meta.url)),encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1'}});
  assert.equal(result.status,0,`${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout,/WOO aliases: 3 runtime decoders; 2 specific classifications; 0 failed/);
});
