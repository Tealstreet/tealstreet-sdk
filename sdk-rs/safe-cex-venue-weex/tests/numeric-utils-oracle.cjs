const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {root,ts,header,capture}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const source=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const names=['toNumber','toIntegerString','normalizePercentage'];
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(selected.length,3,'numeric_utility_inventory');
const sourceText=selected.map(n=>n.getText(ast)).join('\n')+'\nexport { normalizePercentage };';
const context={exports:{}};
vm.runInNewContext(ts.transpileModule(sourceText,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const raws=['null','false','true','0','-0','-0.0','-1e-4000','1e400','-1e400','9007199254740993.0','123.875','1e-7','1e21','""','"-0"','"-1e-4000"','"Infinity"','"NaN"','"-Infinity"','"1.2foo"','"0x10"','"1e+"','".125"','"-.5"','"1"','"-1"','"1.0000000000000002"','"-1.0000000000000002"','"99"','"\\ufeff 12.5tail"','"\\u0085.5"','[]','[null]','["12.5"]','[["-.25"]]','["3","4"]','{}','{"toString":null}','[{"toString":0}]','"𝒜é"'];
const inputs=[{id:'absent',absent:true},...raws.map((raw,i)=>({id:`raw_${i}`,raw}))];
const number=n=>capture(n);
const cases=[];
function add(fn,c,args){
 try {const value=c.absent?undefined:JSON.parse(c.raw);const result=context.exports[fn](value,...args);cases.push({...c,fn,args:args.map(number),result:typeof result==='number'?number(result):{type:'string',value:result}});}
 catch(e){cases.push({...c,fn,args:args.map(number),result:{type:'error',name:e.name,message:e.message}});}
}
for(const c of inputs){
 for(const fallback of [[],[17.25],[-0],[NaN],[Infinity]]) add('toNumber',c,fallback);
 add('toIntegerString',c,[]);
 for(const args of [[131,100],[17,0],[17,-0],[17,NaN],[Infinity,3],[17,Infinity],[-17,-3]]) add('normalizePercentage',c,args);
}
// Native -0 collapses through String(-0); a string '-0' is the discriminating supplement.
assert.equal(capture(context.exports.toNumber(-0)).bits,'0000000000000000','source_zero_collapse');
assert.equal(capture(context.exports.toNumber('-0')).bits,'8000000000000000','source_zero_supplement');
const depth=names.map(fn=>{const raw='['.repeat(150)+'".25"'+']'.repeat(150); return {fn,raw,result:context.exports[fn](JSON.parse(raw),131,100)};});
const callable=names.map(fn=>({fn,result:context.exports[fn]({toString:()=>'.25'},131,100)}));
assert.equal(callable[0].result,.25); assert.equal(callable[1].result,'.25'); assert.equal(callable[2].result,25);
const refusals=names.map(fn=>({fn,raw:'"\\ud800"',native:capture(context.exports[fn]('\ud800',131,100))}));
assert.deepEqual(refusals[1].native,{utf16:[0xd800]},'numeric_utility_source_utf16');
process.stdout.write(JSON.stringify({header,cases,depth,callable,refusals}));
