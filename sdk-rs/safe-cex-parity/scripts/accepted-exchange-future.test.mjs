import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {exchangeFutureAliasProbe} from './accepted-exchange-future.mjs';
const snapshot=JSON.parse(fs.readFileSync(new URL('../accepted-set/exchange-future-fixture.json',import.meta.url)));
const suffixes=["<'static,::serde_json::Value>","<'static>","<'static>"];
function fixture(index=0){
 const alias=structuredClone(snapshot.aliases[index]);
 const contract={category:'non-decoder',reason:'One fixed actual alias, not task/callback execution.',evidence:'Synthetic pure test of a cached normalized compiler alias shape.',generics:structuredClone(alias.generics),rawTarget:structuredClone(alias.rawTarget),target:structuredClone(alias.target)};
 const symbols=[['core::pin::Pin','struct'],['alloc::boxed::Box','struct'],['core::future::future::Future','trait'],['core::result::Result','enum']];
 if(index===2)symbols.push(['core::ops::function::FnMut','trait'],['alloc::vec::Vec','struct']);
 const rows=symbols.map(([canonical,kind])=>({name:alias.name,canonical,kind,artifact:{kind:'sysroot'}}));
 const workspaces=index===1?[['safe_cex_transport::provider::RestExecution','safe-cex-transport'],['safe_cex_transport::provider::RestFailure','safe-cex-transport']]:index===2?[['safe_cex_types::IncomeRecord','safe-cex-types']]:[];
 for(const [canonical,pkg] of workspaces)rows.push({name:alias.name,canonical,kind:'struct',artifact:{kind:'workspace',packageId:`synthetic:${pkg}`,owner:{id:`synthetic:${pkg}`,name:pkg}}});
 return {alias,contract,context:{macroOrigins:['serde_json'],externalTypeAliases:rows}};
}
function probe(r){return exchangeFutureAliasProbe(r.alias,r.contract,r.context);}
test('three exact aliases retain actual internally fixed instantiations and complete expected types',()=>{
 assert.equal(snapshot.aliases.length,3);
 for(let i=0;i<3;i++){const row=fixture(i),p=probe(row);assert.equal(p.suffix,suffixes[i]);assert.ok(p.monomorphization.includes('actual'));assert.ok(p.expectedType.includes('::std::future::Future'));assert.ok(p.expectedType.includes("'static"));}
 assert.equal(probe(fixture(0)).expectedType,'::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output=::std::result::Result<::serde_json::Value,crate::runtime::ExchangeError>>+\'static>>');
 assert.equal(probe(fixture(1)).expectedType,'::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output=::std::result::Result<::safe_cex_transport::RestExecution,::safe_cex_transport::RestFailure>>+\'static>>');
 const income=probe(fixture(2));assert.equal(income.probeKind,'unsized-income-fetcher');assert.ok(income.expectedType.startsWith('dyn ::std::ops::FnMut(crate::income_paging::IncomePageParams)->'));assert.equal(income.canonical,'core::ops::function::FnMut');
});
test('unknown or cross-package same-spelling aliases receive no approval',()=>{
 for(let i=0;i<3;i++){const r=fixture(i);r.alias.name='fake::'+r.alias.ident;assert.equal(probe(r),undefined);}
});
test('parameter and lifetime bounds defaults order and extra traits fail closed',()=>{
 const mutations=[r=>r.alias.generics.params.reverse(),r=>r.alias.generics.params[0].kind.lifetime.outlives.push("'static"),r=>r.alias.generics.params[1].kind.type.default={primitive:'u8'},r=>r.alias.generics.where_predicates.push({bound_predicate:{}}),r=>r.alias.rawTarget.resolved_path.args.angle_bracketed.args[0].type.resolved_path.args.angle_bracketed.args[0].type.dyn_trait.traits.push({trait:{path:'core::marker::Send'},generic_params:[]})];
 assert.equal(mutations.length,5);for(const mutate of mutations){const r=fixture();mutate(r);assert.throws(()=>probe(r),/fixed exchange future alias probe/);}
});
test('raw expanded errors outputs and FnMut input cannot be redirected even with refreshed contract pins',()=>{
 const mutations=[r=>r.alias.rawTarget.resolved_path.path='alloc::rc::Rc',r=>r.alias.target.resolved_path.path='alloc::rc::Rc'];
 for(const mutate of mutations){const r=fixture();mutate(r);r.contract.rawTarget=structuredClone(r.alias.rawTarget);r.contract.target=structuredClone(r.alias.target);assert.throws(()=>probe(r),/target changed/);}
 const http=fixture(1);http.alias.target=structuredClone(http.alias.rawTarget);http.contract.target=structuredClone(http.alias.target);assert.throws(()=>probe(http),/expanded target changed/);
 const income=fixture(2);income.alias.rawTarget.dyn_trait.traits[0].trait.args.parenthesized.inputs[0]={primitive:'u8'};income.contract.rawTarget=structuredClone(income.alias.rawTarget);assert.throws(()=>probe(income),/source target changed/);
});
test('contracts cannot inject caller-controlled target expected type identity mode or classification',()=>{
 for(const key of ['instantiation','aliasProbeExpected','aliasProbeKind','aliasProbeSuffix','nativeCanonical']){const r=fixture();r.contract[key]='Other';assert.throws(()=>probe(r),/caller cannot supply/);}
 for(const key of ['category','evidence','reason','generics','rawTarget','target']){const r=fixture();r.contract[key]=key==='category'?'decoder':['evidence','reason'].includes(key)?'':{};assert.throws(()=>probe(r),/fixed exchange future alias probe/);}
});
test('every required sysroot role refuses missing wrong-kind and foreign artifact identities',()=>{
 for(let i=0;i<3;i++)for(const original of fixture(i).context.externalTypeAliases.filter(r=>r.artifact.kind==='sysroot')){
  const r=fixture(i);r.context.externalTypeAliases=r.context.externalTypeAliases.filter(x=>x.canonical!==original.canonical);assert.throws(()=>probe(r),/actual sysroot .* identity required/);
  const wrong=fixture(i);wrong.context.externalTypeAliases.find(x=>x.canonical===original.canonical).artifact.kind='workspace';assert.throws(()=>probe(wrong),/actual sysroot .* identity required/);
  const kind=fixture(i);kind.context.externalTypeAliases.find(x=>x.canonical===original.canonical).kind='wrong';assert.throws(()=>probe(kind),/actual sysroot .* identity required/);
 }
});
test('workspace payload identities refuse wrong package registry missing and mismatched owner IDs',()=>{
 for(const i of [1,2])for(const original of fixture(i).context.externalTypeAliases.filter(r=>r.artifact.kind==='workspace')){
  for(const mutate of [r=>{r.context.externalTypeAliases=r.context.externalTypeAliases.filter(x=>x.canonical!==original.canonical);},r=>{r.context.externalTypeAliases.find(x=>x.canonical===original.canonical).artifact.owner.name='fake-package';},r=>{r.context.externalTypeAliases.find(x=>x.canonical===original.canonical).artifact.kind='registry';},r=>{r.context.externalTypeAliases.find(x=>x.canonical===original.canonical).artifact.owner.id='other';}]){const r=fixture(i);mutate(r);assert.throws(()=>probe(r),/actual workspace .* identity required/);}
 }
});
test('missing live context source identity and serde origin remain refusals',()=>{
 const r=fixture();assert.throws(()=>exchangeFutureAliasProbe(r.alias,r.contract),/actual compiler context required/);
 r.context.macroOrigins=[];assert.throws(()=>probe(r),/actual serde_json dependency required/);
 for(const mutate of [r=>{r.alias.ident='Other';},r=>{r.alias.span.filename='other/src/runtime.rs';}]){const row=fixture();mutate(row);assert.throws(()=>probe(row),/identity changed|identifier changed/);}
});
