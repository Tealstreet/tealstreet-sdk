// The counted control mutates only the existing Callback fixture and restores it.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';

const scripts=path.dirname(fileURLToPath(import.meta.url));
const fixture=path.resolve(scripts,'../fixtures/accepted-set');
const aliasName='accepted_set_reexport_control::Callback';
const expectedTarget='super::Callback<::serde_json::Value>';
const fixedScope='one fixed serde_json::Value actual callable-alias probe; no claim for every I';
const mutationName='generic callable alias native root identity';
const expectedReceiptNames=[
  'accepted_set_control::Side','accepted_set_control::private::Record',
  'accepted_set_reexport_control::Callback','accepted_set_reexport_control::ClosedSide',
  'accepted_set_reexport_control::ExternalJson','src/lib.rs::LocalHelper',
].sort();
function cargo(args,extraEnv={}) {
  const result=spawnSync('cargo',['--color','never',...args],{cwd:fixture,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,CARGO_BUILD_JOBS:'2',...extraEnv}});
  assert.equal(result.status,0,`callable preparation failed; compiler failures are not controls\n${result.error??''}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
function discoverActualAlias() {
  const metadata=JSON.parse(cargo(['metadata','--no-deps','--locked','--format-version','1']));
  const pkg=metadata.packages.find(pkg=>pkg.name==='accepted-set-reexport-control');
  assert.ok(pkg&&metadata.workspace_members.includes(pkg.id),'CALLABLE compiler workspace package');
  const targets=pkg.targets.filter(target=>target.kind.includes('lib'));
  assert.equal(targets.length,1,'CALLABLE unique production library target');
  cargo(['rustdoc','--locked','-p',pkg.name,'--lib','--jobs','2','--','-Z','unstable-options','--output-format','json','--document-private-items','--document-hidden-items','-A','rustdoc::all'],{RUSTC_BOOTSTRAP:'1'});
  const doc=JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'doc',`${targets[0].name.replaceAll('-','_')}.json`)));
  const found=inventory(doc);
  const aliases=found.publicAliases.filter(alias=>alias.name===aliasName);
  assert.equal(aliases.length,1,'CALLABLE exactly one fresh compiler alias');
  assert.equal(aliases[0].ident,'Callback');
  assert.equal(path.resolve(fixture,aliases[0].span.filename),path.join(fixture,'reexport/src/lib.rs'));
  return {metadata,alias:aliases[0]};
}
function child(mutate=false) {
  const result=spawnSync(process.execPath,[fileURLToPath(import.meta.url),'--callable-probe',...(mutate?['--wrong-root']:[])],{encoding:'utf8',maxBuffer:64*1024*1024});
  assert.ok(!result.error,String(result.error));
  const output=`${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output,/compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  const markers=output.split('\n').filter(line=>line.startsWith('CALLABLE_NATIVE_EMITTER ')).map(line=>JSON.parse(line.slice('CALLABLE_NATIVE_EMITTER '.length)));
  assert.equal(markers.length,1,`CALLABLE post-validation emitter witness must execute exactly once; child exit ${result.status}\n${output}`);
  assert.equal(markers[0].targetType,expectedTarget,'CALLABLE actual alias must be the Rust probe target');
  assert.equal(markers[0].validatedRoot,'alloc::rc::Rc','CALLABLE static compiler validation must precede mutation');
  assert.equal(markers[0].scope,fixedScope,'CALLABLE fixed Value scope remains explicit');
  assert.equal(markers[0].emittedRoot,mutate?'alloc::boxed::Box':'alloc::rc::Rc');
  return {...result,output};
}

if(process.argv[2]==='--callable-probe') {
  const mutate=process.argv.includes('--wrong-root');
  const probes=pathToFileURL(path.join(scripts,'accepted-probes.mjs')).href;
  const key='__accepted_callable_native_control';
  globalThis[key]={mutate,observations:[]};
  let rewritten=0;
  const hook=registerHooks({load(url,context,nextLoad){
    const loaded=nextLoad(url,context);
    if(url!==probes)return loaded;
    const source=typeof loaded.source==='string'?loaded.source:Buffer.from(loaded.source).toString('utf8');
    const anchor='function aliasBody(type, targetType, contract) {';
    assert.equal(source.split(anchor).length-1,1,'CALLABLE unique post-validation alias emitter');
    const injected=`${anchor}\n  if(type.name===${JSON.stringify(aliasName)}) {\n    const control=globalThis[${JSON.stringify(key)}];\n    assert.equal(targetType,${JSON.stringify(expectedTarget)},"CALLABLE actual alias target");\n    assert.equal(contract.canonical,"alloc::rc::Rc","CALLABLE statically validated canonical root");\n    const observation={targetType,validatedRoot:contract.canonical,scope:contract.monomorphization};\n    if(control.mutate) contract={...contract,canonical:"alloc::boxed::Box"};\n    observation.emittedRoot=contract.canonical;\n    control.observations.push(observation);\n  }`;
    rewritten++;
    return {...loaded,source:source.replace(anchor,injected)};
  }});
  const runner=path.join(scripts,'accepted-set.mjs');
  process.argv=[process.execPath,runner,`--workspace=${fixture}`,`--contracts=${path.join(fixture,'contracts.json')}`,'--package=accepted-set-reexport-control'];
  const observations=globalThis[key].observations;
  try {await import(pathToFileURL(runner).href);}
  finally {
    hook.deregister();
    for(const observation of observations)console.log(`CALLABLE_NATIVE_EMITTER ${JSON.stringify(observation)}`);
    delete globalThis[key];
  }
  // Preserve an earlier compiler/inventory refusal; cleanup must not replace it.
  assert.equal(rewritten,1,'CALLABLE child hook must rewrite only the actual emitter module');
  assert.equal(observations.length,1,'CALLABLE compiler alias reaches native emitter exactly once');
} else {
  test('generic callable aliases execute their actual fixed native probe and reject root identity drift',{timeout:1200000},()=>{
    const consumer=path.join(fixture,'reexport/src/lib.rs');
    const contractPath=path.join(fixture,'contracts.json');
    const original=fs.readFileSync(consumer,'utf8');
    const originalContracts=fs.readFileSync(contractPath,'utf8');
    const before='pub type Callback = Box<dyn Fn(&str)>;';
    // Explicit return keeps this within the existing bounded callable grammar.
    const after='pub type Callback<I> = std::rc::Rc<dyn Fn(I) -> I>;';
    assert.equal(original.split(before).length-1,1,'CALLABLE existing mapped/exempt fixture alias must be selected once');
    try {
      const actualSource=original.replace(before,after);
      fs.writeFileSync(consumer,actualSource);
      // Bootstrap discovery uses fresh compiler JSON directly: the old Box
      // contract cannot discharge a changed generic alias via --inventory.
      const {metadata,alias}=discoverActualAlias();
      const contracts=JSON.parse(originalContracts);
      contracts[aliasName]={category:'non-decoder',evidence:'Compiled actual Callback alias with one internally fixed Value instantiation.',reason:'Typed callback; this receipt covers only Value and makes no universal I claim.',generics:alias.generics,rawTarget:alias.rawTarget,target:alias.target};
      const actualContracts=JSON.stringify(contracts);
      fs.writeFileSync(contractPath,actualContracts);
      const receipts=()=>JSON.parse(fs.readFileSync(path.join(metadata.target_directory,'accepted-set-results.json')));
      const baseline=()=>{
        const result=child();assert.equal(result.status,0,result.output);
        const rows=receipts();assert.deepEqual(rows.map(row=>row.name).sort(),expectedReceiptNames,'CALLABLE independent compiler-owner receipt floor');
        const callback=rows.find(row=>row.name===aliasName);
        assert.equal(callback.classificationOnly,true);assert.equal(callback.category,'non-decoder-alias');
        assert.equal(callback.deserialize,false);assert.equal(callback.monomorphization,fixedScope);assert.deepEqual(callback.failures,[]);
        assert.equal(fs.readFileSync(consumer,'utf8'),actualSource,'CALLABLE source unchanged across native control');
        assert.equal(fs.readFileSync(contractPath,'utf8'),actualContracts,'CALLABLE compiler pins unchanged across native control');
      };
      baseline();
      const broken=child(true);assert.equal(broken.status,1,broken.output);
      assert.match(broken.output,/AssertionError/);
      assert.match(broken.output,/Callback.*non-decoder alias compiler root identity mismatch/);
      const failures=receipts().filter(row=>row.failures.length);
      assert.deepEqual(failures.map(row=>row.name),[aliasName],'CALLABLE only the actual callback native identity fails');
      assert.deepEqual(failures[0].failures,['non-decoder alias compiler root identity mismatch']);
      console.log(`${mutationName}: named assertion; process exit 1`);
      baseline();
    } finally {
      fs.writeFileSync(consumer,original);
      fs.writeFileSync(contractPath,originalContracts);
    }
  });
}
