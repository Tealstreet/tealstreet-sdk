const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root,load}=require('../tests/configuration.cjs');
const {PositionSide,PositionMode,MarginType}=load(path.join(root,'packages/safe-cex/src/types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitrueExchange');
const methods={};
for(const name of ['fetchPositionMetadata','getPositionAccounts','getAccountPositionSnapshots']) {
 const members=klass.members.filter(n=>n.name?.getText(source)===name);
 assert.equal(members.length,1,`actual initializer census: ${name}`);
 assert(ts.isArrowFunction(members[0].initializer));
 const code=ts.transpileModule(`return (${members[0].initializer.getText(source)});`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 methods[name]=context=>new Function('PositionSide','PositionMode','MarginType',code).call(context,PositionSide,PositionMode,MarginType);
}
function bits(n){if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function native(m){return {symbol:m.symbol,side:m.side,leverage:bits(m.leverage),positionMode:m.positionMode,marginType:m.marginType};}
const initial=[{symbol:'BTC',side:'long',leverage:7,positionMode:'hedged',marginType:'isolated'},{symbol:'ETH',side:'short',leverage:13,positionMode:'oneway',marginType:'cross'}];
const snapshot=(symbol='SOL',side='short',leverageLevel=19,positionType=2)=>({symbol,side,leverageLevel,positionType});
const account=positions=>({data:{account:[{marginCoin:'USDT',positionVos:positions.map(p=>({contractName:p.symbol,positions:[{side:p.side==='long'?'BUY':'SELL',leverageLevel:p.leverageLevel,positionType:p.positionType,volume:3,openPrice:101}]}))}]}});
const cases=[];
function add(label,opts={}){cases.push({label,initial,symbols:['ETH','SOL','BTC','SOL'],currentRaw:null,accountRaw:null,disposed:false,disposeAfterAwait:false,fetchError:false,...opts});}
for(const symbols of [[],['BTC'],['ETH','BTC','ETH'],['SOL'],['ETH','SOL','BTC','SOL']]) {
 for(const disposed of [false,true]) {
  add('missing snapshot',{symbols,disposed});
  add('valid fetched snapshot',{symbols,disposed,accountRaw:JSON.stringify(account([snapshot()]))});
  add('explicit empty snapshot',{symbols,disposed,currentRaw:'[]'});
  add('explicit supplied snapshot',{symbols,disposed,currentRaw:JSON.stringify([snapshot('ETH','long',17,1),snapshot()])});
 }
}
for(const raw of ['null','false','true','0','-0','-3','1.25','1e400','-1e400','1e-4000','9007199254740993','"2"','"0x10"','"bad"','[]','[23]','[null]','[1,2]','{}','{"toString":1}']) {
 for(const symbol of ['SOL',''])add('leverage coercion before symbol guard',{currentRaw:`[{"symbol":${JSON.stringify(symbol)},"side":"short","leverageLevel":${raw},"positionType":2}]`});
}
for(const raw of ['2','2.0','"2"','[2]','true','null','1','3','1e400'])add('strict position type',{currentRaw:`[{"symbol":"SOL","side":"long","leverageLevel":29,"positionType":${raw}}]`});
add('last duplicate wins keeps insertion slot',{symbols:['SOL','BTC','SOL','ETH'],currentRaw:JSON.stringify([snapshot('SOL','long',17,1),snapshot('BTC','short',23,2),snapshot('SOL','short',31,2)])});
add('failed rebuild preserves cache',{currentRaw:'[{"symbol":"SOL","side":"long","leverageLevel":17,"positionType":2},{"symbol":"ETH","side":"short","leverageLevel":{"toString":1},"positionType":1}]'});
for(const symbols of [['BTC'],['SOL']])for(const fetchError of [false,true])add('await boundary',{symbols,fetchError,disposeAfterAwait:true,accountRaw:JSON.stringify(account([snapshot()])),midCache:[{...initial[0],symbol:'SOL',leverage:41}]});
for(const raw of ['{}','{"data":{"account":[]}}',JSON.stringify(account([])),JSON.stringify(account([snapshot('SOL','long','37','2')])),JSON.stringify(account([snapshot('SOL','long','bad',2)]))])add('fetched schema',{accountRaw:raw});
(async()=>{
 const rows=[];
 for(const input of cases){
  let calls=0;const events=[];
  const context={positionMetadataBySymbol:new Map(input.initial.map(m=>[m.symbol,{...m}])),isDisposed:input.disposed};
  for(const [name,bind] of Object.entries(methods))context[name]=bind(context);
  context.fetchBalanceAndPositionsRaw=async()=>{calls++;events.push('fetch');await Promise.resolve();events.push('resume');if(input.disposeAfterAwait)context.isDisposed=true;if(input.midCache)context.positionMetadataBySymbol=new Map(input.midCache.map(m=>[m.symbol,{...m}]));if(input.fetchError)throw new Error('fetch rejected');return input.accountRaw===null?undefined:JSON.parse(input.accountRaw);};
  let result;
  try{result={ok:(await context.fetchPositionMetadata(input.symbols,input.currentRaw===null?undefined:JSON.parse(input.currentRaw))).map(native)};}catch(e){result={error:{name:e.name,message:e.message}};}
  rows.push({input,result,calls,events,disposed:context.isDisposed,cache:[...context.positionMetadataBySymbol.values()].map(native)});
 }
 assert.equal(rows.length,100,'BITRUE-METADATA source corpus total');
 const utf8Raw='[{"symbol":"\\ud800","side":"long","leverageLevel":17,"positionType":2}]';
 const depthRaw='[{"symbol":"SOL","side":"short","leverageLevel":'+'['.repeat(160)+'19'+']'.repeat(160)+',"positionType":1}]';
 for(const [raw,symbol,leverage] of [[utf8Raw,'\ud800',17],[depthRaw,'SOL',19]]) {
  const context={positionMetadataBySymbol:new Map(),isDisposed:false};
  for(const [name,bind] of Object.entries(methods))context[name]=bind(context);
  const output=await context.fetchPositionMetadata([symbol],JSON.parse(raw));
  assert.equal(output.length,1,'CHOSEN metadata supplement source count');
  assert.equal(output[0].symbol,symbol,'CHOSEN metadata supplement source symbol');
  assert.equal(output[0].leverage,leverage,'CHOSEN metadata supplement source leverage');
 }
 const context={positionMetadataBySymbol:new Map(initial.map(m=>[m.symbol,{...m}])),isDisposed:false};
 for(const [name,bind] of Object.entries(methods))context[name]=bind(context);
 const duplicated=await context.fetchPositionMetadata(['BTC','BTC']);
 const duplicateIdentity=duplicated[0]===duplicated[1], cacheIdentity=duplicated[0]===context.positionMetadataBySymbol.get('BTC');
 assert(duplicateIdentity && cacheIdentity,'source metadata aliases');
 duplicated[0].symbol='MUTATED';
 let keyFetches=0;context.fetchBalanceAndPositionsRaw=async()=>{keyFetches++;return undefined;};
 const byKey=await context.fetchPositionMetadata(['BTC','BTC']);
 const keyIdentity=byKey.length===2 && byKey[0]===duplicated[0] && byKey[1]===duplicated[0];
 assert.equal(keyFetches,0,'source Map key independent of record symbol');
 assert(keyIdentity,'source old key retains aliased record');
 const keySymbol=byKey[0].symbol;
 duplicated[0].leverage=31;
 const mutated=[duplicated[1].leverage,context.positionMetadataBySymbol.get('BTC').leverage];
 assert.deepEqual(mutated,[31,31],'source metadata mutation aliases');
 const replaced=await context.fetchPositionMetadata(['BTC'],[snapshot('BTC','short',41,1)]);
 const detached=duplicated[0]!==replaced[0];
 const retained=[duplicated[0].leverage,duplicated[1].leverage,replaced[0].leverage];
 assert(detached,'source cache replacement detaches old handles');
 assert.deepEqual(retained,[31,31,41],'source replacement keeps old object alive');
 const identity={duplicateIdentity,cacheIdentity,mutated,detached,retained,keyFetches,keyIdentity,keySymbol};
 process.stdout.write(JSON.stringify({header:require('./capture-header.cjs')(),rows,utf8Raw,depthRaw,identity}));
})().catch(e=>{console.error(e);process.exitCode=1;});
