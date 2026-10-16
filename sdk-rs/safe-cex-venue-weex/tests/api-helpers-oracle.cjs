const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, capture, load} = require('./types-oracle.cjs');
const file = path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.api.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['WEEX_RATE_LIMIT_MAX_RETRIES', 'WEEX_READ_ONLY_POST_ENDPOINTS', 'isWeexReadOnlyRequest',
  'shouldRetryWeexRequest', 'WEEX_RETRY_CONFIG', 'signWeexMessage'];
const nodes = ast.statements.filter(n => ts.isVariableStatement(n) &&
  n.declarationList.declarations.some(d => names.includes(d.name.getText(ast))));
assert.equal(nodes.length, 6, 'api_helpers::source_inventory');
const actualNetworkError = require('axios-retry').isNetworkError;
let networkCalls = 0, delayCalls = 0, observedError;
const context = {exports: {}, ...load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts')),
  createHmac: require('create-hmac'),
  isNetworkError: error => {networkCalls++; return actualNetworkError(error);},
  getWeexRateLimitRetryDelayMs: error => {delayCalls++; observedError = error; return error.delay;}};
vm.runInNewContext(ts.transpileModule(nodes.map(n => n.getText(ast)).join('\n'),
  {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText,
  context, {filename: file});
const config = context.exports.WEEX_RETRY_CONFIG;
assert.equal(config.retryCondition, context.exports.shouldRetryWeexRequest, 'api_helpers::source_condition_identity');
const units = text => Array.from({length: text.length}, (_, i) => text.charCodeAt(i));
const signatures = [
  ['empty', '', ''], ['ascii', 'key', 'message'], ['request', 'secret', '1700000000000GET/capi/v3/account'],
  ['nul', 'k\0ey', 'm\0sg'], ['unicode', '日本語', 'éßſ'], ['nonbmp', '🔑', '🚀'],
  ['lone_key_high', '\ud800', 'message'], ['lone_message_low', 'key', '\udc00'],
  ['pair', '\ud800\udc00', '\ud801\udc01'], ['reversed', '\udc00\ud800', '\udc01\ud801'],
  ['adjacent_high', '\ud800\ud801', '\udc00\udc01'], ['long_key', '鍵'.repeat(100), 'block'],
  ['long_message', 'key', 'é'.repeat(130)],
].map(([id, secret, message]) => ({id, secret: units(secret), message: units(message),
  result: context.exports.signWeexMessage(secret, message)}));
const income = context.ENDPOINTS.INCOME;
const conditionInputs = [
  ['network_get', {config: {method: 'GET'}, code: 'ECONNRESET'}],
  ['network_post', {config: {method: 'POST', url: '/capi/v3/order'}, code: 'ECONNRESET'}],
  ['rate_first', {config: {method: 'GET'}, response: {status: 429}}],
  ['rate_second', {config: {method: 'GET', 'axios-retry': {retryCount: 1}}, response: {status: 429}}],
  ['rate_income', {config: {method: 'POST', url: income}, response: {status: 429}}],
  ['rate_delete', {config: {method: 'DELETE'}, response: {status: 429}}],
  ['tls_head', {config: {method: 'HEAD'}, code: 'CERT_HAS_EXPIRED'}],
  ['server_error', {config: {method: 'GET'}, response: {status: 500}, code: 'ECONNRESET'}],
];
const conditions = conditionInputs.map(([id, error]) => {
  networkCalls = 0;
  const result = config.retryCondition(error), count = error.config['axios-retry']?.retryCount;
  return {id, method: units(error.config.method), ...(error.config.url ? {url: units(error.config.url)} : {}),
    ...(error.response ? {status: capture(error.response.status)} : {}),
    ...(count !== undefined ? {count: capture(count)} : {}),
    network: actualNetworkError(error), networkCalls, result};
});
const delays = [
  ['rate', 0, 429, 12345], ['rate_zero', 1, 429, 0], ['rate_negative_zero', -1, 429, -0],
  ['rate_nan', NaN, 429, NaN], ['rate_infinity', Infinity, 429, Infinity],
  ['server', NaN, 500, 12345], ['absent', -Infinity, undefined, 12345],
  ['status_nan', 8, NaN, 12345], ['status_infinity', 9, Infinity, 12345],
].map(([id, count, status, delay]) => {
  delayCalls = 0; observedError = undefined;
  const error = {response: status === undefined ? undefined : {status}, delay};
  const result = config.retryDelay(count, error);
  assert.equal(observedError === error, status === 429, `api_helpers::source_delay_error::${id}`);
  return {id, count: capture(count), ...(status === undefined ? {} : {status: capture(status)}),
    delay: capture(delay), delayCalls, result: capture(result)};
});
process.stdout.write(JSON.stringify({metadata: {retries: config.retries, shouldResetTimeout: config.shouldResetTimeout},
  signatures, conditions, delays}));
