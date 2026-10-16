const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root,load}=require('../tests/configuration.cjs');
const {ENDPOINTS}=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'));
function extractor(relative,klassName){
 const file=path.join(root,relative),source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===klassName),compiled=new Map();
 return (name,context)=>{if(!compiled.has(name)){const members=klass.members.filter(n=>n.name?.getText(source)===name);assert.equal(members.length,1,'source initializer '+name);assert(ts.isArrowFunction(members[0].initializer));compiled.set(name,new Function('ENDPOINTS',ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText));}return compiled.get(name).call(context,ENDPOINTS);};
}
const venue=extractor('packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts','BitrueExchange'),storeMethod=extractor('packages/safe-cex/src/store/store.base.ts','DefaultStore');
const methods=['fetchAndSyncSymbolMaxLeverage','fetchSymbolMaxLeverage','updateSymbolMetadata'];
function bits(n){if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function market(id,symbol,max){return {id,symbol,base:'BTC',quote:'USDT',active:true,precision:{amount:.01,price:.02},limits:{amount:{min:3,max:71},leverage:{min:2,max}},info:{sentinel:'preserve'}};}
function native(m){return {id:m.id,symbol:m.symbol,amountMin:bits(m.limits.amount.min),amountMax:bits(m.limits.amount.max),leverageMin:bits(m.limits.leverage.min),leverageMax:bits(m.limits.leverage.max)};}
const valid={code:0,data:[{contractName:'BTC',brackets:[{initialLeverage:11},{initialLeverage:31}]},{contractName:'ETH',brackets:[{initialLeverage:17},{initialLeverage:47}]}]};
const responses=[null,'{}',JSON.stringify(valid),JSON.stringify({...valid,code:'200'}),JSON.stringify({...valid,code:'00'}),'{"code":0,"data":{"brackets":[{"initialLeverage":31},null]}}','{"code":0,"data":{"brackets":[{"initialLeverage":{"toString":1}}]}}','{"code":200,"data":{"brackets":[]}}','{"code":0,"data":{"brackets":[{"initialLeverage":1e400},{"initialLeverage":23}]}}'];
(async()=>{const rows=[];
 for(const method of methods)for(const symbol of ['BTC','ETH','missing','constructor',''])for(const publicOnly of [false,true])for(const raw of responses)for(const fault of ['none','request','subscriber'])for(const initial of [7,0,-3,NaN,Infinity,-Infinity]){
  const markets=[market('ID-A','BTC',initial),market('ID-B','ETH',19)];
  const store={state:{markets},marketIndexMap:new Map(markets.map((m,i)=>[m.id,i])),marketsMap:Object.fromEntries(markets.map(m=>[m.id,m])),marketsBySymbolMap:Object.fromEntries(markets.map(m=>[m.symbol,m])),marketsVersion:0,listeners:new Set()};
  Object.defineProperty(store,'marketsBySymbol',{get:()=>store.marketsBySymbolMap});
  let notifications=0,later=0;store.listeners.add(()=>{notifications++;if(fault==='subscriber')throw new Error('subscriber rejected');});store.listeners.add(()=>later++);
  for(const name of ['updateInArray','notify','updateMarket'])store[name]=storeMethod(name,store);
  const calls=[],events=[],context={store,isPublicDataOnly:publicOnly,xhr:{get:async(url,config)=>{calls.push({url,config});events.push('request');await Promise.resolve();events.push('response');if(fault==='request')throw new Error('request rejected');return {data:raw===null?undefined:JSON.parse(raw)};}}};
  for(const name of [...methods,'isSuccessCode','parseMaxLeverageFromBracketResponse','setMarketMaxLeverage'])context[name]=venue(name,context);
  const value=await context[method](symbol);
  rows.push({method,symbol,publicOnly,raw,fault,initial:bits(initial),result:value===null?{kind:'null'}:value===undefined?{kind:'undefined'}:{kind:'number',bits:bits(value)},calls,events,notifications,later,version:store.marketsVersion,markets:markets.map(native)});
 }
 assert.equal(rows.length,4860,'BITRUE-LEVERAGE-READ source total');
 const supplements=[];const surrogateRaw='"\\ud800"';const largeCount=200000;
 for(const method of methods)for(const kind of ['utf8','large']){
  const markets=[market('ID-A','BTC',7)];
  const store={state:{markets},marketIndexMap:new Map([['ID-A',0]]),marketsMap:{'ID-A':markets[0]},marketsBySymbolMap:{BTC:markets[0]},marketsVersion:0,listeners:new Set()};
  Object.defineProperty(store,'marketsBySymbol',{get:()=>store.marketsBySymbolMap});
  for(const name of ['updateInArray','notify','updateMarket'])store[name]=storeMethod(name,store);
  const data=kind==='large'?{code:0,data:{brackets:Array.from({length:largeCount},(_,i)=>({initialLeverage:2+i%2}))}}:valid;
  const context={store,isPublicDataOnly:false,xhr:{get:async()=>({data})}};
  for(const name of [...methods,'isSuccessCode','parseMaxLeverageFromBracketResponse','setMarketMaxLeverage'])context[name]=venue(name,context);
  const value=await context[method](kind==='utf8'?JSON.parse(surrogateRaw):'BTC');
  const result=value===null?{kind:'null'}:value===undefined?{kind:'undefined'}:{kind:'number',bits:bits(value)};
  supplements.push({method,kind,result,maximum:bits(store.marketsBySymbolMap.BTC.limits.leverage.max)});
 }
 assert.equal(supplements.length,6,'CHOSEN leverage read supplement count');
 process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),rows,supplements,surrogateRaw,largeCount}));
})().catch(e=>{console.error(e);process.exitCode=1;});
