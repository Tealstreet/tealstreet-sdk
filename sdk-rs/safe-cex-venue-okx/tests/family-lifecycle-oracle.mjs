import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {ts} from '../../safe-cex-store/tests/source_loader.mjs';
const root=new URL('../../../packages/safe-cex/src/exchanges/okx/',import.meta.url);
const read=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const global=read('okx.exchange.ts'),owner=global.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXExchange');
const eu=read('okxeu.exchange.ts'),euOwner=eu.statements.find(ts.isClassDeclaration);
function member(ast,cls,name){const found=cls.members.filter(n=>n.name?.getText(ast)===name);assert.equal(found.length,1,name);return found[0].getText(ast)}
const body=member(global,owner,'start')+'\n'+member(global,owner,'tick');
const overlay=member(eu,euOwner,'fetchAccountMarketOverlay');
const demoCtor=file=>{const a=read(file),c=a.statements.find(ts.isClassDeclaration);return c.members.find(ts.isConstructorDeclaration).getText(a)};
const pack=v=>v===undefined?{u:true}:typeof v==='number'?{n:Number.isNaN(v)?'nan':(()=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex')})()}:typeof v==='string'?{s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:Array.isArray(v)?v.map(pack):v&&typeof v==='object'?{o:Object.entries(v).map(([k,v])=>[k,pack(v)])}:v;
let input='';for await(const chunk of process.stdin)input+=chunk;
const corpus=JSON.parse(input).cases;
async function fixture({method,case:c}){
 const events=[],err=Object.assign(new TypeError('boom'),{code:'LIFECYCLE',ids:[4,9]});let w,timer;
 const markets=[{id:'A',limits:{max:7}},{id:'A',limits:{max:9}},{id:'B',limits:{max:11}}],tickers=[{id:'T'}],orders=[{id:'O'}],positions=[{id:'P'}],balance={info:{retained:true}};
 const store={loaded:{old:true},balance,positions:c==='fallback'?null:positions,orders:c==='fallback'?null:orders};
 const effect=(n,...args)=>{events.push([n,...args.map(pack)]);if(c==='fail:'+n)throw err;};
 class Base {constructor(name,opts,given){if(method==='constructor')effect('super',name,opts,given);w=this;this.store=store;this.isDisposed=c==='disposed';this.isPublicDataOnly=c==='public';this.needsAccountMarketOverlay=c==='overlay'||c==='overlay-dispose';}createXhrWithSecondaryKey(opts,f){assert.equal(f,context.createAPI);effect('secondary',opts);return {tag:'xhr'};}}
 const socket=tag=>class{constructor(parent){assert.equal(parent,w,'source original socket parent');effect(tag);this.tag=tag}};
 const context={BaseExchange:Base,patternClientOrderIdRule:()=>{effect('fields');return {tag:'rule'}},BROKER_ID:'broker',uuid:()=>'',GRAFANA_ENDPOINTS_EVERY:{tag:'every'},GRAFANA_ENDPOINTS_PERIODIC:{tag:'periodic'},createAPI:()=>assert.fail('factory delegated'),OKXPublicWebsocket:socket('public'),OKXPrivateWebsocket:socket('private'),OKXBusinessWebsocket:socket('business'),loop:(cb,delay)=>{assert.equal(typeof cb,'function');effect('loop',delay);timer=cb}};
 const compile=text=>vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 let result,failed=false;
 try {
 if(method==='constructor'){
  const C=compile(owner.getText(global).replace(/^export\s+/,'')+"\nOKXExchange");
  w=new C(c.startsWith('eu')?'okxeudemo':c==='unknown'?'future-okx':'okx',{testnet:false,secret:'\ud800'}, {tag:'store'});
  assert.equal(typeof w.start,'function','source own arrow method');assert.ok(Object.hasOwn(w,'start'));assert.equal(typeof Object.getPrototypeOf(w).getInstrumentTypes,'function','source callable prototype');
  result={region:w.region,xhr:w.xhr,public:w.publicWebsocket,private:w.privateWebsocket,business:w.businessWebsocket};
 }else if(method==='demo'||method==='eu-demo'){
  const expected=method==='eu-demo'?'eu-base':'global-base';
  class Super{constructor(name,opts,given){effect(expected,name,opts,given);w=this;this.opts=opts;}}
  context[method==='eu-demo'?'OKXEUExchange':'OKXExchange']=Super;
  const C=compile(`class Selected extends ${method==='eu-demo'?'OKXEUExchange':'OKXExchange'} {isDemo=true;${demoCtor(method==='eu-demo'?'okxeudemo.exchange.ts':'okxdemo.exchange.ts')}}\nSelected`);
  const opts={testnet:c==='true',secret:'\ud800'};const original=opts.testnet;
  w=new C('name',opts,{tag:'store'});assert.equal(opts.testnet,original);assert.equal(Object.getPrototypeOf(C.prototype),Super.prototype,'source inherited prototype');assert.ok(Object.hasOwn(w,'isDemo'));if(method==='eu-demo'){context.OKXExchange=Super;const EU=compile(euOwner.getText(eu).replace(/^export\s+/,'')+'\nOKXEUExchange');assert.equal(typeof EU.prototype.getInstrumentTypes,'function','source EU callable prototype');assert.equal(Object.getPrototypeOf(EU.prototype),Super.prototype);}result={demo:w.isDemo,opts:w.opts};
 }else{
  const C=compile(`class Selected extends BaseExchange {${body}\n${overlay}}\nSelected`);w=new C();
  const fetch=(name,value)=>{effect(name);if(c==='dispose:'+name)w.isDisposed=true;if(c==='flip-public'&&name==='markets')w.isPublicDataOnly=true;return c==='reject:'+name?Promise.reject(err):Promise.resolve(value)};
  w.initExternalPublicData=()=>fetch('external',c==='external');
  w.fetchMarkets=()=>fetch('markets',c==='empty'?[]:markets);
  w.fetchTickersRaw=()=>fetch('raw',{raw:true});w.parseTickers=value=>{effect('parse',value);return tickers};
  w.loadAccountMarketOverlay=async cb=>{effect('overlay');assert.equal(typeof cb,'function');const p=cb();await p;if(c==='overlay-dispose')w.isDisposed=true;};
  w.fetchAccountMarketOverlay=method==='overlay'?w.fetchAccountMarketOverlay:()=>fetch('accountOverlay',undefined);
  w.log=m=>effect('log',m);w.getOrFetchPositionMode=()=>fetch('mode',undefined);
  w.fetchBalanceAndPositions=()=>fetch('balancePositions',{balance,positions});w.fetchOrders=()=>fetch('orders',orders);
  w.publicWebsocket={connectAndSubscribe:()=>effect('connectPublic')};w.privateWebsocket={connectAndSubscribe:()=>effect('connectPrivate')};w.businessWebsocket={connectAndSubscribe:()=>effect('connectBusiness')};
  store.update=p=>{effect('update',p);Object.assign(store,p)};
  w.formatBalanceUsedAvail=(info,p,o)=>{assert.equal(info,balance.info);assert.equal(p,c==='fallback'?p:positions);assert.equal(o,c==='fallback'?o:orders);effect('format',info,p,o);return fetch('formatted',{info:{new:true}})};
  w.getTickInterval=()=>{effect('interval');return -0};
  const tick=()=>{effect('tick');return Promise.resolve()};
  if(method==='start'){w.tick=tick;result=await w.start();}
  if(method==='tick'){result=await w.tick();if(c==='timer'){w.tick=tick;await timer();}}
  if(method==='overlay'){
    const f=await w.fetchAccountMarketOverlay();
    if(f===null)result=null;else{
      const shared=[{id:'B',limits:{old:true},shared:true},{id:'X'},{id:'A',limits:{old:true}}];
      const a=f(shared);assert.equal(a.length,2);assert.equal(a[0].limits,markets[2].limits);assert.equal(a[1].limits,markets[1].limits);assert.notEqual(a[0],shared[0]);
      markets[1].limits={max:13};result=[a,f(shared)];
    }
  }
 }
 }catch(e){assert.equal(e,err,'original source failure');failed=true;}
 for(let i=0;i<8;i++)await Promise.resolve();
 return {method,case:c,events,failed,result:pack(result)};
}
assert.ok(corpus.length>0,'source nonempty corpus');
process.stdout.write(JSON.stringify(await Promise.all(corpus.map(fixture))));
