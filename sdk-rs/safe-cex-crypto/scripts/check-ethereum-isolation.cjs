// Validate tracked oracle sources outside every repository node_modules ancestor.
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),assert=require('node:assert/strict');
const {createRequire}=require('node:module');
const root=path.resolve(__dirname,'../../..');
const deps=process.env.SAFE_CEX_CRYPTO_ORACLE_DEPS;
assert.ok(deps&&path.isAbsolute(deps),'SAFE_CEX_CRYPTO_ORACLE_DEPS must name the isolated frozen install');
const resolved=fs.realpathSync(createRequire(path.join(deps,'package.json')).resolve('ethers'));
assert.ok(resolved.startsWith(fs.realpathSync(deps)+path.sep),'ethers must resolve inside the isolated install');
const mirror=fs.mkdtempSync(path.join(require('node:os').tmpdir(),'safe-cex-ethereum-oracle-'));
try {
 const names=['primitives.cjs','primitives.json','icap-maximum.cjs','icap-maximum.json','typed-validation.cjs','typed-validation.json','aster-preimages.cjs','aster-preimages.json'].map(name=>'sdk-rs/safe-cex-crypto/tests/fixtures/ethereum/'+name).concat(['sdk-rs/safe-cex-crypto/tests/fixtures/oracle-packages.cjs','sdk-rs/safe-cex-crypto/tests/fixtures/oracle-packages.json','packages/safe-cex/src/exchanges/asterdex/asterdex.api.ts']);
 for(const name of names){assert.equal(cp.spawnSync('git',['ls-files','--error-unmatch',name],{cwd:root}).status,0,'mirror copies tracked source only');const target=path.join(mirror,name);fs.mkdirSync(path.dirname(target),{recursive:true});fs.copyFileSync(path.join(root,name),target);}
 for(let directory=mirror;;directory=path.dirname(directory)){assert.ok(!fs.existsSync(path.join(directory,'node_modules')),'mirror has no node_modules ancestor');if(directory===path.dirname(directory))break;}
 for(const name of ['primitives.cjs','icap-maximum.cjs','typed-validation.cjs','aster-preimages.cjs']) {
  const target=path.join(mirror,'sdk-rs/safe-cex-crypto/tests/fixtures/ethereum',name);
  const result=cp.spawnSync(process.execPath,[target],{env:process.env,encoding:'utf8'});assert.equal(result.status,0,`${name}: ${result.stderr}`);
  const code=fs.readFileSync(target,'utf8').replace("oracleRequire('ethers')","oracleRequire('deliberately-missing-ethereum-oracle-package')");
  assert.notEqual(code,fs.readFileSync(target,'utf8'),'control must mutate the runtime import');
  const control=cp.spawnSync(process.execPath,['-e',`const Module=require('module');const m=new Module(${JSON.stringify(target)});m.filename=${JSON.stringify(target)};m.paths=Module._nodeModulePaths(require('path').dirname(m.filename));m._compile(${JSON.stringify(code)},m.filename);`],{env:process.env,encoding:'utf8'});
  assert.notEqual(control.status,0,'missing import must fail');assert.match(control.stderr,/MODULE_NOT_FOUND/,'missing import diagnostic');
  if(name==='aster-preimages.cjs') {
    const missing=cp.spawnSync(process.execPath,[target],{env:{...process.env,SAFE_CEX_TYPESCRIPT_PATH:'/deliberately-missing-ethereum-compiler'},encoding:'utf8'});
    assert.notEqual(missing.status,0,'missing compiler must fail');assert.match(missing.stderr,/MODULE_NOT_FOUND/,'missing compiler diagnostic');
    const changed=fs.readFileSync(target,'utf8').replace("selected.join('\\n')", "selected.join('\\n').replace('ethers.hashMessage(ethers.getBytes(payloadHash))','ethers.keccak256(payloadHash)')");
    assert.notEqual(changed,fs.readFileSync(target,'utf8'),'control mutates actual helper body');
    const mutation=cp.spawnSync(process.execPath,['-e',`const Module=require('module');const m=new Module(${JSON.stringify(target)});m.filename=${JSON.stringify(target)};m.paths=Module._nodeModulePaths(require('path').dirname(m.filename));m._compile(${JSON.stringify(changed)},m.filename);`],{env:process.env,encoding:'utf8'});
    assert.notEqual(mutation.status,0,'changed TS composition must fail');assert.match(mutation.stderr,/actual Aster TS signing preimage drift/,'actual TS composition diagnostic');
  }
  console.log(`${name}: isolated pass; missing-import control fails MODULE_NOT_FOUND`);
 }
}finally{fs.rmSync(mirror,{recursive:true,force:true});}
