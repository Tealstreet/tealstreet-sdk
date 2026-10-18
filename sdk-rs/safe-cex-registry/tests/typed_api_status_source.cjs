'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const axios = require('axios');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function compile(source) {
  const result = ts.transpileModule(source, {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
    reportDiagnostics: true,
  });
  assert.equal(result.diagnostics.length, 0);
  return result.outputText;
}
const metaScope = {exports: {}};
vm.runInNewContext(compile(fs.readFileSync(path.join(root, 'exchanges/bybit/bybit.response.ts'), 'utf8')), metaScope);
assert.equal(metaScope.exports.getBybitResponseMeta({code: -1}).hasExplicitStatus, true,
  'an aliased code prevents the API interceptor from synthesizing retMsg');
async function validate(venue, klass, tunneled) {
  const source = fs.readFileSync(path.join(root, `exchanges/${venue === 'kucoinspot' ? 'kucoin' : venue}/${venue}.exchange.ts`), 'utf8');
  const tree = ts.createSourceFile(venue, source, ts.ScriptTarget.Latest, true);
  const owner = tree.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === klass);
  assert.ok(owner, klass);
  const member = owner.members.filter(node => node.name?.getText(tree) === 'validateAccount');
  assert.equal(member.length, 1);
  const scope = {ENDPOINTS: {ACCOUNT_MARGIN: '/v5/account/info'},
    KUCOIN_SPOT_ENDPOINTS: {ACCOUNTS: '/api/v1/accounts'}};
  vm.runInNewContext(compile(`class Slice {${member[0].getText(tree)}} globalThis.Slice = Slice;`), scope);
  const errors = [], requests = [];
  const exchange = new scope.Slice();
  Object.assign(exchange, {emitAndLogError: value => errors.push(value),
    fetchBalance() {assert.fail('a refused account must not request balance');},
    xhr: axios.create({adapter: async config => {
      requests.push(config.url);
      const response = {status:404, data:{code:-1}, headers:{}, config};
      if (!tunneled) throw new axios.AxiosError('Request failed with status code 404', 'ERR_BAD_REQUEST', config, undefined, response);
      return response;
    }})});
  const result = await exchange.validateAccount();
  assert.equal(requests.length, 1);
  return {error:result.error, errors, requests};
}
(async () => {
  const results = [];
  for (const [venue, klass] of [['bybit','BybitExchange'], ['kucoin','KucoinExchange'], ['kucoinspot','KucoinSpotExchange']]) {
    const how = await validate(venue, klass, true);
    if (venue === 'bybit') {
      assert.equal(how.error, "Cannot read properties of undefined (reading 'includes')");
      assert.equal(how.errors.length, 2);
      assert.equal(how.errors[0], undefined);
      const direct = await validate(venue, klass, false);
      assert.equal(direct.error, 'Request failed with status code 404');
      assert.equal(direct.errors.length, 1);
    } else {
      assert.equal(how.error, venue === 'kucoin' ? 'Failed to validate account' : 'Failed to validate KuCoin Spot account');
      assert.equal(how.errors.length, venue === 'kucoin' ? 1 : 0);
    }
    results.push({venue, how});
  }
  process.stdout.write(JSON.stringify(results));
})().catch(error => {console.error(error);process.exitCode = 1;});
