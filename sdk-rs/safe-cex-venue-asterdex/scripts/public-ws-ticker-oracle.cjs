const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

let source = fs.readFileSync(
  path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts'),
  'utf8',
);
if (process.env.ASTERDEX_PUBLIC_TICKER_CONTROL === 'swap-percentage') {
  const original = 'update.percentage = parseFloat(data.P)';
  if (!source.includes(original)) throw Error('handleTickerUpdate source control target missing');
  source = source.replace(original, 'update.percentage = parseFloat(data.c)');
}
const ast = ts.createSourceFile('public-ws.ts', source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(ast) === 'handleTickerUpdate');
if (!member) throw Error('Missing handleTickerUpdate');
const method = ts.createPrinter().printNode(ts.EmitHint.Unspecified, member, ast);
const compiled = ts.transpileModule(
  'export class AsterDEXPublicWebsocket {' + method + '}',
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } },
).outputText;
const output = { exports: {} };
new Function('exports', 'module', compiled)(output.exports, output);
const numberBits = value => {
  if (Number.isNaN(value)) return '7ff8000000000000';
  if (process.env.ASTERDEX_PUBLIC_TICKER_CAPTURE_CONTROL === 'erase-zero' && Object.is(value, -0)) value = 0;
  if (process.env.ASTERDEX_PUBLIC_TICKER_CAPTURE_CONTROL === 'erase-finite' && value === 1.25) value = 1.5;
  const bytes = Buffer.allocUnsafe(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const encode = value => JSON.parse(JSON.stringify(value, (_, v) =>
  typeof v === 'number' ? { numberBits: numberBits(v) } : v,
));
const cases = [
  { name: 'all-fields', input: { s: 'BTCUSDT', c: '12.5px', P: '21.25', v: '30.75', q: '40.5' }, ticker: true },
  { name: 'empty-guards', input: { s: 'BTCUSDT', c: '', P: '0', v: null, q: false }, ticker: true },
  { name: 'signed-zero-and-finite', input: { s: 'ZEROUSDT', c: '-0', P: '1.25', v: '2.5', q: '3.75' }, ticker: true },
  { name: 'nan-last', input: { s: 'NANUSDT', c: 'NaN', P: '6', v: '7', q: '8' }, ticker: true },
  { name: 'missing-ticker', input: { s: 'ETHUSDT', c: '7', P: '8', v: '9', q: '10' }, ticker: false },
  { name: 'constructor-symbol', input: { s: 'constructor', c: '3', P: '4', v: '5', q: '6' }, ticker: false },
  { name: 'proto-symbol', input: { s: '__proto__', c: '13', P: '14', v: '15', q: '16' }, ticker: false },
];
for (const [index, symbol] of ['toString', 'toLocaleString', 'valueOf', 'hasOwnProperty', 'isPrototypeOf', 'propertyIsEnumerable', '__defineGetter__', '__defineSetter__', '__lookupGetter__', '__lookupSetter__'].entries()) {
  cases.push({ name: `prototype-${symbol}`, input: { s: symbol, c: String(100 + index), P: String(200 + index), v: String(300 + index), q: String(400 + index) }, ticker: false });
}
for (const c of cases) {
  const updates = [];
  const events = [];
  const tickersBySymbol = {};
  if (c.ticker) tickersBySymbol[c.input.s] = { symbol: c.input.s };
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
    emitter: { emit: (...args) => events.push(args) },
  };
  instance.handleTickerUpdate(c.input);
  c.output = { updates: encode(updates), events: encode(events) };
}
process.stdout.write(JSON.stringify({ captureArchitecture: `${process.platform}-${process.arch}`, captureNodeMajor: Number(process.versions.node.split('.')[0]), cases }));
