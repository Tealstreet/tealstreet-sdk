const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,header,capture}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['toNumber','normalizeBalance'];
const nodes=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(nodes.length,2,'balance_source_inventory');
const context={exports:{}};
vm.runInNewContext(ts.transpileModule(nodes.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const inputs=[
 ['distinct','[{"asset":"USDT","balance":"101.25","availableBalance":"37.5","unrealizePnl":"-4.75","frozen":"9.125"}]'],
 ['second','[{"asset":"BTC","balance":999},{"coin":"usdt","balance":81,"availableBalance":13,"unrealizePnl":7},{"asset":"USDT","balance":777}]'],
 ['fallback','[{"asset":"BTC","balance":33,"availableBalance":44,"unrealizePnl":8}]'],
 ['aliases','[{"asset":"","coin":"uSdT","balance":null,"availableBalance":null,"amount":17,"frozen":-3}]'],
 ['zero','[{"asset":"USDT","balance":0,"availableBalance":0,"amount":19,"frozen":"-0"}]'],
 ['signed','[{"balance":"-0","availableBalance":"-0","unrealizePnl":"-0","frozen":0}]'],
 ['empty','[]'],['null_row','[null]'],['null','null'],['object','{}'],
 ['primitive_rows','[0,false,"abc",{},[]]'],
 ['asset_error','[{"asset":{"toString":null}}]'],
 ['number_error','[{"balance":{"toString":null},"availableBalance":{"toString":null}}]'],
 ['selection_short_circuit','[{"asset":"USDT","balance":9},null]'],
 ['selection_error_before_fallback','[{"asset":"BTC","balance":9},null]'],
 ['numeric','[{"balance":9007199254740993.0,"availableBalance":1e400,"unrealizePnl":-1e400,"frozen":-1e-4000}]'],
 ['coercion','[{"asset":["usdt"],"balance":["15.5"],"availableBalance":true,"unrealizePnl":"3tail","frozen":{}}]'],
 ['max_overflow','[{"balance":1.7976931348623157e308,"availableBalance":-1.7976931348623157e308}]'],
];
const cases=inputs.map(([id,raw])=>{try{const value=context.exports.normalizeBalance(JSON.parse(raw));return {id,raw,fields:Object.fromEntries(['total','free','used','upnl'].map(k=>[k,capture(value[k])])),info:capture(value.info)};}catch(e){return{id,raw,error:{name:e.name,message:e.message}};}});
const surrogate='[{"asset":"USDT","tag":"\\ud800"}]';
const deep='[{"asset":"USDT","extra":'+'['.repeat(150)+'0'+']'.repeat(150)+'}]';
const refusals=[surrogate,deep].map(raw=>({raw,source:context.exports.normalizeBalance(JSON.parse(raw)).total}));
const callable=context.exports.normalizeBalance([{asset:{toString:()=> 'usdt'},balance:{toString:()=> '7.25'}}]);
assert.equal(callable.total,7.25,'balance_callable');
process.stdout.write(JSON.stringify({header,cases,refusals,callable:capture(callable.total)}));
