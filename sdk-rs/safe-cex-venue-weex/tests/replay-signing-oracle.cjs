// Synthetic credentials. Execute the actual request interceptor; no venue network.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const crypto = require('node:crypto');
const { root, ts, load } = require('./types-oracle.cjs');
const dependencies = path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '..');
const qs = require(path.join(dependencies, 'qs'));
const { AxiosHeaders } = require(path.join(dependencies, 'axios'));
const venue = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.api.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const source = ['serializeWeexParams', 'signWeexMessage', 'createAPI'].map(name => {
  const node = tree.statements.find(node => ts.isVariableStatement(node) &&
    node.declarationList.declarations.some(decl => decl.name.getText(tree) === name));
  assert.ok(node, name);
  return node.getText(tree);
}).join('\n');
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  reportDiagnostics: true,
});
assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
let hook;
const scope = { ...venue, exports: {}, qs, AxiosHeaders, createHmac: crypto.createHmac,
  virtualClock: { getCurrentTimeMs: () => 123456 },
  bigIntAsStringJsonParse: JSON.parse, retry: () => {}, WEEX_RETRY_CONFIG: {},
  safeCexAxiosCreate: () => ({ interceptors: {
    request: { use: fn => { hook = fn; } }, response: { use: () => {} },
  } }),
};
vm.runInNewContext(compiled.outputText, scope, { filename: file });
scope.exports.createAPI({ key: 'k', secret: 's', passphrase: 'p' }, {});
const cases = [
  ['object', { symbol: 'BTCUSDT', price: '12.34' }],
  ['serialized', '{"symbol":"BTCUSDT","price":"12.34"}'],
  ['escaped', 'quoted "text" \\ newline\n雪'],
  ['plain', 'raw request bytes'], ['empty', ''], ['null', null], ['false', false], ['zero', 0],
];
process.stdout.write(JSON.stringify(cases.map(([id, data]) => {
  const result = hook({ url: '/private', method: 'post', data });
  assert.strictEqual(result.data, data, `${id}: interceptor preserves body identity`);
  return { id, data, signature: result.headers.get('ACCESS-SIGN') };
})));
