const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function subject(spot, names, globals = {}) {
  const file = path.join(root, 'packages/safe-cex/src/exchanges/bybit', spot ? 'bybitspot.exchange.ts' : 'bybit.exchange.ts');
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === (spot ? 'BybitSpotExchange' : 'BybitExchange'));
  const methods = names.map(name => {
    const hits = cls.members.filter(node => node.name?.getText(source) === name);
    assert.equal(hits.length, 1, name); return hits[0].getText(source);
  });
  vm.runInNewContext(ts.transpileModule(`class Subject {${methods.join('\n')}}globalThis.api=new Subject();`, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, globals);
  return globals.api;
}
async function main() {
  let profiles = 0;
  for (const spot of [false, true]) {
    const api = subject(spot, ['parseTickers']);
    const cache = [{opaque:'\ud800'}];
    api.store = {tickers:cache, marketsBySymbol:{BTC:{id:'native',symbol:'BTC',isInverse:true,isExpiring:true}}};
    assert.equal(api.parseTickers(null), cache);
    const row = {symbol:'BTC',get lastPrice(){api.store.marketsBySymbol={ETH:{id:'next',symbol:'ETH'}};return '2\ud800';},volume24h:'-0',turnover24h:Infinity,get unused(){throw Error('unread ticker field');}};
    const tickers = api.parseTickers([row,{symbol:'ETH',lastPrice:'3',volume24h:'1'}]);
    assert.equal(tickers.length,2);assert.equal(tickers[1].id,'next');const [ticker]=tickers;
    assert.equal(ticker.last,2); assert(Object.is(ticker.volume,spot ? -0 : Infinity));
    assert(Object.is(ticker.quoteVolume,-0)); assert(Number.isNaN(ticker.bid)); assert.equal(ticker.fundingRate,0); profiles++;
    let release, reject;
    const globals = {virtualClock:{start:()=>new Promise(resolve=>release=resolve)},LogSeverity:{Warning:'warning'}};
    const start = subject(spot, ['start'], globals);
    const logs = [], events = [];
    Object.assign(start,{options:{key:'key',secret:'secret'},isPublicDataOnly:false,
      needsAccountMarketOverlay:false,initExternalPublicData:async()=>true,
      fetchMarginAccountInfos:()=>new Promise((_,no)=>reject=no),fetchAndCacheApiKeyInfo:async()=>{},
      log:(...args)=>logs.push(args),privateWebsocket:{connectAndSubscribe:()=>events.push('connect')},
      fetchOrdersPositionsAndBalance:async()=>events.push('snapshots'),tick:()=>events.push('tick')});
    const pending = start.start(); release();
    for(let i=0;i<10;i++)await Promise.resolve();
    reject({message:'\ud800x'}); await pending;
    assert.deepEqual(events,['connect','snapshots','tick']);
    assert.deepEqual(logs,[[`Bybit market data is ready, but the private API was rejected (\ud800x) — trading is unavailable`,'warning']]);profiles++;
  }
  const raw = {symbol:'BTCUSDT',baseCoin:'BTC',quoteCoin:'USDT',status:'Trading',
    lotSizeFilter:{qtyStep:'0.001',minOrderQty:'0.001',maxOrderQty:'100'},priceFilter:{tickSize:'0.1'},
    opaque:{text:'\ud800',number:Infinity,missing:undefined}};
  const globals = {ENDPOINTS:{MARKETS:'/v5/market/instruments-info'},v:(object,key)=>object[key]};
  const api = subject(true,['fetchMarketsHelper'],globals);let calls=0;
  api.xhr={get:async(endpoint,config)=>{calls++;assert.equal(endpoint,globals.ENDPOINTS.MARKETS);assert.equal(config.params.category,'spot');return {data:{retMsg:'OK',result:{list:[raw]}}};}};
  const pending=api.fetchMarketsHelper('spot');assert.equal(calls,1);const [market]=await pending;
  assert.equal(market.info,raw);assert(Object.hasOwn(market.info.opaque,'missing'));assert.equal(market.info.opaque.text.charCodeAt(0),0xd800);profiles++;
  return {profiles};
}
module.exports=main;
if(require.main===module)main().then(value=>process.stdout.write(JSON.stringify(value)+'\n')).catch(error=>{console.error(error);process.exitCode=1;});
