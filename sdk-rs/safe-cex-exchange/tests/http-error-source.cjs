'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const actualRequire = createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const {AxiosError} = actualRequire('axios');
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/axiosErrorEnhancer.ts');
const compiled = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  fileName: filename, reportDiagnostics: true,
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
});
assert.equal(compiled.diagnostics.length, 0);
const loaded = {exports: {}};
new Function('require', 'module', 'exports', compiled.outputText)(actualRequire, loaded, loaded.exports);
module.exports = loaded.exports;

if (require.main === module) {
  const {enhanceAxiosError} = loaded.exports;
  const network = enhanceAxiosError(new AxiosError('connect ECONNREFUSED', 'ECONNREFUSED',
    {method: 'get', url: '/fapi/v2/account'}, {}), 'bitrue');
  const bingx = [400, 503].map(status => {
    const config = {method: 'post', url: '/openApi/swap/v2/trade/order'};
    const error = new AxiosError(`Request failed with status code ${status}`, 'ERR_BAD_RESPONSE', config,
      undefined, {status, data: {code: status === 400 ? 100001 : 500}, config});
    return enhanceAxiosError(error, 'bingx').message;
  });
  assert.equal(network.name, 'AxiosError');
  assert.equal(network.code, 'ECONNREFUSED');
  assert.equal(network.message, '[bitrue] Network error (ECONNREFUSED) to /fapi/v2/account');
  assert.deepEqual(bingx, [
    '[bingx] POST /openApi/swap/v2/trade/order failed with 400 undefined: 100001',
    '[bingx] POST /openApi/swap/v2/trade/order failed with 503 undefined: 500',
  ]);
  process.stdout.write(JSON.stringify({network: {name: network.name, message: network.message, code: network.code}, bingx}));
}
