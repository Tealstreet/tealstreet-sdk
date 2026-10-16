import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import vm from 'node:vm';
import ts from 'typescript';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const source = ts.createSourceFile('bybit.exchange.ts', readFileSync(root+'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts','utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitExchange');
const names=['getPositionUpdatedTime','getPositionReliabilityScore','dedupePositionsById','parseTickers'];
const members=klass.members.filter(m=>names.includes(m.name?.getText(source)));
assert.equal(members.length,4);
const context=vm.createContext({});
vm.runInContext(ts.transpileModule(`globalThis.venue = new class { ${members.map(m=>m.getText(source)).join('\n')} };`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const make=(id,info,contracts=0)=>({id,symbol:'BTCUSDT',side:'long',entryPrice:10,notional:20,leverage:5,unrealizedPnl:3,contracts,liquidationPrice:7,...(info===undefined?{}:{info})});
const values=[undefined,null,'', ' ', '\uFEFF', '\u0085',0,-1,1,'0','-1','100','1e3','0xff','0b11','0o10','Infinity','NaN','2tail',false,true,[],[null],[''],['12'],[[12]],{},{toString:null},{toString:1},[{toString:null}],{valueOf:0},'\uFEFF10\uFEFF','\u008510'];
const cases=[[],[make('a')]];
for (const key of ['updatedTime','seq','positionStatus','avgPrice']) for (const value of values) {
 const info=value===undefined?{}:{[key]:value};
 const p=make('a',info), real=make('a',{updatedTime:'42',seq:'2',positionStatus:'Normal',avgPrice:'1'});
 cases.push([p,real],[real,p],[p,make('b',{}),real],[p,make('a',info,1)]);
}
// Exact tie must keep first; later timestamp wins only after equal reliability.
cases.push([make('a',{updatedTime:'1',seq:0}),make('a',{updatedTime:'2',seq:0})]);
cases.push([make('a',{updatedTime:'2',seq:0}),make('a',{updatedTime:'1',seq:0})]);
let seed=17; const random=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/2**32);
for(let i=0;i<500;i++) cases.push(Array.from({length:8},()=>make(String(Math.floor(random()*3)),Object.fromEntries(['updatedTime','seq','positionStatus','avgPrice'].flatMap(k=>{const v=values[Math.floor(random()*values.length)];return v===undefined?[]:[[k,v]];})),random()<0.5?0:1)));
for (const key of ['updatedTime','seq','avgPrice']) {
 const bad=make('a',{[key]:{toString:null}});
 cases.push([bad],[bad,make('b',{})],[make('b',{}),bad]);
}
const capture=f=>{try{return f();}catch{return {error:true};}};
const positions=cases.map(positions=>({scores:positions.map(p=>[capture(()=>context.venue.getPositionUpdatedTime(p)),capture(()=>context.venue.getPositionReliabilityScore(p))]),positions:capture(()=>context.venue.dedupePositionsById(positions))}));
const market=(symbol,expiring)=>({id:'id:'+symbol,symbol,base:'BTC',quote:'USDT',active:true,precision:{price:0.01,amount:0.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}},isExpiring:expiring});
const cached=[{id:'cached-id',symbol:'cached-symbol',bid:1,ask:2,last:3,mark:4,index:5,percentage:6,openInterest:7,fundingRate:8,fundingTime:9,volume:10,quoteVolume:11}];
const tickerValues=[...values,'-0','-0.0','1e','1e+','1.2x','.2e-2junk','+Infinitytail','-Infinity','1e999','0x12','１２','1_000',null,[],[12]];
const tickerCases=[{raw:null,markets:[],cached},{raw:[],markets:[],cached}];
for (const value of tickerValues) for(const expiring of [false,true]) {
 const raw={symbol:'BTCUSDT'};
 for (const field of ['lastPrice','bid1Price','ask1Price','markPrice','indexPrice','price24hPcnt','openInterest','fundingRate','nextFundingTime','volume24h']) if(value!==undefined) raw[field]=value;
 tickerCases.push({raw:[raw,{symbol:'unlisted',lastPrice:'1'}],markets:[market('BTCUSDT',expiring)],cached});
}
// Vary one field at a time: an earlier throw must not mask lazy funding guards.
for(const field of ['lastPrice','bid1Price','ask1Price','markPrice','indexPrice','price24hPcnt','openInterest','fundingRate','nextFundingTime','volume24h']) for(const value of tickerValues) for(const expiring of [false,true]) {
 const raw={symbol:'BTCUSDT',...Object.fromEntries(['lastPrice','bid1Price','ask1Price','markPrice','indexPrice','price24hPcnt','openInterest','fundingRate','nextFundingTime','volume24h'].map(k=>[k,'12.5']))};
 if(value===undefined) delete raw[field]; else raw[field]=value;
 tickerCases.push({raw:[raw],markets:[market('BTCUSDT',expiring)],cached});
}
tickerCases.push({raw:[null],markets:[],cached},{raw:[{},42,false,[]],markets:[],cached});
for(const symbol of [null,42,0,1e21,1e-7,[],{}, {toString:null}]) {
 let key; try { key=String(symbol); } catch { key='unused'; }
 tickerCases.push({raw:[{symbol,lastPrice:'1'}],markets:[market(key,false)],cached});
}
// Inverse markets: volume from turnover24h (coin), quoteVolume from volume24h (USD contracts).
const inverseMarket={...market('BTCUSD',false),quote:'USD',isInverse:true};
for(const [volume24h,turnover24h] of [['1000','0.05'],['1000',undefined],[undefined,'0.05'],['1e999','-0'],[null,'1.2x']]) {
 const raw={symbol:'BTCUSD',...Object.fromEntries(['lastPrice','bid1Price','ask1Price','markPrice','indexPrice','price24hPcnt','openInterest','fundingRate','nextFundingTime'].map(k=>[k,'20000']))};
 if(volume24h!==undefined) raw.volume24h=volume24h;
 if(turnover24h!==undefined) raw.turnover24h=turnover24h;
 tickerCases.push({raw:[raw],markets:[inverseMarket],cached});
}
const prototypeStart=tickerCases.length;
for(const symbol of ['constructor','toString','__proto__']) tickerCases.push({raw:[{symbol,lastPrice:'1'}],markets:[],cached});
const baseTickerCases = [...tickerCases];
const tickerExpected = (c, venue)=>{

 venue.store={marketsBySymbol:Object.fromEntries(c.markets.map(m=>[m.symbol,m])),tickers:c.cached};
 let rows;
 try { rows=venue.parseTickers(c.raw); } catch { return {error:true}; }
 return {rows,nonfinite:rows.map(t=>['bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'].map(k=>Number.isNaN(t[k])?'NaN':t[k]===Infinity?'Infinity':t[k]===-Infinity?'-Infinity':'finite')),negativeZero:rows.map(t=>['bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'].map(k=>Object.is(t[k],-0)))};
};
const tickers=tickerCases.map(c=>tickerExpected(c, context.venue));
const inverseRow=tickers[prototypeStart-5].rows[0];
assert.deepEqual([inverseRow.volume,inverseRow.quoteVolume],[0.05,1000],'primary inverse volume reads turnover24h');
for(let i=prototypeStart;i<tickers.length;i++) {
 assert.equal(tickers[i].rows.length,1,'live TS must retain inherited market lookup divergence');
 assert.equal(tickers[i].rows[0].id,undefined);
 tickers[i]={rows:[],negativeZero:[],nonfinite:[]}; // Declared divergence: Rust requires an owned typed market.
}
for(const variant of ['bybitinverse','bybitspot']) {
 const file=ts.createSourceFile(variant,readFileSync(root+`packages/safe-cex/src/exchanges/bybit/${variant}.exchange.ts`,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(ts.isClassDeclaration);
 const method=cls.members.find(m=>m.name?.getText(file)==='parseTickers');
 const local=vm.createContext({});
 vm.runInContext(ts.transpileModule(`globalThis.venue = new class { ${method.getText(file)} };`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,local);
 for(let i=0;i<baseTickerCases.length;i++) {
  const c={...baseTickerCases[i],spot:variant==='bybitspot',inverseClass:variant==='bybitinverse'};
  let expected=tickerExpected(c,local.venue);
  if(i>=prototypeStart) {
   assert.equal(expected.rows.length,1,`${variant} inherited-key witness`);
   assert.equal(expected.rows[0].id,undefined);
   expected={rows:[],negativeZero:[],nonfinite:[]};
  }
  tickerCases.push(c); tickers.push(expected);
 }
}
const expected={positions,tickers};
// Equivalent JSON spellings exercise JS numeric coercion, including numeric -0.
const input=JSON.stringify({positions:cases,tickers:tickerCases}).replaceAll('\"symbol\":42,','\"symbol\":42.0,').replaceAll('\"symbol\":0,','\"symbol\":-0.0,');
assert.equal(cases.length,1041,'EXCHANGE-STATE-CARDINALITY source cases');
assert.equal(tickerCases.length,3228,'EXCHANGE-STATE-CARDINALITY source tickerCases');
const run=spawnSync('cargo',['run','--quiet','--locked','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--example','exchange-state'],{cwd:root,encoding:'utf8',input,maxBuffer:16*1024*1024});
assert.equal(run.status,0,run.error?.message??run.stderr);
// Normalize JSON transport on both sides; separate flags above retain signed-zero evidence.
assert.deepEqual(JSON.parse(JSON.stringify(JSON.parse(run.stdout))),JSON.parse(JSON.stringify(expected)), "exchange-state decoded field identity");
console.log(`Bybit position selection: ${cases.length} live-source cases; ${tickerCases.length} ticker cases (expiring funding, prefix parsing, signed zero and cached fallback).`);
