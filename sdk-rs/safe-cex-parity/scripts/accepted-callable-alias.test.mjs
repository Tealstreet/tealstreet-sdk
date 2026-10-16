// Pure synthetic compiler/alias fixtures. No Cargo, rustc, existing target cache,
// developer toolchain, environment-specific source pointer or live SDK oracle.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { genericAliasProbe, requireCallableCompilerIdentity } from './accepted-callable-alias.mjs';
import { identityAliasProbe } from './accepted-identity-alias.mjs';
import { inventory } from './accepted-inventory.mjs';
import { aliasTargetIdentity } from './accepted-aliases.mjs';

// A portable normalized AST snapshot, with relative source filenames and no
// artifact paths. Compiler/artifact ownership below is explicitly synthetic.
const snapshot = JSON.parse(fs.readFileSync(new URL('./callable-alias-fixture.json', import.meta.url))).aliases;
const expectedNames = ['safe_cex::factory::ExchangeConstructor', 'safe_cex::factory::ExchangeLoader'];
const contract = alias => ({ category: 'non-decoder', evidence: 'Pure fixture for an actual callable alias shape.', reason: 'One Value instantiation is not generic decoder proof.', generics: alias.generics, rawTarget: alias.rawTarget, target: alias.target });

// IDs are explicit fixture identities, not guessed or reconstructed live IDs.
const paths = {
  7: {crate_id:2,path:['core','ops','function','Fn'],kind:'trait'},
  10: {crate_id:3,path:['alloc','rc','Rc'],kind:'struct'},
  11: {crate_id:5,path:['safe_cex_store','contract','Store'],kind:'trait'},
  12: {crate_id:4,path:['safe_cex_types','ExchangeName'],kind:'enum'},
  13: {crate_id:4,path:['safe_cex_types','ExchangeOptions'],kind:'struct'},
  14: {crate_id:2,path:['core','result','Result'],kind:'enum'},
  15: {crate_id:3,path:['alloc','string','String'],kind:'struct'},
  16: {crate_id:2,path:['core','pin','Pin'],kind:'struct'},
  17: {crate_id:3,path:['alloc','boxed','Box'],kind:'struct'},
  18: {crate_id:2,path:['core','future','future','Future'],kind:'trait'},
  19: {crate_id:0,path:['safe_cex','factory','ExchangeConstructor'],kind:'type_alias'},
  20: {crate_id:0,path:['safe_cex','factory','ExchangeLoader'],kind:'type_alias'},
};
const ids = new Map(Object.entries(paths).map(([id,item])=>[item.path.join('::'),Number(id)]));
function withIds(value) {
  if (Array.isArray(value)) return value.map(withIds);
  if (!value || typeof value !== 'object') return value;
  const result = Object.fromEntries(Object.entries(value).map(([key,item])=>[key,withIds(item)]));
  if (typeof value.path === 'string') {
    assert.ok(ids.has(value.path), `fixture path needs an explicit compiler ID: ${value.path}`);
    result.id = ids.get(value.path);
  }
  return result;
}
function compilerDoc(files) {
  const module=(id,name,items)=>({id,crate_id:0,name,visibility:'public',attrs:[],inner:{module:{items,is_crate:id===0,is_stripped:false}}});
  const doc={format_version:61,root:0,paths:structuredClone(paths),external_crates:{
    2:{name:'core',path:files.core},3:{name:'alloc',path:files.alloc},
    4:{name:'safe_cex_types',path:files.types},5:{name:'safe_cex_store',path:files.store},
  },index:{0:module(0,'safe_cex',[1]),1:module(1,'factory',[19,20])}};
  assert.deepEqual(snapshot.map(alias=>alias.name).sort(),expectedNames);
  for (const alias of snapshot) {
    const id=ids.get(alias.name);
    doc.index[id]={id,crate_id:0,name:alias.ident,span:alias.span,visibility:'public',attrs:[],inner:{type_alias:{type:withIds(alias.rawTarget),generics:alias.generics}}};
  }
  return doc;
}
function aliasRows(doc) {
  return inventory(doc).externalTypeAliases.filter(row=>expectedNames.includes(row.name));
}
function resolveRows(doc, fixture) {
  return aliasRows(doc).map(row=>({...row,artifact:aliasTargetIdentity(row,fixture.artifacts,fixture.members,fixture.sysroot)}));
}
function fixture(run) {
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'callable-alias-pure-'));
  try {
    const sysroot=path.join(directory,'sysroot');
    const libraries=path.join(sysroot,'lib','rustlib','fixture-target','lib');
    fs.mkdirSync(libraries,{recursive:true});
    const files={core:path.join(libraries,'libcore.rmeta'),alloc:path.join(libraries,'liballoc.rmeta'),types:path.join(directory,'libsafe_cex_types.rmeta'),store:path.join(directory,'libsafe_cex_store.rmeta')};
    for(const file of Object.values(files))fs.writeFileSync(file,'synthetic compiler artifact fixture');
    const members=[{id:'fixture-types'},{id:'fixture-store'}];
    const artifacts=[{package_id:'fixture-types',filenames:[files.types],features:['fixture']},{package_id:'fixture-store',filenames:[files.store],features:['fixture']}];
    const f={directory,sysroot,files,members,artifacts};f.doc=compilerDoc(files);f.rows=resolveRows(f.doc,f);f.aliases=inventory(f.doc).publicAliases;
    f.context={macroOrigins:['serde_json'],externalTypeAliases:f.rows};
    run(f);
  } finally {fs.rmSync(directory,{recursive:true,force:true});}
}

test('both reviewed callable alias shapes keep their internally fixed alias instantiation',()=>fixture(f=>{
  assert.deepEqual(f.aliases.map(alias=>alias.name).sort(),expectedNames);
  for(const alias of f.aliases) {
    const probe=genericAliasProbe(alias,contract(alias),f.context);
    assert.equal(probe.suffix,'<::serde_json::Value>');assert.equal(probe.canonical,'alloc::rc::Rc');
    assert.match(probe.monomorphization,/no claim for every I/);
  }
}));

test('ordinary identity path and Option identity mutant retain their original behavior',()=>fixture(f=>{
  const identity={...f.aliases[0],rawTarget:{generic:'I'},target:{generic:'I'}};
  assert.deepEqual(genericAliasProbe(identity,undefined,f.context),identityAliasProbe(identity));
  assert.throws(()=>genericAliasProbe(identity),/actual compiler origin context required/);
  assert.throws(()=>genericAliasProbe(identity,undefined,{}),/actual compiler origin context required/);
  assert.throws(()=>genericAliasProbe(identity,undefined,{macroOrigins:[]}),/actual serde_json dependency required/);
  assert.throws(()=>genericAliasProbe(identity,undefined,{macroOrigins:['syn']}),/actual serde_json dependency required/);
  assert.deepEqual(genericAliasProbe(identity,undefined,{macroOrigins:['serde_json']}),identityAliasProbe(identity),'plain I needs no fabricated external target records');
  assert.equal(genericAliasProbe({...identity,generics:{params:[]}}),undefined,'non-generic aliases retain their context-free shape behavior');
  const option={resolved_path:{path:'core::option::Option',args:{angle_bracketed:{args:[{type:{generic:'I'}}],constraints:[]}}}};
  assert.throws(()=>genericAliasProbe({...identity,rawTarget:option,target:option},undefined,f.context),/source must directly name its type parameter/);
}));

test('callable pins and bounded grammar reject redirect target and generic changes',()=>fixture(f=>{
  const original=f.aliases[0];
  for(const change of [
    alias=>{alias.generics.params[0].kind.type.bounds=[{trait_bound:{trait:{path:'Copy'}}}];},
    alias=>{alias.generics.params[0].kind.type.default={primitive:'bool'};},
    alias=>{alias.generics.params.push({name:"'a",kind:{lifetime:{outlives:[]}}});},
    alias=>{alias.rawTarget.resolved_path.args.angle_bracketed.args[0].type.dyn_trait.lifetime="'static";},
    alias=>{alias.target.resolved_path.args.angle_bracketed.args[0].type.dyn_trait.traits[0].trait.args.parenthesized.inputs=[];},
  ]) {const changed=structuredClone(original);change(changed);assert.throws(()=>genericAliasProbe(changed,contract(original),f.context));}
  // Refresh only fixture pins to witness structural refusal independently.
  const nestedBounds=structuredClone(f.aliases[1]);
  function addNestedBound(node) {
    if(!node||typeof node!=='object')return;
    if(node.dyn_trait?.traits?.[0]?.trait?.path==='core::future::future::Future')node.dyn_trait.traits.push({trait:{path:'core::marker::Send',args:null},generic_params:[]});
    for(const value of Object.values(node)){if(Array.isArray(value))value.forEach(addNestedBound);else addNestedBound(value);}
  }
  addNestedBound(nestedBounds.rawTarget);addNestedBound(nestedBounds.target);
  assert.throws(()=>genericAliasProbe(nestedBounds,contract(nestedBounds),f.context),/multiple dyn bounds unsupported/);
  const bounded=structuredClone(original);bounded.generics.params[0].kind.type.bounds=[{trait_bound:{trait:{path:'Copy'}}}];
  assert.throws(()=>genericAliasProbe(bounded,contract(bounded),f.context),/bounds\/defaults\/lifetimes/);
  assert.throws(()=>genericAliasProbe(original,{...contract(original),instantiation:'Unrelated'},f.context),/cannot redirect/);
  assert.throws(()=>genericAliasProbe(original,{...contract(original),category:'decoder'},f.context),/non-decoder/);
  assert.throws(()=>genericAliasProbe(original,contract(original)),/actual compiler origin context required/);
  const futureRoot={...original,rawTarget:{resolved_path:{path:'core::pin::Pin'}},target:{resolved_path:{path:'core::pin::Pin'}}};
  assert.throws(()=>genericAliasProbe(futureRoot,contract(futureRoot),f.context),/source must directly name its type parameter/);
}));

test('callable compiler identity refuses missing registry and absent serde dependencies',()=>fixture(f=>{
  const alias=f.aliases[0],probe=genericAliasProbe(alias,contract(alias),f.context);
  assert.throws(()=>requireCallableCompilerIdentity(alias,probe,[],['serde_json']),/actual sysroot Rc identity/);
  const wrong=f.rows.map(row=>row.canonical==='alloc::rc::Rc'?{...row,artifact:{kind:'registry'}}:row);
  assert.throws(()=>requireCallableCompilerIdentity(alias,probe,wrong,['serde_json']),/actual sysroot Rc identity/);
  const wrongFn=f.rows.map(row=>row.canonical==='core::ops::function::Fn'?{...row,artifact:{kind:'registry'}}:row);
  assert.throws(()=>requireCallableCompilerIdentity(alias,probe,wrongFn,['serde_json']),/actual sysroot Fn identity/);
  assert.throws(()=>requireCallableCompilerIdentity(alias,probe,f.rows,[]),/actual serde_json dependency/);
}));

test('synthetic compiler Fn IDs resolve to sysroot along with Rc',()=>fixture(f=>{
  for(const alias of f.aliases)requireCallableCompilerIdentity(alias,genericAliasProbe(alias,contract(alias),f.context),f.rows,['serde_json']);
  const functions=f.rows.filter(row=>row.canonical==='core::ops::function::Fn');
  assert.equal(functions.length,3,'constructor one; loader outer plus expanded constructor one');
  assert.ok(functions.every(row=>row.kind==='trait'&&row.artifact.kind==='sysroot'));
  assert.equal(f.rows.filter(row=>row.canonical==='alloc::rc::Rc').length,5,'constructor two; loader three');
  const stores=f.rows.filter(row=>row.canonical==='safe_cex_store::contract::Store');
  assert.equal(stores.length,2,'one nested Store occurrence per actual alias');
  assert.ok(stores.every(row=>row.kind==='trait'&&row.artifact.kind==='workspace'&&row.artifact.packageId==='fixture-store'));
}));

test('compiler-selected same-spelling workspace Fn never inherits sysroot permission',()=>fixture(f=>{
  const fake=path.join(f.directory,'librenamed_core.rmeta');fs.writeFileSync(fake,'synthetic selected compiler artifact');
  const copy=structuredClone(f.doc);
  // Model cfg/import selection changing the compiler owner while the canonical
  // spelling stays identical. This is not a compiled source-cfg witness.
  copy.paths[7].crate_id=6;copy.external_crates[6]={name:'renamed_core',path:fake};
  const selected={...f,members:[...f.members,{id:'renamed-workspace-core'}],artifacts:[...f.artifacts,{package_id:'renamed-workspace-core',filenames:[fake],features:['selected-binding']}]};
  const rows=resolveRows(copy,selected);
  const functions=rows.filter(row=>row.canonical==='core::ops::function::Fn');
  assert.equal(functions.length,3);
  assert.ok(functions.every(row=>row.artifact.kind==='workspace'&&row.artifact.packageId==='renamed-workspace-core'));
  for(const alias of f.aliases)assert.throws(()=>genericAliasProbe(alias,contract(alias),{macroOrigins:['serde_json'],externalTypeAliases:rows}),/actual sysroot Fn identity required/);
}));

test('unresolved selected Fn compiler identity refuses before any path-name approval',()=>fixture(f=>{
  const copy=structuredClone(f.doc);copy.external_crates[2].path=null;
  assert.throws(()=>aliasRows(copy),/external alias trait lacks compiler identity/);
}));
