const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

let source = fs.readFileSync(
  path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts'),
  'utf8',
);
if (process.env.ASTERDEX_PUBLIC_TRADE_CONTROL === 'swap-side') {
  const original = "side: data.m ? 'SELL' : 'BUY'";
  if (!source.includes(original)) throw Error('handleTradeUpdate source control target missing');
  source = source.replace(original, "side: data.m ? 'BUY' : 'SELL'");
}
const ast = ts.createSourceFile('public-ws.ts', source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(ast) === 'handleTradeUpdate');
if (!member) throw Error('Missing handleTradeUpdate');
const method = ts.createPrinter().printNode(ts.EmitHint.Unspecified, member, ast);
const compiled = ts.transpileModule(
  'export class AsterDEXPublicWebsocket {' + method + '}',
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } },
).outputText;
const output = { exports: {} };
new Function('exports', 'module', compiled)(output.exports, output);
const numberBits = value => {
  if (Number.isNaN(value)) return '7ff8000000000000';
  if (process.env.ASTERDEX_PUBLIC_TRADE_CONTROL === 'erase-zero' && Object.is(value, -0)) value = 0;
  if (process.env.ASTERDEX_PUBLIC_TRADE_CONTROL === 'erase-finite' && value === 12.5) value = 13;
  const bytes = Buffer.allocUnsafe(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const encode = value => JSON.parse(JSON.stringify(value, (_, v) =>
  typeof v === 'number' ? { numberBits: numberBits(v) } : v,
));
const cases = [
  { name: 'maker-sell', input: { e: 'aggTrade', E: 10, s: 'BTCUSDT', a: 101, p: '12.5px', q: '2.25', f: 102, l: 103, T: 20, m: true }, ticker: true },
  { name: 'taker-buy-no-ticker', input: { e: 'aggTrade', E: 11, s: 'ETHUSDT', a: 201, p: '-0', q: 'Infinity', f: 202, l: 203, T: 21, m: false }, ticker: false },
  { name: 'constructor-symbol', input: { e: 'aggTrade', E: 12, s: 'constructor', a: 301, p: '1', q: '2', f: 302, l: 303, T: 22, m: false }, ticker: false },
  { name: 'proto-symbol', input: { e: 'aggTrade', E: 13, s: '__proto__', a: 401, p: '3', q: '4', f: 402, l: 403, T: 23, m: true }, ticker: false },
  { name: 'nan-price', input: { e: 'aggTrade', E: 14, s: 'NANUSDT', a: 501, p: 'NaN', q: '1', f: 502, l: 503, T: 24, m: false }, ticker: false },
];
for (const [index, symbol] of ['toString', 'toLocaleString', 'valueOf', 'hasOwnProperty', 'isPrototypeOf', 'propertyIsEnumerable', '__defineGetter__', '__defineSetter__', '__lookupGetter__', '__lookupSetter__'].entries()) {
  cases.push({ name: `prototype-${symbol}`, input: { e: 'aggTrade', E: 30 + index, s: symbol, a: 601 + index, p: String(11 + index), q: String(21 + index), f: 701 + index, l: 801 + index, T: 40 + index, m: index % 2 === 0 }, ticker: false });
}
for (const c of cases) {
  const updates = [];
  const trades = [];
  const ticker = { symbol: c.input.s };
  const tickersBySymbol = {};
  if (c.ticker) tickersBySymbol[c.input.s] = ticker;
  const instance = new output.exports.AsterDEXPublicWebsocket();
  instance.parent = {
    store: {
      tickersBySymbol,
      updateTicker: (target, update) => updates.push({
        targetType: typeof target,
        targetString: String(target),
        update,
      }),
    },
    emitter: { emit: (name, trade) => trades.push({ name, trade }) },
  };
  instance.handleTradeUpdate(c.input);
  const firstUpdate = updates[0];
  c.output = {
    updates: encode(updates),
    trades: encode(trades),
    tickerHit: !firstUpdate
      ? { kind: 'missing' }
      : c.ticker
        ? { kind: 'mapped' }
        : { kind: 'inherited', member: firstUpdate.targetString },
  };
  c.captureArchitecture = `${process.platform}-${process.arch}`;
  c.captureNodeMajor = Number(process.versions.node.split('.')[0]);
}
process.stdout.write(JSON.stringify({ captureArchitecture: `${process.platform}-${process.arch}`, captureNodeMajor: Number(process.versions.node.split('.')[0]), cases }));
