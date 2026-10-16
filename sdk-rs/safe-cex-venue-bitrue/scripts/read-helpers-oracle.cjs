const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const {ts,root,load}=require('../tests/configuration.cjs');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitrueExchange');
const {reverseWsSymbol}=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.utils.ts'));
const names=['isSuccessCode','normalizeTimestampMs','normalizeTradeSymbol','pickTradeOrderId','resolveTradeQty'];
const methods={};
for(const name of names) {
 const members=klass.members.filter(n=>n.name?.getText(source)===name);
 assert.equal(members.length,1,`actual initializer census: ${name}`);
 assert(ts.isArrowFunction(members[0].initializer));
 // Transpile the actual initializer, unchanged. `this` supplies only the
 // explicitly recorded store dependency; no class/constructor claim is made.
 const code=ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 methods[name]=context=>new Function('reverseWsSymbol',code).call(context,reverseWsSymbol);
}
function bits(n) {if(Number.isNaN(n))return 'NaN';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function fromBits(s){return Buffer.from(s,'hex').readDoubleBE();}
function observe(f){try {const v=f();return typeof v==='number'?{type:'number',bits:bits(v)}:v===undefined?{type:'undefined'}:{type:typeof v,value:v};}catch(e){return {type:'error',name:e.name};}}
const rows=[];
function add(method,args,context={}) {
 const values=args.map(arg=>arg===null?undefined:arg.bits!==undefined?fromBits(arg.bits):JSON.parse(arg.raw));
 rows.push({method,args,result:observe(()=>methods[method](context)(...values))});
}
const raw=s=>({raw:s}),num=n=>({bits:Number.isNaN(n)?'7ff8000000000000':bits(n)});
for(const value of [null,...['null','false','true','0','-0','0.0','200','200.00000000000001','201','"0"','"200"','"00"','"200.0"','" 200"','""','[]','[200]','{}','1e400','-1e400'].map(raw)])add('isSuccessCode',[value]);
const timestamps=[0,-0,1,-1,.001,-.001,1e12-0.0001220703125,1e12,1e12+0.0001220703125,1e12+1.9,Infinity,-Infinity,NaN,Number.MIN_VALUE,-Number.MIN_VALUE,Number.MAX_VALUE,-Number.MAX_VALUE];
add('normalizeTimestampMs',[null]);
for(const n of timestamps)add('normalizeTimestampMs',[num(n)]);
let random=0x125fedca98765432n;
for(let i=0;i<256;i++){random=BigInt.asUintN(64,random*6364136223846793005n+1442695040888963407n);add('normalizeTimestampMs',[{bits:random.toString(16).padStart(16,'0')}]);}
for(const value of [null,...['null','false','true','0','-0','1e400','-1e400','1e-400','9007199254740993','""','"ethusdt"','"e_btcusdt"','"e-btc-usdt"','"ß_usdt"','[]','[null,"btc_usdt"]','{}','{"toString":1}'].map(raw)])add('normalizeTradeSymbol',[value]);
for(const side of ['BUY','SELL','buy','unknown','']) {
 for(const ids of [{bidId:111,askId:222,tradeId:333},{bidId:null,askId:222,tradeId:333},{bidId:111,askId:null,tradeId:333},{tradeId:333},{},{bidId:0,askId:false,tradeId:333},{bidId:'',askId:'ask',tradeId:'trade'},{bidId:['bid'],askId:['ask'],tradeId:['trade']},{bidId:{toString:1},askId:{toString:2}}])add('pickTradeOrderId',[raw(JSON.stringify({side,...ids}))]);
}
add('pickTradeOrderId',[raw('{"side":{"toString":1},"bidId":11,"askId":22}')]);
function qty(entry,multiplier,price,symbol='E-BTC-USDT') {
 const market=multiplier===null?{}:{info:{multiplier:JSON.parse(multiplier.raw)}};
 const args=[raw(entry),raw(JSON.stringify(symbol)),num(price)];
 const before=rows.length;
 add('resolveTradeQty',args,{store:{marketsBySymbol:{'E-BTC-USDT':market,'E-ETH-USDT':{info:{multiplier:99}}}}});
 rows[before].multiplier=multiplier;
}
// Inputs keep qty, amount, multiplier and price distinct; each consistency
// branch has a supplement whose native result differs from the raw quantity.
for(const [entry,mul,price] of [
 ['{"qty":2,"amount":30}','3',5],['{"qty":2,"amount":10}','3',5],
 ['{"qty":2,"amount":35}','3',5],['{"qty":0,"amount":30}','3',5],
 ['{"qty":-2,"amount":30}','3',5],['{"qty":2,"amount":0}','3',5],
 ['{"qty":2,"amount":30}','3',0],['{}','3',5],
 ['{"qty":null,"amount":"30"}','"3"',5],
 ['{"qty":"bad","amount":30}','3',5],['{"qty":2,"amount":"bad"}','3',5],
 ['{"qty":2,"amount":30}','0',5],['{"qty":2,"amount":30}','null',5],
 ['{"qty":2,"amount":30}','{"toString":1}',5],
 ['{"qty":{"toString":1},"amount":30}','3',5],
 ['{"qty":2,"amount":{"toString":1}}','3',5],
 ['{"qty":2,"amount":30}','3',NaN],['{"qty":2,"amount":30}','3',Infinity],
 ['{"qty":1e400,"amount":1e400}','3',5],
 ['{"qty":-0,"amount":-0}','3',5],
 ['{"qty":2,"amount":30.0000001}','3',5],
 ['{"qty":2,"amount":30.000001}','3',5],
 ['{"qty":2,"amount":10.00000001}','3',5],
 ['{"qty":2,"amount":10.00001}','3',5],
 ])qty(entry,raw(mul),price);
qty('{"qty":2,"amount":30}',null,5);
for(const text of ['-0','1e400','-1e400','1e-400','9007199254740993','true','[]','["3"]','{}']) {
 qty(`{"qty":${text},"amount":30}`,raw('3'),5);
 qty(`{"qty":2,"amount":${text}}`,raw('3'),5);
 qty('{"qty":2,"amount":30}',raw(text),5);
}
qty('{"qty":2,"amount":990}',raw('3'),5,'E-ETH-USDT');
qty('{"qty":2,"amount":30}',raw('3'),5,'E-MISSING-USDT');
assert.equal(rows.at(-2).result.bits,bits(198),'second symbol selects its own multiplier');
assert.equal(rows.at(-1).result.bits,bits(2),'missing symbol does not select first market');
// State the branch evidence explicitly so future corpus edits cannot silently
// collapse the scaled-quantity and amount-fallback controls.
assert.equal(rows.find(r=>r.method==='resolveTradeQty').result.bits,bits(6),'scaled quantity differs from raw quantity');
const counts=Object.fromEntries(names.map(n=>[n,rows.filter(r=>r.method===n).length]));
const utf8=[];
for(const [method,text] of [
 ['isSuccessCode','"\\ud800"'],['normalizeTradeSymbol','"\\ud800"'],
 ['pickTradeOrderId','{"side":"BUY","bidId":"\\ud800","askId":"distinct"}'],
 ['resolveTradeQty','{"qty":"\\ud800","amount":30}']
]) {
 const context={store:{marketsBySymbol:{}}};
 const observed=observe(()=>methods[method](context)(JSON.parse(text),'E-BTC-USDT',5));
 if(observed.type==='string') {observed.units=Array.from({length:observed.value.length},(_,i)=>observed.value.charCodeAt(i));delete observed.value;}
 utf8.push({method,raw:text,result:observed});
}
assert.deepEqual(utf8.map(r=>r.result),[{type:'boolean',value:false},{type:'string',units:[55296]},{type:'string',units:[55296]},{type:'number',bits:'NaN'}],'source UTF-16 supplement');
process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),counts,rows,utf8}));
