'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { ts, root, values } = require('../scripts/source.cjs');
assert.equal(process.versions.node, '24.20.0', 'book-sort oracle uses CI Node');
assert.match(process.versions.v8, /^13\.6\./);
const file = 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts';
const tree = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
const owner = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'AsterDEXExchange');
const members = ['getFapiPath', 'fetchOrderBook'].map(name => {
  const member = owner.members.find(n => n.name?.getText(tree) === name);
  assert(member, name);
  return member.getText(tree);
}).join('\n');
const compiled = ts.transpileModule(`export class Source { ${members} }`, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const result = {};
new Function('exports', 'getAsterDEXFapiPath', compiled)(result, values.getAsterDEXFapiPath);
function bits(value) {
  if (Number.isNaN(value)) return 'NaN';
  const bytes = Buffer.alloc(8);
  bytes.writeDoubleBE(value);
  return bytes.toString('hex');
}
(async () => {
  const output = [];
  for (const length of [22, 32, 63, 64, 65, 95, 127, 128, 129, 257]) {
    const levels = Array.from({ length }, (_, i) => [
      i % 7 === 0 || i % 11 === 0 ? 'NaN' : String((i * 197 + 558) % 997),
      String(i + 1),
    ]);
    const depth = { lastUpdateId: 123, bids: levels, asks: levels };
    const source = new result.Source();
    source.apiVersion = 'v1';
    let calls = 0;
    source.publicAPI = { get: async (url, config) => {
      assert.equal(url, '/fapi/v1/depth');
      assert.deepEqual(config, { params: { symbol: 'BTCUSDT', limit: 1000 } });
      calls++;
      return { data: depth };
    }};
    source.log = message => { throw new Error(`Unexpected source error: ${message}`); };
    const book = await source.fetchOrderBook('BTCUSDT', 1000);
    assert.equal(calls, 1);
    const pack = rows => rows.map(row => Object.fromEntries(Object.entries(row).map(([k, v]) => [k, bits(v)])));
    output.push({ name: length, depth, bids: pack(book.bids), asks: pack(book.asks) });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack); process.exitCode = 1; });
