import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts, loadSource} from '../../safe-cex-store/tests/source_loader.mjs';
const file = relative => new URL('../../../packages/safe-cex/src/' + relative, import.meta.url);
function tree(relative) {
  const url = file(relative);
  return ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
}
function member(source, name) {
  const cls = source.statements.find(ts.isClassDeclaration);
  const found = cls.members.find(row => row.name?.getText(source) === name);
  assert(found, 'maintained member ' + name);
  return found.getText(source);
}
const socket = tree('exchanges/kucoin/kucoin.ws-public.ts');
const exchange = tree('exchanges/kucoin/kucoin.exchange.ts');
const types = tree('exchanges/kucoin/kucoin.types.ts');
const endpoints = types.statements.find(row => ts.isVariableStatement(row) && row.declarationList.declarations.some(d => d.name.getText(types) === 'ENDPOINTS'));
assert(endpoints);
const {updateOrderBookByPrice} = loadSource(file('utils/updateOrderBook.ts'));
const {calcOrderBookTotal} = loadSource(file('utils/orderbook.ts'));
const payloads = [[['3','4']], '12', '😀', '', null, 7, {'0':['3','4'],length:1}, true];
const bits = n => {
  if (Number.isNaN(n)) return 'nan';
  const b = Buffer.alloc(8); b.writeDoubleBE(n); return b.toString('hex');
};
const observe = book => Object.fromEntries(['bids','asks'].map(side => [side, book[side].map(row => [bits(row.price),bits(row.amount),bits(row.total)])]));
if (process.argv.includes('--candles')) {
  const cases = [];
  for (const spot of [false, true]) {
    const source = spot ? tree('exchanges/kucoin/kucoinspot.ws-public.ts') : socket;
    const table = spot ? tree('exchanges/kucoin/kucoinspot.types.ts') : types;
    const declaration = (ast, name) => {
      const found = ast.statements.find(row => ts.isVariableStatement(row) && row.declarationList.declarations.some(d => d.name.getText(ast) === name));
      assert(found, 'maintained declaration ' + name); return found.getText(ast);
    };
    const module = {exports:{}};
    const code = declaration(table, spot ? 'KUCOIN_SPOT_INTERVAL' : 'WS_INTERVAL')
      + (spot ? declaration(source, 'normalizeTimestamp') : '')
      + 'export class Selected {' + member(source, 'listenOHLCV') + '}';
    vm.runInNewContext(ts.transpileModule(code, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, {module,exports:module.exports,Date,clearTimeout(){}});
    const receiver = new module.exports.Selected(), candles = [];
    Object.assign(receiver, {isConnected:true,isDisposed:false,messageHandlers:{},parent:{toApiSymbol:()=> 'XBTUSDTM'},send(){},subscribeTopic(){},unsubscribeTopic(){},cleanupMessageHandlerStats(){}});
    const cleanup = receiver.listenOHLCV({symbol:'SYM',interval:'1m'}, candle => candles.push(candle));
    assert.equal(receiver.messageHandlers.candle_SYM_1min.length,1);
    receiver.messageHandlers.candle_SYM_1min[0]({data:{candles:['1700000000','11.25','17.5','23.75','5.5','31.125']}});
    assert.equal(candles.length,1); assert.equal(Object.keys(candles[0]).length,6);
    cases.push({spot, fields:Object.fromEntries(Object.entries(candles[0]).map(([key,value])=>[key,bits(value)]))});
    cleanup(); assert.equal(receiver.messageHandlers.candle_SYM_1min?.length || 0,0);
  }
  process.stdout.write(JSON.stringify(cases)); process.exit(0);
}
const cases = [];
for (const side of ['bids','asks']) for (const [index, payload] of payloads.entries()) {
  const handlers = {}, books = []; let requests = 0;
  const module = {exports:{}};
  const code = endpoints.getText(types) + '\nexport class Parent {' + member(exchange,'toApiSymbol') + '}\nexport class Socket {' + member(socket,'listenOrderBook') + '}';
  vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, {
    module,exports:module.exports,updateOrderBookByPrice,calcOrderBookTotal,Date,
    setTimeout(){throw Error('connected fixture cannot wait');},clearTimeout(){},
  });
  const parent = new module.exports.Parent();
  parent.store = {marketsBySymbol:{BTCUSDT:{id:'XBTUSDTM'}}};
  parent.xhr = {get:async route => {
    assert.equal(route,'/api/v1/level2/snapshot'); requests++;
    return {data:{code:'200000',data:{bids:[],asks:[],sequence:1}}};
  }};
  parent.emitAndLogError = message => {throw Error('unexpected snapshot error: '+message);};
  const receiver = new module.exports.Socket();
  Object.assign(receiver,{parent,messageHandlers:handlers,isConnected:true,isDisposed:false,send(){},subscribeActiveTicker(){},unsubscribeActiveTicker(){},cleanupMessageHandlerStats(){}});
  const cleanup = receiver.listenOrderBook('BTCUSDT', book => books.push(observe(book)));
  for(let i=0;i<8;i++) await Promise.resolve();
  assert.equal(requests,1); assert.equal(books.length,1);
  assert.equal(handlers.orderBook_BTCUSDT.length,1);
  const message = {type:'update',data:{sequence:2,bids:[],asks:[],[side]:payload}};
  let error = null;
  try {handlers.orderBook_BTCUSDT[0](message);} catch(e) {error=e.name;}
  cases.push({side,index,message,error,books});
  cleanup();
}
assert.equal(cases.length,16,'two sides by eight independent iterable payloads');
process.stdout.write(JSON.stringify(cases));
