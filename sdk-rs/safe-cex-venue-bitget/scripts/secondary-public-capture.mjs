import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
import {sourceStore} from '../../safe-cex-store/tests/store_source.mjs';
const require=createRequire(import.meta.url);
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const directory=path.join(root,'packages/safe-cex/src/exchanges/bitget');
const read=file=>ts.createSourceFile(file,fs.readFileSync(path.join(directory,file),'utf8'),ts.ScriptTarget.Latest,true);
const ast=read('bitget.ws-secondary-public.ts');
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BitgetSecondaryPublicWebsocket');
assert(klass,'SECONDARY actual whole class');
const protocol=read('bitget.types.ts');
const constant=protocol.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(protocol)==='BASE_WS_URL');
assert(constant,'SECONDARY actual URL');
const url=new Function(ts.transpileModule('const BASE_WS_URL='+constant.initializer.getText(protocol)+';',{}).outputText+';return BASE_WS_URL;')();
const code=ts.transpileModule(klass.getText(ast).replace(/^export\s+/,''),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const {Store}=sourceStore();
let monotonic=123.25;
// Only imported/base dependency operations are supplied. The entire source
// secondary class, including constructor, callbacks and ticker helpers, executes.
class Base {
  constructor(parent){this.parent=parent;this.pingAt=0;}
  get isDisposed(){return this.parent.isDisposed;}
  createWebsocket(url){
    const listeners=new Map();
    return {url,readyState:1,addEventListener:(name,fn)=>listeners.set(name,fn),listeners};
  }
  send(text){this.parent.sent.push(text);}
  trackLastReceived(){}
  handlePongEvent(){this.parent.store.update({latency:Math.round((monotonic-this.pingAt)/2)});}
  recordMessageHandlerStats(){}
  emitWsReconnected(){this.parent.emitter.emit('reconnected');}
  onClose=()=>{};
}
globalThis.performance={now:()=>monotonic};
const Source=new Function('BaseWebSocket','resolveTransportRouting','BASE_WS_URL','jsonParse','ENABLE_RECONNECTING_WEBSOCKET',code+';return BitgetSecondaryPublicWebsocket;')(
  Base,()=>({wsPublic:{fallback:'wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public'}}),url,text=>{try{return JSON.parse(text);}catch{return null;}},false,
);
const getters=new Map();
for(const [name,file,owner] of [['bitget','bitget.exchange.ts','BitgetExchange'],['bitgetusdc','bitgetusdc.exchange.ts','BitgetUSDCExchange'],['bitgetinverse','bitgetinverse.exchange.ts','BitgetInverseExchange']]){
  const ast=read(file),klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===owner);
  const member=klass.members.find(n=>ts.isGetAccessorDeclaration(n)&&n.name?.getText(ast)==='apiProductType');
  assert(member,'SECONDARY actual parent getter '+name);
  const emitted=ts.transpileModule('class Profile {'+member.getText(ast)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  const Profile=new Function(emitted+';return Profile;')();
  getters.set(name,Object.getOwnPropertyDescriptor(Profile.prototype,'apiProductType').get);
}
const symbols=['7','BTC','ETH','2','SOL','XRP','1','BNB','__proto__'];
const seed={id:'btc',symbol:'BTC',bid:0,ask:0,last:0,mark:0,index:0,percentage:0,openInterest:7001.5,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const encode=value=>typeof value==='number'?{number:Object.is(value,-0)?'-0':String(value),bits:(()=>{const b=new ArrayBuffer(8),v=new DataView(b);v.setFloat64(0,value,false);return v.getBigUint64(0,false).toString(16).padStart(16,'0');})()}:Array.isArray(value)?value.map(encode):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([k,v])=>[k,encode(v)])):value;
const cases=[],socketPresence=[];
for(const name of ['bitget','bitgetusdc','bitgetinverse'])for(const testnet of [false,true])for(const index of [0,1,2]){
  monotonic=123.25;
  const store=new Store();
  store.update({markets:symbols.map(symbol=>({id:symbol,symbol,base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.1},limits:{amount:{min:0.01,max:100},leverage:{min:1,max:125}}})),tickers:[structuredClone(seed)]});
  const handlers=new Map();
  const emitter={off(name,fn){handlers.set(name,(handlers.get(name)||[]).filter(f=>f!==fn));},on(name,fn){handlers.set(name,[...(handlers.get(name)||[]),fn]);},emit(name,data){for(const fn of handlers.get(name)||[])fn(data);}};
  const parent={store,name,options:{testnet},isDisposed:false,sent:[],emitter};
  Object.defineProperty(parent,'apiProductType',{get(){return getters.get(name).call(parent);}});
  const receiver=new Source(parent,index);
  assert.equal(receiver.index,index,'SECONDARY source constructor index');
  assert.equal(receiver.messageCount,0,'SECONDARY source constructor count');
  assert.equal(receiver.connectionAttempts,0,'SECONDARY source constructor attempts');
  receiver.connectAndSubscribe();
  socketPresence.push(Boolean(receiver.ws));
  assert.equal(receiver.connectionAttempts,1,'SECONDARY source attempt');
  for(const event of ['open','message','close'])assert.equal(typeof receiver.ws.listeners.get(event),'function','SECONDARY bound listener '+event);
  receiver.ws.listeners.get('open')();
  receiver.onOpen();
  assert.equal((handlers.get('markets:changed')||[]).length,1,'SECONDARY source one market listener');
  emitter.emit('markets:changed',{added:['__proto__','4','toString','11','NEW','LAST'].map(symbol=>({symbol}))});
  if(index===0)assert.equal(Object.hasOwn(receiver.topics.ticker,'__proto__'),false,'SECONDARY source inherited setter');
  const frame='{"arg":{"channel":"ticker"},"data":[{"instId":"BTC","bidPr":"11.25","askPr":"22.5","lastPr":"33.75","markPrice":"44.5","indexPrice":"55.25","change24h":"0.665","fundingRate":"0.00775","nextFundingTime":"88000","baseVolume":"99.5","quoteVolume":"110.75"}]}';
  receiver.ws.listeners.get('message')({data:frame});
  assert.equal(store.tickers[0].last,33.75,'SECONDARY source bound ticker');
  assert.equal(receiver.messageCount,1,'SECONDARY source counted ticker');
  monotonic=137.25;
  receiver.ws.listeners.get('message')({data:'pong'});
  assert.equal(store.latency,7,'SECONDARY source bound pong');
  receiver.onMessage({data:Buffer.from('pong')});
  assert.equal(store.latency,7,'SECONDARY source binary pong');
  assert.equal(receiver.messageCount,3,'SECONDARY source final count');
  cases.push({input:{name,testnet,index},output:encode({symbols:Object.keys(receiver.topics.ticker),sent:parent.sent,count:receiver.messageCount,latency:store.latency,ticker:store.tickers[0]})});
}
const lifetime=new Source({isDisposed:false},0);
const utfStore=new Store();utfStore.update({tickers:[structuredClone(seed)]});
const utfReceiver=new Source({store:utfStore,isDisposed:false},0);
const utfFrame=String.raw`{"arg":{"channel":"ticker"},"data":[{"instId":"BTC","lastPr":"93"},{"instId":"\ud800","lastPr":"1"}]}`;
utfReceiver.onMessage({data:utfFrame});
assert.equal(JSON.parse(utfFrame).data[1].instId.charCodeAt(0),0xd800,'SECONDARY source lone surrogate accepted');
assert.equal(utfStore.tickers[0].last,93,'SECONDARY source first ASCII row commits');
process.stdout.write(JSON.stringify({cases,synchronousDialHasSocket:socketPresence.length===18&&socketPresence.every(Boolean),retainsParent:Object.hasOwn(lifetime,'parent')&&lifetime.parent.isDisposed===false,utf16FirstRow:utfStore.tickers[0].last})+'\n');
