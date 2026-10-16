import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {ts,loadSource} from '../../safe-cex-store/tests/source_loader.mjs';
import {loadSource as enumsSource} from './source-loader.mjs';
const require=createRequire(import.meta.url),root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const enums=enumsSource(new URL('types.ts',root));
function execute(url,imports={}){const m={exports:{}};vm.runInNewContext(ts.transpileModule(fs.readFileSync(url,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{module:m,exports:m.exports,require:n=>{assert.ok(Object.hasOwn(imports,n),n);return imports[n]}});return m.exports}
const constants=execute(new URL('exchanges/okx/okx.types.ts',root),{'../../types':enums}),math=execute(new URL('utils/safe-math.ts',root),{'@tealstreet/math/src/isNotNumber':loadSource(new URL('../../../packages/math/src/isNotNumber.ts',import.meta.url))});
const {inverseObj}=execute(new URL('utils/inverseObj.ts',root)),{omitUndefined}=execute(new URL('utils/omitUndefined.ts',root),{'lodash/omitBy':require('lodash/omitBy')}),{createPositionId}=execute(new URL('utils/createPositionId.ts',root),{'../types':enums}),{normalizeExecutionPositionSide}=execute(new URL('utils/normalizeExecutionPositionSide.ts',root),{'../types':enums});
const BigNumber=require('bignumber.js'),partition=require('lodash/partition');
const file=new URL('exchanges/okx/okx.exchange.ts',root),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='OKXExchange');
const names={single:'placeOrder',place:'placeOrders',orders:'mapOrders',positions:'mapPositions',algo:'formatAlgoOrder',create:'formatCreateOrder',trigger:'getReduceOnlyTriggerOrderType',fills:'fetchHistoricalFills'};
const bodies=Object.values(names).map(name=>{const found=cls.members.filter(n=>n.name?.getText(ast)===name);assert.equal(found.length,1,name);return found[0].getText(ast)}).join('\n');
const declarations=['getOKXTriggerPxType','parseOKXTriggerPxType'].map(name=>{const n=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(n,name);return n.getText(ast)}).join('\n');
const pack=v=>v===undefined?{u:true}:typeof v==='number'?{n:Number.isNaN(v)?'nan':(()=>{const b=Buffer.alloc(8);b.writeDoubleBE(v);return b.toString('hex')})()}:typeof v==='string'?{s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:typeof v==='function'?{fn:v.name}:Array.isArray(v)?v.map(pack):v&&typeof v==='object'?{o:Object.entries(v).map(([k,v])=>[k,pack(v)])}:v;
let input='';for await(const chunk of process.stdin)input+=chunk;
async function fixture({method,case:c}){
 const effects=[],original=Object.assign(new TypeError('DOWN'),{code:'ORDER',ids:[6,8]});const fresh=new Set();let uuidIndex=0;
 const effect=(name,...args)=>{effects.push([name,...args.map(pack)]);if(c==='fail:'+name)throw original};
 function ErrorCtor(m){effect('newError',m);const e=new Error(m);fresh.add(e);return e}
 const uuid=execute(new URL('utils/uuid.ts',root),{'nanoid':{nanoid:()=>{effect('uuid');return `id_${++uuidIndex}`}}}).uuid;
 const context={...enums,...constants,...math,inverseObj,omitUndefined,createPositionId,normalizeExecutionPositionSide,BigNumber,partition,uuid,Error:ErrorCtor,virtualClock:{getCurrentTimeMs:()=>{effect('clock');return -0}}};
 const C=vm.runInNewContext(ts.transpileModule(declarations+'\nclass Selected {'+bodies+'}\nSelected',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context),w=new C();
 const market={id:'I',symbol:'SYM',precision:{amount:.1,price:.5},limits:{amount:{max:c==='below-step'?.05:c==='bad-max'?Infinity:10}}},ticker={mark:c==='no-mark'?0:12};
 if(c==='bad-step')market.precision.amount=0;
 const positions=[{id:'SYM:short',symbol:'SYM',side:'short',entryPrice:8,leverage:7}];
 w.store={marketsBySymbol:c==='missing'?{}:{SYM:market},marketsById:c==='missing'?{}:{I:market,OTHER:{symbol:'OTHER'}},tickersById:c==='no-ticker'?{}:{I:ticker},positions:c==='no-position'?[]:positions};w._accountPositionMode=c==='no-mode'?null:'hedged';w.isPublicDataOnly=c==='public';
 w.emitAndLogError=m=>effect('emit',m);w.contractsToBase=(a,m,p)=>{assert.equal(m,market);effect('base',a,p);return c==='zero-base'?0:a*2};w.contractsToNotional=(a,m,p)=>{assert.equal(m,market);effect('notional',a,p);return a*p*2};
 w.toFiniteNumber=x=>{const n=parseFloat(String(x??''));return Number.isFinite(n)?n:0};w.convertMarginValueToUsd=(a,ccy,m,p)=>{assert.equal(m,market);effect('margin',a,ccy,p);return a*2};
 w.resolveTriggerReferencePrice=(m,type,last)=>{assert.equal(m,market);effect('reference',type,last);return c==='no-reference'?0:c==='negative-reference'?-1:10};w.getPositionSide=o=>{effect('positionSide',o);return 'net'};
 w.resolveCallerClientOrderId=o=>{effect('resolveCaller',o);return o.clientOrderId};w.assertSingleNativeClientOrderId=(id,n)=>{effect('singleId',id,n);if(c==='caller-split')throw original};
 const algoDelegate=o=>{effect('formatAlgo',o);return {tag:o.tag??o.type}};w.assertCallerClientOrderIds=o=>effect('preflight',o);w.getOrFetchPositionMode=async()=>effect('mode');
 w.isAlgoOrder=t=>['conditional','stop_market','take_profit_market','trailing_stop_market'].includes(t);w.getAlgoOrderPlacementPriority=t=>t==='stop_market'?0:t==='take_profit_market'?1:t==='conditional'?2:3;
 w.formatCreateOrder=method==='create'?w.formatCreateOrder:o=>{effect('formatCreate',o);return [{sz:c==='small'?'0':'1',tag:o.tag}]};w.deriveAlgoOrdersFromNormalOrdersOpts=orders=>{effect('derive',orders);return c==='derived'?[{type:'stop_market',tag:'derived'}]:[]};
 w.formatAlgoOrder=method==='algo'?w.formatAlgoOrder:algoDelegate;w.placeOrderBatch=async rows=>{effect('normalBatch',rows);return ['normal']};w.placeAlgoOrderBatch=async rows=>{effect('algoBatch',rows);return ['algo']};
 const triggerBody=w.getReduceOnlyTriggerOrderType;w.getReduceOnlyTriggerOrderType=method==='trigger'?triggerBody:(o,m)=>{assert.equal(m,market);effect('reduceType',o);return c==='take-trigger'?'take_profit_market':'stop_market'};
 w.placeOrders=method==='place'?w.placeOrders:async args=>{effect('placeOrders',args);return ['single']};
 let result,failed=false,error;
 try{
 const opts={symbol:'SYM',type:c==='market'?'market':c==='stop'?'stop_market':c==='take'?'take_profit_market':c==='trailing'?'trailing_stop_market':method==='algo'?'conditional':'limit',side:'buy',amount:c==='split'||c==='caller-split'?20.3:c==='zero'?0:c==='nan'?NaN:2.3,price:c==='zero-price'?0:12.7,triggerPrice:c==='bad-trigger'?0:13.2,timeInForce:c==='post-only'?'PostOnly':'GoodTillCancel',reduceOnly:c==='reduce',...(c==='caller'||c==='caller-split'?{clientOrderId:'caller'}:{}),...(c==='lone'?{clientOrderId:'\ud800'}:{})};
 if(method==='single')result=await w.placeOrder(opts);
 if(method==='place')result=await w.placeOrders(c==='empty'?[]:[{type:'limit',tag:'L'},{type:'conditional',tag:'C'},{type:'take_profit_market',tag:'T'},{type:'stop_market',tag:'S'}]);
 if(method==='algo')result=w.formatAlgoOrder(opts);if(method==='create')result=w.formatCreateOrder(opts);
 if(method==='trigger')result=w.getReduceOnlyTriggerOrderType({side:c==='sell'?'sell':'buy',triggerPx:c==='invalid'?'bad':c==='zero'?'0':c==='equal'?'10':c==='sell'?'12':'8',triggerPxType:'mark',last:'9'},market);
 if(method==='orders'){
   const row={instId:'I',ordType:c==='trigger'||c==='take-trigger'||c==='opening-trigger'?'trigger':c==='oco'?'oco':c==='conditional'?'conditional':c==='trailing'||c==='no-position'?'move_order_stop':c==='post-only'?'post_only':c==='prototype'?'constructor':'limit',ordId:'O',algoId:c==='lone'?'\ud800':'A',state:c==='prototype'?'constructor':'live',side:c==='prototype'?'constructor':'buy',sz:'3.5',accFillSz:'1',px:'12.5',last:'12',triggerPx:'13',ordPx:'15',reduceOnly:c==='boolean-reduce'?true:c==='opening-trigger'?false:'true',slTriggerPx:'10',tpTriggerPx:'14',triggerPxType:'index',slTriggerPxType:'mark',tpTriggerPxType:'last',callbackSpread:'1.5',moveTriggerPx:'2.5'};
   if(c==='empty')result=w.mapOrders([]);else result=w.mapOrders([row]);if(c==='prototype'){assert.equal(typeof result[0].type,'function');assert.equal(result[0].type,result[0].status);assert.equal(result[0].type,result[0].side);}
 }
 if(method==='positions')result=w.mapPositions(c==='empty'?[]:[{instId:'I',pos:c==='zero'?'0':c==='nan'?'bad':'-3.5',posSide:c==='long'?'long':'net',avgPx:'11.5',upl:'1.2',ccy:'USD',lever:'',liqPx:'2.5',mgnMode:c==='isolated'?'isolated':'cross'}]);
 if(method==='fills'){
  const fill=(id,t,extra={})=>({tradeId:id,billId:'B'+id,instId:'I',ordId:'O',side:'Sell',fillPx:'12.5',fillSz:'2',fillTime:t,fee:'-.3',feeCcy:'USD',fillPnl:'1.2',posSide:'Close Long',execType:'M',...extra});
  const recent=[fill('a','1700000000000'),fill('b','1700000001000')],history=[fill('a','1700000000000',{fillSz:'3'}),fill('c','1699999999000')];
  if(c==='filtered'){recent.push(fill('bad','1700000002000',{side:'invalid'}),fill('wrong','1700000003000',{instId:'OTHER'}),fill('zero','0'),fill('sz','1700000000000',{fillSz:0}));}
  if(c==='fallback'){for(const r of [...recent,...history]){delete r.tradeId;delete r.billId;}}
  if(c==='lone')recent[0].tradeId='\ud800';
  w.getMarketInstrumentType=m=>{assert.equal(m,market);return 'SWAP'};
  w.xhr={get:async(endpoint,config)=>{const route=endpoint===constants.ENDPOINTS.FILLS?'recent':'history';effect('get',route,config.params);return {data:{data:c==='bad-data'?{}:route==='recent'?recent:history}}}};
  result=await w.fetchHistoricalFills({symbol:c==='no-symbol'?undefined:'SYM',limit:c==='full'?2:c==='zero-limit'?0:5,...(c==='window'?{from:1700000000,to:1700000001}:{}),...(c==='order-filter'?{orderId:'OTHER'}:{})});
 }
 }catch(e){if(e===original){failed=true;error={original:true,name:e.name,message:pack(e.message),code:e.code,ids:e.ids}}else{assert.ok(fresh.has(e),'source constructed error identity');failed=true;error={original:false,name:e.name,message:pack(e.message)}}}
 return {method,case:c,effects,failed,result:pack(result),...(error?{error}:{})};
}
const cases=JSON.parse(input).cases;assert.ok(cases.length>0,'source nonempty corpus');process.stdout.write(JSON.stringify(await Promise.all(cases.map(fixture))));
