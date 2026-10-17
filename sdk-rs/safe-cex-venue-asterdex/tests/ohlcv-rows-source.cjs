'use strict';
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const { ts, root, values } = require('../scripts/source.cjs');
const file = 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts';
const tree = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), ts.ScriptTarget.Latest, true);
const owner = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'AsterDEXExchange');
const members = ['getFapiPath', 'mapTimeframe', 'fetchOHLCV'].map(name => {
  const member = owner.members.find(n => n.name?.getText(tree) === name);
  assert(member, name);
  return member.getText(tree);
}).join('\n');
const compiled = ts.transpileModule(`export class Source { ${members} }`, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText;
const result = {};
new Function('exports', 'getAsterDEXFapiPath', compiled)(result, values.getAsterDEXFapiPath);
(async () => {
  const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
  assert.equal(cases.length, 5);
  const output = [];
  for (const version of ['v1', 'v3']) for (const entry of cases) {
    const source = new result.Source();
    source.apiVersion = version;
    const requests = [];
    source.publicAPI = { get: async (url, config) => {
      requests.push({ url, params: config.params });
      return { data: entry.rows };
    }};
    source.log = message => { throw new Error(`Unexpected source error: ${message}`); };
    const candles = await source.fetchOHLCV({ symbol: 'BTCUSDT', interval: '1m', limit: 2, from: 123000, to: 999000 });
    output.push({ version, name: entry.name, candles, requests });
  }
  process.stdout.write(JSON.stringify(output, (_, value) =>
    typeof value === 'number' && !Number.isFinite(value) ? String(value) : value));
})().catch(error => { process.stderr.write(error.stack); process.exitCode = 1; });
