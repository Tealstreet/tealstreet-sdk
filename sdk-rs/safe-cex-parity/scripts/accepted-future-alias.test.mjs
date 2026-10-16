// Synthetic compiler-context fixtures; native controls separately use Cargo data.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { genericAliasProbe } from './accepted-callable-alias.mjs';
import { aliasContract } from './accepted-aliases.mjs';
const snapshot=JSON.parse(fs.readFileSync(new URL('./future-alias-fixture.json',import.meta.url)));
function fixture(){
  const alias=structuredClone(snapshot);
  const contract={category:'non-decoder',evidence:'One fixed static Value test.',reason:'Caller-owned future is not a raw decoder.',generics:alias.generics,rawTarget:alias.rawTarget,target:alias.target};
  const context={macroOrigins:['serde_json'],externalTypeAliases:[
    {canonical:'core::pin::Pin',kind:'struct'},
    {canonical:'alloc::boxed::Box',kind:'struct'},
    {canonical:'core::future::future::Future',kind:'trait'},
  ].map(row=>({...row,name:alias.name,artifact:{kind:'sysroot'}}))};
  return {alias,contract,context};
}
test('local future aliases retain the actual fixed static Value alias probe',()=>{
  const {alias,contract,context}=fixture();
  const result=genericAliasProbe(alias,contract,context);
  assert.equal(alias.name,'safe_cex_candle_cache::types::LocalFuture');
  assert.equal(result.suffix,"<'static,::serde_json::Value>");
  assert.equal(result.canonical,'core::pin::Pin');
  assert.equal(result.monomorphization,"one fixed 'static/serde_json::Value actual local-future alias probe; no claim for every lifetime or T");
  const classified=aliasContract(alias,{[alias.name]:contract},context);
  assert.equal(classified.aliasProbeSuffix,result.suffix);
  assert.equal(classified.canonical,result.canonical);
});
test('local future alias pins reject output lifetime and generic redirection',()=>{
  const mutations=[
    ({contract})=>{contract.instantiation='Other';},
    ({contract})=>{contract.category='decoder';},
    ({alias})=>{alias.generics.params[0].kind.lifetime.outlives=["'static"];},
    ({alias})=>{alias.generics.params[1].kind.type.default={primitive:'u8'};},
    ({alias})=>{alias.generics.where_predicates.push({bound_predicate:{}});},
    ({alias})=>{alias.rawTarget.resolved_path.args.angle_bracketed.constraints.push({name:'Unexpected'});},
    ({alias})=>{alias.target.resolved_path.args.angle_bracketed.args[0].type.resolved_path.args.angle_bracketed.args[0].type.dyn_trait.lifetime="'static";},
    ({alias})=>{alias.rawTarget.resolved_path.args.angle_bracketed.args[0].type.resolved_path.args.angle_bracketed.args[0].type.dyn_trait.traits[0].trait.args.angle_bracketed.constraints[0].binding.equality.type={primitive:'u8'};},
  ];
  assert.equal(mutations.length,8);
  for(const mutate of mutations){const row=fixture();mutate(row);assert.throws(()=>genericAliasProbe(row.alias,row.contract,row.context),/fixed local future alias probe/);}
  const row=fixture();row.contract.generics=structuredClone(row.alias.generics);row.contract.generics.params[1].name='Other';
  assert.throws(()=>genericAliasProbe(row.alias,row.contract,row.context),/compiler generics changed/);
});
test('local future alias probes require actual compiler dependency and sysroot context',()=>{
  const row=fixture();
  assert.throws(()=>genericAliasProbe(row.alias,row.contract),/actual compiler context required/);
  assert.throws(()=>genericAliasProbe(row.alias,row.contract,{...row.context,macroOrigins:[]}),/actual serde_json dependency required/);
  for(const canonical of ['core::pin::Pin','alloc::boxed::Box','core::future::future::Future']){
    const context=structuredClone(row.context);
    context.externalTypeAliases=context.externalTypeAliases.filter(record=>record.canonical!==canonical);
    assert.throws(()=>genericAliasProbe(row.alias,row.contract,context),/actual sysroot .* identity required/);
    context.externalTypeAliases=structuredClone(row.context.externalTypeAliases);
    context.externalTypeAliases.find(record=>record.canonical===canonical).artifact.kind='workspace';
    assert.throws(()=>genericAliasProbe(row.alias,row.contract,context),/actual sysroot .* identity required/);
  }
  const context=structuredClone(row.context);context.externalTypeAliases[2].kind='struct';
  assert.throws(()=>genericAliasProbe(row.alias,row.contract,context),/actual sysroot .* identity required/);
});
