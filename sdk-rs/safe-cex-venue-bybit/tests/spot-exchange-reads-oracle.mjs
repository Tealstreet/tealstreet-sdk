import '../scripts/check-candle-iterables.mjs';
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url), root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'node_modules/typescript'));
const read=(p)=>{const file=path.join(root,p);return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),99,true)};
const ast=read('packages/safe-cex/src/exchanges/bybit/bybitspot.exchange.ts');
const cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BybitSpotExchange');
const names=['fetchRawBalance','formatBalance','fetchOrders','fetchMarkets','fetchOHLCV','mapOrder','placeOrder','updateOrder'];
const fields=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(ast)===name);assert(n,'READ actual source '+name);return n.initializer?name+'='+n.initializer.getText(ast)+';':n.getText(ast)}).join('\n');
const enumAst=read('packages/safe-cex/src/types.ts');
const enums=enumAst.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderStatus','OrderType','OrderSide','OrderTimeInForce','OrderStopTriggerType'].includes(n.name.text)).map(n=>n.getText(enumAst).replace(/^export\s+/,'')).join('\n');
const tablesAst=read('packages/safe-cex/src/exchanges/bybit/bybit.types.ts');
const tableNames=['ENDPOINTS','ORDER_TYPE','ORDER_SIDE','ORDER_STATUS','ORDER_TIME_IN_FORCE','ORDER_TIME_IN_FORCE_OUTBOUND','ORDER_TRIGGER_TYPE','POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE','INTERVAL'];
const tables=tableNames.map(name=>{const n=tablesAst.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.getText(tablesAst)===name);assert(n?.initializer,'READ actual table '+name);return 'const '+name+'='+n.initializer.getText(tablesAst)+';'}).join('\n');
const math=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'),'utf8').replace(/^import .*;\n/gm,'').replace(/export\s+/g,'');
const safeMath=new Function('isNotNumber',ts.transpileModule(math,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';return {adjust,subtract};')(Number.isNaN);
const code=ts.transpileModule(enums+tables+'class Source{'+fields+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const unpack=v=>v.u?undefined:v.null?null:'b'in v?v.b:v.n?Buffer.from(v.n).readDoubleLE():v.s?String.fromCharCode(...v.s):v.a?v.a.map(unpack):Object.fromEntries(v.o.map(([k,v])=>[String.fromCharCode(...k),unpack(v)]));
const pack=v=>v===undefined?{u:true}:v===null?{null:true}:typeof v==='boolean'?{b:v}:typeof v==='number'?{n:[...(()=>{const b=Buffer.alloc(8);b.writeDoubleLE(Number.isNaN(v)?NaN:v);return b})()]}:typeof v==='string'?{s:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))}:Array.isArray(v)?{a:v.map(pack)}:{o:Object.entries(v instanceof Error?{message:v.message}:v).map(([k,v])=>[Array.from({length:k.length},(_,i)=>k.charCodeAt(i)),pack(v)])};
const {mapSeries}=require('p-iteration'),omit=require('lodash/omit'),orderBy=require('lodash/orderBy'),times=require('lodash/times'),dayjs=require('dayjs');
const inverseObj=o=>Object.fromEntries(Object.entries(o).map(([k,v])=>[v,k])),omitUndefined=o=>Object.fromEntries(Object.entries(o).filter(([,v])=>v!==undefined)),v=(o,k)=>o?.[k];
const cases=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(cases.length,43,'READ independent source input floor');const outputs=[];
for(const c of cases){
    const scene=c.scene,events=[],requests=[],recorded=[],original={message:'original'};let clock=0,page=0;
    const virtualClock={getCurrentTimeMs:()=>1234+clock++};
    const {Source,ENDPOINTS}=new Function('adjust','subtract','mapSeries','omit','omitUndefined','orderBy','times','dayjs','inverseObj','v','virtualClock','isUnknownOutcome',code+';return {Source,ENDPOINTS};')(safeMath.adjust,safeMath.subtract,mapSeries,omit,omitUndefined,orderBy,times,dayjs,inverseObj,v,virtualClock,()=>scene==='unknown');
    const target=new Source(),event=(name,args=[])=>events.push(pack([name,args]));
    target.forceUnifiedBalance=false;target.unifiedMarginStatus=1;target.accountType='CONTRACT';target.needsReinit=false;
    target.store={balance:{sentinel:'balance',info:['prior']},markets:['stored'],marketsBySymbol:scene==='no-market'?{}:{BTC:{precision:{price:0.5,amount:0.1},limits:{amount:{max:2}},isInverse:scene==='inverse'}},tickersBySymbol:{BTC:{mark:scene==='mark-zero'?0:100}},orders:scene==='missing-parent'?[]:[{id:'parent'}]};
    const selected={raw:'fetchRawBalance',balance:'formatBalance',orders:'fetchOrders',markets:'fetchMarkets',candles:'fetchOHLCV',map:'mapOrder',place:'placeOrder',update:'updateOrder'}[c.op];
    const data=(endpoint,i)=>({retMsg:scene==='promote'&&i===0?'accountType only support UNIFIED.':['api-error','virtual-api-error'].includes(scene)?'bad':scene==='not-modified'?'order not modified':'OK',result:{list:endpoint==='BALANCE'?['account']:endpoint==='UNFILLED_ORDERS'?(scene==='empty'||scene==='pages'&&i===1?[]:[{id:'raw'}]):endpoint==='KLINE'?(scene==='missing-list'?undefined:[['2000','21','24','19','23','17'],['1000','11','14','9','13','7']]):undefined,nextPageCursor:scene==='pages'&&i===0?'next':'',orderId:'id-'+i}});
    const request=async(url,payload)=>{const endpoint=Object.entries(ENDPOINTS).find(([k,val])=>tableNames.includes(k)===false&&val===url&&['BALANCE','UNFILLED_ORDERS','KLINE','CREATE_ORDER','REPLACE_ORDER','SET_TRADING_STOP'].includes(k))?.[0];assert(endpoint,'READ actual endpoint');const i=page++;requests.push(pack([endpoint,payload]));if(['reject','virtual-reject'].includes(scene)||scene==='unknown'||scene==='partial'&&i===0)throw original;return {data:data(endpoint,i)}};
    target.xhr={get:request,post:request};target.emitAndLogError=(...args)=>event('emitAndLogError',args);
    if(selected!=='fetchRawBalance')target.fetchRawBalance=async()=>{event('fetchRawBalance');return data('BALANCE',1).result.list};
    else if(scene==='promote'){const real=target.fetchRawBalance;let first=true;target.fetchRawBalance=async()=>{if(first){first=false;return real.call(target)}event('fetchRawBalance');return data('BALANCE',1).result.list};}
    target.reinitOnThrow=p=>p.catch(e=>{target.needsReinit=true;throw e});
    if(selected!=='mapOrder')target.mapOrder=raw=>['mapped',raw];target.preserveRecentlyPlacedOrders=rows=>{event('preserveRecentlyPlacedOrders');return rows};
    target.fetchMarketsHelper=async category=>{assert.equal(category,'spot');return scene==='null'?[[null]]:scene==='empty'?[]:[['new'],[false]]};
    target.resolveCallerClientOrderId=opts=>{event('resolveCallerClientOrderId');return opts.clientOrderId};target.getOrderExecutionParams=()=>{event('getOrderExecutionParams');return scene==='execution'?{reduceOnly:true}:undefined};target._hasFullSizedPositionOppositeSide=()=>scene==='protected';target.getOrderPositionIdx=async()=>{event('getOrderPositionIdx');return 0};target.assertSingleNativeClientOrderId=(...args)=>{event('assertSingleNativeClientOrderId',args);if(args[0]!==undefined&&args[1]!==1)throw original};target.recordRecentlyPlacedOrderFromOpts=(...args)=>recorded.push(pack(args));target.placeStopLossOrTakeProfit=async(...args)=>{event('placeStopLossOrTakeProfit',args);return ['delegated']};target.placeTrailingStopLoss=async(...args)=>{event('placeTrailingStopLoss',args);return ['delegated']};target.cancelOrders=async(...args)=>event('cancelOrders',args);target.applyLocalOrderUpdateAfterRestWrite=(...args)=>event('applyLocalOrderUpdateAfterRestWrite',args);
    const input=unpack(c.input);let result;
    try{result={ok:pack(await target[selected](input))}}catch(e){result={error:pack(e.message),original:e===original}}
    outputs.push({result,requests,events,recorded});
}
assert.equal(outputs.length,43,'READ independent source output floor');console.log(JSON.stringify(outputs));
