const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const load=require('../../../scripts/ts-source.cjs');
const parse=load(path.resolve(__dirname,'../../../../../packages/safe-cex/src/utils/tryParseJson.ts')).tryParseJson;
const inputs=[['lone-high','{"s":"\\ud800"}',true],['lone-low','{"s":"\\udfff"}',true],['scalar-unicode','{"s":"雪😀\ufeff"}',false],['deep-object','{"n":'.repeat(200)+'0'+'}'.repeat(200),true]];
const cases=inputs.map(([name,text,refused])=>{const result=parse(text);return {name,raw:JSON.stringify(text),refused,sourceKind:typeof result,sourceWire:JSON.stringify(result),sourceStringUnits:typeof result.s==='string'?Array.from({length:result.s.length},(_,i)=>result.s.charCodeAt(i)):null};});
const target=path.join(__dirname,'boundaries.json'),result=JSON.stringify({source:'actual tryParseJson; valid JSON surrogate escape is a representation boundary, not invalid JSON',cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});else assert.equal(result,fs.readFileSync(target,'utf8'),'tryParseJson boundary oracle drift');
