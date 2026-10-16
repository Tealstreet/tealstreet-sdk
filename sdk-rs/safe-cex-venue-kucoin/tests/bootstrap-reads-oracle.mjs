import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const read=p=>ts.createSourceFile(p,fs.readFileSync(path.join(root,'packages/safe-cex/src',p),'utf8'),ts.ScriptTarget.Latest,true);
const source=read('exchanges/kucoin/kucoin.exchange.ts'),cls=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='KucoinExchange');
const names={start:'start',tick:'tick',meta:'fetchPositionMetadata',positions:'fetchPositions',open:'fetchOpenPositions',markets:'fetchMarkets',tickers:'fetchTickers',ohlcv:'fetchOHLCV'};
const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
function declaration(src,name){const n=src.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(src)===name));assert.ok(n,name);return n.getText(src);}
function module(text,context={}){const m={exports:{}};vm.runInNewContext(transpile(text),{...context,module:m,exports:m.exports});return m.exports;}
const kt=read('exchanges/kucoin/kucoin.types.ts'),{ENDPOINTS,INTERVAL}=module(['ENDPOINTS','INTERVAL'].map(n=>declaration(kt,n)).join('\n'));
const types=read('types.ts'),enums=module(types.statements.filter(n=>ts.isEnumDeclaration(n)&&['MarginType','PositionMode','PositionSide'].includes(n.name.text)).map(n=>n.getText(types)).join('\n'));
const math=read('utils/safe-math.ts'),{multiply}=module([declaration(math,'POW10_CACHE'),math.statements.find(ts.isForStatement).getText(math),...['pow10','afterDecimal','multiply'].map(n=>declaration(math,n))].join('\n'));
const {createPositionId}=module(declaration(read('utils/createPositionId.ts'),'createPositionId'),enums);
const flatten=require('lodash/flatten'),mapSeries=require('p-iteration').mapSeries;
class FixedDate extends Date{constructor(...args){super(...(args.length?args:[5_000_000]));}static now(){return 5_000_000;}}
const dayModule={exports:{}};vm.runInNewContext(fs.readFileSync(require.resolve('dayjs'),'utf8'),{module:dayModule,exports:dayModule.exports,Date:FixedDate});const dayjs=dayModule.exports;
const pack=v=>{if(v===undefined)return{u:true};if(typeof v==='number'){if(Number.isNaN(v))return{n:'nan'};const b=Buffer.alloc(8);b.writeDoubleBE(v);return{n:b.toString('hex')};}if(Array.isArray(v))return v.map(pack);if(v&&typeof v==='object')return{o:Object.entries(v).map(([k,v])=>[k,pack(v)])};return v;};
async function fixture(method,c){
 const events=[];let disposed=c==='disposed',afterPositions=false,timerCallback,originalTimerReturn;
 const original=Object.assign(new TypeError(c==='no-message'?'':'DOWN'),{code:'READ',ids:[3,7],response:{data:{msg:c==='no-message'?'':'VENUE'}}});
 const markets=[{id:'M0'},{id:'M1'}],tickers=[{id:'T0'}],orders=[{id:'O0'}],balance={total:123},positions=[{id:'REAL',symbol:'SYM',contracts:2},{id:'OTHER',symbol:'OTHER',contracts:3}];
 const receiver={store:{loaded:{seed:true},markets:[],marketsBySymbol:{SYM:{id:'API1'},OTHER:{id:'API2'}},marketsById:{API1:{id:'API1',symbol:'SYM'}}},apiMarginCoin:'USDT'};
 Object.defineProperty(receiver,'isDisposed',{get(){if(c==='dispose-positions-once'&&afterPositions){afterPositions=false;return true;}return disposed;}});
 receiver.isPublicDataOnly=c==='public'||c==='external-public';
 let snapshots=[];Object.defineProperty(receiver,'snapshotWebsockets',{get:()=>snapshots,set:v=>{events.push(['snapshots',pack(v)]);snapshots=v;}});
 const connections=[{id:'CONN0'},{id:'CONN1'}],sockets=connections.map((connection,i)=>{const ws={id:`WS${i}`};Object.defineProperty(ws,'connectAndSubscribe',{value:function(){assert.equal(this,ws);events.push(['snap',this.id]);return connection;}});return ws;});
 sockets.forEach=function(cb){for(let i=0;i<2;i++)assert.equal(cb(sockets[i]),connections[i]);};
 receiver.publicWebsocket={connectAndSubscribe(){events.push(['public']);}};receiver.privateWebsocket={connectAndSubscribe(){events.push(['private']);}};
 receiver.initExternalPublicData=async()=>{events.push(['init']);if(c==='reject'||c==='no-message')throw original;return c==='external'||c==='external-public';};
 receiver.store.update=payload=>{events.push(['update',pack(payload)]);Object.assign(receiver.store,payload);};
 receiver.store.addOrUpdatePositionsFull=value=>{assert.equal(value,positions);events.push(['addFull',pack(value)]);};
 receiver.log=value=>events.push(['log',pack(value)]);receiver.emitAndLogError=(...args)=>events.push(['emit',...args.map(pack)]);
 receiver.getTickInterval=()=>250;
 receiver.toInternalSymbol=value=>value==='API1'?'SYM':value==='API2'?'OTHER':'THIRD';receiver.contractSizeFor=()=>0.1;
 const validPosition={symbol:'API1',currentQty:c==='nan'?'oops':'-4',positionSide:c==='nan'?'':'LONG',markPrice:'10',avgEntryPrice:'1.5',leverage:'7x',unrealisedPnl:'-2.5',liquidationPrice:'0.8',marginMode:'CROSS',positionMode:'0'};
 let raw;
 if(method==='open')raw=c==='non-array'?{}:[{currentQty:'0'},validPosition];
 else if(method==='markets')raw=[c==='defaults'?{symbol:'API1',quoteCoin:'USDT',rootSymbol:'ETH',status:'ONLINE',isQuanto:true}:{symbol:'API1',settleCurrency:'USDT',baseCurrency:'XBT',status:'Open',lotSize:'2',tickSize:'0.5',minOrderQty:'3',maxOrderQty:'99',maxLeverage:'8',multiplier:'0.1',isQuanto:true},{symbol:'API2',settleCurrency:'USDT',baseCurrency:'ETH',status:'ONLINE',expireDate:'FUTURE'},{symbol:'API3',settleCurrency:'USDC',baseCurrency:'ETH',status:'Open'}];
 else if(method==='tickers'){const valid={symbol:'API1',price:'8.5',bestBidPrice:'8',bestAskPrice:'9',markPrice:'',indexPrice:'8.25',volumeOf24h:'',volume24h:'6',size:'2',turnoverOf24h:'77',turnover24h:'21'},unknown={symbol:'NO'};raw=c==='object'?{main:valid,missing:unknown}:c==='non-object'?'no':[valid,unknown];}
 else if(method==='ohlcv')raw=[['1234500','1','3','0.5','2','7.5']];
 else raw=[];
 receiver.xhr={get:async(...args)=>{
  events.push(['get',...args.map(pack)]);if(c==='reject')throw original;
  const endpoint=args[0],margin=endpoint===ENDPOINTS.GET_MARGIN_MODE,mode=endpoint===ENDPOINTS.GET_POSITION_MODE,leverage=endpoint===ENDPOINTS.POSITION_LEVERAGE;
  const bad=c==='bad'||c==='bad-margin'&&margin||c==='bad-mode'&&mode||c==='bad-leverage'&&leverage;
  const body=margin?{marginMode:c==='isolated'?'ISOLATED':'CROSS'}:mode?{positionMode:c==='hedged'?'1':'0'}:leverage?{leverage:'7'}:raw;
  return{data:{code:bad?'400000':'200000',msg:bad?'DENIED':'',data:body}};
 }};
 const context={...enums,ENDPOINTS,INTERVAL,multiply,flatten,mapSeries,createPositionId,dayjs,
  loop:(cb,interval)=>{timerCallback=cb;events.push(['loop',pack(interval)]);},
  createKucoinSnapshotWebsockets:(parent,count)=>{assert.equal(parent,receiver);events.push(['createSnapshots',pack(count)]);return sockets;},
 };
 const selected=names[method];
 for(const name of [...Object.values(names),'parsePositionMode','parseMarginType','parseRowPositionMode']){const member=cls.members.find(n=>n.name?.getText(source)===name);assert.ok(member,name);receiver[name]=vm.runInNewContext(transpile(`(function(){return (${member.initializer.getText(source)});})`),context).call(receiver);}
 const metadata={symbol:'MISSING',side:'short',leverage:3,positionMode:'hedged',marginType:'cross',id:'OVERRIDE',contracts:99,tag:'KEPT'};
 const delegates={
  fetchMarkets:async()=>{events.push(['fetchMarkets']);if(c==='dispose-markets')disposed=true;return markets;},
  fetchTickers:async()=>{events.push(['fetchTickers']);return tickers;},
  fetchOrders:async()=>{events.push(['fetchOrders']);if(c==='dispose-orders')disposed=true;return orders;},
  fetchPositions:async()=>{events.push(['fetchPositions']);afterPositions=true;if(method==='tick'&&c==='reject')throw original;return positions;},
  fetchBalance:async()=>{events.push(['fetchBalance']);if(c==='dispose-balance')disposed=true;return balance;},
  fetchOpenPositions:async()=>{events.push(['fetchOpen']);return positions;},
  fetchPositionMetadata:async symbols=>{events.push(['fetchMetadata',pack(symbols)]);return symbols.map(()=>metadata);},
  tick:async()=>{events.push(['tick']);},
 };
 for(const[name,binding]of Object.entries(delegates))if(name!==selected)receiver[name]=binding;
 const symbols=method==='positions'?c==='no-symbols'?undefined:c==='empty'?[]:c==='duplicate'?['SYM','MISSING','MISSING']:['SYM','MISSING']:c==='missing'?['MISSING']:['SYM','OTHER'];
 if(c==='raw-predicate'){symbols.includes=function(symbol){return Array.prototype.includes.call(this,symbol)?'INCLUDED':'';};positions.filter=function(predicate){assert.equal(predicate(positions[0]),'INCLUDED');assert.equal(predicate(positions[1]),'');return[positions[0]];};}
 const opts={symbol:c==='unknown'?'UNKNOWN':'SYM',interval:'1m'};if(c==='explicit')Object.assign(opts,{limit:2000,to:120_000_000});if(c==='from'||c==='zero-limit')opts.from=1_000_000;if(c==='zero-limit')opts.limit=0;
 const args=method==='tick'?[c==='skip-balance'?1:undefined]:method==='meta'||method==='positions'?[symbols]:method==='ohlcv'?[opts]:[];
 let result;
 try{result={ok:pack(await receiver[selected](...args))};}
 catch(e){assert.equal(e,original,`${method}/${c}: ${e.stack}`);result={error:{name:e.name,message:e.message,fresh:false,code:e.code,ids:e.ids}};}
 let timer;
 if(timerCallback){receiver.tick=count=>{events.push(['timerTick',pack(count)]);return originalTimerReturn=Promise.resolve('TIMER');};const returned=timerCallback();assert.equal(returned,originalTimerReturn);timer=await returned;}
 return{method,case:c,result,timer:pack(timer),events};
}
let input='';for await(const part of process.stdin)input+=part;
const outputs=[];for(const c of JSON.parse(input).cases)outputs.push(await fixture(c.method,c.case));
assert.equal(outputs.length,50);process.stdout.write(JSON.stringify(outputs));
