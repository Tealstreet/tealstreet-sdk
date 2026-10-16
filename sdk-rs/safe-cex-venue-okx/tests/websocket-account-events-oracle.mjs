import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {ts,loadSource} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource as enumsSource} from './source-loader.mjs';
const require=createRequire(import.meta.url),root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const ast=file=>ts.createSourceFile(file,fs.readFileSync(new URL('exchanges/okx/'+file,root),'utf8'),ts.ScriptTarget.Latest,true);
const privateAst=ast('okx.ws-private.ts'),publicAst=ast('okx.ws-public.ts'),businessAst=ast('okx.ws-business.ts');
const owner=a=>a.statements.find(ts.isClassDeclaration),p=owner(privateAst),u=owner(publicAst),b=owner(businessAst);
const member=(a,c,name)=>{const found=c.members.filter(n=>n.name?.getText(a)===name);assert.equal(found.length,1,name);return found[0].getText(a)};
const selected=['handleOrderTopic','isDuplicateFillEvent','handlePositionTopic','handleAccountTopic','parseFinite','sumFinite'].map(n=>member(privateAst,p,n)).join('\n')+'\n'+member(publicAst,u,'onMarketsChanged');
const {roundUSD}=loadSource(new URL('utils/roundUSD.ts',root));
const mathModule={exports:{}};const mathCode=ts.transpileModule(fs.readFileSync(new URL('utils/safe-math.ts',root),'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
vm.runInNewContext(mathCode,{module:mathModule,exports:mathModule.exports,require:name=>{assert.equal(name,'@tealstreet/math/src/isNotNumber');return loadSource(new URL('../../../packages/math/src/isNotNumber.ts',import.meta.url))}});const {subtract}=mathModule.exports;
const {OrderStatus}=enumsSource(new URL('types.ts',root));
const sumBy=require('lodash/sumBy');
const pack=v=>v===undefined?{u:true}:typeof v==='number'?{n:Number.isNaN(v)?'nan':(()=>{const a=Buffer.alloc(8);a.writeDoubleBE(v);return a.toString('hex')})()}:typeof v==='string'?{s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:Array.isArray(v)?v.map(pack):v instanceof Map?Array.from(v,([k,v])=>[pack(k),pack(v)]):v instanceof Set?Array.from(v,pack):v&&typeof v==='object'?{o:Object.entries(v).map(([k,v])=>[k,pack(v)])}:v;
let incoming='';for await(const chunk of process.stdin)incoming+=chunk;const cases=JSON.parse(incoming).cases;
function fixture({method,case:c}){
 const events=[],err=Object.assign(new TypeError('boom'),{code:'SOCKET',ids:[4,9]});let w,now=60010;
 const effect=(name,...args)=>{events.push([name,...args.map(pack)]);if(c==='fail:'+name)throw err;};
 const coerce=()=>Object.defineProperty({},'toString',{value(){effect('coerce');throw err}});
 const mappedOrders=[{id:'mapped',side:'sell',symbol:'SYM'}],positions=[{id:'P',unrealizedPnl:1.235},{id:'Q',unrealizedPnl:'bad'}],market={id:'I',multiplier:2};
 let balance={used:c==='invalid-balance'?Infinity:3,upnl:c==='invalid-balance'?NaN:2,tag:'retained'};
 const receiver={parent:{},store:{marketsById:{I:market},balance},emitter:{},recentFillKeys:new Map([['stale',9],['edge',10],['future',11]]),fillDedupeWindowMs:60000,topics:{tickers:[{channel:'old',instId:'old'}]},ws:c==='no-socket'?null:{},isDisposed:c==='disposed'};
 class Base {constructor(){Object.assign(this,receiver);w=this;}}
 class TracedMap extends Map{constructor(){super();effect('construct','Map')}}class TracedSet extends Set{constructor(){super();effect('construct','Set')}}
 class Fallback {constructor(){effect('construct','WsDirectFallback');this.tag='fallback'}}
 const context={BaseWebSocket:Base,WsDirectFallback:Fallback,Map:TracedMap,Set:TracedSet,OrderStatus,roundUSD,subtract,sumBy,Date:{now:()=>{effect('clock');return now}}};
 const compile=text=>vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 let result,failed=false;
 try {
 if(method==='class-private'||method==='class-public'||method==='class-business'){
  const P=compile(p.getText(privateAst).replace(/^export\s+/,'')+'\nOKXPrivateWebsocket');context.OKXPrivateWebsocket=P;
  const C=method==='class-private'?P:compile((method==='class-public'?u.getText(publicAst):b.getText(businessAst)).replace(/^export\s+/,'')+'\n'+(method==='class-public'?'OKXPublicWebsocket':'OKXBusinessWebsocket'));
  w=new C();assert.equal(typeof w.onMessage,'function');assert.ok(Object.hasOwn(w,'onMessage'));if(method!=='class-public')assert.equal(typeof Object.getOwnPropertyDescriptor(C.prototype,'endpoint').get,'function','source callable endpoint prototype');else assert.equal(Object.getPrototypeOf(C.prototype),Base.prototype,'source public base prototype');
  if(method==='class-public'){
   const callbacks=w.messageHandlers;for(const name of ['handleTickerEvents','handleMarkPriceEvents','handleIndexTickerEvents','handleOpenInterestEvents','handleFundingRateEvents'])w[name]=d=>effect(name,d);
   const data={tag:'D'};for(const channel of Object.keys(callbacks)){assert.equal(callbacks[channel].length,1);callbacks[channel][0](data)}w.handleTickerEvents=d=>effect('replacement',d);callbacks.tickers[0](data);
   result={sequences:[...w.orderBookSequenceByHandler],recovering:[...w.recoveringOrderBooks],fallback:w.directFallback,topics:w.topics,channels:Object.keys(callbacks)};
  }else{result={keys:[...w.recentFillKeys],fallback:w.directFallback,window:w.fillDedupeWindowMs,...(method==='class-business'?{topics:w.topics}:{})};if(method==='class-business'){assert.equal(Object.getPrototypeOf(C.prototype),P.prototype);assert.equal(w.resyncsPrivateStateOnReopen,true);}}
 }else{
  const C=compile(`class Selected extends BaseWebSocket {${selected}}\nSelected`);w=new C();
  const terminal=(rows,status)=>{assert.equal(rows,mappedOrders);effect('terminal',rows,status)};
  w.store.markOrdersTerminal=terminal;w.store.addOrUpdateOrders=rows=>{assert.equal(rows,mappedOrders);effect('orders',rows)};
  w.parent.mapOrders=rows=>{effect('mapOrders',rows);return c==='unmapped'?[]:mappedOrders};
  w.parent.mapPositions=rows=>{effect('mapPositions',rows);return c==='empty'?[]:positions};
  w.parent.contractsToBase=(amount,m,price)=>{assert.equal(m,market);effect('base',amount,m,price);return amount*2};
  w.parent.convertMarginValueToUsd=(amount,ccy,m,price)=>{effect('margin',amount,ccy,m,price);return amount*2};
  w.store.addOrUpdatePositions=rows=>{assert.equal(rows,positions);effect('positions',rows);if(c==='balance-reentry')w.store.balance={used:77,upnl:88,tag:'changed'}};
  w.store.update=payload=>{effect('update',payload);w.store.balance=payload.balance};w.emitter.emit=(name,fill)=>effect('emit',name,fill);
  w.parent.getIndexTickerInstId=m=>{effect('index',m.id);return m.id+'-INDEX'};
  w.send=wire=>{const decoded=JSON.parse(wire);effect('send',decoded);if(c==='send-reentry'&&events.filter(e=>e[0]==='send').length===1)w.topics['funding-rate'][20].instId='changed'};
  const keys=w.recentFillKeys;for(const [name,event]of [['has','has'],['set','remember'],['delete','forget']]){const original=keys[name].bind(keys);keys[name]=(...args)=>{effect(event,...args);return original(...args)}}
  const row={instId:'I',ordId:'O',state:c==='partial'?'partially_filled':c==='live'?'live':c==='cancel'?'canceled':c==='mmp'?'mmp_canceled':c==='unknown'?'unknown':'filled',tradeId:'T',fillTime:'F',uTime:'U',fillPx:'12.5x',fillSz:c==='zero'?0:'2'};
  if(c==='missing')row.instId='MISSING';if(c==='nan')row.fillSz='bad';if(c==='duplicate')w.isDuplicateFillEvent=o=>{effect('dedupe',o);return true};else if(method==='order')w.isDuplicateFillEvent=o=>{effect('dedupe',o);return false};
  if(c==='zero-trade')row.tradeId=0;if(c==='fallback')delete row.tradeId;if(c==='string-zero')row.tradeId='0';if(c==='lone')row.ordId='\ud800';if(c==='throw-price')row.fillPx=coerce();
  if(method==='order')result=w.handleOrderTopic({data:c==='batch'?[row,{...row,state:'canceled'}]:[row]});
  if(method==='dedupe'){
   const a=w.isDuplicateFillEvent(row);assert.equal(Map.prototype.has.call(keys,'stale'),false);assert.equal(Map.prototype.has.call(keys,'edge'),true);now=120010;const d=w.isDuplicateFillEvent(row);now=120011;const e=w.isDuplicateFillEvent(row);result=[a,d,e];
  }
  if(method==='position')result=w.handlePositionTopic({data:[{instId:'I',mmr:c==='nonfinite'?'Infinity':'1.235',ccy:'USDT',markPx:'12.5'},{instId:'MISSING',mmr:'bad',ccy:'USD',markPx:'-0'}]});
  if(method==='account'){const data=c==='missing'?undefined:c==='object'?{}:c==='empty'?[]:c==='invalid'?[{totalEq:'bad'},{totalEq:Infinity}]:c==='overflow'?[{totalEq:'1e308'},{totalEq:'1e308'}]:c==='throw-price'?[{totalEq:coerce()}]:c==='negative-zero'?[{totalEq:'-0'}]:[{totalEq:'10.235usd'},{totalEq:'bad'},{totalEq:0},{totalEq:null}];result=w.handleAccountTopic({data});}
  if(method==='markets'){const count=c==='empty'?0:c==='boundary'||c==='send-reentry'?21:2;const added=Array.from({length:count},(_,i)=>({id:`M${i}`}));result=w.onMarketsChanged({added});}
 }
 }catch(e){assert.equal(e,err,'source original failure identity');failed=true;}
 return {method,case:c,events,failed,result:pack(result),...(method==='dedupe'?{keys:pack(w.recentFillKeys)}:{}),...(method==='account'||method==='position'?{balance:pack(w.store.balance)}:{}),...(method==='markets'?{topics:pack(w.topics)}:{})};
}
assert.ok(cases.length>0,'source nonempty corpus');process.stdout.write(JSON.stringify(cases.map(fixture)));
