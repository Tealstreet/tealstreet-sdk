const fs=require('node:fs'),os=require('node:os'),path=require('node:path'),assert=require('node:assert/strict'),{spawnSync}=require('node:child_process');
const root=path.resolve(__dirname,'../../..');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH||path.dirname(require.resolve('typescript/package.json'));
const temp=fs.mkdtempSync(path.join(os.tmpdir(),'canonical-isolation-'));
const relative='sdk-rs/safe-cex-venue-woo/scripts/public-canonical-semantics.cjs';
const source='packages/safe-cex/src/exchanges/woo/woo.v3.public-parity.ts';
const bootstrap=`const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),{createRequire}=require('node:module');const oracle=process.argv[1],mode=process.argv[2],local=createRequire(oracle);const wrapped={...fs,readFileSync(file,...args){let data=fs.readFileSync(file,...args);if(mode==='mutant'&&String(file).endsWith('woo.v3.public-parity.ts')){data=data.replace('fundingRate: stats.estimatedFundingRate','fundingRate: stats.volume24h');}return data;}};let code=fs.readFileSync(oracle,'utf8');if(mode==='noop')code='process.stdout.write("{}")';vm.runInNewContext(code,{require:id=>id==='node:fs'?wrapped:local(id),__dirname:path.dirname(oracle),process,console},{filename:'canonical-oracle'});`;
function run(mode,ts=compiler){return spawnSync(process.execPath,['-e',bootstrap,path.join(temp,relative),mode],{encoding:'utf8',env:{...process.env,SAFE_CEX_TYPESCRIPT_PATH:ts}});}
function checkMutation(child){assert.equal(child.status,1,'mutated source must assert-fail');assert.match(child.stderr,/source statistics fundingRate/,'mutation must fail its own assertion');}
try{
 for(const file of [relative,source]){fs.mkdirSync(path.dirname(path.join(temp,file)),{recursive:true});fs.copyFileSync(path.join(root,file),path.join(temp,file));}
 for(let dir=temp;;dir=path.dirname(dir)){assert.equal(fs.existsSync(path.join(dir,'node_modules')),false,'mirror has no ancestor node_modules');if(path.dirname(dir)===dir)break;}
 const good=run('good');assert.equal(good.status,0,good.stderr);assert.equal(JSON.parse(good.stdout).projections.length,5);
 const missing=run('good',path.join(temp,'missing-typescript'));assert.equal(missing.status,1);assert.match(missing.stderr,/MODULE_NOT_FOUND/);
 checkMutation(run('mutant'));
 const noop=run('noop');assert.equal(noop.status,0);assert.throws(()=>checkMutation(noop),/mutated source must assert-fail/);
 console.log('canonical isolation: four child probes; independent property-call floor 50; missing compiler refused, source mutation caught, no-op rejected');
}finally{fs.rmSync(temp,{recursive:true,force:true});}
