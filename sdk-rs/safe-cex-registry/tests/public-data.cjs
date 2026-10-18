const fs = require('node:fs');
const assert = require('node:assert/strict');
const {root, ts, load} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const fixture = require('./fixtures/public-data.json');
const configSource = fs.readFileSync(root + '/packages/safe-cex/src/config/config.ts', 'utf8');
const configAst = ts.createSourceFile('config.ts', configSource, ts.ScriptTarget.Latest, true);
const configDecls = configAst.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations]);
const config = {};
for (const name of ['ENABLE_RECONNECTING_WEBSOCKET', 'WS_MAX_RECONNECTION_DELAY', 'WS_MIN_RECONNECTION_DELAY', 'RECONNECTING_WEBSOCKET_TUNING']) {
  const decl = configDecls.find(d => d.name.getText(configAst) === name);
  // Pin module-load Math.random, while executing the actual initializer.
  config[name] = new Function('Math', 'WS_MAX_RECONNECTION_DELAY', 'WS_MIN_RECONNECTION_DELAY', 'return ' + decl.initializer.getText(configAst))({...Math, random: () => 0.25}, config.WS_MAX_RECONNECTION_DELAY, config.WS_MIN_RECONNECTION_DELAY);
}
let now = 1000;
let nextTimer = 0;
const timers = new Map();
const scheduled = [];
const sockets = [];
class Socket {
  static OPEN = 1;
  constructor(url) { this.url = url; this.readyState = 0; this.handlers = {}; this.sent = []; this.closes = 0; sockets.push(this); }
  addEventListener(event, callback) { this.handlers[event] = callback; }
  send(text) { this.sent.push(JSON.parse(text)); }
  close() { this.closes++; this.readyState = 3; this.handlers.close(); }
  open() { this.readyState = 1; this.handlers.open(); }
  message(data) { this.handlers.message({data: typeof data === 'string' ? data : JSON.stringify(data)}); }
}
function execute(file, external) {
  const code = ts.transpileModule(fs.readFileSync(root + file, 'utf8'), {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS}}).outputText;
  const captured = {exports: {}};
  const requireHost = name => {
    if (name === '../types') return {LogSeverity: {Info: 'info', Warning: 'warning', Error: 'error'}};
    if (name === '../config/config') return config;
    if (name.includes('/jsonParse')) return load(root + '/packages/json-parse/src/jsonParse.ts');
    if (name.includes('ReconnectingWebsocket')) return {ReconnectingWebSocket: Socket};
    if (name === './external-public-data') return external;
    throw Error('unexpected source dependency ' + name);
  };
  new Function('module', 'exports', 'require', 'Date', 'WebSocket', 'setTimeout', 'clearTimeout', code)(captured, captured.exports, requireHost, {now: () => now}, Socket,
    (callback, delay) => { const id = ++nextTimer; timers.set(id, callback); scheduled.push(delay); return id; },
    id => timers.delete(id));
  return captured.exports;
}
const external = execute('/packages/safe-cex/src/exchanges/external-public-data.ts');
const {InProcessPublicDataProvider} = execute('/packages/safe-cex/src/exchanges/in-process-public-data.ts', external);
function parent() {
  const updates = [], logs = [];
  return {name: 'bybit', updates, logs,
    store: {tickers: [], markets: [], loaded: {balance: false, orders: false, markets: false, tickers: false, positions: false}, update(data) { updates.push(data); Object.assign(this, data); }},
    adaptExternalMarkets(markets) { return markets.map(m => ({...m, symbol: 'ADAPT:' + m.symbol})); },
    emitter: {emit(event, message, severity) { assert.equal(event, 'log'); logs.push([message, severity]); }}
  };
}
const p = parent();
const data = {tickers: [{...fixture.ticker}], markets: [{...fixture.market}], loaded: {tickers: true, markets: true}};
const callbacks = [], delegated = [];
let cleanupCount = 0;
const source = {store: {...data, subscribe(callback) { callbacks.push(callback); return () => cleanupCount++; }},
  listenOrderBook(symbol, callback) { delegated.push(symbol); callback(fixture.orderbook); return () => cleanupCount++; },
  listenOHLCV(options, callback) { delegated.push(options); callback(fixture.candle); return () => cleanupCount++; }};
const ip = new InProcessPublicDataProvider(p, source);
assert.equal(ip.isConnected, false); assert.equal(ip.isInitialized, false); assert.equal(ip.lastMessageTimestamp, undefined);
ip.connect(); assert.equal(p.updates.length, 2); assert.equal(ip.isHealthy, true);
now = 2000; callbacks[0](data); assert.equal(p.updates.length, 3);
data.tickers[0].bid = 151; now = 3000; callbacks[0](data);
assert.equal(p.updates.length, 3); assert.equal(p.store.tickers[0].bid, 151); assert.equal(ip.lastMessageTimestamp, 3000);
data.tickers.push({...fixture.ticker, id: 'ETH', symbol: 'ETH'}); now = 4000; callbacks[0](data);
assert.equal(p.updates.length, 3, 'source same-array length mutation stays filtered'); assert.equal(p.store.tickers.length, 2);
now = 4500; callbacks[0]({tickers: [], markets: []}); assert.equal(ip.lastMessageTimestamp, 4000);
now = 5000; callbacks[0]({...data, tickers: data.tickers.slice()}); assert.equal(p.updates.length, 4);
assert.equal('markets' in p.updates[3], false);
now = 25000; assert.equal(ip.isHealthy, false); now = 24999; assert.equal(ip.isHealthy, true);
const ob = value => assert.equal(value.bids[0].price, 91);
const cc = value => assert.equal(value.high, 223);
const unsub = ip.subscribeOrderbook('BTC:PERP', ob); unsub(); unsub();
ip.subscribeCandle(fixture.candleOptions, cc);
ip.connect(); assert.equal(callbacks.length, 2); ip.dispose(); ip.dispose();
assert.equal(cleanupCount, 3); assert.equal(ip.isConnected, false); assert.equal(ip.isInitialized, true);
now = 30000; callbacks[0](data); assert.equal(ip.lastMessageTimestamp, 24999);
const inProcess = {updates: p.updates.length, cleanupCount, last: ip.lastMessageTimestamp, logs: p.logs};

now = 1000;
const gp = parent();
const gateway = new external.ExternalPublicDataProvider(gp, {url: 'wss://fixture', exchangeId: 'bybit', authToken: 'secret'});
const seen = [];
gateway.subscribeOrderbook('BTC:PERP', value => seen.push(['book', value.bids[0].price, value.asks[0].amount]));
const candleOpts = {...fixture.candleOptions};
const unCandle = gateway.subscribeCandle(candleOpts, value => seen.push(['candle', value.timestamp, value.open, value.high, value.low, value.close, value.volume]));
gateway.connect(); const socket = sockets.at(-1); assert.deepEqual(Object.keys(socket.handlers), ['open', 'message', 'close', 'error']); socket.open();
assert.equal(socket.sent.length, 5);
socket.message({type: 'data', exchangeId: 'bybit', channel: 'tickers', data: [fixture.ticker]});
socket.message({type: 'data', exchangeId: 'bybit', channel: 'ticker', data: {...fixture.ticker, bid: 163, ask: 167}});
assert.equal(gp.store.tickers[0].bid, 163); assert.equal(gp.store.tickers[0].ask, 167);
const markerInfo = {'$serde_json::private::Number':'17'};
socket.message({type:'data',exchangeId:'bybit',channel:'tickers',data:[{...fixture.ticker,bid:163,ask:167,info:markerInfo}]});
assert.deepEqual(Object.keys(gp.store.tickers[0].info), ['$serde_json::private::Number'], 'source literal info own key');
assert.equal(gp.store.tickers[0].info['$serde_json::private::Number'], '17', 'source literal info string');
assert.equal(gp.store.tickers[0].bid, 163, 'source literal info sibling');

socket.message({type: 'data', exchangeId: 'bybit', channel: 'market', data: [fixture.market]});
assert.equal(gp.store.markets[0].symbol, 'ADAPT:BTC:PERP');
socket.message({type: 'data', exchangeId: 'bybit', channel: 'orderbook', symbol: 'BTC:PERP', data: fixture.orderbook});
socket.message({type: 'data', exchangeId: 'bybit', channel: 'candle', symbol: 'BTC:PERP', interval: '1m', data: fixture.candle});
assert.deepEqual(seen, [['book', 91, 11], ['candle', 1700, 201, 223, 197, 211, 31]]);
socket.message({type: 'data', exchangeId: 'bybit', channel: 'orderbook', symbol: 'UNSUBSCRIBED', data: null});
socket.message({type: 'data', exchangeId: 'bybit', channel: 'candle', symbol: 'UNSUBSCRIBED', interval: '1m', data: null});
socket.message({type: 'subscribed', exchangeId: 'bybit', channel: 'orderbook', symbol: 'BTC:PERP'});
socket.message({type: 'unsubscribed', exchangeId: 'bybit', channel: 'candle'});
socket.message({type: 'error', exchangeId: 'bybit', error: 'refusal'});
socket.message({type: 'pong', exchangeId: 'bybit'});
assert.deepEqual(gp.logs.slice(-3).map(row => row[0]), ['[bybit:Gateway] Subscribed to orderbook:BTC:PERP', '[bybit:Gateway] Unsubscribed from candle', '[bybit:Gateway] Gateway error: refusal']);
function fire() { const [id, callback] = timers.entries().next().value; timers.delete(id); callback(); }
now = 7000; fire(); assert.equal(socket.sent.filter(m => m.type === 'ping').length, 0);
now = 7001; fire(); assert.equal(socket.sent.filter(m => m.type === 'ping').length, 1);
now = 8000; fire(); assert.equal(socket.sent.filter(m => m.type === 'ping').length, 1);
now = 9000; socket.message('bad json'); assert.equal(gateway.lastMessageTimestamp, 9000);
now = 10000; socket.message({exchangeId: 'other'}); assert.equal(gateway.lastMessageTimestamp, 10000);
now = 30000; assert.equal(gateway.isHealthy, false); fire(); assert.equal(socket.closes, 0);
now = 30001; fire(); assert.equal(socket.closes, 1); assert.equal(gateway.lastMessageTimestamp, undefined); assert.equal(scheduled.at(-1), 1500);
fire(); const replacement = sockets.at(-1); replacement.open(); assert.equal(replacement.sent.length, 5);
candleOpts.symbol = 'CHANGED'; candleOpts.interval = '5m'; unCandle(); const changedUnsubscribe = replacement.sent.at(-1);
const live = [];
let removeLater;
const third = () => live.push('third');
gateway.subscribeOrderbook('LIVE', () => { live.push('first'); removeLater(); gateway.subscribeOrderbook('LIVE', third); });
removeLater = gateway.subscribeOrderbook('LIVE', () => live.push('removed'));
replacement.message({type: 'data', exchangeId: 'bybit', channel: 'orderbook', symbol: 'LIVE', data: fixture.orderbook});
assert.deepEqual(live, ['first', 'third']);
const sentinel = new Error('first callback refusal'); let afterThrow = false;
gateway.subscribeOrderbook('FAIL', () => { throw sentinel; }); gateway.subscribeOrderbook('FAIL', () => { afterThrow = true; });
assert.throws(() => replacement.message({type: 'data', exchangeId: 'bybit', channel: 'orderbook', symbol: 'FAIL', data: fixture.orderbook}), error => error === sentinel);
assert.equal(afterThrow, false);
gateway.dispose(); assert.equal(gateway.isConnected, false); assert.equal(gateway.isInitialized, true);
replacement.handlers.open(); assert.equal(gateway.isConnected, false, 'late source open still executes with missing socket');
replacement.handlers.error({message: ''}); assert.equal(gp.logs.at(-1)[0], '[bybit:Gateway] Gateway WebSocket error: Unknown error');
const gatewaySummary = {seen, live, nativeBid: gp.store.tickers[0].bid, nativeAsk: gp.store.tickers[0].ask, scheduled, logs: gp.logs};
timers.clear(); now = 1000;
const cap = new external.ExternalPublicDataProvider(parent(), {url: 'wss://fixture', exchangeId: 'bybit'});
cap.connect(); const before = sockets.length;
for (let i = 0; i < 99; i++) { sockets.at(-1).handlers.close(); assert.equal(timers.size, 1); fire(); }
assert.equal(sockets.length - before, 99);
sockets.at(-1).message('bad json'); assert.equal(cap.lastMessageTimestamp, 1000); assert.equal(cap.isInitialized, false);
for (let i = 0; i < 100; i++) { sockets.at(-1).handlers.close(); assert.equal(timers.size, 1, 'source reconnect reset allows each turn'); fire(); }
assert.equal(sockets.length - before, 199);
sockets.at(-1).handlers.close(); assert.equal(timers.size, 0, 'source 101st close cap after reset');
const reentrantParent = parent();
const seedFlags = {tickers:true, markets:false};
const seedSource = {store:{tickers:[{...fixture.ticker}], markets:[{...fixture.market}], loaded:seedFlags, subscribe(){return()=>{};}}};
const seedUpdate = reentrantParent.store.update;
reentrantParent.store.update = function(update) { seedUpdate.call(this, update); seedFlags.markets=true; };
new InProcessPublicDataProvider(reentrantParent, seedSource).connect();
assert.equal(reentrantParent.updates.length,2,'PROVIDER source retained loaded flag reread after ticker notification');
assert.equal(reentrantParent.store.markets[0].symbol,'ADAPT:BTC:PERP','PROVIDER source adapted market after reentrant loaded mutation');
const unit = String.fromCharCode(0xd800);
const unitParent = parent();
const unitGateway = new external.ExternalPublicDataProvider(unitParent, {url:'wss://fixture', exchangeId:unit, authToken:unit});
let unitBooks=0, unitCandles=0;
unitGateway.subscribeOrderbook(unit, () => unitBooks++);
unitGateway.subscribeCandle({...fixture.candleOptions,symbol:unit}, () => unitCandles++);
unitGateway.connect();const unitSocket=sockets.at(-1);unitSocket.open();
assert.equal(unitSocket.sent[0].data.token.charCodeAt(0),0xd800,'PROVIDER source transmitted auth token unit');
assert.equal(unitSocket.sent[0].exchangeId.charCodeAt(0),0xd800,'PROVIDER source transmitted exchange ID unit');
assert.equal(unitSocket.sent.find(m=>m.channel==='orderbook').symbol.charCodeAt(0),0xd800,'PROVIDER source transmitted book subscription unit');
assert.equal(unitSocket.sent.find(m=>m.channel==='candle').symbol.charCodeAt(0),0xd800,'PROVIDER source transmitted candle subscription unit');
const sendUnit=(channel,data,extra={})=>unitSocket.message({type:'data',exchangeId:unit,channel,data,...extra});
sendUnit('tickers',[{...fixture.ticker,symbol:unit,bid:1},{...fixture.ticker,symbol:'\ufffd',bid:2}]);
assert.equal(unitParent.store.tickers[0].symbol.charCodeAt(0),0xd800,'PROVIDER source stored batch unit');
sendUnit('ticker',{...fixture.ticker,symbol:unit,bid:42});
assert.deepEqual(unitParent.store.tickers.map(t=>t.bid),[42,2],'PROVIDER source exact symbol update distinguishes replacement');
sendUnit('orderbook',fixture.orderbook,{symbol:unit});
sendUnit('candle',fixture.candle,{symbol:unit,interval:'1m'});
assert.equal(unitBooks,1,'PROVIDER source exact book callback selection');
assert.equal(unitCandles,1,'PROVIDER source generated valid candle callback selection');
sendUnit('market',[{...fixture.market,symbol:unit}]);
assert.equal(unitParent.store.markets[0].symbol.charCodeAt(6),0xd800,'PROVIDER source adapted stored market unit');
const beforeReopen=unitSocket.sent.length;unitSocket.open();
assert.equal(unitSocket.sent.slice(beforeReopen).find(m=>m.channel==='orderbook').symbol.charCodeAt(0),0xd800,'PROVIDER source resubscribed key unit');
const unitInParent=parent(), unitDelegated=[];
const unitInSource={store:{tickers:[{...fixture.ticker,symbol:unit}],markets:[],loaded:{tickers:true,markets:false},subscribe(){return()=>{};}},listenOrderBook(symbol){unitDelegated.push(symbol);return()=>{};},listenOHLCV(options){unitDelegated.push(options.symbol);return()=>{};}};
const unitIn=new InProcessPublicDataProvider(unitInParent,unitInSource);unitIn.connect();
assert.equal(unitInParent.store.tickers[0].symbol.charCodeAt(0),0xd800,'PROVIDER source in-process seeded batch unit');
unitIn.subscribeOrderbook(unit,()=>{});unitIn.subscribeCandle({...fixture.candleOptions,symbol:unit},()=>{});
assert.equal(unitDelegated.length,2,'PROVIDER source independent delegated subscription floor');
for(let i=0;i<2;i++)assert.equal(unitDelegated[i].charCodeAt(0),0xd800,'PROVIDER source delegated symbol unit');
// Outbound field assertions follow every native state/payload assertion.
assert.equal(socket.sent[0].channel, 'auth'); assert.equal(socket.sent[0].data.token, 'secret');
assert.equal(socket.sent[3].symbol, 'BTC:PERP'); assert.equal(socket.sent[4].interval, '1m');
assert.equal(changedUnsubscribe.symbol, 'CHANGED'); assert.equal(changedUnsubscribe.interval, '5m');
// Native JS values were asserted before serializing source observations.
const auditParent = parent();
const auditGateway = new external.ExternalPublicDataProvider(auditParent, {url:'wss://fixture',exchangeId:'binance',authToken:'audit-token'});
auditGateway.connect(); const auditSocket=sockets.at(-1); auditSocket.open();
assert.deepEqual(auditSocket.sent[0], {type:'subscribe',exchangeId:'binance',channel:'auth',data:{token:'audit-token'}});
auditSocket.message({type:'subscribed',exchangeId:'binance',channel:'auth'});
assert.ok(auditParent.logs.some(([message])=>message.endsWith('Subscribed to auth')));
auditSocket.message({type:'future-type',exchangeId:'binance',channel:'future-channel'});
auditSocket.message({type:'data',exchangeId:'binance',channel:'future-channel',data:null});
assert.equal(auditParent.updates.length,0,'GATEWAY_ENVELOPE unknown tags are ignored');
for (const [channel,data] of [['market',{...fixture.market,symbol:'GWUSDT'}],['tickers',{...fixture.ticker,symbol:'GWUSDT',last:10}]]) {
  auditSocket.message({type:'data',exchangeId:'binance',channel,data:[data],symbol:null,interval:{unused:true},error:17,requestId:false,extra:{keep:'unused'}});
}
assert.equal(auditParent.store.markets[0].symbol,'ADAPT:GWUSDT');
assert.equal(auditParent.store.tickers[0].last,10);
const gatewayTypeAudit={auth:true,markets:auditParent.store.markets.length,tickers:auditParent.store.tickers.length,last:auditParent.store.tickers[0].last};
auditGateway.dispose();
process.stdout.write(JSON.stringify({inProcess, gateway: gatewaySummary, gatewayTypeAudit}));
