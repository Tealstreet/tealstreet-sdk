const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'BitgetExchange');
const names = ['listenOHLCV', 'listenOrderBook'];
const members = names.map(name => {
  const member = klass.members.find(n => n.name?.getText(ast) === name);
  assert(member && ts.isPropertyDeclaration(member), `SUBSCRIPTION actual ${name}`);
  return member.getText(ast);
});
// Execute the actual source bodies. Transpilation erases source types; this
// supplement does not claim semantic TypeScript checking or socket assembly.
const emitted = ts.transpileModule(`class Source { ${members.join('\n')} }`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const Source = new Function(emitted + ';return Source;')();
const source = new Source();
const options = {symbol:'BTCUSDT',interval:'1m',from:-0,to:937,limit:23};
const symbol = 'ETHUSDT';
const candle = {timestamp:101,open:11,high:17,low:7,close:13,volume:29,lastSide:'buy'};
const book = {bids:[{price:103,amount:3,total:309}],asks:[{price:107,amount:5,total:535}]};
const calls = [], events = [], cleanups = [];
const candleCallback = value => { assert.strictEqual(value, candle, 'SUBSCRIPTION source candle identity'); events.push('candle'); };
const bookCallback = value => { assert.strictEqual(value, book, 'SUBSCRIPTION source book identity'); events.push('book'); };
const candleCleanup = () => cleanups.push('candle');
const bookCleanup = () => cleanups.push('book');
source.publicWebsocket = {
  listenOHLCV(received, callback) {
    calls.push('listenOHLCV');
    assert.strictEqual(received, options, 'SUBSCRIPTION source options identity');
    assert(Object.is(received.from, -0), 'SUBSCRIPTION source signed zero');
    assert.strictEqual(callback, candleCallback, 'SUBSCRIPTION source candle callback identity');
    callback(candle);
    return candleCleanup;
  },
  listenOrderBook(received, callback) {
    calls.push('listenOrderBook');
    assert.equal(received, symbol, 'SUBSCRIPTION source symbol');
    assert.strictEqual(callback, bookCallback, 'SUBSCRIPTION source book callback identity');
    callback(book);
    return bookCleanup;
  },
};
const returnedCandle = source.listenOHLCV(options, candleCallback);
const returnedBook = source.listenOrderBook(symbol, bookCallback);
assert.strictEqual(returnedCandle, candleCleanup, 'SUBSCRIPTION source candle cleanup identity');
assert.strictEqual(returnedBook, bookCleanup, 'SUBSCRIPTION source book cleanup identity');
assert.deepEqual(cleanups, [], 'SUBSCRIPTION source cleanup deferred');
returnedCandle(); returnedBook();
assert.deepEqual(calls, names, 'SUBSCRIPTION source calls');
assert.deepEqual(events, ['candle','book'], 'SUBSCRIPTION source callbacks');
assert.deepEqual(cleanups, ['candle','book'], 'SUBSCRIPTION source cleanup calls');
process.stdout.write(JSON.stringify({calls,events,cleanups}) + '\n');
