const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {spawnSync}=require('node:child_process');
const runtime=require('./oracle-runtime.cjs');
const root=path.resolve(__dirname,'../../..');
const target=path.join(root,'sdk-rs/target');fs.mkdirSync(target,{recursive:true});
const fixture=fs.mkdtempSync(path.join(target,'woo-runtime-isolation-'));
const packages=['typescript','axios','qs','create-hmac','lodash'];
const realRoot=name=>path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH
  ?require.resolve(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name,'package.json'))
  :require.resolve(name+'/package.json'));
try {
 const ambient=path.join(fixture,'node_modules');const isolated=path.join(fixture,'isolated/node_modules');
 fs.mkdirSync(ambient,{recursive:true});fs.mkdirSync(isolated,{recursive:true});
 for(const name of packages) {fs.symlinkSync(realRoot(name),path.join(ambient,name),'dir');fs.symlinkSync(realRoot(name),path.join(isolated,name),'dir');}
 for(const [missing,script] of [['qs','signing-oracle.cjs'],['lodash','utils-oracle.cjs'],['lodash','response-oracle.cjs']]) {
  // Real ambient packages remain available above the deliberately incomplete install.
  fs.unlinkSync(path.join(isolated,missing));
  assert.ok(fs.existsSync(path.join(ambient,missing,'package.json')));
  const result=spawnSync(process.execPath,[path.join(__dirname,script),'--check'],{encoding:'utf8',env:{...process.env,SAFE_CEX_TYPESCRIPT_PATH:path.join(isolated,'typescript')}});
  assert.equal(result.status,1,`${missing}: ambient fallback must not satisfy the oracle`);
  assert.match(result.stderr,/MODULE_NOT_FOUND/);
  assert.ok(result.stderr.includes(path.join(isolated,missing)),`${missing}: diagnostic must name the isolated package`);
  fs.symlinkSync(realRoot(missing),path.join(isolated,missing),'dir');
 }
 // The same real packages must work once the isolated install is complete.
 for(const script of ['signing-oracle.cjs','utils-oracle.cjs','response-oracle.cjs']) {
  const result=spawnSync(process.execPath,[path.join(__dirname,script),'--check'],{encoding:'utf8',env:{...process.env,SAFE_CEX_TYPESCRIPT_PATH:path.join(isolated,'typescript')}});
  assert.equal(result.status,0,result.stderr);
 }
 console.log('WOO runtime isolation: missing real qs/lodash refused with ambient packages present; complete install passes');
} finally {fs.rmSync(fixture,{recursive:true,force:true});}
