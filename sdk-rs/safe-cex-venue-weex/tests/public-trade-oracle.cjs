const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,header,capture,load}=require('./types-oracle.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['toNumber','toIntegerString','normalizeTimestampToMs','normalizePublicTrade'];
const nodes=ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(nodes.length,4,'public_trade_source_inventory');
const context={exports:{},...load(path.join(root,'packages/safe-cex/src/types.ts'))};
vm.runInNewContext(ts.transpileModule(nodes.map(n=>n.getText(ast)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
process.env.TZ='UTC';
const inputs=[
 ['distinct','{"id":"native-id","price":"11.25","qty":"-3.5","quoteQty":"79.125","time":1700000000123}'],
 ['aliases','{"t":123,"p":"12.5","q":"4.25","v":8,"T":1700000000123}'],
 ['precedence','{"id":"first","t":"second","price":11,"p":22,"qty":3,"q":4,"time":1700000000123,"T":1800000000456,"quoteQty":7,"v":8}'],
 ['fallback_id','{"price":1e-7,"qty":2e21,"createdTime":"2024-01-02T03:04:05"}'],
 ['nullish','{"id":null,"price":null,"p":9,"qty":null,"q":7,"time":null,"T":1700000000123,"quoteQty":null,"v":13}'],
 ['empty_id','{"id":"","price":9,"qty":7,"time":1700000000123}'],
 ['zero_price','{"price":0,"p":99,"qty":4,"time":1700000000123}'],
 ['zero_qty','{"price":7,"qty":"-0","time":1700000000123}'],
 ['zero_time','{"price":7,"qty":8,"time":0}'],
 ['empty','{}'],['null','null'],
 ['price_error','{"price":{"toString":null},"qty":{"toString":null}}'],
 ['time_error_before_guard','{"price":0,"qty":0,"time":{"toString":null}}'],
 ['quote_error','{"price":7,"qty":8,"time":1700000000123,"quoteQty":{"toString":null}}'],
 ['numeric','{"id":9007199254740993.0,"price":1.7976931348623157e308,"qty":-1.7976931348623157e308,"quoteQty":"-0","time":1e18}'],
];
let index=0;
for(const buyer of [true,false,null,'true']) for(const m of [true,false,null,'true']) for(const isBuyerMaker of [true,false,null,'true']) inputs.push(['flags_'+index++,JSON.stringify({price:7,qty:8,time:1700000000123,buyer,m,isBuyerMaker})]);
const cases=inputs.map(([id,raw])=>{try{const n=context.exports.normalizePublicTrade(JSON.parse(raw),'ETH-USDT');return {id,raw,result:n===null?null:{id:n.id,symbol:n.symbol,side:n.side,fields:Object.fromEntries(['price','qty','quoteQty','timestamp'].map(k=>[k,capture(n[k])]))}};}catch(e){return{id,raw,error:{name:e.name,message:e.message}};}});
const callable=context.exports.normalizePublicTrade({price:{toString:()=> '7.25'},qty:2,time:1700000000123},'X');
assert.equal(callable.price,7.25,'public_trade_callable');
const refusals=[
 '{"price":7,"qty":8,"time":1700000000123,"id":"\\ud800"}',
 '{"price":7,"qty":8,"time":1700000000123,"extra":'+'['.repeat(150)+'0'+']'.repeat(150)+'}'
].map(raw=>({raw,accepted:context.exports.normalizePublicTrade(JSON.parse(raw),'X')!==null}));
process.stdout.write(JSON.stringify({header,cases,refusals,callable:capture(callable.price)}));
