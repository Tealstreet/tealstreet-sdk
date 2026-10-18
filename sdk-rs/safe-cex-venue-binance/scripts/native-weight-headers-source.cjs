const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire = process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? name => require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '..', name))
  : require;
const axios = runtimeRequire('axios');
const filename = path.resolve(__dirname,
  '../../../packages/safe-cex/src/exchanges/binance/binance.exchange.ts');
const text = fs.readFileSync(filename, 'utf8');
const ast = ts.createSourceFile(filename, text, ts.ScriptTarget.Latest, true);
const declaration = ast.statements.find(node =>
  ts.isClassDeclaration(node) && node.name.text === 'BinanceExchange');
const members = ['getResponseHeader', 'pauseBackgroundReconciliation',
  'observeRequestWeight', 'installRequestWeightGuards'].map(name => {
  const member = declaration.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, `actual source member ${name}`);
  return member.getText(ast).replace(/^private /, '');
});
let now;
const constants = ['REQUEST_WEIGHT_LIMIT_PER_MINUTE',
  'REQUEST_WEIGHT_BACKOFF_THRESHOLD', 'REQUEST_WEIGHT_BACKOFF_MS'].map(name => {
  const statement = ast.statements.find(node => ts.isVariableStatement(node) &&
    node.declarationList.declarations.some(declaration => declaration.name.getText(ast) === name));
  assert.ok(statement, `actual source constant ${name}`);
  return statement.getText(ast);
});
const context = { exports: {}, Date: { now: () => now } };
vm.runInNewContext(ts.transpileModule(`${constants.join('\n')} class Weight {
  backgroundReconciliationPausedUntil=0; ${members.join('\n')}
}; exports.Weight=Weight;`, { compilerOptions: {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS,
} }).outputText, context);

(async () => {
  const rows = JSON.parse(fs.readFileSync(0, 'utf8'));
  const result = [];
  for (const row of rows) {
    now = row.now;
    const owner = new context.exports.Weight();
    let original;
    const client = axios.create({ adapter: async config => {
      const response = { status: row.status, statusText: '', data: [], config,
        headers: { [row.header]: row.value } };
      if (row.status >= 400) {
        original = new axios.AxiosError('weight limit',
          axios.AxiosError.ERR_BAD_REQUEST, config, undefined, response);
        throw original;
      }
      return response;
    } });
    owner.installRequestWeightGuards([client]);
    let response;
    try {
      response = await client.get('/fapi/v1/openOrders');
      assert.equal(row.status, 200);
    } catch (error) {
      assert.equal(error, original);
      assert.ok(row.status >= 400);
      response = error.response;
    }
    assert.ok(response.headers instanceof axios.AxiosHeaders);
    result.push(owner.backgroundReconciliationPausedUntil);
  }
  process.stdout.write(JSON.stringify(result));
})().catch(error => { console.error(error); process.exitCode = 1; });
