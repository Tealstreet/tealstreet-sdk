import ts from 'typescript';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const root=new URL('../../../packages/safe-cex/src/exchanges/bybit/',import.meta.url);
const keys=['bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'];
const initial={id:'BTCUSDT',symbol:'BTCUSDT',...Object.fromEntries(keys.map((k,i)=>[k,101+i]))};
const values=[null,false,true,0,1,-1,'','0','-0','12.5suffix','0x10','Infinity','-Infinity','NaN','junk','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const fields=['bid1Price','ask1Price','lastPrice','markPrice','indexPrice','price24hPcnt','openInterest','fundingRate','nextFundingTime','volume24h','turnover24h'];
const encode=v=>typeof v==='number'&&!Number.isFinite(v)?String(v):Object.is(v,-0)?'-0':v;
const cases=[];
for(const [variant,category] of [['bybit','linear'],['bybit','inverse'],['bybitinverse','inverse'],['bybitspot','spot']]) {
 const file=ts.createSourceFile(variant,readFileSync(new URL(`${variant}.ws-public.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(ts.isClassDeclaration);
 const method=cls.members.find(m=>m.name?.getText()==='handleTickersEvent');
 // Arrow lexical this is supplied by evaluating inside an ordinary factory.
 const make=vm.runInNewContext(ts.transpileModule(`(function(){return ${method.initializer.getText(file)};})`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText.replace(/;\s*$/,''));
 const failure=Object.assign(new Error('ticker subscriber rejected'),{name:'TickerSubscriberError',response:{status:418}});
 const committed={...initial};let later=0;
 const listeners=[()=>{throw failure;},()=>{later++;}];
 const failingStore={tickersBySymbol:{BTCUSDT:committed},updateTicker(t,patch){Object.assign(t,patch);for(const listener of listeners)listener();}};
 assert.throws(()=>make.call({parent:{store:failingStore}})({data:{symbol:'BTCUSDT',lastPrice:'42'}}),e=>e===failure,`${variant}: subscriber error identity`);
 assert.equal(committed.last,42,`${variant}: ticker commit precedes rejection`);
 assert.equal(later,0,`${variant}: rejection stops subsequent work`);
 for(const data of [{symbol:{toString:null}},{symbol:'BTCUSDT',lastPrice:{toString:null}}]) {
  assert.throws(()=>make.call({parent:{store:failingStore}})({data}),e=>e.name==='TypeError',`${variant}: coercion error class`);
 }
 const add=frame=>{
  const ticker={...initial};let calls=0,notifications=0,error;
  const store={tickersBySymbol:{BTCUSDT:ticker},updateTicker(t,patch){calls++;if(t===ticker){Object.assign(t,patch);if(Object.keys(patch).length)notifications++;}}};
  try{make.call({parent:{store},category})(frame);}catch(e){error=e.message;}
  cases.push({variant,category,frame,initial,result:Object.fromEntries(keys.map(k=>[k,encode(ticker[k])])),calls,notifications,error});
 };
 for(const field of fields) for(const value of values) { add({data:{symbol:'BTCUSDT',[field]:value}});add({data:{symbol:'UNTRACKED',[field]:value}}); }
 // A later coercion failure must not flush earlier parsed fields to Store.
 for(let index=1;index<fields.length;index++) {
  const prefix=Object.fromEntries(fields.slice(0,index).map(field=>[field,'12.5']));
  add({data:{symbol:'BTCUSDT',...prefix,[fields[index]]:{toString:null}}});
 }
 add(null);add({});add({data:null});
 for(const symbol of [0,false,'',null,[],['BTCUSDT'],{toString:null},'constructor','__proto__','toString']) add({data:{symbol,lastPrice:{toString:null}}});
 add({data:{symbol:'BTCUSDT',...Object.fromEntries(fields.map((f,i)=>[f,String(i+1.5)]))}});
}
console.log(JSON.stringify({cases,keys}));
