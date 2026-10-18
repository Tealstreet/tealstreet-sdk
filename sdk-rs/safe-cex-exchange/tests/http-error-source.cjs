'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const actualRequire = createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
const axios = actualRequire('axios');
const {AxiosError} = axios;
const filename = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/axiosErrorEnhancer.ts');
const compiled = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  fileName: filename, reportDiagnostics: true,
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
});
assert.equal(compiled.diagnostics.length, 0);
const loaded = {exports: {}};
new Function('require', 'module', 'exports', compiled.outputText)(actualRequire, loaded, loaded.exports);
module.exports = loaded.exports;

const handlers = new Map();
function loadHandler(name) {
  assert(['BaseHandler', 'CallMethodHandler', 'ThrottleManager'].includes(name));
  if (handlers.has(name)) return handlers.get(name);
  const file = path.resolve(__dirname, '../../../packages/safe-cex/src/webworker/handlers', name + '.ts');
  const js = ts.transpileModule(fs.readFileSync(file, 'utf8'), {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  const module = {exports: {}};
  new Function('require', 'module', 'exports', js)(name => loadHandler(name.replace('./', '')), module, module.exports);
  handlers.set(name, module.exports);
  return module.exports;
}

if (require.main === module) (async () => {
  const {enhanceAxiosError} = loaded.exports;
  const server = require('node:net').createServer();
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const baseURL = `http://127.0.0.1:${server.address().port}`;
  await new Promise(resolve => server.close(resolve));
  const refused = await axios.get('/fapi/v2/account', {baseURL, proxy: false, timeout: 1000})
    .then(() => assert.fail('closed native port must reject'), error => error);
  assert.equal(refused.code, 'ECONNREFUSED');
  assert(refused.request, 'actual Axios network rejection has a request');
  const network = enhanceAxiosError(new AxiosError(refused.message, refused.code, refused.config, refused.request), 'bitrue');
  const nodeNetwork = enhanceAxiosError(refused, 'bitrue');
  assert.equal(nodeNetwork.name, 'Error');
  assert.equal(nodeNetwork.message, network.message);
  assert.equal(nodeNetwork.code, network.code);
  assert.equal(network.isAxiosError, true);
  const {CallMethodHandler} = loadHandler('CallMethodHandler');
  const replies = [];
  const handler = new CallMethodHandler('account', 'bitrue', {
    fetchOrders: async () => { throw network; },
  }, {postMessage: reply => replies.push(reply)});
  await handler.handleCallMethod('network-proof', 'fetchOrders', [[]]);
  assert.equal(replies.length, 1);
  assert.equal(replies[0].data.success, false);
  const workerNetwork = replies[0].data.result;
  assert.equal(workerNetwork.message, network.message);
  assert.equal(workerNetwork.code, network.code);
  assert.equal(Object.hasOwn(workerNetwork, 'isAxiosError'), false);
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
  process.stdout.write(JSON.stringify({network: {name: network.name, message: network.message, code: network.code}, nodeNetwork: {name: nodeNetwork.name, message: nodeNetwork.message, code: nodeNetwork.code}, workerNetwork, bingx}));
})().catch(error => { console.error(error); process.exitCode = 1; });
