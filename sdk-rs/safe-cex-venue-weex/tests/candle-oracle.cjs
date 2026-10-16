const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,header,capture}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['toNumber','normalizeTimestampToMs','normalizeTimestampToSeconds','normalizeCandle'];
const selected=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(selected.length,4,'candle_source_inventory');
const context={exports:{}};
vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
process.env.TZ='UTC';
const inputs=[['tuple','[1700000000123,"11.25","17.75","3.125","13.5","29.875",0,"unused",0,"unused","unused"]'],['object','{"timestamp":1700000000123,"open":"11.25","high":"17.75","low":"3.125","close":"13.5","volume":"29.875"}'],['aliases','{"t":1700000000123,"o":11.25,"h":17.75,"l":3.125,"c":13.5,"v":29.875}'],['precedence','{"timestamp":1700000000123,"t":1800000000000,"time":1900000000000,"open":11.25,"o":91,"high":17.75,"h":92,"low":3.125,"l":93,"close":13.5,"c":94,"volume":29.875,"v":95}'],['nullish','{"timestamp":null,"t":null,"time":1700000000123,"open":null,"o":11.25,"high":null,"h":17.75,"low":null,"l":3.125,"close":null,"c":13.5,"volume":null,"v":29.875}'],['empty','{}'],['short','[1700000000123]'],['absent',null],['null','null'],['primitive','3'],['zero','{"timestamp":0,"t":1700000000123,"open":"","o":9,"high":false,"h":8}'],['signed_zero','[1700000000123,"-0",-0,"-1e-4000",-1e-4000,"0"]'],['numeric','[1700000000123,9007199254740993.0,1e400,-1e400,0.100000000000000005,1e-4000]'],['coercion','[1700000000123,["7.5"],true,null,"12tail",{}]'],['error_order','{"timestamp":{"toString":null},"open":{"toString":null}}'],['open_error','{"timestamp":1700000000123,"open":{"toString":null}}'],['date','["1969-12-31T23:59:59.999Z","1","2","3","4","5"]']];
for(const t of ['1e308','1e18','1e15','1e12','"not a date"','"2024-01-02T03:04:05"']) inputs.push(['time_'+t,`[${t},"1","2","3","4","5"]`]);
const cases=inputs.map(([id,raw])=>{try {const n=context.exports.normalizeCandle(raw===null?undefined:JSON.parse(raw));return {id,raw,result:n===null?null:Object.fromEntries(Object.entries(n).map(([k,v])=>[k,capture(v)]))};}catch(e){return {id,raw,error:{name:e.name,message:e.message}};}});
const refusals=['{"open":"\\ud800"}','{"open":'+'['.repeat(150)+'"7"'+']'.repeat(150)+'}'].map(raw=>({raw,source:capture(context.exports.normalizeCandle(JSON.parse(raw)))}));
const callable=context.exports.normalizeCandle({open:{toString:()=> '7.25'}});
assert.equal(callable.open,7.25,'candle_callable_source');
// Observe the real Date.parse call before a later throwing numeric coercion.
const events=[];
context.Date={parse(value){events.push('date_parse');return Date.parse(value);}};
const orderRaw='{"timestamp":"2024-01-02T03:04:05","open":{"toString":null}}';
let orderError;
try {context.exports.normalizeCandle(JSON.parse(orderRaw));} catch(e){orderError={name:e.name,message:e.message};}
assert.deepEqual(events,['date_parse'],'candle_source_date_before_open');
assert.equal(orderError.name,'TypeError','candle_source_open_error');
process.stdout.write(JSON.stringify({header,cases,refusals,callable:capture(callable.open),order:{raw:orderRaw,events,error:orderError}}));
