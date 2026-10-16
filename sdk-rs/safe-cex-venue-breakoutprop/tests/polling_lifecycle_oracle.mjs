// Actual complete venue exchange, public polling subclass and DefaultStore.
// Base constructors/disposal, API/rate-limit, clock and HTTP are leaf fixtures.
// No selected venue method is replaced. Enumeration stops at the cases below.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const lodash=require('lodash');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function ast(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);}
function body(path){const a=ast(path);return a.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(a)).join('\n').replace(/\bexport /g,'');}
function bits(x){const a=new DataView(new ArrayBuffer(8));a.setFloat64(0,x);return a.getBigUint64(0).toString(16).padStart(16,'0');}
function fixture(){
  let f={now:0,hook:null,events:[],ready:true,replies:[],requests:[],releases:[],socketDisposes:0,baseFail:false};
  class DateFixture extends Date{static now(){const hook=f.hook;f.hook=null;if(hook)hook();return f.now;}}
  class BaseFixture{
    constructor(name,options,store){f.events.push('base');this.name=name;this.options=options;this.store=store;this.isDisposed=false;}
    dispose(){this.isDisposed=true;if(f.baseFail)throw Error('reset failed');this.store.reset();}
  }
  class SocketFixture{
    constructor(parent){f.events.push('socket');this.parent=parent;}
    cleanupMessageHandlerStats(topic){assert.equal(this.messageHandlers[topic].length,0);f.events.push('cleanup:'+topic);}
    dispose(){assert.equal(this.parent.isDisposed,true);f.socketDisposes++;f.events.push('socket-dispose');}
  }
  let next=0;
  function createAPI(options,exchange){
    const id=next++;assert.equal(options,exchange.options);assert.equal(exchange.store,f.store);assert.equal(exchange.sessionToken,null);assert.equal(exchange.publicWebsocket,undefined);
    if(id===1)assert.ok(exchange.xhr);
    f.events.push('api:'+id);
    return {async post(url,body){f.requests.push({id,url,body});const reply=f.replies.shift()??{sessionToken:'token',timeout:'0:0:1'};if(!f.ready)await new Promise(r=>f.releases.push(r));if(reply instanceof Error)throw reply;return {data:reply};}};
  }
  const c=vm.createContext({Date:DateFixture,Error,structuredClone,uniqBy:lodash.uniqBy,BaseExchange:BaseFixture,BaseWebSocket:SocketFixture,createAPI,rateLimit(client,opts){assert.equal(opts.maxRPS,5);f.events.push('limit:5');return client;},virtualClock:{getCurrentTimeMs:()=>DateFixture.now()}});
  function run(code){vm.runInContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,c);}
  const types=ast('types.ts');const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','OrderStatus','PositionSide','OrderType','OrderTimeInForce','MarginType','PositionMode'].includes(n.name.text));assert.equal(enums.length,7);
  const seconds=types.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(types)==='timeframeToSeconds'));assert.ok(seconds);
  run(enums.map(n=>n.getText(types).replace('export ','')).join('\n')+'\n'+seconds.getText(types).replace('export ','')+'\n'+body('utils/clone.ts')+'\n'+body('store/store.base.ts')+'\nglobalThis.Store=DefaultStore;');
  run(body('exchanges/breakoutprop/breakoutprop.types.ts')+'\n'+body('exchanges/breakoutprop/breakoutprop.ws-public.ts')+'\n'+body('exchanges/breakoutprop/breakoutprop.exchange.ts')+'\nglobalThis.ExchangeClass=BreakoutpropExchange;');
  f.c=c;f.store=new c.Store();f.ex=new c.ExchangeClass('breakoutprop',{key:'k',secret:'s'},f.store);
  f.flush=async()=>{for(let iteration=0;iteration<32;iteration++)await Promise.resolve();};
  f.release=()=>{f.ready=true;for(const r of f.releases.splice(0))r();};return f;
}
const ticker={id:'t',symbol:'BTC',bid:100,ask:102,last:101,mark:101,index:101,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const poll=fixture();poll.now=129999;const socket=poll.ex.publicWebsocket;assert.equal(socket.parent,poll.ex);assert.equal(Object.keys(socket.messageHandlers).length,0);
const candles=[];const u1=socket.listenOHLCV({symbol:'BTC',interval:'1m'},c=>candles.push(c));const old=socket.messageHandlers['poll_kline.BTC'];const u2=socket.listenOHLCV({symbol:'BTC',interval:'1m'},c=>candles.push(c));const list=socket.messageHandlers['poll_kline.BTC'];assert.equal(old.length,1);assert.equal(list.length,2);assert.notEqual(old,list);for(const cb of list)cb({...ticker,last:-0});assert.equal(candles.length,2);assert.notEqual(candles[0],candles[1]);assert.equal(candles[0].timestamp,120);for(const c of candles){for(const field of['open','high','low','close'])assert.equal(bits(c[field]),'8000000000000000');assert.equal(c.volume,0);assert.equal(Object.hasOwn(c,'lastSide'),false);}const candleTimestamp=candles[0].timestamp;const negativeZero=bits(candles[0].open);poll.now=240001;list[0]({...ticker,last:5});assert.equal(candles.length,3);assert.notEqual(candles[0],candles[2]);assert.equal(bits(candles[0].open),'8000000000000000');assert.equal(candles[2].open,5);assert.equal(candles[2].timestamp,240);u1();assert.equal(list.length,2);assert.equal(socket.messageHandlers['poll_kline.BTC'].length,1);u2();u2();assert.equal(poll.events.filter(e=>e==='cleanup:poll_kline.BTC').length,2);
const books=[];const ub=socket.listenOrderBook('BTC',b=>books.push(b));const cb=socket.messageHandlers['poll_book.BTC'][0];cb({...ticker,bid:-0,ask:Infinity});assert.equal(books[0].bids.length,2);assert.equal(books[0].asks.length,2);for(const level of books[0].bids){assert.equal(bits(level.price),'8000000000000000');assert.equal(level.amount,1);assert.equal(level.total,1);}for(const level of books[0].asks){assert.equal(level.price,Infinity);assert.equal(level.amount,1);assert.equal(level.total,1);}const positiveInfinity=bits(books[0].asks[0].price);cb({...ticker,bid:99,ask:103});assert.equal(books[0],books[1]);assert.equal(books[0].bids[0].price,99);ub();const cleanupCount=poll.events.filter(e=>e.startsWith('cleanup:')).length;assert.equal(cleanupCount,3);
const ctorCase=fixture();const ctor=ctorCase.events.slice();assert.deepEqual(ctor,['base','api:0','limit:5','api:1','socket']);assert.notEqual(ctorCase.ex.xhr,ctorCase.ex.unlimitedXHR);assert.equal(ctorCase.ex.accountCode,null);assert.equal(ctorCase.ex.sessionTokenRequest,null);assert.equal(ctorCase.ex.sessionTokenExpiresAt,0);assert.equal(Object.keys(ctorCase.ex.recentWsRequests).length,0);assert.deepEqual(Array.from(ctorCase.ex.grafanaEndpointsEvery),['accounts/{accountCode}/orders','accounts/{accountCode}/orders/{orderCode}']);assert.deepEqual(Array.from(ctorCase.ex.grafanaEndpointsPeriodic),['accounts/{accountCode}/portfolio']);
let wrapperCalls=0;const bUnsub=ctorCase.ex.listenOrderBook('BTC',()=>wrapperCalls++);ctorCase.ex.publicWebsocket.messageHandlers['poll_book.BTC'][0](ticker);const cUnsub=ctorCase.ex.listenOHLCV({symbol:'BTC',interval:'1m'},()=>wrapperCalls++);ctorCase.ex.publicWebsocket.messageHandlers['poll_kline.BTC'][0](ticker);assert.equal(wrapperCalls,2);bUnsub();cUnsub();ctorCase.ex.dispose();const disposeCalls=ctorCase.socketDisposes;assert.equal(disposeCalls,1);
const fail=fixture();fail.baseFail=true;assert.throws(()=>fail.ex.dispose(),/reset failed/);assert.equal(fail.ex.isDisposed,true);const failureDisposeCalls=fail.socketDisposes;assert.equal(failureDisposeCalls,0);
const missing=fixture();missing.ex.publicWebsocket=undefined;for(const [member,fn]of[['listenOrderBook',()=>missing.ex.listenOrderBook('BTC',()=>{})],['listenOHLCV',()=>missing.ex.listenOHLCV({symbol:'BTC',interval:'1m'},()=>{})],['dispose',()=>missing.ex.dispose()]]){assert.throws(fn,e=>e.name==='TypeError'&&e.message===`Cannot read properties of undefined (reading '${member}')`);}
const session=fixture();session.ready=false;const a=session.ex.getSessionToken(),marker=session.ex.sessionTokenRequest,b=session.ex.getSessionToken();assert.equal(marker,session.ex.sessionTokenRequest);assert.equal(session.requests.length,1);assert.equal(session.requests[0].id,0);assert.equal(session.requests[0].url,'/login');assert.equal(session.requests[0].body.domain,'default');assert.equal(session.requests[0].body.username,'k');assert.equal(session.requests[0].body.password,'s');assert.equal(Object.keys(session.requests[0].body).length,3);const coalescedRequests=session.requests.length;session.release();assert.equal(await a,'token');assert.equal(await b,'token');assert.equal(session.ex.sessionTokenRequest,null);assert.equal(session.ex.sessionTokenExpiresAt,1000);assert.equal(await session.ex.getSessionToken(),'token');assert.equal(session.requests.length,1);
session.hook=()=>session.ex.sessionToken='clock-live';const clockLive=await session.ex.getSessionToken();assert.equal(clockLive,'clock-live');session.now=1000;session.replies.push({sessionToken:'renewed',timeout:'0:0:2'});assert.equal(await session.ex.getSessionToken(),'renewed');assert.equal(session.requests.length,2);const expiry=session.ex.sessionTokenExpiresAt;assert.equal(expiry,3000);
session.ex.invalidateSession();session.replies.push({sessionToken:'',timeout:'0:0:1'});await assert.rejects(session.ex.getSessionToken(),/No session token received/);assert.equal(session.ex.sessionTokenRequest,null);session.replies.push({sessionToken:'bad',timeout:'0:60:0'});await assert.rejects(session.ex.getSessionToken(),/Invalid DXtrade session timeout: 0:60:0/);assert.equal(session.ex.sessionToken,null);session.hook=()=>session.ex.sessionToken='settle-live';session.replies.push({sessionToken:'wire-token',timeout:'0:0:1'});const settleLive=await session.ex.getSessionToken();assert.equal(settleLive,'settle-live');
const abandoned=fixture();abandoned.ready=false;void abandoned.ex.getSessionToken();await abandoned.flush();assert.ok(abandoned.ex.sessionTokenRequest);abandoned.release();await abandoned.flush();const abandonedToken=abandoned.ex.sessionToken;assert.equal(abandonedToken,'token');assert.equal(abandoned.ex.sessionTokenRequest,null);
const abandonedReports=[];const reject=e=>abandonedReports.push(e.message);process.on('unhandledRejection',reject);abandoned.ex.invalidateSession();abandoned.ready=false;abandoned.replies.push(new Error('login failed'));void abandoned.ex.getSessionToken();abandoned.release();await abandoned.flush();await new Promise(r=>setImmediate(r));assert.equal(abandoned.ex.sessionTokenRequest,null);abandoned.ex.invalidateSession();abandoned.replies.push(new Error('handled'));await assert.rejects(abandoned.ex.getSessionToken(),/handled/);await new Promise(r=>setImmediate(r));process.removeListener('unhandledRejection',reject);assert.deepEqual(abandonedReports,['login failed']);
process.stdout.write(JSON.stringify({ctor,candleTimestamp,negativeZero,positiveInfinity,cleanupCount,wrapperCalls,disposeCalls,failureDisposeCalls,coalescedRequests,expiry,clockLive,settleLive,abandonedToken,abandonedReports}));
