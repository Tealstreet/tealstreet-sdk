// Read-only source mutation: never edits the reference or its fixtures.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
if(process.env.HYPERLIQUID_HERITAGE_SOURCE) {
 const read=fs.readFileSync;
 fs.readFileSync=function(file,...args) {
  const result=read.call(this,file,...args);
  if(String(file).endsWith('/'+process.env.HYPERLIQUID_HERITAGE_SOURCE)&&typeof result==='string') {
   assert.match(result,/interface\s+\w+\s*\{/,'mutation requires an interface');
   return result.replace(/(interface\s+\w+)\s*\{/,'$1 extends ExtraNumeric {')+'\ninterface ExtraNumeric { hiddenNumeric: number; }\n';
  }
  return result;
 };
} else {
 const {spawnSync}=require('node:child_process');
 const cases=['types','oracle','typed-numbers','typed-null-boundaries','record-shapes','literal-unions'].map(name=>[name,'hyperliquid.types.ts']);
 cases.push(['record-shapes','hyperliquid.signing.ts']);
 for(const [name,source] of cases) {
  const child=spawnSync(process.execPath,['--require',__filename,path.join(__dirname,name+'.cjs'),'--check'],{encoding:'utf8',env:{...process.env,HYPERLIQUID_HERITAGE_SOURCE:source}});
  assert.notEqual(child.status,0,`${name}/${source} must refuse inherited numeric fields`);
  assert.match(child.stderr,/Unsupported interface heritage:/,`${name}/${source} must fail at the heritage guard, not drift or a compiler failure`);
 }
 console.log(cases.length+' inherited-number controls refused by name');
}
