const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { root, ts } = require('../scripts/accounts-source.cjs');
const emitters = [];
let venueFiles = 0;
function inventory(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const filename = path.join(directory, entry.name);
    if (entry.isDirectory()) { inventory(filename); continue; }
    if (!filename.endsWith('.ts')) continue;
    venueFiles++;
    const ast = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
    function inspect(node) {
      if ((ts.isPropertyAssignment(node) || ts.isShorthandPropertyAssignment(node))
          && node.name.getText(ast) === 'timestamp') {
        let tickerContext = false;
        for (let owner = node.parent; owner; owner = owner.parent) {
          if (owner.name && /ticker/i.test(owner.name.getText(ast))) tickerContext = true;
        }
        if (tickerContext) assert.equal(path.relative(root, filename),
          'packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts',
          'TICKER-TIMESTAMP unclassified ticker timestamp property');
      }
      if (ts.isObjectLiteralExpression(node)) {
        const names = node.properties.map(property => property.name?.getText(ast));
        if (['id', 'symbol', 'bid', 'ask', 'timestamp'].every(name => names.includes(name))) {
          const field = node.properties.find(property => property.name?.getText(ast) === 'timestamp');
          assert(ts.isPropertyAssignment(field), 'TICKER-TIMESTAMP census requires explicit timestamp expression');
          emitters.push({ file: path.relative(root, filename), expression: field.initializer.getText(ast) });
        }
      }
      if (ts.isBinaryExpression(node) && node.operatorToken.kind === ts.SyntaxKind.EqualsToken
          && ts.isPropertyAccessExpression(node.left) && node.left.name.text === 'timestamp') {
        for (let owner = node.parent; owner; owner = owner.parent) {
          if (owner.name && /ticker/i.test(owner.name.getText(ast))) {
            assert.fail('TICKER-TIMESTAMP unclassified ticker timestamp assignment');
          }
        }
      }
      ts.forEachChild(node, inspect);
    }
    inspect(ast);
  }
}
inventory(path.join(root, 'packages/safe-cex/src/exchanges'));
assert.deepEqual(emitters, [{ file: 'packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts',
  expression: 'virtualClock.getCurrentTimeMs()' }], 'TICKER-TIMESTAMP direct emitter inventory');
const file = 'packages/safe-cex/src/exchanges/paradex/paradex.exchange.ts';
const text = fs.readFileSync(path.join(root, file), 'utf8');
const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const found = [];
function visit(node) {
  if (ts.isPropertyDeclaration(node) && node.name.getText(source) === 'fetchTickers') found.push(node);
  ts.forEachChild(node, visit);
}
visit(source);
assert.equal(found.length, 1, 'TICKER-TIMESTAMP unique actual method');
const code = ts.transpileModule(`class Subject { ${found[0].getText(source)} }; return Subject;`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
}).outputText;
const values = [123.25, -0, 987.5];
let calls = 0;
const Subject = new Function('virtualClock', 'ENDPOINTS', 'LogSeverity', 'describeError', code)(
  { getCurrentTimeMs: () => values[calls++] }, { TICKERS: '/tickers' },
  { Debug: 'debug', Error: 'error' }, String,
);
const subject = new Subject();
subject.log = () => {};
subject.xhr = { get: async () => ({ data: { results: [
  { symbol: 'A-PERP', volume_24h: '41.5' },
  { symbol: 'EXCLUDED-C', volume_24h: '999' },
  { symbol: 'B-PERP', volume_24h: '53.5' },
  { symbol: 'C-PERP', volume_24h: '67.5' },
] } }) };
function bits(value) {
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
}
subject.fetchTickers().then(tickers => {
  assert.equal(tickers.length, 3, 'TICKER-TIMESTAMP source count');
  assert.equal(calls, 3, 'TICKER-TIMESTAMP clock call count');
  const rows = tickers.map((ticker, index) => {
    assert.equal(typeof ticker.timestamp, 'number', 'TICKER-TIMESTAMP pre-serialization type');
    assert.equal(bits(ticker.timestamp), bits(values[index]), 'TICKER-TIMESTAMP clock identity');
    return { wire: JSON.stringify(ticker), timestampBits: bits(ticker.timestamp), quoteVolume: ticker.quoteVolume };
  });
  process.stdout.write(JSON.stringify({ header: { architecture: `${process.platform}-${process.arch}`,
    node: process.version, venueFiles, emitters, source: file,
    sourceSha256: crypto.createHash('sha256').update(text).digest('hex') }, rows }));
}).catch(error => { console.error(error); process.exitCode = 1; });
