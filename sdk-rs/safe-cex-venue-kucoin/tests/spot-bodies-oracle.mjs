import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const read=p=>ts.createSourceFile(p,fs.readFileSync(path.join(root,'packages/safe-cex/src',p),'utf8'),ts.ScriptTarget.Latest,true);
const source=read('exchanges/kucoin/kucoinspot.exchange.ts'),cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='KucoinSpotExchange');
const names={start:'start',tick:'tick',balance:'fetchBalance',orders:'fetchOrders',place:'placeOrder',update:'updateOrder',fills:'fetchHistoricalFills',ohlcv:'fetchOHLCV'};
const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
function declaration(src,name){const n=src.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(src)===name));assert.ok(n,name);return n.getText(src);}
function module(text,context={}){const m={exports:{}};vm.runInNewContext(transpile(text),{...context,module:m,exports:m.exports});return m.exports;}
const kt=read('exchanges/kucoin/kucoinspot.types.ts'),{KUCOIN_SPOT_ENDPOINTS,KUCOIN_SPOT_INTERVAL}=module(['KUCOIN_SPOT_ENDPOINTS','KUCOIN_SPOT_INTERVAL'].map(n=>declaration(kt,n)).join('\n'));
const types=read('types.ts'),enums=module(types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderType','OrderSide','OrderTimeInForce'].includes(n.name.text)).map(n=>n.getText(types)).join('\n'));
const math=read('utils/safe-math.ts'),{adjust}=module([declaration(math,'POW10_CACHE'),math.statements.find(ts.isForStatement).getText(math),...['pow10','afterDecimal','adjust'].map(n=>declaration(math,n))].join('\n'));
const {omitUndefined}=module(declaration(read('utils/omitUndefined.ts'),'omitUndefined'),{omitBy:require('lodash/omitBy')});
class FixedDate extends Date{constructor(...args){super(...(args.length?args:[5_000_000]));}static now(){return 5_000_000;}}
const dayModule={exports:{}};vm.runInNewContext(fs.readFileSync(require.resolve('dayjs'),'utf8'),{module:dayModule,exports:dayModule.exports,Date:FixedDate});const dayjs=dayModule.exports;
const pack=v=>{if(v===undefined)return{u:true};if(typeof v==='number'){if(Number.isNaN(v))return{n:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return{n:b.toString('hex')};}if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return{o:Object.entries(v).map(([k,v])=>[k,pack(v)])};return v;};
async function fixture(method,c){
 const events=[];let disposed=c==='disposed',closes=0,timerCallback,originalTimerReturn;
 const original=Object.assign(new TypeError('DOWN'),{code:'READ',ids:[3,7],response:{data:{msg:'VENUE'}}});
 const markets=[{id:'M0'},{id:'M1'}],tickers=[{id:'T0'}],orders=[{id:'OLD'}],positions=[{id:'POS'}],balance={total:123};
 const store={loaded:{seed:true},markets,tickers,orders,balance,marketsBySymbol:c==='missing'?{}:{SYM:{id:'API',precision:{amount:0.1,price:0.5}}},update(p){events.push(['update',pack(p)]);Object.assign(this,p);},addOrUpdatePositionsFull(p){events.push(['positions',pack(p)]);}};
 const receiver={store,get isDisposed(){return disposed;},isPublicDataOnly:c==='public'||c==='external-public',
  initExternalPublicData:async()=>{events.push(['external']);if(c==='reject')throw original;return c==='external'||c==='external-public';},
  log:msg=>events.push(['log',pack(msg)]),emitAndLogError:msg=>events.push(['emit',pack(msg)]),
  publicWebsocket:{connectAndSubscribe(){events.push(['publicConnect']);}},privateWebsocket:{connectAndSubscribe(){events.push(['privateConnect']);}},
  getTickInterval:()=>77,getUsdPrice:currency=>{if(c==='close')throw original;return currency==='ETH'||currency==='__proto__'?10:1;},
  fetchActiveOrderSymbols:async()=>{events.push(['activeSymbols']);return c==='empty'?[]:['SYM'];},mapOrder:o=>o,
  resolveCallerClientOrderId:opts=>{events.push(['resolveClientId',pack(opts)]);return c==='market'?undefined:'CLIENT';},
 };
 let raw=[];
 if(method==='balance')raw=c==='empty'?[]:[{currency:'ETH',available:'2',holds:'0.5',balance:'2.5'},{currency:'USDT',available:'3',holds:'1',balance:'4'}];
 if(method==='balance'&&c==='prototype')raw[0].currency='__proto__';
 if(method==='fills')raw=[c==='defaults'?{symbol:'SYM',orderId:'F',feeCurrency:'USDT'}:{createdAt:'1234x',symbol:'SYM',orderId:'F',price:'12.5',size:'2',side:'sell',fee:'0.25',feeCurrency:'USDT'}];
 if(method==='ohlcv')raw=[['300','3','4','5','2','6'],['100','1','2','3','0.5','7.5']];
 if(method==='balance'&&c==='close')raw[Symbol.iterator]=()=>{let i=0;return{next:()=>i<raw.length?{value:raw[i++],done:false}:{done:true},return:()=>{closes++;return{done:true};}};};
 const request=async(kind,...args)=>{
  events.push([kind,...args.map(pack)]);if(c==='reject')throw original;
  const page=args[1]?.params?.pageNum,bad=c==='bad'||c==='bad-later'&&page===2;
  let body=raw;
  if(method==='orders'){const count=c==='non-array'?0:c==='missing-total'&&page===1?50:1;body={items:c==='non-array'?'NO':Array.from({length:count},(_,i)=>({id:`P${page}-${i}`}))};if(c!=='missing-total')body.totalPage=c==='max-pages'?101:c==='paged'||c==='bad-later'?2:1;}
  if(method==='place')body=c==='empty-id'?{}:{orderId:'PLACED'};
  if(method==='update')body=c==='fallback'?{}:{newOrderId:'REPLACED'};
  if(method==='fills')body={items:raw};
  return{data:{code:bad?'400000':'200000',msg:bad?'DENIED':'',data:body}};
 };
 receiver.xhr={get:(...args)=>request('get',...args)};receiver.unlimitedXHR={post:(...args)=>request('post',...args)};
 const context={...enums,KUCOIN_SPOT_ENDPOINTS,KUCOIN_SPOT_INTERVAL,adjust,omitUndefined,dayjs,Date:FixedDate,uuid:()=> 'UUID',loop:(cb,interval)=>{timerCallback=cb;events.push(['loop',pack(interval)]);}};
 vm.runInNewContext(transpile(['KUCOIN_SPOT_OPEN_ORDERS_PAGE_SIZE','KUCOIN_SPOT_OPEN_ORDERS_MAX_PAGES'].map(n=>declaration(source,n)).join('\n')),context);
 const selected=names[method];
 for(const name of Object.values(names)){const member=cls.members.find(n=>n.name?.getText(source)===name);assert.ok(member,name);receiver[name]=vm.runInNewContext(transpile(`(function(){return (${member.initializer.getText(source)});})`),context).call(receiver);}
 const delegates={fetchMarkets:async()=>{events.push(['fetchMarkets']);if(c==='dispose-markets')disposed=true;return markets;},fetchTickers:async()=>{events.push(['fetchTickers']);return tickers;},fetchPositions:async()=>{events.push(['fetchPositions']);if(c==='dispose-positions')disposed=true;return positions;},fetchOrders:async()=>{events.push(['fetchOrders']);if(c==='reject')throw original;return orders;},fetchBalance:async()=>{events.push(['fetchBalance']);if(c==='dispose-all')disposed=true;return balance;},tick:async()=>{events.push(['tick']);if(c==='dispose-tick')disposed=true;}};
 for(const[name,binding]of Object.entries(delegates))if(name!==selected)receiver[name]=binding;
 let opts={symbol:c==='missing'&&method==='fills'?'':'SYM'};
 if(method==='place')Object.assign(opts,{type:c==='unsupported'?'stop_loss':c==='market'?'market':'limit',side:'sell',amount:2.37,price:12.7,timeInForce:c==='fok'?'FillOrKill':c==='post'?'PostOnly':'ImmediateOrCancel'},c==='attached'?{stopLoss:1}:{});
 if(method==='update')opts={order:{symbol:'SYM',id:'ORIGINAL'},update:c==='undefined'?{price:undefined}:{price:12.7,amount:2.37}};
 if(method==='fills'&&c!=='defaults')Object.assign(opts,{orderId:'ORD',limit:200,from:1000,to:2000});
 if(method==='ohlcv'){opts.interval='1m';if(c==='limit')Object.assign(opts,{limit:2000,to:120_000_000});if(c==='from'||c==='zero-limit')opts.from=1_000_000;if(c==='zero-limit')opts.limit=0;}
 const args=method==='orders'?[c==='discovery'||c==='empty'?undefined:['SYM']]:['place','update','fills','ohlcv'].includes(method)?[opts]:[];
 let result;
 try{const value=await receiver[selected](...args);if(method==='balance'&&c==='prototype'){assert.deepEqual(Object.keys(value.currencies),['USDT']);assert.deepEqual(Object.keys(value.coins),['USDT']);assert.equal(Object.getPrototypeOf(value.currencies).info,raw[0]);assert.equal(Object.getPrototypeOf(value.coins).notional,25);}result={ok:pack(value)};}
 catch(e){if(c==='reject'){assert.equal(e,original);assert.equal(e.name,'TypeError');assert.equal(e.message,'DOWN');}else assert.equal(e.name,'Error');result={error:{name:e.name,message:e.message,fresh:e!==original,code:e.code??null,ids:e.ids??null}};}
 let timer;
 if(timerCallback){receiver.tick=()=>{events.push(['timerTick']);return originalTimerReturn=Promise.resolve('TIMER');};const returned=timerCallback();assert.equal(returned,originalTimerReturn);timer=await returned;}
 return{method,case:c,result,timer:pack(timer),closes,events};
}
let input='';for await(const part of process.stdin)input+=part;
const outputs=[];for(const c of JSON.parse(input).cases)outputs.push(await fixture(c.method,c.case));
assert.equal(outputs.length,55);process.stdout.write(JSON.stringify(outputs));
