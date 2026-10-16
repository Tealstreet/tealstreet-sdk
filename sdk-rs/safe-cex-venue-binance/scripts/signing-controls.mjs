import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
const filename = new URL('./signing.mjs', import.meta.url);
const source = fs.readFileSync(filename, 'utf8').replaceAll('import.meta.url', JSON.stringify(filename.href));
const input = JSON.stringify([{config:{data:{x:1}}, options:{key:'k',secret:'s'}, mode:'callback',now:1,window:2}]);
function compare(code) {
  const child=spawnSync(process.execPath,['--input-type=module','--eval',code],{input,encoding:'utf8'});
  assert.equal(child.status,0,child.stderr);
  const result=JSON.parse(child.stdout);
  assert.equal(result[0]?.calls?.[0]?.payload,'x=1&timestamp=1&recvWindow=2','live signer must use original receive window');
}
compare(source);
const mutation=source.replace("fs.readFileSync(filename, 'utf8')", "fs.readFileSync(filename, 'utf8').replace('data.recvWindow = recvWindow;', 'data.recvWindow = recvWindow + 1;')");
assert.notEqual(mutation,source,'control must mutate the loaded source');
assert.throws(()=>compare(mutation),error=>error.code==='ERR_ASSERTION' && error.message.includes('live signer must use original receive window'));
assert.throws(()=>compare('process.stdout.write("[]")'),error=>error.code==='ERR_ASSERTION' && error.message.includes('live signer must use original receive window'));
for (const name of ['qs','axios','create-hmac','lodash/omit']) {
  const broken=source.replace(`runtimeRequire('${name}')`,`runtimeRequire('binance-intentionally-missing-package/${name}')`);
  assert.notEqual(broken,source,`missing-package control must mutate ${name}`);
  const child=spawnSync(process.execPath,['--input-type=module','--eval',broken],{input,encoding:'utf8'});
  assert.notEqual(child.status,0,`${name} import must fail loudly`);
  assert.match(child.stderr,/MODULE_NOT_FOUND/,`${name} must fail at module resolution`);
}
console.log('Binance signer source mutation, no-op and four missing-package controls passed');

// Keep the real ambient packages present while removing them from the selected install.
const require=createRequire(import.meta.url);
const probe=fs.mkdtempSync(path.join(process.cwd(),'.binance-oracle-controls-'));
try {
 const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH || path.dirname(require.resolve('typescript/package.json'));
 fs.symlinkSync(path.resolve(compiler),path.join(probe,'typescript'),'dir');
 const child=spawnSync(process.execPath,['--input-type=module','--eval',source],{input,encoding:'utf8',env:{...process.env,SAFE_CEX_TYPESCRIPT_PATH:path.join(probe,'typescript')}});
 assert.notEqual(child.status,0,'missing real qs must not resolve through ambient ancestors');
 assert.match(child.stderr,/MODULE_NOT_FOUND/);
 assert.ok(child.stderr.includes(path.join(probe,'qs')),'failure must name qs inside selected install');
} finally {fs.rmSync(probe,{recursive:true,force:true});}
