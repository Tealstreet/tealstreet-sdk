const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {root,ts,header,capture}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const source=fs.readFileSync(file,'utf8');
const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
const names=['normalizeTimestampToMs','normalizeTimestampToSeconds'];
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(selected.length,2,'timestamp_source_inventory');
const context={exports:{}};
vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const rawCases=[['null','null'],['false','false'],['true','true'],['zero','0'],['negative_zero','-0'],['underflow','-1e-4000'],['overflow','1e400'],['negative_overflow','-1e400'],['rounded_integer','9007199254740993.0'],['nanoseconds','1700000000123456789'],['microseconds','1700000000123456'],['milliseconds','1700000000123.75'],['seconds','1700000000.875'],['submillisecond','0.0009'],['huge_finite','1.7976931348623157e308'],['negative_number','-1'],['empty','""'],['space','" "'],['binary','"0b10"'],['hex','"0x10"'],['prefix_junk','"123tail"'],['invalid','"no-date"'],['array','["2023-11-14T22:13:20.987Z"]'],['empty_array','[]'],['object','{}'],['coercion_error','{"toString":null}']];
for(const n of [1e12-0.001,1e12,1e12+0.001,1e15-1,1e15,1e15+1,1e18-128,1e18,1e18+128])rawCases.push([`threshold_${n}`,String(n)]);
for(const date of ['2023-11-14T22:13:20.987Z','2023-11-14T22:13:20.987','2023-11-14','1969-12-31T23:59:59.999Z','1970-01-01T00:00:00Z','2024-02-30','2024-01-01T24:00:00','Tue, 14 Nov 2023 22:13:20 GMT','01/02/49','01/02/50','+275760-09-13T00:00:00.000Z','+275760-09-13T00:00:00.001Z','-271821-04-20T00:00:00.000Z']) rawCases.push([date,JSON.stringify(date)]);
const inputs=[{id:'absent',absent:true},...rawCases.map(([id,raw])=>({id,raw}))];
const cases=[];
for(const [zone,offset] of [['UTC',0],['Etc/GMT+5',18000000],['Etc/GMT-9',-32400000]]){
 process.env.TZ=zone;
 for(const c of inputs)for(const fn of names){try{cases.push({...c,fn,zone,offset,result:capture(context.exports[fn](c.absent?undefined:JSON.parse(c.raw)))});}catch(e){cases.push({...c,fn,zone,offset,result:{type:'error',name:e.name,message:e.message}});}}
}
process.env.TZ='UTC';
const refusals=names.map(fn=>({fn,raw:'"\\ud800"',native:capture(context.exports[fn]('\ud800'))}));
const depth=names.map(fn=>{const raw='['.repeat(150)+'"1700000000.875"'+']'.repeat(150);return{fn,raw,native:capture(context.exports[fn](JSON.parse(raw)))};});
const callable=names.map(fn=>({fn,native:capture(context.exports[fn]({valueOf:()=>1700000000.875}))}));
assert.notEqual(cases.find(c=>c.id==='seconds'&&c.fn===names[0]).result.bits,cases.find(c=>c.id==='seconds'&&c.fn===names[1]).result.bits,'timestamp_unit_supplement');
process.stdout.write(JSON.stringify({header,cases,refusals,depth,callable}));
