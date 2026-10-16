// Field-identity supplement over original archive bytes; capture host darwin arm64.
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {gunzipSync} from 'node:zlib';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const source=new URL('../../../mobile-services/candle-server/scripts/replay-day.ts',import.meta.url);
let text=fs.readFileSync(source,'utf8');
if(process.argv.includes('--mutate-columns'))text=text.replace('const [timestamp, open, high, low, close, volume]','const [timestamp, close, high, low, open, volume]');
const ast=ts.createSourceFile(source.pathname,text,ts.ScriptTarget.Latest,true);
const declaration=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='load'));assert.ok(declaration,'actual replay loader must exist');
const module={exports:{}};
const code=ts.transpileModule(declaration.getText(ast).replaceAll('import.meta.url','archiveBase')+'\nmodule.exports.load=load;',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInThisContext('(function(module,readFileSync,gunzipSync,archiveBase){'+code+'\n})')(module,fs.readFileSync,gunzipSync,source.href);
const files=['BTCUSDT-1s-2024-01-15.csv.gz','BTCUSDT-1m-2025-01-15.csv','BTCUSDT-8h-2025-01-15.csv','BTCUSDT-1mo-2023-02.csv','BTCUSDT-1mo-2023-03.csv'];
const fields=['timestamp','open','high','low','close','volume'];let records=0;
for(const file of files){
 const bytes=fs.readFileSync(new URL('../fixtures/'+file,source));const lines=(file.endsWith('.gz')?gunzipSync(bytes):bytes).toString().trim().split('\n').filter(x=>!x.startsWith('open_time'));
 const decoded=module.exports.load(file);assert.equal(decoded.length,lines.length,'archive loader record count');
 for(let i=0;i<lines.length;i++){
  const columns=lines[i].split(',');for(let j=0;j<fields.length;j++)assert.ok(Object.is(decoded[i][fields[j]],Number(columns[j])),`archive loader field identity ${fields[j]} ${file}/${i}`);
 }
 records+=lines.length;
}
assert.equal(records,87845);process.stdout.write(JSON.stringify({files:5,records,fieldsPerRecord:6,captureArchitecture:process.platform+' '+process.arch}));
