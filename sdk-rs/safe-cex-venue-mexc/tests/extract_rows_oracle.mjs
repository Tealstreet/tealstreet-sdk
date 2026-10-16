import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {root,ts} from './source.mjs';
const file=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const source=fs.readFileSync(file,'utf8');const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const selected=[];
for(const statement of ast.statements)if(ts.isVariableStatement(statement))for(const d of statement.declarationList.declarations)if(['extractData','extractRows'].includes(d.name.getText(ast)))selected.push('const '+d.getText(ast)+';');
assert.equal(selected.length,2,'SOURCE independent exact helper declarations');
const context={};vm.runInNewContext(ts.transpileModule(selected.join('\n')+'\nthis.extractRows=extractRows;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const payload={data:{data:[[1,2]],items:[7]}};
const actualArrayPrototype=Object.getPrototypeOf(payload.data.data);
assert.equal(actualArrayPrototype,Array.prototype,"SOURCE supplied array belongs to actual host prototype");
const output=[];let calls=0;const invoke=(payload,keys)=>{calls++;return context.extractRows(payload,keys);};
for(const [key,expected]of [['0',[1,2]],['00',[7]],['-0',[7]],['__proto__',[]]]){
 const rows=invoke(payload,[key,'items']);
 assert.deepEqual(Array.from(rows),expected,'SOURCE independent exact array property key '+key);
 assert.equal(rows===actualArrayPrototype,key==='__proto__','SOURCE actual returned stock Array.prototype identity');
 output.push({key,rows:Array.from(rows),isArrayPrototype:rows===actualArrayPrototype});
}
const first=invoke(payload,['__proto__','items']);
const originalLength=actualArrayPrototype.length;
assert.equal(originalLength,0,'SOURCE independent clean prototype length');
try{
 actualArrayPrototype.push({marker:42});
 assert.equal(first.length,1,'SOURCE earlier result observes producer Array.prototype mutation');
 assert.equal(first[0].marker,42,'SOURCE earlier result retains new prototype element');
 const later=invoke(payload,['__proto__','items']);
 assert.equal(later,first,'SOURCE later lookup returns same mutated prototype owner');
 assert.equal(later[0].marker,42,'SOURCE later result sees producer mutation');
 output.push({key:'live-prototype',before:0,after:first.length,marker:first[0].marker,sameOwner:later===first});
}finally{actualArrayPrototype.length=originalLength;}
for(const [label,escaped,expected]of [
 ['lone','\\ud800',[0xd800]],
 ['bmp','\\u00e9',[0x00e9]],
 ['astral','\\ud83d\\ude03',[0xd83d,0xde03]],
 ['whitespace','\\ufeff\\u3000',[0xfeff,0x3000]],
]){
 const unicodePayload=JSON.parse('{"data":[{"symbol":"'+escaped+'"}]}');
 const rows=invoke(unicodePayload,[]);
 assert.equal(rows,unicodePayload.data,'SOURCE returned input array owner');
 assert.equal(rows.length,1,'SOURCE independent Unicode row count');
 assert.equal(typeof rows[0].symbol,'string','SOURCE program-visible returned symbol is a string');
 const units=Array.from({length:rows[0].symbol.length},(_,i)=>rows[0].symbol.charCodeAt(i));
 assert.deepEqual(units,expected,'SOURCE exact program-visible returned UTF-16 units '+label);
 output.push({key:'unicode-'+label,units});
}
assert.equal(calls,10,'SOURCE independent extractRows actual call floor');
assert.equal(output.length,9,'SOURCE independent array property plus live-alias root floor');
console.log(JSON.stringify(output));
