const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

let source = fs.readFileSync(
  path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts'),
  'utf8',
);
if (process.env.ASTERDEX_DEPTH_CONTROL === 'allow-zero') {
  const original = 'price <= 0 || amount <= 0';
  if (!source.includes(original)) throw Error('handleDepthSnapshot source control target missing');
  source = source.replace(original, 'price <= 0 || amount < 0');
}
const ast = ts.createSourceFile('public-ws.ts', source, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(ast) === 'handleDepthSnapshot');
if (!member) throw Error('Missing handleDepthSnapshot');
const method = ts.createPrinter().printNode(ts.EmitHint.Unspecified, member, ast);
const compiled = ts.transpileModule(
  'export class AsterDEXPublicWebsocket {' + method + '}',
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } },
).outputText;
const output = { exports: {} };
new Function('exports', 'module', 'calcOrderBookTotal', compiled)(
  output.exports,
  output,
  orderBook => {
    for (const side of [orderBook.bids, orderBook.asks]) {
      let total = 0;
      for (let i = 0; i < side.length; i++) {
        total += side[i].amount;
        side[i].total = total;
      }
    }
  },
);

const cases = [
  { name: 'sorted-and-callback-isolation', input: { s: 'BTCUSDT', b: [['2', '3'], ['4', '5'], ['3', '7']], a: [['8', '11'], ['6', '13'], ['7', '17']] }, ticker: true, subscribers: 2 },
  { name: 'fractional-levels', input: { s: 'ETHUSDT', b: [['1.25', '0.5']], a: [['1.5', '0.75']] }, ticker: true, subscribers: 1 },
  { name: 'hex-number', input: { s: 'HEXUSDT', b: [['0x10', '2']], a: [['17', '3']] }, ticker: true, subscribers: 1 },
  { name: 'zero-quantity', input: { s: 'ZEROUSDT', b: [['1', '0']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'empty-bids', input: { s: 'EMPTYUSDT', b: [], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'crossed-book', input: { s: 'CROSSUSDT', b: [['2', '1']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'numeric-prefix', input: { s: 'PREFIXUSDT', b: [['1junk', '1']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'numeric-json-value', input: { s: 'NUMUSDT', b: [[1, '1']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'wrong-pair-length', input: { s: 'PAIRUSDT', b: [['1', '1', 'extra']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'empty-string', input: { s: 'STRINGUSDT', b: [[' ', '1']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'bom-only', input: { s: 'BOMUSDT', b: [['\ufeff', '1']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'null-bids', input: { s: 'NULLUSDT', b: null, a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'infinite-level', input: { s: 'INFUSDT', b: [['Infinity', '1']], a: [['2', '1']] }, ticker: true, subscribers: 1 },
  { name: 'missing-asks', input: { s: 'MISSINGUSDT', b: [['1', '1']] }, ticker: true, subscribers: 1 },
  { name: 'book-without-ticker', input: { s: 'NOTICKERUSDT', b: [['1', '1']], a: [['2', '1']] }, ticker: false, subscribers: 1 },
];

for (const c of cases) {
  const tickerUpdates = [];
  const callbacks = [];
  const tickersBySymbol = {};
  if (c.ticker) tickersBySymbol[c.input.s] = { symbol: c.input.s };
  const instance = new output.exports.AsterDEXPublicWebsocket();
  instance.parent = {
    store: {
      tickersBySymbol,
      updateTicker: (target, update) => tickerUpdates.push({ target, update }),
    },
  };
  const listeners = new Map();
  for (let index = 0; index < c.subscribers; index++) {
    listeners.set(index, book => {
      callbacks.push(JSON.parse(JSON.stringify(book)));
      if (index === 0 && c.subscribers > 1) book.bids[0].amount = 999;
    });
  }
  instance.orderBookSubscriptions = new Map([
    [`${c.input.s.toLowerCase()}@depth20@100ms`, { callbacks: listeners }],
  ]);
  instance.handleDepthSnapshot(c.input);
  c.output = { tickerUpdates, callbacks };
}
if (process.env.ASTERDEX_DEPTH_CONTROL === 'empty-callbacks') {
  const target = cases.find(c => c.name === 'fractional-levels');
  if (!target) throw Error('Missing fractional-levels callback control target');
  target.output.callbacks = [];
}
process.stdout.write(JSON.stringify({
  captureArchitecture: `${process.platform}-${process.arch}`,
  captureNodeMajor: Number(process.versions.node.split('.')[0]),
  cases,
}));
