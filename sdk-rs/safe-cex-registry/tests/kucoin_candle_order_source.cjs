'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { createRequire } = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const installed = createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript'));
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/kucoin');
const source = (name) => {
  const filename = path.join(root, name);
  return ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
};
const print = (code) => ts.transpileModule(code, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true },
}).outputText;
const result = [];
(async () => {
  for (const [name, owner] of [['kucoin', 'KucoinExchange'], ['kucoinspot', 'KucoinSpotExchange']]) {
    const file = source(`${name}.exchange.ts`);
    const cls = file.statements.find((n) => ts.isClassDeclaration(n) && n.name?.text === owner);
    const member = cls.members.filter((n) => n.name?.getText(file) === 'fetchOHLCV');
    assert.equal(member.length, 1, `${owner}.fetchOHLCV source member`);
    const constants = { exports: {} };
    const table = source(`${name}.types.ts`);
    const names = name === 'kucoinspot' ? ['KUCOIN_SPOT_ENDPOINTS', 'KUCOIN_SPOT_INTERVAL'] : ['ENDPOINTS', 'INTERVAL'];
    const declarations = table.statements.filter((n) => ts.isVariableStatement(n) && n.declarationList.declarations.some((d) => names.includes(d.name.getText(table))));
    assert.equal(declarations.length, 2, 'actual endpoint and interval tables');
    vm.runInNewContext(print(declarations.map((n) => n.getText(table)).join('\n')), {
      module: constants, exports: constants.exports, require: () => ({}),
    }, { filename: table.fileName });
    const context = { ...constants.exports, dayjs: installed('dayjs') };
    vm.runInNewContext(print(`class Source { ${member[0].getText(file)} } globalThis.Source = Source;`), context);
    const raw = name === 'kucoinspot'
      ? [['1700000060', '31', '32', '39', '29', '7'], ['1700000000', '-0', '12', '19', '9', '5']]
      : [['1700000060000', '31', '39', '29', '32', '7'], ['1700000000000', '-0', '19', '9', '12', '5']];
    const seen = [];
    const receiver = new context.Source();
    Object.assign(receiver, {
      store: { marketsBySymbol: {} },
      xhr: { get: async (url, config) => { seen.push({ url, params: config.params }); return { data: { code: '200000', data: raw } }; } },
      emitAndLogError: () => { throw Error('unexpected source failure'); },
    });
    const options = { symbol: 'BTC-USDT', interval: '1m', limit: 2, to: 1700000120000 };
    const candles = await receiver.fetchOHLCV(options);
    assert.equal(seen.length, 1, 'actual source HTTP call');
    assert.equal(candles.length, 2, 'both maintained source rows');
    result.push({ name, raw, options, request: seen[0], keys: candles.map(Object.keys), json: JSON.stringify(candles), values: candles, negativeZeroOpen: candles.map((c) => Object.is(c.open, -0)) });
  }
  process.stdout.write(JSON.stringify(result));
})().catch((error) => { console.error(error); process.exitCode = 1; });
