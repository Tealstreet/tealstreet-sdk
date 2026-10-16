import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource as enumSource} from './source-loader.mjs';
const require=createRequire(import.meta.url), modules='';
const dayjs=require(modules+'dayjs'), AxiosHeaders=require(modules+'axios').AxiosHeaders;
const qs=require(modules+'qs'),hmac=require(modules+'create-hmac');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function selected(file,cls,names){
 const url=new URL(file,root),ast=ts.createSourceFile(url.pathname,fs.readFileSync(url,'utf8'),ts.ScriptTarget.Latest,true);
 const c=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===cls);assert.ok(c,cls);
 return names.map(name=>{const a=c.members.filter(n=>n.name?.getText(ast)===name);assert.equal(a.length,1,name);return a[0].getText(ast)}).join('\n');
}
function execute(text,context){return vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,context)}
function loadSource(url,expose=[],select=[],deps={},globals={}){
 let text=fs.readFileSync(url,'utf8');
 if(select.length){const ast=ts.createSourceFile(url.pathname,text,ts.ScriptTarget.Latest,true);text=ast.statements.filter(n=>ts.isImportDeclaration(n)||(ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>select.includes(d.name.getText(ast))))||(ts.isFunctionDeclaration(n)&&select.includes(n.name?.text))).map(n=>n.getText(ast)).join('\n');}
 const m={exports:{}};execute(text+'\n'+expose.map(n=>'exports.'+n+'='+n+';').join('\n'),{module:m,exports:m.exports,require:n=>{assert.ok(Object.hasOwn(deps,n),n);return deps[n];},...globals});return m.exports;
}
const {BASE_URLS,RECV_WINDOW,ENDPOINTS,INTERVAL}=loadSource(new URL('exchanges/okx/okx.types.ts',root),['BASE_URLS','RECV_WINDOW','ENDPOINTS','INTERVAL'],['BASE_URLS','RECV_WINDOW','ENDPOINTS','INTERVAL'],{'../../types':enumSource(new URL('types.ts',root))});
const {calcOrderBookTotal}=loadSource(new URL('utils/orderbook.ts',root),['calcOrderBookTotal'],['calcOrderBookTotal']);
const {updateOrderBook}=loadSource(new URL('utils/updateOrderBook.ts',root),['updateOrderBook'],['binarySearchPrice','updateOrderBook']);
const {multiply}=loadSource(new URL('utils/safe-math.ts',root),['multiply'],[],{'@tealstreet/math/src/isNotNumber':loadSource(new URL('../../../packages/math/src/isNotNumber.ts',import.meta.url))});
const pack=v=>v===undefined?{u:true}:typeof v==='number'?{n:Number.isNaN(v)?'nan':(()=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex')})()}:typeof v==='string'?{s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:typeof v==='function'?{fn:true}:Array.isArray(v)?v.map(pack):v&&typeof v==='object'?{o:Object.entries(v).map(([k,v])=>[k,pack(v)])}:v;
let raw='';for await(const part of process.stdin)raw+=part;
async function fixture(f){
 const effects=[],original=Object.assign(new TypeError('DOWN'),{code:'IO'}),fresh=new Set();
 const effect=(name,...args)=>{effects.push([name,...args.map(pack)]);if(f.case==='fail:'+name)throw original;};
 function ErrorCtor(message){effect('newError',message);const e=new Error(message);fresh.add(e);return e;}
 const context={dayjs,ENDPOINTS,INTERVAL,virtualClock:{getCurrentTimeMs:()=>{effect('clock');return f.now;}},omitUndefined:o=>Object.fromEntries(Object.entries(o).filter(([,v])=>v!==undefined)),OKX_BILL_TYPE_BY_INCOME_TYPE:{COMMISSION:'2',FUNDING_FEE:'8'},LogSeverity:{Warning:'warning'},sleep:async ms=>effect('sleep',ms),oldestCursorId:(rows,key)=>{effect('cursor',key);return f.case==='cap'?String(page):f.cursor??rows.map(r=>r[key]).sort((a,b)=>BigInt(a)<BigInt(b)?-1:1)[0];},mapOkxBillToIncomeRecords:(row,resolve)=>{const symbol=resolve(row.instId);return row.records.map(r=>({...r,symbol}));}};
 const bodies=selected('exchanges/okx/okx.exchange.ts','OKXExchange',['fetchIncomeHistory','fetchOHLCV']);
 const Exchange=execute('class Selected{'+bodies+'}\nSelected',context),w=new Exchange();
 const market={id:'I',symbol:'SYM',precision:{amount:.1}};
 w.isPublicDataOnly=f.public;w.store={marketsBySymbol:f.missing?{}:{SYM:market},marketsById:{I:market}};
 w.emitAndLogError=m=>effect('emit',m);w.log=(...args)=>effect('log',...args);
 let page=0;w.xhr={get:async(endpoint,config)=>{effect('get',endpoint,config.params);const rows=f.pages[Math.min(page++,f.pages.length-1)];return {data:{data:rows}};}};
 let result,failed=false,error,identity={};
 try{
 if(f.method==='income')result=await w.fetchIncomeHistory(f.opts??undefined);
 if(f.method==='candles')result=await w.fetchOHLCV(f.opts);
 if(f.method==='book'||f.method==='candle-sub'){
  const book=f.method==='book';const body=selected(book?'exchanges/okx/okx.ws-public.ts':'exchanges/okx/okx.ws-business.ts',book?'OKXPublicWebsocket':'OKXBusinessWebsocket',[book?'listenOrderBook':'listenOHLCV']);
  const timers=new Map();let timerId=0;const ctx={INTERVAL,multiply,calcOrderBookTotal,updateOrderBook,setTimeout:(cb,ms)=>{effect('timer',ms);timers.set(++timerId,cb);return timerId;},clearTimeout:id=>{effect('clear',id);timers.delete(id);}};
  const Socket=execute('class Selected{'+body+'}\nSelected',ctx),s=new Socket();
  const key=book?'books.I':'candle.1m.I';const existing=()=>{};
  s.store={loaded:{markets:!f.unloaded},markets:f.missing?[]:[market],tickersBySymbol:f.noTicker?{}:{SYM:{last:f.last??10}}};s.isConnected=!f.wait;s.isDisposed=f.disposed;s.topics={};s.messageHandlers=f.shared?{[key]:[existing]}:{};
  s.orderBookSequenceByHandler={delete:key=>effect('seqDelete',key)};s.recoveringOrderBooks={delete:key=>effect('recoverDelete',key)};
  s.cleanupMessageHandlerStats=key=>effect('cleanup',key);s.send=wire=>effect('send',wire);
  const listener=book?s.listenOrderBook:s.listenOHLCV;
  s.listenOHLCV=f.unloaded?(...a)=>effect('retry',...a):s.listenOHLCV;s.listenOrderBook=f.unloaded?(...a)=>effect('retry',...a):s.listenOrderBook;
  const observations=[],refs=[];const cb=x=>{observations.push(structuredClone(x));refs.push(x);};
  const unsubscribe=book?listener('SYM',cb):listener({symbol:'SYM',interval:'1m'},cb);
  if(f.cancelEarly)unsubscribe();
  if(f.fireTimer&&timers.size){s.isConnected=true;const [id,fn]=timers.entries().next().value;timers.delete(id);fn();}
  if(!f.cancelEarly){for(const frame of f.frames??[]){const parser=s.messageHandlers[key]?.at(-1);if(parser&&parser!==existing)parser(frame);}}
  const same=refs.length>1&&refs[0]===refs[1];
  if(!f.cancelEarly)unsubscribe();if(f.twice)unsubscribe();
  result={observations,shared:!!s.messageHandlers[key]?.includes(existing),handlers:s.messageHandlers[key]?.length??0,topics:s.topics,timers:timers.size,same,cleared:book&&refs.length?refs[0].bids.length===0&&refs[0].asks.length===0:null};
 }
 if(f.method==='factory'){
  const xhr={interceptors:{response:{use:cb=>{effect('responseUse');xhr.response=cb;}},request:{use:cb=>{effect('requestUse');xhr.request=cb;}}}};
  const reads=loadSource(new URL('utils/retryReadsOnly.ts',root),['networkErrorOnRead'],['isReadOnlyMethod','onlyReads','networkErrorOnRead'],{'axios-retry':require(modules+'axios-retry')});
  const signatures=loadSource(new URL('utils/signatureRejection.ts',root),['SIGNATURE_RETRY_CONFIG_KEY','hasRetriedSignature','signatureRejectedResponse','resendOnSignatureRejection'],['SIGNATURE_RETRY_CONFIG_KEY','hasRetriedSignature','signatureRejectedResponse','resendOnSignatureRejection']);
  const api=loadSource(new URL('exchanges/okx/okx.api.ts',root),['createAPI'],['isPublicEndpoint','isOkxSignatureRejection','formatOkxApiErrorMessage','createAPI'],{'axios':{AxiosHeaders:class extends AxiosHeaders{constructor(o){effect('headers',o);super(o);}}},'axios-retry':(x,opts)=>{effect('retryInstall',opts.retries);xhr.condition=opts.retryCondition;},'create-hmac':hmac,'qs':qs,'../../remoteSigningAdapter':{markSkipRemoteSigning:c=>{effect('skip');c.safeCexSkipRemoteSigning=true;}},'../../safeCexAxiosCreate':{safeCexAxiosCreate:(config,options,exchange)=>{effect('factory',config);identity.options=options===f.opts;identity.exchange=exchange===f.exchange;return xhr;},installTransportAdapter:(x,o,n,variant)=>{effect('adapter',n,variant);assert.equal(x,xhr);assert.equal(o,f.opts);}},'../../utils/virtualClock':{virtualClock:{getCurrentTime:()=>{effect('isoClock');return new Date(f.now);}}},'../../utils/retryReadsOnly':reads,'../../utils/signatureRejection':signatures,'./okx.types':{BASE_URLS,RECV_WINDOW}}, {Error:ErrorCtor});
  const out=api.createAPI(f.opts,f.exchange);identity.xhr=out===xhr;
  let config=f.request;let response=f.response??{data:{code:'0'}};
  if(config){const next=xhr.request(config);identity.request=next===config;config={...next,headers:next.headers instanceof AxiosHeaders?next.headers.toJSON():next.headers};}
  const originalResponse=response;response=xhr.response(response);identity.response=response===originalResponse;
  const retry=[];for(const method of ['get','head','post'])retry.push(xhr.condition(Object.assign(new Error('network'),{code:'ERR_NETWORK',config:{method}})));
  result={config,response,retry,identity};
 }
 }catch(e){failed=true;error={original:e===original,name:e.name,message:pack(e.message)};if(e!==original)assert.ok(fresh.has(e),`fresh error ${f.method}:${f.case} ${e.stack}`);}
 return {effects,failed,result:pack(result),...(error?{error}:{})};
}
process.stdout.write(JSON.stringify(await Promise.all(JSON.parse(raw).map(fixture))));
