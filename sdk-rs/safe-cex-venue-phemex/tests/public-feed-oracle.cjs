const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const input = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures/public-feed.json'), 'utf8'));
assert.equal(crypto.createHash('sha256').update(JSON.stringify(input.frames)).digest('hex'),
  '30fadb12521d4bbd30033f80d396ccbe10645b8950b68280d9a5daced9cf01ea');

function members(file, names) {
  const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const cls = ast.statements.find(n => ts.isClassDeclaration(n) &&
    (path.basename(file) !== 'base.ws.ts' || n.name?.text === 'BaseWebSocket'));
  return names.map(name => {
    const found = cls.members.filter(n => n.name?.getText(ast) === name);
    assert.equal(found.length, 1, name);
    return found[0].getText(ast);
  }).join('\n');
}
const venue = path.join(root, 'packages/safe-cex/src/exchanges/phemex');
const body = members(path.join(venue, 'phemex.ws-public.ts'),
  ['listenOHLCV', 'getKlineInterval', 'dispatchTopic', 'onMessage', 'ping']);
const timeframes = members(path.join(venue, 'phemex.exchange.ts'), ['timeframes']);
const pong = members(path.join(root, 'packages/safe-cex/src/exchanges/base.ws.ts'), ['handlePongEvent']);
const parseSource = fs.readFileSync(path.join(root, 'packages/json-parse/src/jsonParse.ts'), 'utf8');
let now = 1;
const context = vm.createContext({ performance: { now: () => now }, setTimeout: () => 1, clearTimeout() {} });
const source = `${parseSource.replace(/export /g, '')}
class Timeframes { ${timeframes} }
class Subject { ${body}\n${pong} }
globalThis.api = new Subject(); globalThis.timeframes = new Timeframes().timeframes;`;
vm.runInContext(ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
}).outputText.replace(/export /g, ''), context);
const api = context.api;
const candles = [], latencies = [];
api.parent = { timeframes: context.timeframes };
api.topics = {};
api.messageHandlers = {};
api.isConnected = true;
api.isDisposed = false;
api.requestId = 1200;
api.trackLastReceived = () => {};
api.recordMessageHandlerStats = () => {};
api.updateSubscriptionLimitWarning = () => {};
api.send = () => {};
api.shouldEnforcePongMissReconnect = () => false;
api.store = { update: ({ latency }) => latencies.push(latency) };
api.listenOHLCV({ symbol: input.symbol, interval: '1m' }, candle => candles.push(candle));
assert.equal(input.frames.length, 906);
let pongs = 0, kline = 0, books = 0;
for (const frame of input.frames) {
  const raw = Buffer.from(frame.base64, 'base64').toString('utf8');
  const json = JSON.parse(raw);
  now = frame.atMs + 1;
  if (json.kline_p) kline++;
  if (json.orderbook_p) books++;
  if (json.result === 'pong' || json.method === 'server.ping') {
    // Controlled elapsed times prevent unrelated scheduled pings in the native witness.
    const at = 100 + pongs * 100;
    now = at - [0, 7, 18, 1, 30, 4][pongs % 6];
    api.ping();
    now = at;
    pongs++;
  }
  api.onMessage({ data: raw });
}
assert.equal(kline, 16);
assert.equal(books, 854);
assert.equal(pongs, 12);
assert.equal(candles.length, 19);
for (const candle of candles) {
  assert.equal(typeof candle.timestamp, 'number');
  for (const key of ['open', 'high', 'low', 'close', 'volume']) assert.equal(typeof candle[key], 'string');
}
assert.deepEqual(latencies, [0, 4, 9, 1, 15, 2, 0, 4, 9, 1, 15, 2]);
process.stdout.write(JSON.stringify({ candles, latencies }));
