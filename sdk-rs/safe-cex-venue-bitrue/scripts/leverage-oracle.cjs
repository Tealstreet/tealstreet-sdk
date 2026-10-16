const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root}=require('../tests/configuration.cjs');
function extractor(relative,klassName) {
 const file=path.join(root,relative),source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===klassName);
 const compiled=new Map();
 return (name,context)=>{
  if(compiled.has(name))return compiled.get(name).call(context);
  const members=klass.members.filter(n=>n.name?.getText(source)===name);
  assert.equal(members.length,1,`actual initializer census: ${name}`);
  assert(ts.isArrowFunction(members[0].initializer));
  const code=ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{fileName:'actual.ts',compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  const factory=new Function(code);compiled.set(name,factory);
  return factory.call(context);
 };
}
const venue=extractor('packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts','BitrueExchange');
const storeMethod=extractor('packages/safe-cex/src/store/store.base.ts','DefaultStore');
function bits(n){if(Number.isNaN(n))return 'NaN';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function inputBits(n){return Number.isNaN(n)?'7ff8000000000000':bits(n);}
function observe(f){try{const v=f();return v===null?{type:'null'}:typeof v==='number'?{type:'number',bits:bits(v)}:{type:'undefined'};}catch(e){return {type:'error',name:e.name,...(e.name==='Error'?{message:e.message,response:e.response,...(e.code===undefined?{}:{code:e.code})}:{})};}}
const errors=[],brackets=[],updates=[];
const prefix='maximum leverage does not exceed';
for(const input of [undefined,'','other error',prefix+' 20',prefix+' 20.5',prefix+' 0',prefix+' -20',prefix+' +20',prefix+' 1e2',prefix+' .5',prefix+' 1.',prefix+' 0002.30',prefix+' ١٢',prefix+' '+('9'.repeat(400)),prefix+' 0 then '+prefix+' 12',prefix+'bad then '+prefix+' 13','MAXIMUM LEVERAGE DOES NOT EXCEED 17.25','maximum leverage doeſ not exceed 23']) {
 errors.push({input:input??null,result:observe(()=>venue('parseMaxLeverageFromErrorMessage',{})(input))});
}
// Exercise every scalar classified as whitespace by either runtime, plus
// near-miss control characters. Only one corpus subprocess is used.
for(const c of ['\t','\n','\r','\v','\f',' ','\u00a0','\u1680',...Array.from({length:11},(_,i)=>String.fromCharCode(0x2000+i)),'\u2028','\u2029','\u202f','\u205f','\u3000','\ufeff','\u0085','\u001c','\u200b']) {
 const input=prefix+c+'31.75';errors.push({input,result:observe(()=>venue('parseMaxLeverageFromErrorMessage',{})(input))});
}
const a=(symbol,values)=>({contractName:symbol,brackets:values.map(initialLeverage=>({initialLeverage}))});
for(const data of [undefined,null,[],{},false,0,'text',a('BTC',[3,7,5]),[a('BTC',[3,7]),a('ETH',[11,23])],[null,a('ETH',[11,23])],{contractName:'BTC',brackets:[{initialLeverage:3},null]},{brackets:[{initialLeverage:{toString:1}}]},{brackets:[{},[],false,'x']},a('BTC',[null,false,true,'3.5','bad',0,-3,[]]),{brackets:'bad'}]) {
 for(const symbol of [undefined,'','BTC','ETH','missing']) {
  const raw=JSON.stringify(data===undefined?{}:{data});
  brackets.push({raw,symbol:symbol??null,result:observe(()=>venue('parseMaxLeverageFromBracketResponse',{})(JSON.parse(raw),symbol))});
 }
}
for(const text of ['1e400','-1e400','-0','1e-400','9007199254740993']) {
 const raw=`{"data":{"brackets":[{"initialLeverage":${text}},{"initialLeverage":17}]}}`;
 brackets.push({raw,symbol:null,result:observe(()=>venue('parseMaxLeverageFromBracketResponse',{})(JSON.parse(raw)))});
}
function market(id,symbol,max){return {id,symbol,base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.02},limits:{amount:{min:3,max:71},leverage:{min:2,max}},info:{sentinel:'preserve'}};}
function native(m){return {id:m.id,symbol:m.symbol,amountMin:bits(m.limits.amount.min),amountMax:bits(m.limits.amount.max),leverageMin:bits(m.limits.leverage.min),leverageMax:bits(m.limits.leverage.max)};}
for(const symbol of ['BTC','ETH','missing',...Object.getOwnPropertyNames(Object.prototype)]) {
 for(const initial of [7,NaN,Infinity,-Infinity,-0]) for(const maximum of [0,-0,-1,NaN,Infinity,-Infinity,7,7+0.000000001,7+0.00000002,31]) {
  for(const listenerKind of [0,1,2]) {
  const failListener=listenerKind!==0,recovered=listenerKind===2;
  const markets=[market('ID-A','BTC',initial),market('ID-B','ETH',19)];
  const store={state:{markets},marketIndexMap:new Map(markets.map((m,i)=>[m.id,i])),marketsMap:Object.fromEntries(markets.map(m=>[m.id,m])),marketsBySymbolMap:Object.fromEntries(markets.map(m=>[m.symbol,m])),marketsVersion:0,listeners:new Set()};
  Object.defineProperty(store,'marketsBySymbol',{get:()=>store.marketsBySymbolMap});
  let notifications=0,laterNotifications=0;store.listeners.add(()=>{notifications++;if(failListener)throw Object.assign(new Error('observer failure'),{response:{status:409,data:{msg:'listener-specific'}},...(recovered?{code:'E_RECOVERED'}:{})});});
  store.listeners.add(()=>laterNotifications++);
  for(const name of ['updateInArray','notify','updateMarket'])store[name]=storeMethod(name,store);
  const result=observe(()=>venue('setMarketMaxLeverage',{store})(symbol,maximum));
  updates.push({recovered,failListener,laterNotifications,symbol,initial:inputBits(initial),maximum:inputBits(maximum),result,version:store.marketsVersion,notifications,markets:markets.map(native)});
  }
 }
}
const utf8Raw='"maximum leverage does not exceed 5\\ud800"';
const utf8Result=observe(()=>venue('parseMaxLeverageFromErrorMessage',{})(JSON.parse(utf8Raw)));
assert.equal(utf8Result.bits,bits(5),'source surrogate message supplement');
const bracketUtf8Raw='{"data":{"brackets":[{"initialLeverage":"\\ud800"},{"initialLeverage":17}]}}';
assert.equal(observe(()=>venue('parseMaxLeverageFromBracketResponse',{})(JSON.parse(bracketUtf8Raw))).bits,bits(17),'source bracket UTF-16 supplement');
const symbolUtf8Raw='"\\ud800"';
assert.deepEqual(observe(()=>venue('setMarketMaxLeverage',{store:{marketsBySymbol:{}}})(JSON.parse(symbolUtf8Raw),31)),{type:'undefined'},'source market symbol UTF-16 supplement');
const largeCount=200000;
const largeResult=observe(()=>venue('parseMaxLeverageFromBracketResponse',{})({data:{brackets:Array.from({length:largeCount},(_,i)=>({initialLeverage:2+i%2}))}}));
assert.deepEqual(largeResult,{type:'error',name:'RangeError'},'live source argument expansion limit supplement');
process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),errors,brackets,updates,utf8Raw,utf8Result,bracketUtf8Raw,symbolUtf8Raw,largeCount,largeResult}));
