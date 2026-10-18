import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import dayjs from 'dayjs';
import orderBy from 'lodash/orderBy.js';

const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || fileURLToPath(new URL('../../../node_modules/typescript', import.meta.url)));
const root = new URL('../../../packages/safe-cex/src/exchanges/bybit/', import.meta.url);
const read = name => ts.createSourceFile(name, readFileSync(new URL(name, root), 'utf8'), ts.ScriptTarget.Latest, true);
for (const variant of ['bybit', 'bybitinverse', 'bybitspot']) {
  const file = read(`${variant}.exchange.ts`);
  const tableImport = file.statements.find(n => ts.isImportDeclaration(n) && n.importClause?.namedBindings?.elements?.some(e => e.name.text === 'INTERVAL'));
  assert.ok(tableImport, `${variant}: maintained interval import`);
  const tables = read(`${tableImport.moduleSpecifier.text.slice(2)}.ts`);
  const declarations = tables.statements.filter(ts.isVariableStatement).flatMap(s => [...s.declarationList.declarations]);
  const imports = ['ENDPOINTS', 'INTERVAL'].map(name => {
    const declaration = declarations.find(d => d.name.getText(tables) === name);
    assert.ok(declaration?.initializer, `maintained ${variant}.${name}`);
    return `const ${name} = ${declaration.initializer.getText(tables)};`;
  }).join('\n');
  const cls = file.statements.find(n => ts.isClassDeclaration(n) && n.members.some(m => m.name?.getText(file) === 'fetchOHLCV'));
  const member = cls.members.find(m => m.name?.getText(file) === 'fetchOHLCV');
  assert.ok(member, `${variant}: maintained fetchOHLCV`);
  const code = ts.transpileModule(`${imports}\nclass Source { ${member.getText(file)} }`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const source = new Function('dayjs', 'orderBy', `${code}; return new Source();`)(dayjs, orderBy);
  source.validateCandleInterval = () => {};
  source.getCategoryForSymbol = () => variant === 'bybitinverse' ? 'inverse' : 'linear';
  source.xhr = { get: async () => ({data: {retCode: 0, result: {list: ['234567', '123456suffix']}}}) };
  const candles = await source.fetchOHLCV({symbol: 'BTCUSDT', interval: '1m', from: 1000, to: 2000, limit: 2});
  assert.deepEqual(candles, [
    {timestamp: 0.001, open: 2, high: 3, low: 4, close: 5, volume: 6},
    {timestamp: 0.002, open: 3, high: 4, low: 5, close: 6, volume: 7},
  ], `${variant}: iterator destructuring reads the first six characters`);
  source.xhr.get = async () => ({data: {retCode: 0, result: {list: [123456]}}});
  await assert.rejects(source.fetchOHLCV({symbol: 'BTCUSDT', interval: '1m', limit: 1}), TypeError,
    `${variant}: noniterable rows remain errors`);
}
