const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/lighter/lighter.utils.ts'), 'utf8');
const utils = {};
vm.runInNewContext(ts.transpileModule(source + '\nexports.normalizeNonNegativeNumber = normalizeNonNegativeNumber; exports.readNumber=readNumber; exports.decimalsFromScale=decimalsFromScale;', {compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText, {exports:utils, require(name){assert.fail(`unexpected import ${name}`);}});
const outcome = f => {try {return {ok:f()??null};} catch(error) {return {error:{name:error.name,message:error.message}};}};
const inputs = [undefined,null,false,true,0,-0,-0.5,1.99,-1.99,1e20,1e21,1e-7,9007199254740992,'',' ','\uFEFF0012\uFEFF','\u00850012','0x10','0b10','12abc','abc-12tail','--12','- 12','+12','１２','𝟘','😀-12','é-12','-0','000','000001','0\n','12\n34','Infinity','NaN','1e999','9'.repeat(309),'9'.repeat(308),[],[null],[12],['0012'],[null,12],{}, {toString:null},{toString:0},{toString:false},{toString:'12'},{toString:[]},[{toString:null}],[[{toString:0}]],{valueOf:12},'0x'+'a'.repeat(40),'0X'+'a'.repeat(40),'\uFEFF0x'+'F'.repeat(40)+'\uFEFF','0x'+'g'.repeat(40)];
let seed=0x1ace;
for(let i=0;i<400;i++) {seed=(Math.imul(seed,1664525)+1013904223)>>>0; inputs.push(`${['','-', '+','x','\uFEFF','\u0085','😀'][seed%7]}${seed}${['','x','.5','e-2','\n','-3'][seed%6]}`);}
const scalars=inputs.map(value=>({...(value===undefined?{}:{value}),index:outcome(()=>utils.parseLighterApiKeyIndex(value)),string:outcome(()=>utils.toLighterApiKeyIndexString(value)),fallback:outcome(()=>utils.toLighterApiKeyIndexString(value,'')),positive:outcome(()=>utils.parseLighterPositiveIntegerString(value)),address:outcome(()=>utils.isLighterL1Address(value)),nonnegative:outcome(()=>utils.normalizeNonNegativeNumber(value))}));
const market=(id='0',info)=>({id,symbol:'BTC/USDC',base:'BTC',quote:'USDC',active:true,precision:{amount:0.01,price:0.1},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}},...(info===undefined?{}:{info})});
const minimums=[];
for(const value of inputs.slice(0,58)) for(const info of [{min_base_amount:value,min_quote_amount:value},{minBaseAmount:value,minQuoteAmount:value},{min_base_amount:value,minBaseAmount:'7.5x',min_quote_amount:value,minQuoteAmount:'8.5x'},{min_base_amount:null,minBaseAmount:value,min_quote_amount:null,minQuoteAmount:value}]) {const m=market('0',info);minimums.push({market:m,expected:outcome(()=>utils.getLighterOrderMinimums(m))});}
for(const info of [undefined,null,false,[],{},'text',{min_base_amount:{toString:null},min_quote_amount:{toString:0}}]) {const m=market('0',info);minimums.push({market:m,expected:outcome(()=>utils.getLighterOrderMinimums(m))});}
for(const info of [{min_base_amount:'1.25',min_quote_amount:'7.5'},{minBaseAmount:'9.75',minQuoteAmount:'2.5'}]) {const m=market('0',info);minimums.push({market:m,expected:outcome(()=>utils.getLighterOrderMinimums(m))});}
const order=(symbol,id)=>({symbol,id,status:'open',type:'limit',side:'buy',price:1,amount:1,filled:0,remaining:1,reduceOnly:false,lastUpdate:0});
const cancels=[];
for(const id of ['0','-0','\uFEFF12x','\u008512','0x10','0b10','-12','1e21','9'.repeat(309),'9'.repeat(308),'9007199254740993','NaN','']) {
 const markets=Object.fromEntries([['z',market(id)],['a',market('7')]]);
 const orders=['missing','z','a','z','constructor','__proto__','toString'].map((s,i)=>order(s,`${i}-9007199254740993`));
 cancels.push({markets,orders,expected:utils.resolveLighterCancelTargets(orders,markets)});
}
const reads=[];
for(const value of inputs.slice(0,58))for(const allowZero of [undefined,false,true])for(const keys of [['a','b'],['b','a'],['missing','a','b'],['constructor','__proto__','toString','b'],[]]){
 const source={a:value,b:3};reads.push({source,keys,allowZero,expected:outcome(()=>utils.readNumber(source,keys,{allowZero}))});
}
for(const source of [undefined,null])reads.push({source,keys:['a'],expected:outcome(()=>utils.readNumber(source,['a']))});
const scales=[undefined,NaN,Infinity,-Infinity,-1,-0,0,1,100.00000000000006,9.999999999999672e-254];
const buffer=new ArrayBuffer(8),view=new DataView(buffer);
for(let exponent=-323;exponent<=308;exponent++){
 const value=10**exponent;if(!Number.isFinite(value)||value===0)continue;
 view.setFloat64(0,value);const bits=view.getBigUint64(0);
 for(let offset=-256n;offset<=256n;offset++){if(bits+offset<=0n)continue;view.setBigUint64(0,bits+offset);scales.push(view.getFloat64(0));}
}
const scaleCases=scales.map(scale=>({scale,expected:utils.decimalsFromScale(scale)??null}));
const decimalMarkets=[];
const amountDecimalKeys=['amountDecimals','quantityDecimals','baseDecimals','sizeDecimals','sizePrecision'];
const priceDecimalKeys=['priceDecimals','quoteDecimals','tickDecimals','pricePrecision'];
const amountScaleKeys=['amountScale','quantityScale','baseScale','sizeScale'];
const priceScaleKeys=['priceScale','quoteScale','tickScale'];
const addDecimals=info=>{const m=market('0',info);decimalMarkets.push({market:m,amount:outcome(()=>utils.getLighterAmountDecimals(m)),price:outcome(()=>utils.getLighterPriceDecimals(m))});};
for(const keys of [amountDecimalKeys,priceDecimalKeys,amountScaleKeys,priceScaleKeys])for(const key of keys)for(const value of inputs.slice(0,58))for(const remaining of [undefined,2, {toString:null}])addDecimals(Object.fromEntries(keys.map(k=>[k,k===key?value:remaining])));
for(const scale of [100.00000000000006,9.999999999999672e-254,1e30,1e-30,1e31,1e308,Number.MIN_VALUE])for(const key of [...amountScaleKeys,...priceScaleKeys])addDecimals({[key]:scale});
for(const info of [undefined,null,false,[],{},'text'])addDecimals(info);
for(const amount of [0,-0,0.0001,1e-7,1e20,1e21])for(const price of [0,-0,0.001,1e-7,1e20,1e21]){const m=market();m.precision={amount,price};decimalMarkets.push({market:m,amount:outcome(()=>utils.getLighterAmountDecimals(m)),price:outcome(()=>utils.getLighterPriceDecimals(m))});}
if(process.env.LIGHTER_UTILS_CARDINALITY_MUTATION==='empty-scalars')scalars.length=0;
if(process.env.LIGHTER_UTILS_CARDINALITY_MUTATION==='short-scalars')scalars.splice(1);
if(process.env.LIGHTER_UTILS_CARDINALITY_MUTATION==='empty-expected')for(const row of cancels)row.expected=[];
if(process.env.LIGHTER_UTILS_CARDINALITY_MUTATION==='short-expected')cancels[0].expected.pop();
process.stdout.write(JSON.stringify({scalars,minimums,cancels,reads,scaleCases,decimalMarkets},(_key,value)=>typeof value==='number'&&(!Number.isFinite(value)||Object.is(value,-0))?{$number:Object.is(value,-0)?'-0':String(value)}:value));
