const fs = require('node:fs');
const path = require('node:path');
const { ts, root, values } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const printer = ts.createPrinter();
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const names = ['getFapiPath', 'fetchPublicTrades'];
const members = names.map(name => {
  const node = cls.members.find(n => n.name?.getText(ast) === name);
  if (!node) throw Error(`Missing ${name}`);
  let printed = printer.printNode(ts.EmitHint.Unspecified, node, ast);
  if (name === 'fetchPublicTrades' && process.env.ASTERDEX_TRADE_CONTROL === 'lowercase-side') {
    const original = "('SELL' as OrderSide)";
    if (!printed.includes(original)) throw Error('Public trade side control source missing');
    printed = printed.replace(original, "('sell' as OrderSide)");
  }
  return printed;
}).join('\n');

const source = `export class Oracle { apiVersion:any; publicAPI:any; log:any;\n${members}\n}`;
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const loaded = { exports: {} };
new Function('exports', 'getAsterDEXFapiPath', 'LogSeverity', compiled)(
  loaded.exports,
  values.getAsterDEXFapiPath,
  { Error: 'error' },
);
const Oracle = loaded.exports.Oracle;
const encode = value => JSON.parse(JSON.stringify(value, (_, v) =>
  typeof v === 'number' && (!Number.isFinite(v) || Object.is(v, -0))
    ? { number: Object.is(v, -0) ? '-0' : String(v) }
    : v));

const trades = [
  { id: 0, price: '0x10', qty: '12abc', quoteQty: 'ignored', time: -0, isBuyerMaker: true },
  { id: 1e21, price: '-0', qty: '1e309', quoteQty: 'ignored', time: 1e21, isBuyerMaker: false },
  { id: 9007199254740992, price: '', qty: 'Infinity', quoteQty: 'ignored', time: 3.5, isBuyerMaker: true },
];
const oracle = new Oracle();
oracle.apiVersion = 'v1';
let request;
oracle.publicAPI = { get: async (url, options) => {
  request = { url, params: options.params };
  return { data: trades };
} };
oracle.log = () => {};
oracle.fetchPublicTrades('BTCUSDT', 37).then(output => {
  const sideTypes = output.map(trade => typeof trade.side);
  process.stdout.write(JSON.stringify({ trades, symbol: 'BTCUSDT', request, sideTypes, output: encode(output) }));
}).catch(error => {
  process.stderr.write(`${error.name}: ${error.message}\n`);
  process.exitCode = 1;
});
