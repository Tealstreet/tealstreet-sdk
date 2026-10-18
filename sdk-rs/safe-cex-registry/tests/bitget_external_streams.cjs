// Execute the actual exchange and socket stream bodies with a disconnected
// socket, as after Bitget's gateway-only startup. Only their dependencies are
// supplied; routing and cleanup remain source code.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'node_modules/typescript'));
const venue = path.join(root, 'packages/safe-cex/src/exchanges/bitget');
function sourceClass(file, ownerName) {
  const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const owner = source.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === ownerName);
  assert(owner, ownerName);
  const fields = ['listenOrderBook', 'listenOHLCV'].map(name => {
    const field = owner.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(source) === name);
    assert(field?.initializer, `${ownerName}.${name}`);
    return `${name} = ${field.initializer.getText(source)};`;
  });
  const typesFile = path.join(venue, 'bitget.types.ts');
  const types = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
  const interval = types.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations]).find(d => d.name.getText(types) === 'INTERVAL');
  assert(interval?.initializer, 'actual Bitget interval mapping');
  const code = ts.transpileModule(`const INTERVAL = ${interval.initializer.getText(types)}; class Source {${fields.join('\n')}}`, {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
  return new Function('setTimeout', 'clearTimeout', `${code}; return Source;`)(schedule, cancel);
}
let waits = 0;
const timers = new Map();
function schedule(callback, delay) {
  assert.equal(delay, 100);
  const id = ++waits;
  timers.set(id, callback);
  return id;
}
function cancel(id) { assert(timers.delete(id), 'cleanup cancels pending connection wait'); }
const Exchange = sourceClass(path.join(venue, 'bitget.exchange.ts'), 'BitgetExchange');
const Socket = sourceClass(path.join(venue, 'bitget.ws-public.ts'), 'BitgetPublicWebsocket');
const source = new Exchange();
let bookSubscriptions = 0, candleSubscriptions = 0, directCalls = 0, books = 0, candles = 0;
source.externalPublicDataProvider = {
  subscribeOrderbook() { bookSubscriptions++; return () => {}; },
  subscribeCandle() { candleSubscriptions++; return () => {}; },
};
const socket = new Socket();
Object.assign(socket, {parent:source, isConnected:false, isDisposed:false, messageHandlers:{}, topics:{}, send(){directCalls++;}});
source.publicWebsocket = socket;
const book = source.listenOrderBook('GWUSDT', () => books++);
const candle = source.listenOHLCV({symbol:'GWUSDT', interval:'1m'}, () => candles++);
assert.equal(waits, 2, 'both actual socket bodies wait for direct connection');
assert.equal(timers.size, 2);
book(); candle();
assert.equal(timers.size, 0, 'cleanup before connection cancels both waits');
assert.equal(directCalls, 0);
assert.equal(bookSubscriptions + candleSubscriptions, 0, 'Bitget overrides bypass gateway-aware base defaults');
console.log(JSON.stringify({bookSubscribed:bookSubscriptions > 0, candleSubscribed:candleSubscriptions > 0, books, candles}));
