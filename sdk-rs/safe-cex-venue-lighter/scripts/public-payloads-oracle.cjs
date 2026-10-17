const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/lighter/lighter.ws-public.ts');
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const owner = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'LighterPublicWebsocket');
const members = ['parseRestOrderBookLevels', 'parseRestTickerItems'].map(name => {
  const member = owner.members.find(n => n.name?.getText(tree) === name);
  assert.ok(member && ts.isPropertyDeclaration(member) && ts.isArrowFunction(member.initializer), name);
  return member.getText(tree);
});
const output = {};
vm.runInNewContext(ts.transpileModule(`class Subject { ${members.join('\n')} } exports.subject = new Subject();`,
  { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, { exports: output });
const subject = output.subject;
const native = [];
for (const kind of ['array', 'object']) {
  const row = { price: NaN, amount: Infinity, zero: -0, label: '\ud800', missing: undefined };
  row.cycle = row;
  const bids = [, row, undefined], asks = [];
  const tickers = kind === 'array' ? [, row, undefined] : { 2: row, 1: undefined };
  const payload = { order_book: { bids, asks }, order_book_stats: tickers };
  const book = subject.parseRestOrderBookLevels(payload), items = subject.parseRestTickerItems(payload);
  native.push({ kind, bidsIdentity: book.bids === bids, asksIdentity: book.asks === asks,
    tickerIdentity: items === tickers, bidHole: !Object.hasOwn(book.bids, 0),
    tags: Array.from(items, (value, i) => !Object.hasOwn(items, i) ? 'hole' : value === row ? 'row' : value === undefined ? 'undefined' : 'other'),
    rowIdentity: items.filter(value => value !== undefined).every(value => value === row),
    nan: Number.isNaN(row.price), infinity: row.amount === Infinity, zero: Object.is(row.zero, -0),
    units: [row.label.charCodeAt(0)], ownUndefined: Object.hasOwn(row, 'missing') && row.missing === undefined,
    cycle: row.cycle === row });
}
const reads = [];
for (const method of ['parseRestOrderBookLevels', 'parseRestTickerItems']) {
  const trace = [], row = {}, bids = [row], book = {};
  Object.defineProperty(book, 'bids', { get() { trace.push('bids'); return bids; } });
  Object.defineProperty(book, 'asks', { get() { trace.push('asks'); return []; } });
  const payload = { order_book: book, order_book_stats: [row] };
  for (const name of ['data', 'result']) Object.defineProperty(payload, name, {
    get() { trace.push(name); return {}; }
  });
  subject[method](payload);
  reads.push({ method, trace });
}
process.stdout.write(JSON.stringify({ native, reads }));
