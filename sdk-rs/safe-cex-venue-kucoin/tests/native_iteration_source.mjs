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
