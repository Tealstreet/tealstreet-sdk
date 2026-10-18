const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const axios = require('axios');
const retry = require('axios-retry');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function evaluate(file, bindings = {}, selected) {
  const source = fs.readFileSync(path.join(root, file), 'utf8');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  assert.equal(ast.parseDiagnostics.length, 0, file);
  const nodes = ast.statements.filter(node => !ts.isImportDeclaration(node) && !ts.isExportDeclaration(node) &&
    (!selected || ts.isVariableStatement(node) && node.declarationList.declarations.some(decl => selected.includes(decl.name.text))));
  const output = ts.transpileModule(nodes.map(node => node.getText(ast)).join('\n'),
    {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  const exports = {};
  new Function('exports', ...Object.keys(bindings), output)(exports, ...Object.values(bindings));
  return exports;
}
const noop = () => {};
const factory = evaluate('safeCexAxiosCreate.ts', {
  axios, ...evaluate('utils/axiosErrorEnhancer.ts'), isDevelopment: false, isSigningCaptureRequest: () => false,
  markRestRequestStart: noop, noteRestFailure: noop, noteRestSuccess: noop,
  getTransport: () => ({rest: () => ({proxied: false, baseURL: 'https://terminal.invalid'})}),
  createRemoteSigningAdapter: () => {throw Error('unexpected hosted signer');},
});
const api = evaluate('exchanges/breakoutprop/breakoutprop.api.ts', {
  ...factory, ...evaluate('utils/retryReadsOnly.ts', {isNetworkError: retry.isNetworkError}),
  ...evaluate('utils/replaceUrlParams.ts'),
  ...evaluate('exchanges/breakoutprop/breakoutprop.types.ts', {}, ['BASE_URL', 'ENDPOINTS', 'RECV_WINDOW']),
  AxiosHeaders: axios.AxiosHeaders, retry: retry.default, qs: require('qs'),
});
(async () => {
  const exchange = {name: 'breakoutprop', sessionToken: null, grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: {emit: noop}};
  const client = api.createAPI({}, exchange);
  let captured;
  await assert.rejects(client.get('/users/k', {adapter: async () => assert.fail('auth refusal precedes terminal I/O')}), error => {
    captured = {name: error.name, message: error.message};
    return true;
  });
  assert.deepEqual(captured, {name: 'Error', message: '[breakoutprop] Request setup error: No session token'});
  console.log(JSON.stringify(captured));
})().catch(error => {console.error(error); process.exitCode = 1;});
