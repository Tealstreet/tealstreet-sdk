const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const { createRequire } = require('node:module');
const tsPath = process.env.SAFE_CEX_TYPESCRIPT_PATH || path.resolve(__dirname, '../../../node_modules/typescript');
const ts = require(tsPath);
const deps = createRequire(require.resolve(tsPath));
const dayjs = deps('dayjs');
dayjs.extend(deps('dayjs/plugin/utc'));
process.env.TZ = 'UTC';
const root = path.resolve(__dirname, '../../..');
const sourceFile = relative => {
  const file = path.join(root, relative);
  return ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
};
const exchange = sourceFile('packages/safe-cex/src/exchanges/phemex/phemex.exchange.ts');
const cls = exchange.statements.find(ts.isClassDeclaration);
const member = name => {
  const matches = cls.members.filter(row => row.name?.getText(exchange) === name);
  assert.equal(matches.length, 1, name);
  return matches[0].getText(exchange);
};
const types = sourceFile('packages/safe-cex/src/exchanges/phemex/phemex.types.ts');
const endpoints = types.statements.filter(ts.isVariableStatement)
  .flatMap(row => [...row.declarationList.declarations]).find(row => row.name.getText(types) === 'ENDPOINTS');
assert(endpoints);
const code = `const ENDPOINTS = ${endpoints.initializer.getText(types)};
class Source { ${member('timeframes')} ${member('fetchOHLCV')} }
Source`;
const Source = vm.runInNewContext(ts.transpileModule(code, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
}).outputText, { dayjs });
(async () => {
  const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
  const results = [];
  for (const input of cases) {
    const receiver = new Source();
    const requests = [];
    const errors = [];
    receiver.formatSymbolOut = symbol => symbol;
    receiver.emitter = { emit: (...args) => errors.push(args) };
    receiver.xhr = { get: async (url, config) => {
      requests.push({ url, params: config.params });
      return { data: { data: { rows: [[1, 86400, 0, '10', '15', '9', '12', '3']] } } };
    } };
    const to = input.offset === null ? input.to : dayjs(input.to).utcOffset(input.offset);
    const candles = await receiver.fetchOHLCV({ symbol: 'BTCUSDT', interval: input.interval, limit: 2, to });
    assert.deepEqual(errors, []);
    assert.equal(requests.length, 1);
    assert.equal(candles.length, 1);
    results.push({ requests, candles });
  }
  process.stdout.write(JSON.stringify(results));
})().catch(error => { console.error(error); process.exitCode = 1; });
