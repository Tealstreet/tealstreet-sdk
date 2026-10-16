import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {createRequire}from'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=new URL('../../../',import.meta.url),target=process.argv[2],fixture=JSON.parse(fs.readFileSync(new URL('../tests/fixtures/public-listeners.json',import.meta.url)));
const group=target.split('_')[0],venue=target.endsWith('_pm')?'pm':target.endsWith('_spot')?'spot':'futures',stem={futures:'binance',pm:'binancepm',spot:'binancespot'}[venue],className=group==='candles'?{futures:'BinanceMarketWebsocket',pm:'BinancePMMarketWebsocket',spot:'BinancePublicWebsocket'}[venue]:{futures:'BinancePublicWebsocket',pm:'BinancePMPublicWebsocket',spot:'BinancePublicWebsocket'}[venue];
function tree(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true)}
function compile(code,bindings){const exports={};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,...bindings});return exports;}
const source=tree(`packages/safe-cex/src/exchanges/binance/${stem}.ws-public.ts`),cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===className);assert.ok(cls);
const member=cls.members.find(n=>group==='constructors'?ts.isConstructorDeclaration(n):n.name?.getText(source)===(group==='books'?'listenOrderBook':'listenOHLCV'));assert.ok(member);const types=tree(`packages/safe-cex/src/exchanges/binance/${stem}.types.ts`),endpoints=types.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(types)==='ENDPOINTS'));
const update=compile(tree('packages/safe-cex/src/utils/updateOrderBook.ts').text),books=compile(tree('packages/safe-cex/src/exchanges/binance/binance.orderbook.ts').text,{require:id=>{assert.equal(id,'../../utils/updateOrderBook');return update}}),totals=compile(tree('packages/safe-cex/src/utils/orderbook.ts').text);
const primary=()=>Object.assign(new Error('listener fixture'),{name:'RangeError',code:-1000,response:{status:503,data:{msg:'listener detail'}}});const clone=x=>structuredClone(x),snap=x=>JSON.parse(JSON.stringify(x));const results=[];let activeFailureCollector=null;const rejection=e=>{assert.ok(activeFailureCollector,'unexpected rejected background job outside case');activeFailureCollector(e)};process.on('unhandledRejection',rejection);
const cases=fixture[group];assert.equal(cases.length,{candles:14,books:18,constructors:3}[group]);
for(const name of cases){
 const error=primary();
 if(group==='constructors'){
  const trace=[],parent={value:12.3},base={value:3.6},market={value:4.2};class BaseWebSocket{constructor(p){assert.equal(p,parent);trace.push('super');if(name==='super-error')throw error;this.base=base}}class Sibling{constructor(p){assert.equal(p,parent);trace.push('market');if(name==='market-error')throw error;return market}}
  const exports=compile(`class Subject extends BaseWebSocket{${member.getText(source)}} exports.Subject=Subject;`,{BaseWebSocket,BinanceMarketWebsocket:Sibling,BinancePMMarketWebsocket:Sibling});let s,e;try{s=new exports.Subject(parent)}catch(err){e=err}if(s){assert.equal(s.base,base);assert.equal(s.marketWebsocket,market)}results.push({trace,success:!!s,original:e===error,marketValue:s?4.2:null});continue;
 }
 const sent=[],timers=[],cleared=[],cleanup=[],candles=[],bookValues=[],bookRefs=[],gets=[],applies=[],ops=[],failures=[],pending=[];let nano=0;
 const fail=e=>failures.push({original:e===error,name:e.name,message:e.message,code:e.code??null,response:e.response??null});activeFailureCollector=fail;
 const timeout=(action,delay)=>{ops.push('timer');if(name==='timer-error')throw error;const id=name==='zero-ticket'?0:timers.length+1;timers.push({action,delay,id,cancelled:false});return id},clear=id=>{ops.push('clear');cleared.push(id);for(const t of timers)if(t.id===id)t.cancelled=true};const fire=()=>{const actions=timers.filter(t=>!t.cancelled).map(t=>{t.cancelled=true;return t.action});for(const a of actions)try{a()}catch(e){fail(e)}};
 let resets=0;const helperBindings={nanoid(){ops.push('nanoid');return`inner-${++nano}`},setTimeout:timeout,clearTimeout:clear,
 resetBinanceOrderBookSyncState(state){ops.push('reset');books.resetBinanceOrderBookSyncState(state);if(name.startsWith('reset-removes')&&++resets===2)delete subject.messageHandlers['depthUpdate.BTCUSDT']},
 loadBinanceOrderBookSnapshot(book,data,state){ops.push('load');if(name==='load-reject'&&gets.length===1)throw error;books.loadBinanceOrderBookSnapshot(book,data,state)},
 getBufferedBinanceDepthUpdates(rows,id){ops.push('buffer');return books.getBufferedBinanceDepthUpdates(rows,id)},
 applyBinanceDepthUpdate(book,data,state,options){ops.push('apply');applies.push({u:data.u,previous:options?.requirePreviousFinalUpdateId===true});if(name==='apply-reject'&&gets.length===1)throw error;return books.applyBinanceDepthUpdate(book,data,state,options)},
 calcOrderBookTotal(book){ops.push('total');if(name==='total-reject'&&gets.length===1)throw error;totals.calcOrderBookTotal(book)}};
 const exports=compile(`${endpoints.getText(types)} class Subject{${member.getText(source)}}exports.Subject=Subject;`,helperBindings),subject=new exports.Subject(),actual=subject[group==='books'?'listenOrderBook':'listenOHLCV'];subject.messageHandlers={};subject.topics={};subject.nextSubscriptionId=2;subject.orderBookInnerState={};subject.isConnected=!['delayed','unsubscribe-wait','dispose-wait','timer-error','zero-ticket','delayed-unsubscribe'].includes(name);subject.isDisposed=name==='disposed';
 if(name==='initial-nan')subject.topics['btcusdt@kline_1m']=NaN;if(name==='inner-collision')subject.orderBookInnerState['inner-1']={updates:[],isSnapshotLoaded:true};
 subject.send=text=>{ops.push('send');sent.push(JSON.parse(text));if(name==='send-error')throw error};subject.cleanupMessageHandlerStats=key=>{ops.push('cleanup');cleanup.push(key);if(name==='cleanup-error')throw error};
 const client=venue==='spot'?'xhr':'publicXhr';subject.parent={[client]:{get(endpoint,{params}){ops.push('get');gets.push({client,endpoint,...params});const n=gets.length;if(name==='get-sync'&&n===1)throw error;return new Promise((resolve,reject)=>pending.push(()=>{if(name==='get-reject'&&n===1){reject(error);return}const data=clone(fixture.snapshot);if(n>1)data.lastUpdateId=name==='pu-mismatch'?102:105;resolve({data})}))}}};
 const settle=async()=>{const work=pending.splice(0);for(const done of work)done();await new Promise(resolve=>setImmediate(resolve));};
 const observe=f=>{try{return f()}catch(e){fail(e)}};const dispatch=(key,data)=>{for(const p of [...(subject.messageHandlers[key]||[])])observe(()=>p([data]))};
 const callback=group==='candles'?c=>{ops.push('candle');candles.push(snap(c));if(name==='callback-error')throw error}:book=>{ops.push('book');bookValues.push(snap(book));bookRefs.push(book);if(name==='callback-reject'&&gets.length===1)throw error};
 assert.equal(subject[group==='books'?'listenOrderBook':'listenOHLCV'],actual,'actual oracle target body retained');let unsubscribe;
 if(group==='candles'){
  const opts={symbol:'BTCUSDT',interval:'1m'};unsubscribe=observe(()=>subject.listenOHLCV(opts,callback));let second;if(name==='shared')second=subject.listenOHLCV(opts,callback);
  if(['unsubscribe-wait','zero-ticket'].includes(name)&&unsubscribe)observe(unsubscribe);if(name==='dispose-wait')subject.isDisposed=true;if(['delayed','unsubscribe-wait','dispose-wait','zero-ticket'].includes(name)){subject.isConnected=true;fire()}
  const data=clone(fixture.candle);if(name==='live-opts'){opts.symbol='ETHUSDT';opts.interval='5m';data.k.s='ETHUSDT';data.k.i='5m'}if(name==='wrong-events'){data.k.s='ETHUSDT';dispatch('kline.1m.BTCUSDT',clone(data));data.k.s='BTCUSDT';data.k.i='5m'}if(!['send-error','timer-error'].includes(name))dispatch('kline.1m.BTCUSDT',data);
  if(!['unsubscribe-wait','zero-ticket'].includes(name)){if(unsubscribe)observe(unsubscribe);if(second)observe(second);else if(['normal','disposed'].includes(name)&&unsubscribe)observe(unsubscribe)}
 }else{
  unsubscribe=subject.listenOrderBook('BTCUSDT',callback);const parser=subject.messageHandlers['depthUpdate.BTCUSDT']?.[0];if(name==='delayed-unsubscribe'){observe(unsubscribe);subject.isConnected=true;fire()}else{
   const wrong=clone(fixture.first);wrong.s='ETHUSDT';observe(()=>parser([wrong]));assert.equal(gets.length,0);
   const first=clone(name==='gap-buffer'?fixture.gap:fixture.first);if(name==='ignored-buffer'){first.U=99;first.u=99}observe(()=>parser([first]));if(name==='normal')observe(()=>parser([clone(fixture.second)]));if(name==='disposed-settle')subject.isDisposed=true;if(name==='unsubscribe-settle')observe(unsubscribe);
   if(name==='get-sync'){observe(unsubscribe);observe(()=>parser([first]));assert.equal(gets.length,2,'actual source same-turn GET retry after synchronous throw')}
   await settle();if(name==='gap-buffer'){fire();await settle()}if(['gap-live','pu-mismatch'].includes(name)){const next=clone(name==='gap-live'?fixture.gap:fixture.second);if(name==='pu-mismatch')next.pu=9;observe(()=>parser([next]));await settle()}
   if(name==='normal'){observe(()=>parser([clone(fixture.second)]));observe(()=>parser([clone(fixture.next)]))}
   if(['get-reject','load-reject','apply-reject','total-reject','callback-reject'].includes(name)){observe(unsubscribe);observe(()=>parser([first]));await settle()}
   if(name==='reset-removes-disconnected')subject.isConnected=false;if(!['unsubscribe-settle','get-reject','get-sync','load-reject','apply-reject','total-reject','callback-reject'].includes(name))observe(unsubscribe);if(name==='repeated-unsubscribe')observe(unsubscribe);
  }
 }
 if(bookRefs.length>1)assert.ok(bookRefs.every(b=>b===bookRefs[0]),'source same book reference');
 results.push({sent,timers:timers.map(({delay,id})=>({delay,id})),cleared,cleanup,handlers:Object.fromEntries(Object.entries(subject.messageHandlers).map(([k,v])=>[k,v.length])),topics:subject.topics,nextId:subject.nextSubscriptionId,candles,books:bookValues,gets,applies,ops,failures,inner:Object.fromEntries(Object.entries(subject.orderBookInnerState).map(([k,v])=>[k,{loaded:v.isSnapshotLoaded,updates:v.updates}]))});
 activeFailureCollector=null;
}
process.removeListener('unhandledRejection',rejection);process.stdout.write(JSON.stringify(results));
