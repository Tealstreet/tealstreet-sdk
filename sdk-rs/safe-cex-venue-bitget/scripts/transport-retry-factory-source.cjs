'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const axios = require('axios');
const retry = require('axios-retry');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
function evaluate(file, bindings = {}, selected) {
  const source = fs.readFileSync(path.join(root, file), 'utf8');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  assert.equal(ast.parseDiagnostics.length, 0, file);
  const nodes = ast.statements.filter(node => !ts.isImportDeclaration(node) && !ts.isExportDeclaration(node) && (!selected || ts.isVariableStatement(node) && node.declarationList.declarations.some(decl => selected.includes(decl.name.text))));
  const output = ts.transpileModule(nodes.map(node => node.getText(ast)).join('\n'), {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  const exports = {};
  new Function('exports', ...Object.keys(bindings), output)(exports, ...Object.values(bindings));
  return exports;
}
const noop = () => {};
const enhancer = evaluate('packages/safe-cex/src/utils/axiosErrorEnhancer.ts');
const factory = evaluate('packages/safe-cex/src/safeCexAxiosCreate.ts', {
  axios, ...enhancer, isDevelopment: false, isSigningCaptureRequest: () => false,
  markRestRequestStart: noop, noteRestFailure: noop, noteRestSuccess: noop,
  getTransport: () => ({rest: () => ({proxied: false, baseURL: 'https://terminal.invalid'})}),
  createRemoteSigningAdapter: () => {throw Error('unexpected hosted signer');},
});
const protocol = evaluate('packages/safe-cex/src/exchanges/bitget/bitget.types.ts', {}, ['BASE_URL', 'BROKER_ID', 'ENDPOINTS', 'RECV_WINDOW']);
const reads = evaluate('packages/safe-cex/src/utils/retryReadsOnly.ts', {isNetworkError: retry.isNetworkError});
const signature = evaluate('packages/safe-cex/src/utils/signatureRejection.ts');
const getKey = evaluate('packages/safe-cex/src/utils/get-key.ts', {memoize: require('lodash/memoize'), snakeCase: require('lodash/snakeCase'), camelCase: require('lodash/camelCase')});
const api = evaluate('packages/safe-cex/src/exchanges/bitget/bitget.api.ts', {
  ...factory, ...protocol, ...reads, ...signature, ...getKey,
  AxiosHeaders: axios.AxiosHeaders, retry: retry.default,
  qs: require('qs'), createHmac: require('create-hmac'),
  virtualClock: {getCurrentTimeMs: () => 1700000000000},
  appendHttpFailure: () => {throw Error('unexpected venue response refusal');},
});
(async () => {
  const observations = [];
  for (const kind of ['Error', 'AxiosError', 'object']) {
    let attempts = 0, finalError;
    const exchange = {name: 'bitget', grafanaEndpointsEvery: [], grafanaEndpointsPeriodic: [], emitter: {emit: noop}, emitAndLogError: noop};
    const client = api.createAPI({}, exchange);
    try {
      await client.get('/api/v2/mix/market/contracts', {adapter: async config => {
        attempts++;
        if (attempts === 1) {
          const error = kind === 'Error' ? new Error('synthetic network failure') : kind === 'AxiosError' ? new axios.AxiosError('synthetic network failure') : {message: 'synthetic network failure'};
          Object.assign(error, {code: 'ECONNRESET', config});
          throw error;
        }
        return {status: 200, statusText: 'OK', headers: {}, config, data: {code: '00000', data: [], requestTime: 1}};
      }});
    } catch (error) { finalError = error; }
    assert.equal(attempts, kind === 'object' ? 1 : 2, kind);
    if (kind === 'object') {
      assert.equal(finalError instanceof Error, true);
      assert.equal(Object.hasOwn(finalError, 'code'), false);
      assert.equal(Object.hasOwn(finalError, 'config'), false);
      assert.equal(finalError.originalError.code, 'ECONNRESET');
    }
    observations.push({kind, attempts, codePresent: finalError ? Object.hasOwn(finalError, 'code') : null});
  }
  process.stdout.write(JSON.stringify(observations));
})().catch(error => {console.error(error); process.exitCode = 1;});
