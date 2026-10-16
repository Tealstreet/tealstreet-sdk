// Supplemental source-only observations for the full frozen block.
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require = createRequire(import.meta.url);
const {ts, load} = require('../../../safe-cex-venue-weex/tests/types-oracle.cjs');
const deps = createRequire(path.join(path.dirname(process.env.SAFE_CEX_TYPESCRIPT_PATH), 'oracle.cjs'));
const root = fileURLToPath(new URL('../../../../', import.meta.url));
const src = path.join(root, 'packages/safe-cex/src');
const control = process.argv.includes('--control');
const high = JSON.parse('"\\ud800"');
const units = text => Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
function once(text, before, after) {
  assert.equal(text.split(before).length-1,1,'FULL-CONTROL unique source selector');
  return text.replace(before,after);
}
function evaluate(relative, names, bindings={}, owner, transform=text=>text) {
  const file=path.join(src,relative), text=fs.readFileSync(file,'utf8');
  const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
  const container=owner?ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text===owner).members:ast.statements;
  let selected=names.map(name=>{
    const matches=[...container].filter(n=>owner?n.name?.getText(ast)===name:(ts.isFunctionDeclaration(n)&&n.name?.text===name)||(ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name)));
    assert.equal(matches.length,1,'actual source '+relative+'::'+name);
    return matches[0].getText(ast);
  }).join('\n');
  selected=transform(selected);
  selected=owner?`class Selected {${selected}}\nexports.Selected=Selected;`:`${selected}\nObject.assign(exports,{${names.join(',')}});`;
  const compiled=ts.transpileModule(selected,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true},reportDiagnostics:true});
  assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
  const exports={};vm.runInNewContext(compiled.outputText,{exports,...bindings},{filename:relative});
  return owner?new exports.Selected():exports;
}
const enums=load(path.join(src,'types.ts'));
const cases={};
cases['okx-currency-key']=()=>{
  const f=evaluate('exchanges/okx/okx.exchange.ts',['findTickerMarkForCurrency'],{},undefined,text=>control?once(text,'candidates.includes(t.id)','false'):text);
  const id=high+'-USDT-SWAP';
  assert.deepEqual(units(id).slice(0,1),[0xd800],'FULL-OKX-CURRENCY-KEY materialized identifier');
  assert.equal(f.findTickerMarkForCurrency([{id,mark:17}],high),17,'FULL-OKX-CURRENCY-KEY comparison result');
  return {id:'okx-currency-key',units:units(id),result:17,scope:'exported function, fixed declared inputs'};
};
cases['okx-open-interest-key']=()=>{
  const receiver=evaluate('exchanges/okx/okx.ws-public.ts',['handleOpenInterestEvents'],{},'OKXPublicWebsocket',text=>control?once(text,'id: update.instId',"id: 'replacement'"):text);
  let calls=0;
  receiver.store={updateTicker(selector,patch){assert.deepEqual(units(selector.id),[0xd800],'FULL-OKX-OI-KEY Store selector');assert.equal(patch.openInterest,19,'FULL-OKX-OI-KEY patch');calls++;}};
  receiver.handleOpenInterestEvents({data:[{instId:high,oiCcy:'19'}]});
  assert.equal(calls,1,'FULL-OKX-OI-KEY one callback');
  return {id:'okx-open-interest-key',units:[0xd800],openInterest:19,scope:'actual handler; explicit Store callback'};
};
cases['okx-subscribe-unit']=()=>{
  let captured=0;
  const receiver=evaluate('exchanges/okx/okx.ws-public.ts',['subscribe'],{flatten:deps('lodash/flatten'),JSON:{stringify(value){assert.deepEqual(units(value.args[0].instId),[0xd800],'FULL-OKX-SUBSCRIBE-UNIT before intrinsic serialization');captured++;return JSON.stringify(value);}}},'OKXPublicWebsocket',text=>control?once(text,'const batch = topics.slice(i, i + batchSize);',"const batch = topics.slice(i, i + batchSize).map(t=>({...t,instId:'replacement'}));"):text);
  receiver.topics={tickers:[{channel:'tickers',instId:high}]};const sent=[];receiver.send=text=>sent.push(text);receiver.subscribe();
  assert.equal(captured,1,'FULL-OKX-SUBSCRIBE-UNIT captured once');
  assert.equal(sent.length,1,'FULL-OKX-SUBSCRIBE-UNIT one frame');
  return {id:'okx-subscribe-unit',units:[0xd800],wire:sent[0]};
};
cases['okx-message-unit']=()=>{
  const parser=load(path.join(root,'packages/json-parse/src/jsonParse.ts'));
  const receiver=evaluate('exchanges/okx/okx.ws-public.ts',['onMessage','dispatchMessageHandler'],parser,'OKXPublicWebsocket',text=>control?once(text,'? json.arg.channel',"? 'replacement'"):text);
  let called=0,stats=0;receiver.isDisposed=false;receiver.trackLastReceived=()=>{};receiver.directFallback={onMessage(){}};
  receiver.messageHandlers={[high]:[json=>{assert.deepEqual(units(json.arg.channel),[0xd800],'FULL-OKX-MESSAGE-UNIT callback payload');called++;}]};
  receiver.recordMessageHandlerStats=key=>{assert.deepEqual(units(key),[0xd800],'FULL-OKX-MESSAGE-UNIT stats key');stats++;};
  receiver.onMessage({data:'{"arg":{"channel":"\\ud800"},"data":[]}'});
  assert.equal(called,1,'FULL-OKX-MESSAGE-UNIT callback count');assert.equal(stats,1,'FULL-OKX-MESSAGE-UNIT stats count');
  return {id:'okx-message-unit',units:[0xd800],called,stats};
};
for(const [id,owner,method,verb] of [
  ['kraken-validation-prefix','KrakenExchange','validateAccount','get'],
  ['kraken-tickers-prefix','KrakenExchange','fetchTickers','get'],
  ['kraken-orders-prefix','KrakenExchange','fetchOrders','get'],
  ['kraken-spot-balance-prefix','KrakenSpotExchange','fetchBalance','post'],
  ['kraken-spot-candles-prefix','KrakenSpotExchange','fetchOHLCV','get'],
]) cases[id]=()=>{
  const relative='exchanges/kraken/'+(owner==='KrakenExchange'?'kraken.exchange.ts':'krakenspot.exchange.ts');
  const types=load(path.join(src,'exchanges/kraken/'+(owner==='KrakenExchange'?'kraken.types.ts':'krakenspot.types.ts')));
  const receiver=evaluate(relative,[method],types,owner,text=>control?once(text,'try {','try { await Promise.resolve();'):text);
  const calls=[];receiver.xhr={[verb]:(...args)=>{calls.push(args);return new Promise(()=>{});}};receiver.store={marketsBySymbol:{}};
  const pending=receiver[method](...(method==='fetchOHLCV'?[{symbol:'AUDIT/USD',interval:'1m',from:1000}]:[]));
  pending.catch(()=>{});
  assert.equal(calls.length,1,'FULL-'+id.toUpperCase()+' call before caller awaits');
  return {id,verb,endpoint:calls[0][0],scope:'source prefix only; native async body inspected'};
};
const fillBase={fill_id:'ordinary',symbol:'PF_X',fillTime:'2026-08-14T12:34:56Z',side:'buy',price:'3',size:'5'};
cases['kraken-fill-batch-unit']=()=>{
  const f=evaluate('exchanges/kraken/kraken.fills.ts',['toSeconds','num','optionalNum','mapKrakenFill','mapKrakenFills'],enums,undefined,text=>control?once(text,"String(row?.fill_id ?? '')","(row?.fill_id === 'ordinary' ? 'ordinary' : 'replacement')"):text);
  const value=f.mapKrakenFills([fillBase,{...fillBase,fill_id:high}],()=>undefined);
  assert.equal(value.length,2,'FULL-KRAKEN-FILL-BATCH-UNIT fixed two outputs');
  assert.equal(value[0].id,'ordinary','FULL-KRAKEN-FILL-BATCH-UNIT ordinary identity');
  assert.deepEqual(units(value[1].id),[0xd800],'FULL-KRAKEN-FILL-BATCH-UNIT second id');
  return {id:'kraken-fill-batch-unit',ordinary:value[0].id,units:units(value[1].id)};
};
cases['kraken-log-row-unit']=()=>{
  const f=evaluate('exchanges/kraken/kraken.income.ts',['isPositionSizeLeg','num','toMs','mapKrakenAccountLogRow'],{},undefined,text=>control?once(text,"String(row.booking_uid ?? row.id ?? '')","'replacement'"):text);
  const value=f.mapKrakenAccountLogRow({booking_uid:high,date:'2026-08-14T12:34:56Z',asset:'usd',contract:'PF_X',realized_pnl:'3'},()=>undefined);
  assert.equal(value.length,1,'FULL-KRAKEN-LOG-ROW-UNIT fixed one output');
  assert.deepEqual(units(value[0].tranId),[0xd800,58,112,110,108],'FULL-KRAKEN-LOG-ROW-UNIT tranId');
  return {id:'kraken-log-row-unit',units:units(value[0].tranId)};
};
cases['kraken-position-leg-comparison']=()=>{
  const f=evaluate('exchanges/kraken/kraken.income.ts',['isPositionSizeLeg'],{},undefined,text=>control?once(text,'asset === contract','true'):text);
  const low=JSON.parse('"\\udc00"');
  assert.equal(f.isPositionSizeLeg({asset:high,contract:high}),true,'FULL-KRAKEN-POSITION-LEG same retained units');
  assert.equal(f.isPositionSizeLeg({asset:high,contract:low}),false,'FULL-KRAKEN-POSITION-LEG distinct retained units');
  return {id:'kraken-position-leg-comparison',units:[[0xd800],[0xdc00]],same:true,distinct:false,scope:'actual ledger leg classifier; native Value restriction inspected'};
};
cases['kraken-token-unit']=async()=>{
  const types=load(path.join(src,'exchanges/kraken/krakenspot.types.ts'));
  const receiver=evaluate('exchanges/kraken/krakenspot.exchange.ts',['fetchWsToken'],types,'KrakenSpotExchange',text=>control?once(text,'return token;',"return 'replacement';"):text);
  receiver.xhr={post:async()=>({data:{result:{token:high}}})};
  const value=await receiver.fetchWsToken();
  assert.deepEqual(units(value),[0xd800],'FULL-KRAKEN-TOKEN-UNIT returned token');
  return {id:'kraken-token-unit',units:units(value)};
};
cases['kraken-spot-balance-unit']=async()=>{
  const types=load(path.join(src,'exchanges/kraken/krakenspot.types.ts'));
  const receiver=evaluate('exchanges/kraken/krakenspot.exchange.ts',['fetchBalance'],types,'KrakenSpotExchange',text=>control?once(text,'balance: balanceStr',"balance: 'replacement'"):text);
  receiver.xhr={post:async()=>({data:{result:{USDT:high}}})};receiver.store={tickers:[]};receiver.emitAndLogError=error=>{throw Error('unexpected report '+error);};
  const value=await receiver.fetchBalance();
  assert.deepEqual(units(value.currencies.USDT.info.balance),[0xd800],'FULL-KRAKEN-SPOT-BALANCE-UNIT returned raw balance');
  return {id:'kraken-spot-balance-unit',units:units(value.currencies.USDT.info.balance)};
};
const weex='exchanges/weex/weex.utils.ts';
cases['weex-balance-alias-unit']=()=>{
  const f=evaluate(weex,['toNumber','normalizeBalance'],{},undefined,text=>control?once(text,'info: rows,','info: rows.map(row=>({...row})),'):text);
  const input=[{asset:'USDT',balance:'3',tag:high}];const value=f.normalizeBalance(input);
  assert.deepEqual(units(value.info[0].tag),[0xd800],'FULL-WEEX-BALANCE-UNIT retained info');
  input[0].balance='7';assert.equal(value.info[0].balance,'7','FULL-WEEX-BALANCE-ALIAS input to output');
  value.info[0].balance='11';assert.equal(input[0].balance,'11','FULL-WEEX-BALANCE-ALIAS output to input');
  return {id:'weex-balance-alias-unit',units:units(value.info[0].tag),directions:[7,11]};
};
const sharedOmit=(()=>{
  const file=path.join(src,'utils/omitUndefined.ts'),exports={};
  const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
  vm.runInNewContext(code,{exports,require:deps},{filename:'utils/omitUndefined.ts'});return exports;
})();
const weexBindings={...enums,...load(path.join(src,'exchanges/weex/weex.types.ts')),...sharedOmit,...load(path.join(src,'utils/incomeCategoryFromType.ts'))};
const fillNames=['toNumber','toIntegerString','normalizeTimestampToMs','normalizeTimestampToSeconds','normalizeOrderSide','normalizeExecution','normalizeHistoricalTrade'];
const execution={symbol:'B/Q',side:'BUY',price:3,qty:5,time:1700000000123,orderId:'ordinary'};
for(const [id,fn,field,inputField] of [['weex-execution-unit','normalizeExecution','symbol','symbol'],['weex-historical-unit','normalizeHistoricalTrade','orderId','orderId']]) cases[id]=()=>{
  const f=evaluate(weex,fillNames,weexBindings,undefined,text=>control?once(text,fn==='normalizeExecution'?'    symbol,':'    orderId: execution.orderId,',fn==='normalizeExecution'?"    symbol: 'replacement',":"    orderId: 'replacement',"):text);
  const value=f[fn]({...execution,[inputField]:high});
  assert.deepEqual(units(value[field]),[0xd800],'FULL-'+id.toUpperCase()+' returned field');
  return {id,field,units:units(value[field])};
};
cases['weex-income-unit']=()=>{
  const f=evaluate(weex,['toNumber','toIntegerString','normalizeTimestampToMs','normalizeIncome'],weexBindings,undefined,text=>control?once(text,'tranId: toIntegerString(raw.billId ?? raw.tranId ?? raw.id)',"tranId: 'replacement'"):text);
  const value=f.normalizeIncome({billId:high,income:3,time:1700000000123});
  assert.deepEqual(units(value.tranId),[0xd800],'FULL-WEEX-INCOME-UNIT returned tranId');return {id:'weex-income-unit',units:units(value.tranId)};
};
cases['weex-public-trade-unit']=()=>{
  const f=evaluate(weex,['toNumber','toIntegerString','normalizeTimestampToMs','normalizePublicTrade'],weexBindings,undefined,text=>control?once(text,'id: toIntegerString(raw.id ?? raw.t ?? `${timestamp}:${price}:${qty}`)',"id: 'replacement'"):text);
  const value=f.normalizePublicTrade({id:high,price:3,qty:5,time:1700000000123},'B/Q');
  assert.deepEqual(units(value.id),[0xd800],'FULL-WEEX-PUBLIC-TRADE-UNIT returned id');return {id:'weex-public-trade-unit',units:units(value.id)};
};
cases['weex-maker-error-order']=()=>{
  const f=evaluate(weex,fillNames,weexBindings,undefined,text=>control?once(text,'maker: raw.maker,',"maker: (()=>{throw new TypeError('early maker refusal');})(),"):text);
  let error;try{f.normalizeExecution({...execution,maker:null,commission:{toString:null}});}catch(e){error=e;}
  assert.equal(error?.name,'TypeError','FULL-WEEX-MAKER-ERROR-ORDER source class');
  assert.equal(error?.message,'Cannot convert object to primitive value','FULL-WEEX-MAKER-ERROR-ORDER later commission wins');
  return {id:'weex-maker-error-order',name:error.name,message:error.message,native:'earlier maker refusal inspected only'};
};
cases['weex-order-result-unit']=()=>{
  const utility=evaluate(weex,['toIntegerString']);
  const receiver=evaluate('exchanges/weex/weex.exchange.ts',['parseOrderResult'],utility,'WeexExchange',text=>control?once(text,'return id;',"return 'replacement';"):text);
  const value=receiver.parseOrderResult({success:true,orderId:high});
  assert.deepEqual(units(value),[0xd800],'FULL-WEEX-ORDER-RESULT-UNIT actual consumer returned id');return {id:'weex-order-result-unit',units:units(value),scope:'actual parseOrderResult consumer; no network'};
};
assert.equal(Object.keys(cases).length,21,'FULL-SOURCE fixed twenty-one profiles');
const chosen=process.argv[2];const observations=[];
if(chosen){assert.ok(Object.hasOwn(cases,chosen),'named full source profile');observations.push(await cases[chosen]());}
else{assert.equal(control,false,'control requires profile');for(const fn of Object.values(cases))observations.push(await fn());}
process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,observations})+'\n');
