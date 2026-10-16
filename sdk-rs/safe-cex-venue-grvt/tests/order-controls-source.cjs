'use strict';
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm'), assert = require('node:assert/strict');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute compiler required');
const ts = require(compiler), root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function ast(file) { return ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true); }
const exchange = ast('exchanges/grvt/grvt.exchange.ts');
const cls = exchange.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'GrvtExchange');
const names = ['cancelOrder', 'cancelAllOrders', 'cancelSymbolOrders', 'cancelOrders', 'placeOrders', 'updateOrder', 'listenOHLCV', 'listenOrderBook', 'symbolToInstrumentId'];
const members = names.map(name => { const member = cls.members.find(n => n.name?.getText(exchange) === name); assert(member, 'actual member ' + name); return member.getText(exchange); });
const shared = ast('types.ts'), builder = ast('exchanges/grvt/grvt.builder.ts'), semantics = ast('exchanges/grvt/grvt.order-semantics.ts');
const enums = ['OrderType', 'OrderSide', 'OrderTimeInForce', 'OrderStopTriggerType'].map(name => shared.statements.find(n => ts.isEnumDeclaration(n) && n.name.text === name).getText(shared));
const builderClass = builder.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'GrvtBuilderUnauthorizedError').getText(builder);
const trigger = semantics.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations[0].name.getText(semantics) === 'getGrvtTriggerMetadata').getText(semantics).replace(/^export /, '');
const code = [...enums, builderClass, trigger, 'const LogSeverity = {Info:"info",Debug:"debug",Warning:"warning",Error:"error"};', 'export class Venue {', ...members, '}'].join('\n');
const compiled = ts.transpileModule(code, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(compiled.diagnostics.length, 0, 'CONTROLS source compiles');
const context = {exports: {}};
vm.runInNewContext(compiled.outputText, context);
const Venue = context.exports.Venue, BuilderError = context.exports.GrvtBuilderUnauthorizedError;
function error(cause) { return cause ? {name:cause.name,message:cause.message,code:cause.code ?? null,reason:cause.reason ?? null,requiredRaw:cause.requiredRaw ?? null,authorizedRaw:cause.authorizedRaw ?? null} : null; }
function order(id='nonce-17') { return {id,status:'open',symbol:'BTC_USDT_PERP',type:'limit',side:'buy',price:13.25,amount:6.5,filled:0.5,remaining:6,reduceOnly:true,lastUpdate:1009,timeInForce:'ImmediateOrCancel'}; }
function rejection(name) {
  const cause = Error('original'); cause.name = 'TypeError';
  if (name === 'sdk-error' || name === 'emit-error') cause.response = {data:{message:'full message',m:'lite message',code:17,c:9}};
  if (name === 'lite-error') cause.response = {data:{message:'',m:'lite message',code:0,c:3}};
  return cause;
}
(async () => {
  const cancelRows = [], cancelBatchRows = [], placeRows = [], updateRows = [], listenRows = [];
  for (const method of ['cancelOrder', 'cancelAllOrders', 'cancelSymbolOrders']) {
    for (const name of ['ok','fallback','no-client','no-account','endpoint-error','sdk-error','lite-error','log-error','emit-error']) {
      const effects = [], venue = new Venue();
      venue.grvtTradingAccountId = name === 'no-account' ? '' : 'sub-72';
      venue._nonceToOrderId = new Map([['nonce-17', name === 'fallback' ? '' : '0xAB']]);
      venue.log = (message,severity) => { effects.push(['log',message,severity]); if (name === 'log-error') throw Error('log-fail'); };
      venue.emitAndLogError = message => { effects.push(['emit',message]); if (name === 'emit-error') throw Error('emit-fail'); };
      const cancel = async (payload,endpoint) => { effects.push(['sdk',payload,endpoint]); if (['sdk-error','lite-error','emit-error'].includes(name)) throw rejection(name); };
      venue.grvtClient = name === 'no-client' ? null : {authenticatedEndpoint: async () => { effects.push(['endpoint']); if (name === 'endpoint-error') throw rejection(name); return {token:'endpoint-31'}; }, tdgClient:{cancelOrder:cancel,cancelAllOrders:cancel}};
      let caught;
      const pending = venue[method](method === 'cancelOrder' ? order() : 'BTC_USDT_PERP');
      const eager = effects.slice();
      try { await pending; } catch (cause) { caught = cause; }
      cancelRows.push({method,name,error:error(caught),effects,eager});
    }
  }
  for (const name of ['continue','log-error','empty']) {
    const venue = new Venue(), effects = [], orders = name === 'empty' ? [] : [order('one'),order('two'),order('three')];
    venue.cancelOrder = async order => { effects.push(['cancel',order.id]); if (order.id === 'two') throw Error('cancel-fail'); };
    venue.log = (message,severity) => { effects.push(['log',message,severity]); if (name === 'log-error') throw Error('log-fail'); };
    const pending = venue.cancelOrders(orders), eager = effects.slice(); let caught;
    try { await pending; } catch (cause) { caught = cause; }
    cancelBatchRows.push({name,effects,eager,error:error(caught)});
  }
  for (const name of ['continue','builder','preflight','log-error','empty','spoof']) {
    const venue = new Venue(), effects = [], orders = name === 'empty' ? [] : ['ONE','TWO','THREE'].map(symbol => ({symbol,type:'limit',side:'buy',amount:2}));
    venue.assertCallerClientOrderIds = orders => { effects.push(['preflight',orders.length]); if (name === 'preflight') throw Error('preflight-fail'); };
    venue.placeOrder = async order => { effects.push(['place',order.symbol]); if (order.symbol === 'TWO') {
      if (name === 'builder') throw new BuilderError({reason:'cap-too-low',requiredRaw:300,authorizedRaw:7,message:'builder-fail'});
      const failure = Error('place-fail'); if (name === 'spoof') failure.name = 'GrvtBuilderUnauthorizedError'; throw failure;
    } return order.symbol === 'ONE' ? ['id-a','id-b'] : ['id-c']; };
    venue.log = (message,severity) => { effects.push(['log',message,severity]); if (name === 'log-error') throw Error('log-fail'); };
    const pending = venue.placeOrders(orders), eager = effects.slice(); let result = null, caught;
    try { result = await pending; } catch (cause) { caught = cause; }
    placeRows.push({name,effects,eager,error:error(caught),result});
  }
  for (const name of ['amount','price','both','no-tif','stop-price','stop-amount','take-price','missing-trigger','wrong-trigger','missing-price','zero-price','incomplete','split','cancel-error','place-error','log-error','emit-error','null-info']) {
    const venue = new Venue(), effects = [], current = order('o-7'); let update = {amount:2.75};
    if (name === 'price') update = {price:19.5};
    if (name === 'both') update = {amount:2.75,price:19.5};
    if (name === 'no-tif') delete current.timeInForce;
    if (['stop-price','stop-amount','take-price','missing-trigger','wrong-trigger','missing-price','zero-price','incomplete','split','null-info'].includes(name)) {
      current.type = name === 'take-price' ? 'take_profit_market' : 'stop_market';
      current.info = {m:{t:{tt:current.type === 'stop_market' ? 'STOP_LOSS' : 'TAKE_PROFIT',t:{tp:'17.25',tb:'INDEX',cp:false,is:false}}}};
      const trigger = current.info.m.t;
      if (name === 'stop-price' || name === 'take-price') update = {price:19.5};
      if (name === 'missing-trigger') delete current.info;
      if (name === 'wrong-trigger') trigger.tt = 'TAKE_PROFIT';
      if (name === 'missing-price') delete trigger.t.tp;
      if (name === 'zero-price') trigger.t.tp = '0';
      if (name === 'incomplete') delete trigger.t.cp;
      if (name === 'split') trigger.t.is = true;
      if (name === 'null-info') current.info = null;
    }
    venue.cancelOrder = async order => { effects.push(['cancel',order.id]); if (name === 'cancel-error') throw rejection('sdk-error'); };
    venue.placeOrder = async opts => { effects.push(['place',opts]); if (name === 'place-error' || name === 'emit-error') throw rejection('lite-error'); return ['new-a','new-b']; };
    venue.log = (message,severity) => { effects.push(['log',message,severity]); if (name === 'log-error') throw Error('log-fail'); };
    venue.emitAndLogError = message => { effects.push(['emit',message]); if (name === 'emit-error') throw Error('emit-fail'); };
    const input = {order:current,update}, pending = venue.updateOrder(input), eager = effects.slice(); let result = null, caught;
    try { result = await pending; } catch (cause) { caught = cause; }
    updateRows.push({name,input,effects,eager,error:error(caught),result});
  }
  const candle = {timestamp:701,open:2,high:5,low:1,close:4,volume:13}, book = {bids:[{price:7,amount:2,total:2}],asks:[{price:11,amount:3,total:3}]};
  for (const method of ['listenOHLCV','listenOrderBook']) for (const fail of [false,true]) {
    const venue = new Venue(), effects = [], input = method === 'listenOHLCV' ? {symbol:'BTC_USDT_PERP',interval:'5m',from:11,to:23,limit:17} : 'BTC_USDT_PERP';
    venue.publicWebsocket = {[method](arg,callback) { effects.push(['delegate',arg]); if (fail) throw Error('listen-fail'); callback(method === 'listenOHLCV' ? candle : book); return () => { effects.push(['cleanup']); return 'released'; }; }};
    let result = null, caught;
    try { const cleanup = venue[method](input, event => effects.push(['callback',event])); result = cleanup(); } catch (cause) { caught = cause; }
    listenRows.push({method,fail,input,effects,error:error(caught),result});
  }
  assert.equal(cancelRows.length,27); assert.equal(cancelBatchRows.length,3); assert.equal(placeRows.length,6); assert.equal(updateRows.length,18); assert.equal(listenRows.length,4);
  const extraInput = {order:order('extra'), update:{amount:2, extra:7}};
  const extraVenue = new Venue(), extraLogs = [];
  extraVenue.log = message => extraLogs.push(message);
  extraVenue.cancelOrder = async () => {};
  extraVenue.placeOrder = async () => ['extra-id'];
  extraVenue.emitAndLogError = message => { throw Error(message); };
  await extraVenue.updateOrder(extraInput);
  assert.equal(extraLogs[0], 'Updating order extra (fieldCount=2)', 'UPDATE actual extra-key source count');
  process.stdout.write(JSON.stringify({cancelRows,cancelBatchRows,placeRows,updateRows,listenRows,extraInput,extraFirstLog:extraLogs[0]}));
})().catch(cause => {console.error(cause);process.exitCode=1;});
