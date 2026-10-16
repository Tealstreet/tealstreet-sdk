const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

let source = fs.readFileSync(
  path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts'),
  'utf8',
);
if (process.env.ASTERDEX_MARK_PRICE_CONTROL === 'swap-index') {
  const original = 'update.index = parseFloat(data.i)';
  if (!source.includes(original)) throw Error('handleMarkPriceUpdate source control target missing');
  source = source.replace(original, 'update.index = parseFloat(data.p)');
}
const ast = ts.createSourceFile('public-ws.ts', source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(ast) === 'handleMarkPriceUpdate');
if (!member) throw Error('Missing handleMarkPriceUpdate');
const method = ts.createPrinter().printNode(ts.EmitHint.Unspecified, member, ast);
const compiled = ts.transpileModule(
  'export class AsterDEXPublicWebsocket {' + method + '}',
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } },
).outputText;
const output = { exports: {} };
new Function('exports', 'module', compiled)(output.exports, output);
const numberBits = value => {
  if (Number.isNaN(value)) return '7ff8000000000000';
  if (process.env.ASTERDEX_MARK_PRICE_CAPTURE_CONTROL === 'erase-zero' && Object.is(value, -0)) value = 0;
  if (process.env.ASTERDEX_MARK_PRICE_CAPTURE_CONTROL === 'erase-finite' && value === 1.25) value = 1.5;
  const bytes = Buffer.allocUnsafe(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
};
const encode = value => JSON.parse(JSON.stringify(value, (_, v) =>
  typeof v === 'number' ? { numberBits: numberBits(v) } : v,
));
const cases = [
  { name: 'all-fields', input: { s: 'BTCUSDT', p: '12.5px', i: '21.25', r: '30.75', T: 5000 }, ticker: true },
  { name: 'empty-guards', input: { s: 'BTCUSDT', p: '', i: '0', r: null, T: 0 }, ticker: true },
  { name: 'signed-zero-and-finite', input: { s: 'ZEROUSDT', p: '-0', i: '1.25', r: '2.5', T: 5 }, ticker: true },
  { name: 'nan-mark', input: { s: 'NANUSDT', p: 'NaN', i: '6', r: '7', T: 8 }, ticker: true },
  { name: 'missing-ticker', input: { s: 'ETHUSDT', p: '7', i: '8', r: '9', T: 10 }, ticker: false },
  { name: 'constructor-symbol', input: { s: 'constructor', p: '3', i: '4', r: '5', T: 6 }, ticker: false },
  { name: 'proto-symbol', input: { s: '__proto__', p: '13', i: '14', r: '15', T: 16 }, ticker: false },
];
for (const [index, symbol] of ['toString', 'toLocaleString', 'valueOf', 'hasOwnProperty', 'isPrototypeOf', 'propertyIsEnumerable', '__defineGetter__', '__defineSetter__', '__lookupGetter__', '__lookupSetter__'].entries()) {
  cases.push({ name: `prototype-${symbol}`, input: { s: symbol, p: String(100 + index), i: String(200 + index), r: String(300 + index), T: 400 + index }, ticker: false });
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
  instance.handleMarkPriceUpdate(c.input);
  const firstUpdate = updates[0];
  c.output = {
    updates: encode(updates),
    events: encode(events),
    tickerHit: !firstUpdate
      ? { kind: 'missing' }
      : c.ticker
        ? { kind: 'mapped' }
        : { kind: 'inherited', member: firstUpdate.targetString },
  };
}
process.stdout.write(JSON.stringify({ captureArchitecture: `${process.platform}-${process.arch}`, captureNodeMajor: Number(process.versions.node.split('.')[0]), cases }));
