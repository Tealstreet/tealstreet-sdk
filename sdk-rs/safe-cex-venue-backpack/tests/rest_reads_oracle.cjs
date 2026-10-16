process.env.TZ='UTC';
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const {createRequire}=require('node:module'),actualRequire=createRequire(path.join(root,'packages/safe-cex/package.json'));
function unused(id){return new Proxy(function(){throw Error('unused '+id);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+id);}});}
function run(file,req,extra={}){const exports={};const c=ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(c.diagnostics.length,0);vm.runInNewContext(c.outputText,{exports,require:req,...extra});return exports;}
const types=run('packages/safe-cex/src/types.ts',id=>id==='@tealstreet/logging/src/LogSeverity'?run('packages/logging/src/LogSeverity.ts',unused):unused(id));
const constants=run('packages/safe-cex/src/exchanges/backpack/backpack.types.ts',unused);
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function encode(v){if(v===undefined)return {u:true};if(v===null)return null;if(typeof v==='number')return {n:bits(v)};if(typeof v==='string')return {s:units(v)};if(Array.isArray(v))return Array.from({length:v.length},(_,i)=>i in v?encode(v[i]):{hole:true});if(typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));return v;}
let current;
class Base {constructor(name,options,store){this.store=store;this.options=options;}createXhrWithSecondaryKey(){return {};}}
class Socket {}
const Exchange=run('packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts',id=>{if(id==='../../types')return types;if(id==='./backpack.types')return constants;if(id==='../base')return {BaseExchange:Base};if(id==='./backpack.api')return {createAPI(){}};if(id.includes('backpack.ws-'))return {BackpackPublicWebSocket:Socket,BackpackPrivateWebSocket:Socket};if(id==='axios-rate-limit')return x=>x;if(id==='lodash/uniqBy')return actualRequire(id);if(id==='../../utils/virtualClock')return {virtualClock:{getCurrentTimeMs(){current.calls.push('clock');return 1700000000123;}}};if(id==='../../utils/createPositionId')return {createPositionId(symbol,side,mode){current.calls.push('id:'+side);return symbol+'|'+side+'|'+mode;}};return unused(id);}).BackpackExchange;
function fixtures(i){
 const symbol='BTC_USDC',odd='λ\ud800_X_Y';
 const markets=[{symbol,marketType:'PERP',imfFunction:{base:'0.08'},quantityIncrement:'0.125',priceIncrement:'0.75',minQuantity:'2',maxQuantity:'99',minNotional:'7',maxNotional:'987',filters:{price:{minPrice:'1',maxPrice:'800'}}},{symbol:odd,visible:null,marketType:'SPOT',filters:{quantity:{stepSize:'0.007',minQuantity:'0.3',maxQuantity:'17'},price:{tickSize:'0.2'}}},{symbol:'HIDDEN_X',visible:false},{symbol:'invalid'}];
 const tickers=[{symbol,lastPrice:'12.5',priceChangePercent:'0.125',volume:'8.25',quoteVolume:'105'},{symbol:odd,lastPrice:'-0',priceChangePercent:'bad',volume:'Infinity',quoteVolume:'4tail'}];
 const marks=[{symbol,markPrice:'111',indexPrice:'222',fundingRate:'0.003',nextFundingTimestamp:123},{symbol,markPrice:'13.75',indexPrice:'14.5',fundingRate:'-0.002',nextFundingTimestamp:1700000000999}];
 const klines=[{start:'2025-12-15 13:28:00',open:'11',high:'22',low:'3',close:'17',volume:'99'},{start:'invalid',open:'-0',high:'bad',low:'Infinity',close:'4tail',volume:'0'}];
 const depth={bids:[['11','2'],['-0','bad']],asks:[['22','3'],['Infinity','4tail']]};
 const orders=[{id:'old1',symbol,side:'Bid',orderType:'Limit',triggerPrice:'15',triggerQuantity:'2',quantity:'9',price:'14',triggerBy:null},{id:'old2',symbol,side:'Ask',orderType:'Market',triggerPrice:'4',quantity:'3'},{id:'keep',symbol:odd,triggerPrice:null}];
 const positions=[{symbol,netQuantity:'-2',entryPrice:'12',netExposureNotional:'0',pnlUnrealized:'3.75',estLiquidationPrice:'bad'},{symbol:'NAN_X',netQuantity:'bad',entryPrice:'7',netExposureNotional:'88',pnlUnrealized:'-0',estLiquidationPrice:'9'},{symbol,netQuantity:'0'},{symbol,netQuantity:'-3',entryPrice:'77'}];
 const storeMarkets=[{id:symbol,symbol,active:true,isSpot:false},{id:odd,symbol:odd,active:true,isSpot:false},{id:'SPOT',symbol:'SPOT',active:true,isSpot:true},{id:'OFF',symbol:'OFF',active:false,isSpot:false},{id:odd,symbol:odd,active:true,isSpot:false}];
 const valid={symbol,orderId:'o1',timestamp:'1700000001',price:'7.5',quantity:'2.5',side:'Bid',fee:'-0.125',feeSymbol:'USDC',tradeId:123};
 const fills=[valid,{...valid,tradeId:'λ\ud800',side:'Ask',timestamp:'2025-12-15T13:28:00Z',fee:'bad',feeSymbol:''},{...valid,symbol:'OTHER'},{...valid,orderId:'other'},{...valid,timestamp:'bad'},{...valid,timestamp:'0'},{...valid,timestamp:'1700000001123'},{...valid,timestamp:'-1'},{...valid,price:'0'},{...valid,quantity:'Infinity'},{...valid,side:'Other'}];
 if(i===1){markets[0].quantityIncrement='';markets[0].imfFunction.base='0';klines.splice(0);tickers.splice(0);depth.bids.splice(0);fills.splice(0);orders.splice(0);positions.splice(0);}
 if(i===2){markets[0].imfFunction.base='2.2204460492503126e-16';markets[0].priceIncrement='-0';orders[0].triggerBy='LastPrice';}
 if(i===3){markets[0].marketType='SPOT';markets[0].imfFunction.base='1e-320';orders[0].triggerQuantity='';}
 if(i===11)delete marks[0];
 return {markets,tickers,marks,klines,depth,orders,positions,storeMarkets,fills};
}
const families=['markets','tickers','ohlcv','depth','fills','orders','repair','positions'];
const modes=['none','empty','special','fallback','throw','reject','logger','log','marksReject','public','missing','notArray'];
(async()=>{const rows=[];for(const family of families)for(let i=0;i<12;i++){
 const c={family,i,mode:modes[i],calls:[],requests:[],logs:[],created:[],mapped:[],error:{message:i===8?'endpoint not found':'failedλ',toString(){return this.message;}},logError:{message:'loggerFailed',toString(){return this.message;}}};current=c;const f=fixtures(i),e=new Exchange('backpack',{}, {markets:f.storeMarkets,marketsBySymbol:{BTC_USDC:f.storeMarkets[0]}});e.isPublicDataOnly=i===9;e.accountLeverageLimit=8.5;
 e.log=(message,severity)=>{c.calls.push('log');if(i===6||i===7)throw c.logError;c.logs.push([encode(message),severity]);};
 e.getPositionMode=async()=>{c.calls.push('mode');return types.PositionMode.Hedged;};
 e.determineTriggerOrderType=(symbol,side,price)=>{c.calls.push('trigger:'+side);return price>10?types.OrderType.TakeProfit:types.OrderType.StopLoss;};
 e.getTriggerBy=kind=>{c.calls.push('by:'+kind);return kind==='tp'?'MarkPrice':'LastPrice';};
 const repair=e.fixTriggerOrdersWithoutTriggerBy;
 e.mapOrder=o=>{c.calls.push('map');c.mapped.push(o);return o;};
 e.fixTriggerOrdersWithoutTriggerBy=async orders=>{c.calls.push('repair');await repair(orders);};
 const invoke=(method,url,opts)=>{c.calls.push(method+':'+url);c.requests.push({method,url,params:encode(method==='get'?opts?.params:undefined),data:encode(method==='get'?undefined:method==='delete'?opts?.data:opts)});if(i===4)throw c.error;let data;
   if(method==='post'){data={id:'new-'+opts.triggerPrice,...opts};c.created.push(data);}else if(method==='delete'){data={};}else data=({'/api/v1/markets':f.markets,'/api/v1/tickers':f.tickers,'/api/v1/markPrices':f.marks,'/api/v1/klines':f.klines,'/api/v1/depth':f.depth,'/wapi/v1/history/fills':f.fills,'/api/v1/orders':f.orders,'/api/v1/position':f.positions})[url];
   if(i===11&&method==='get'&&['ohlcv','fills'].includes(family))data={bad:true};
   if(i===5||(i===8&&(url==='/api/v1/markPrices'||url==='/api/v1/position')))return Promise.reject(c.error);
   return Promise.resolve({data});};e.xhr={get:(url,opts)=>invoke('get',url,opts),delete:(url,opts)=>invoke('delete',url,opts),post:(url,opts)=>invoke('post',url,opts)};
 const inputs=encode(f);
 const opts={symbol:i===10?'MISSING':i===9?'': 'BTC_USDC',interval:'1m',from:[-0,1700000001.25,NaN,-1234,Infinity][i%5],to:[1700000001123.75,-Infinity,0,undefined,12][i%5],limit:[undefined,-2,2000,NaN,0][i%5],orderId:i%2===0?'o1':undefined};
 let promise;if(family==='markets')promise=e.fetchMarkets();if(family==='tickers')promise=e.fetchTickers();if(family==='ohlcv')promise=e.fetchOHLCV(opts);if(family==='depth')promise=e.fetchOrderBook(opts.symbol);if(family==='fills')promise=e.fetchHistoricalFills(opts);if(family==='orders')promise=e.fetchOrders(i%3===0?['BTC_USDC']:i%3===1?[]:undefined);if(family==='repair')promise=repair(f.orders);if(family==='positions')promise=e.fetchPositions();const before=[...c.calls];let value,outcome='ok',sameError=null;
 try{value=await promise;}catch(error){outcome='error';sameError=error===c.error?'original':error===c.logError?'logger':'generated';}
 const alias=family==='markets'&&value?value.map(x=>f.markets.includes(x.info)):family==='tickers'&&value?value.map(x=>!f.tickers.includes(x.info)&&!f.marks.includes(x.info)):null;
 rows.push({family,i,mode:c.mode,inputs,opts:encode(opts),before,calls:c.calls,requests:c.requests,logs:c.logs,outcome,sameError,value:encode(value),orders:encode(f.orders),alias,created:encode(c.created),mappedAlias:family==='orders'&&value?value.map((v,j)=>v===c.mapped[j]):null,createdAlias:family==='repair'?c.created.map(v=>f.orders.includes(v)):null});
 }
 const witnesses=[];
 for(const kind of ['imf','quote','shrink','syncNotFound']){
  const c={calls:[],logs:[]};current=c;const f=fixtures(0),e=new Exchange('backpack',{}, {markets:f.storeMarkets,marketsBySymbol:{BTC_USDC:f.storeMarkets[0]}});e.accountLeverageLimit=8.5;
  e.log=(s,severity)=>{c.calls.push('log');c.logs.push([encode(s),severity]);};
  e.getPositionMode=async()=>{c.calls.push('mode');return types.PositionMode.Hedged;};
  let reads=0,value,before;
  if(kind==='imf'||kind==='quote'){
   const m=f.markets[0];
   if(kind==='imf')Object.defineProperty(m,'imfFunction',{get(){reads++;return {base:reads===1?'0.01':'0.04'};}});
   else Object.defineProperty(m,'symbol',{get(){reads++;return reads===1?'BTC_USDC':'BTC';}});
   e.xhr={get(){c.calls.push('get:/api/v1/markets');return Promise.resolve({data:[m]});}};
   const p=e.fetchMarkets();before=[...c.calls];const [r]=await p;value={leverage:r.limits.leverage.max,quote:r.quote,base:r.base,sameInfo:r.info===m};
  }else if(kind==='shrink'){
   const orders=[f.orders[0]],order=orders[0];Object.defineProperty(order,'id',{get(){if(++reads===4)orders.length=0;return 'old1';}});
   e.determineTriggerOrderType=(s,side)=>{c.calls.push('trigger:'+side);return types.OrderType.TakeProfit;};e.getTriggerBy=kind=>{c.calls.push('by:'+kind);return 'MarkPrice';};
   e.xhr={delete(){c.calls.push('delete:/api/v1/order');return Promise.resolve({});},post(url,req){c.calls.push('post:/api/v1/order');return Promise.resolve({data:{id:'new-'+req.triggerPrice,...req}});}};
   const p=e.fixTriggerOrdersWithoutTriggerBy(orders);before=[...c.calls];await p;value={length:orders.length,id:orders[0].id};
  }else{
   e.xhr={get(){c.calls.push('get:/api/v1/position');throw {message:'endpoint not found',toString(){return this.message;}};}};
   const p=e.fetchPositions();before=[...c.calls];value=(await p).length;
  }
  witnesses.push({kind,reads,before,calls:c.calls,value:encode(value)});
 }
 process.stdout.write(JSON.stringify({rows,witnesses}));})().catch(e=>{console.error(e);process.exitCode=1;});
