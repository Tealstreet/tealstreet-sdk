import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import test from 'node:test';
import {aliasContract} from './accepted-aliases.mjs';

const name='safe_cex_venue_woo::v3_ws_public::WooV3WsBookLevel';
const target={array:{type:{resolved_path:{path:'alloc::string::String',args:null}},len:'2'}};
const alias={name,ident:'WooV3WsBookLevel',span:{filename:'safe-cex-venue-woo/src/v3_ws_public.rs'},generics:{params:[],where_predicates:[]},rawTarget:target,target};
const contract={positive:[['a','b'],['c','d']],negative:[null,[]],observation:{call:'{ let decoded: $type = serde_json::from_value($value.clone()).unwrap();serde_json::json!([decoded[0].len(),decoded[1].len()]) }',cases:[{input:['a','b'],output:[1,1]},{input:['c','d'],output:[1,1]}]},category:'decoder',shape:'union',evidence:'Actual two-slot String wire decoder, not a tuple or generic family permission.',generics:alias.generics,rawTarget:target,target};
const context={macroOrigins:['serde_json'],externalTypeAliases:[{name,canonical:'alloc::string::String',kind:'struct',artifact:{kind:'sysroot'}}]};

test('retired WOO book array has no special alias permission',()=>{
  assert.throws(()=>aliasContract(alias,{[name]:contract},context),/alias target requires explicit compiler root identity coverage/);
});

test('WOO four structural aliases execute actual native traits and decoded values',()=>{
  const result=spawnSync(process.execPath,[fileURLToPath(new URL('../../safe-cex-venue-woo/scripts/structural-alias-contracts.mjs',import.meta.url))],{cwd:fileURLToPath(new URL('../..',import.meta.url)),encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'1'}});
  assert.equal(result.status,0,`${result.stdout}\n${result.stderr}`);
  assert.match(result.stdout,/WOO aliases: 2 runtime decoders; 2 specific classifications; 0 failed/);
});
