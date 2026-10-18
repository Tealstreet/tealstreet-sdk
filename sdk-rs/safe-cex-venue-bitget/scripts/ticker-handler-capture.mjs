// Whole actual ticker handlers against the actual DefaultStore constructor/update.
import fs from 'node:fs';
import assert from 'node:assert/strict';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
const require=createRequire(import.meta.url),root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const bits=n=>{const b=new ArrayBuffer(8),v=new DataView(b);v.setFloat64(0,n,false);return v.getBigUint64(0,false).toString(16).padStart(16,'0')};
const encode=v=>v===undefined?{$undefined:true}:typeof v==='number'?{$number:Object.is(v,-0)?'-0':String(v),bits:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)])):v;
const numeric=['bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'];
const ticker=(id,symbol)=>({id,symbol,...Object.fromEntries(numeric.map(k=>[k,0]))});
const {Store}=sourceStore(),output=[];
if(process.argv.includes('--id-dictionary-floor')) {
 const results=[];
 for(const name of ['string','number','falsy-number','empty-patch','info-only','own-shadow','updated-prototype','reset-clear','readonly-length','readonly-index','noncanonical-index']) {
  const store=new Store(),proto=ticker('__proto__','__proto__');if(name==='number')proto.last=7;
  store.update({tickers:[proto,...name==='own-shadow'?[ticker('symbol','OWN')]:[]]});
  if(name==='updated-prototype')store.updateTicker({id:'__proto__'},{last:7});
  if(name==='reset-clear')store.reset();
  assert.equal(Object.hasOwn(store.tickersById,'__proto__'),false,'source ID getter omits own prototype key');
  const key=['number','falsy-number','updated-prototype'].includes(name)?'last':'symbol',patch=name==='empty-patch'?{}:name==='info-only'?{info:{ignored:true}}:{[({'readonly-length':'length','readonly-index':'0','noncanonical-index':'00'})[name]||'last']:9};
  let first=0,second=0;store.subscribe(()=>first++);store.subscribe(()=>second++);let error=null;
  try{store.updateTicker({id:key},patch)}catch(e){error=e}
  const throws=['string','number','updated-prototype','readonly-length','readonly-index','noncanonical-index'].includes(name);assert.equal(error?.name,throws?'TypeError':undefined,'source direct independent error kind');
  assert.equal(first,name==='own-shadow'?1:0,'source direct independent first subscriber floor');assert.equal(second,name==='own-shadow'?1:0,'source direct independent second subscriber floor');
  results.push({name,error:error?{name:error.name,message:error.message}:null,first,second,after:store.tickers.map(t=>({id:t.id,symbol:t.symbol,last:encode(t.last)}))});
 }
 assert.equal(results.length,11,'independent direct ID floor');process.stdout.write(JSON.stringify({cases:results,last_index:10}));process.exit(0);
}
if(process.argv.includes('--id-dictionary-finding')) {
 const store=new Store();store.update({tickers:[ticker('__proto__','__proto__')]});
 assert.equal(store.tickers[0].info,undefined,'direct finding native stored input');
 let error;try{store.updateTicker({id:'symbol'},{last:9})}catch(e){error=e}
 assert.equal(error?.name,'TypeError','source direct ID dictionary primitive target');
 assert.equal(store.tickers[0].last,0,'source direct ID dictionary leaves typed row unchanged');
 process.stdout.write(JSON.stringify({name:error.name,message:error.message,last:store.tickers[0].last}));process.exit(0);
}

for(const [file,name] of [['bitget.ws-public.ts','BitgetPublicWebsocket'],['bitget.ws-secondary-public.ts','BitgetSecondaryPublicWebsocket']]) {
 const ast=ts.createSourceFile(file,fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bitget',file),'utf8'),ts.ScriptTarget.Latest,true);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===name);
 const members=['toFiniteNumber','buildTickerUpdate','handleTickerTopic'].map(name=>{const n=klass.members.find(n=>n.name?.getText(ast)===name);if(!n)throw Error('missing ticker handler '+name);return n.getText(ast)});
 const code=ts.transpileModule('class Source {'+members.join('\n')+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
 const Source=new Function(code+';return Source')(),cases=[];
 function capture(name,input,seed=[ticker('btc','BTC'),ticker('eth','ETH')],mode='normal',configure=()=>{}) {
  const store=new Store();store.update({tickers:structuredClone(seed)});configure(store);
  const events=[],getter=Object.getOwnPropertyDescriptor(Store.prototype,'tickersBySymbol').get;
  Object.defineProperty(store,'tickersBySymbol',{get(){return new Proxy(getter.call(store),{get(target,key,receiver){const value=Reflect.get(target,key,receiver);events.push({phase:'lookup',key:String(key),type:typeof value,id:encode(value?.id)});return value;}})}});
  const update=store.updateTicker;
  store.updateTicker=(value,patch)=>{events.push({phase:'write',type:typeof value,id:encode(value.id),key:String(value.id),patch:encode(patch)});return update(value,patch)};
  const subscriberError=new Error('subscriber witness');
  store.subscribe(()=>{events.push({phase:'first',state:encode(store.tickers)});if(mode==='throw')throw subscriberError;});
  store.subscribe(()=>events.push({phase:'second'}));
  const receiver=new Source();receiver.parent={store};let error=null;
  try{receiver.handleTickerTopic(input)}catch(e){if(mode==='throw')assert.equal(e,subscriberError,'source original subscriber error identity');error={name:e.name,message:e.message}}
  cases.push({name,input:encode(input),seed:encode(seed),mode,events,error,after:encode(store.tickers)});
 }
 for(const input of [undefined,null,false,1,'x',[],{}, {data:null},{data:{}},{data:[]},{data:[null,false,1,'x',[],{}]}])capture('root:'+cases.length,input);
 const values=[undefined,null,false,true,0,-0,1,'','BTC','ETH','missing',[],['BTC'],{}, {toString:1},'\ufeff','\u0085','🦀'];
 for(const field of ['instId','symbol'])for(let i=0;i<values.length;i++)capture(`symbol:${field}:${i}`,{data:[{[field]:values[i],...(field==='instId'?{symbol:'ETH'}:{}),lastPr:'9'}]});
 for(const key of ['bidPr','askPr','lastPr','markPrice','indexPrice','change24h','fundingRate','nextFundingTime','baseVolume','quoteVolume'])for(const v of [null,0,-0,'7','-0','junk','Infinity','1e308'])capture(`value:${key}:${String(v)}:${Object.is(v,-0)}`,{data:[{instId:'BTC',[key]:v}]});
 capture('native-NaN-blocks-alias',{data:[{instId:'BTC',bidPr:NaN,bidPrice:'7'}]},undefined,'input-domain-finding');
 capture('sequential-partial-throw',{data:[{instId:'BTC',lastPr:'1'},{instId:{toString:1},lastPr:'2'},{instId:'ETH',lastPr:'3'}]});
 capture('notification-aborts-batch',{data:[{instId:'BTC',lastPr:'1'},{instId:'ETH',lastPr:'2'}]},undefined,'throw');
 capture('same-value-still-notifies',{data:[{instId:'BTC',lastPr:'0'},{instId:'BTC',lastPr:'0'}]});
 for(const key of Object.getOwnPropertyNames(Object.prototype))for(const sentinel of [true,false])capture(`inherited:${key}:${sentinel}`,{data:[{instId:key,lastPr:'9'}]},[ticker('btc','BTC'),...(sentinel?[ticker('undefined','sentinel')]:[])]);
 for(const key of ['constructor','toString','__proto__'])capture('own-or-prototype:'+key,{data:[{instId:key,lastPr:'9'}]},[ticker('own',key),ticker('undefined','sentinel')]);
 capture('poisoned-dictionary-write',{data:[{instId:'symbol',lastPr:'9'}]},[ticker('poison','__proto__'),ticker('undefined','sentinel')],'prototype-write');
 const utf16=['\ud800','x\udc00'].map(id=>{const store=new Store();store.update({tickers:[ticker('x',id)]});const r=new Source();r.parent={store};r.handleTickerTopic({data:[{instId:id,lastPr:'9'}]});return {input:JSON.stringify(id),last:store.tickers[0].last};});
 if(process.argv.includes('--prototype-floor')) {
  cases.length=0;
  const shapeStore=new Store();shapeStore.update({tickers:[ticker("poison","__proto__"),ticker("undefined","sentinel")]});
  const table=shapeStore.tickersBySymbol;
  const getterShape={hasOwnPrototype:Object.hasOwn(table,"__proto__"),hasOwnSymbol:Object.hasOwn(table,"symbol"),inheritedSymbol:table.symbol,idKey:String(table.symbol.id),prototypeIsRow:Object.getPrototypeOf(table)===shapeStore.tickers[0]};
  assert.deepEqual(getterShape,{hasOwnPrototype:false,hasOwnSymbol:false,inheritedSymbol:"__proto__",idKey:"undefined",prototypeIsRow:true},"maintained Store plain dictionary prototype shape");
  const poison=(last=0,info)=>({...ticker('poison','__proto__'),last,...info!==undefined?{info}:{}}),sentinel=()=>ticker('undefined','sentinel');
  const input=(key,patch={lastPr:'9'})=>({data:[{instId:key,...patch}]});
  capture('symbol-sentinel',input('symbol'),[poison(),sentinel()]);
  capture('symbol-no-sentinel',input('symbol'),[poison()]);
  capture('own-shadow',input('symbol'),[poison(),ticker('own','symbol'),sentinel()]);
  capture('id-field',input('id'),[poison(),sentinel()]);
  capture('number-positive',input('last'),[poison(3),sentinel()]);
  capture('number-negative-zero',input('last'),[poison(-0),sentinel()]);
  capture('number-nan',input('last'),[poison(NaN),sentinel()]);
  capture('number-infinity',input('last'),[poison(Infinity),sentinel()]);
  capture('optional-absent',input('midPrice'),[poison(),sentinel()]);
  capture('optional-positive',input('midPrice'),[{...poison(),midPrice:42},sentinel()]);
  capture('truthy-info-stripped',input('info'),[poison(0,{id:'undefined'}),sentinel()]);
  capture('info-false',input('info'),[poison(0,false),sentinel()]);
  capture('info-null',input('info'),[poison(0,null),sentinel()]);
  capture('info-id-error-stripped',input('info'),[poison(0,{id:{toString:1}}),sentinel()]);
  capture('empty-update',input('symbol',{}),[poison(),sentinel()]);
  capture('patched-prototype',{data:[{instId:'__proto__',lastPr:'5'},{instId:'last',lastPr:'9'}]},[poison(),sentinel()]);
  capture('second-prototype',input('last'),[poison(3),{...ticker('second','__proto__'),last:0},sentinel()]);
  capture('retained-prototype-alias',input('symbol'),[poison(),sentinel()],'normal',store=>store.updateTicker({id:'poison'},{symbol:'renamed'}));
  capture('replacement-clear',input('symbol'),[poison(),sentinel()],'normal',store=>store.update({tickers:[ticker('btc','BTC'),sentinel()]}));
  capture('reset-clear',input('symbol'),[poison(),sentinel()],'normal',store=>store.reset());
  capture('own-constructor',input('constructor'),[poison(),ticker('own','constructor'),sentinel()]);
  capture('notification-throws',{data:[{instId:'symbol',lastPr:'9'},{instId:'__proto__',lastPr:'5'}]},[poison(),sentinel()],'throw');
  assert.equal(cases.length,22,'independent prototype scenario floor');
  const probe=new Store();probe.update({tickers:[poison(),sentinel()]});const dictionary=probe.tickersBySymbol;
  assert.equal(Object.hasOwn(dictionary,'__proto__'),false,'source getter has no own prototype key');
  assert.equal(Object.keys(dictionary).includes('__proto__'),false,'source getter enumeration excludes prototype');
  assert.equal(dictionary.symbol,'__proto__','source getter traverses native ticker prototype');
  for(const c of cases.filter(c=>c.name.includes('stripped')))assert.equal(c.after[0].info,undefined,'source truthy info removed by real ingress');
  for(const c of cases){const first=c.events.filter(e=>e.phase==='first');if(c.name==='symbol-sentinel')assert.equal(first.length,1,'source original prototype write notifies once');if(c.name==='number-nan'||c.name==='number-negative-zero')assert.equal(first.length,0,'source falsy numeric prototype field skips mutation');}
  output.push({file,cases:cases.map(c=>({name:c.name,error:c.error,after:c.after.map(t=>({id:t.id,symbol:t.symbol,last:t.last})),first:c.events.filter(e=>e.phase==='first').length,second:c.events.filter(e=>e.phase==='second').length})),last_index:21,getter:getterShape});continue;
 }
 if(process.argv.includes('--field-identity')) {
  cases.length=0;
  const seeds=[ticker('identity-btc','BTC'),ticker('identity-eth','ETH')].map((row,j)=>({...row,...Object.fromEntries(numeric.map((key,i)=>[key,1000*(j+1)+i+0.25]))}));
  capture('field-identity:unchanged',{data:[]},seeds);
  capture('field-identity:partial',{data:[{instId:'BTC',lastPr:'77.5'}]},seeds);
  capture('field-identity:all',{data:[{instId:'ETH',bidPr:'11.25',askPr:'22.5',lastPr:'33.75',markPrice:'44.5',indexPrice:'55.25',change24h:'0.665',fundingRate:'0.00775',nextFundingTime:'88000',baseVolume:'99.5',quoteVolume:'110.75'}]},seeds);
 }
 output.push({file,cases,utf16});
}
process.stdout.write(JSON.stringify(output)+'\n');
