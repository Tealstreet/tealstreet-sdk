const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const dependency = require('./oracle-runtime.cjs');
const ts = dependency('typescript');
const root = path.resolve(__dirname, '../../..');
const read = name => fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/woo', name), 'utf8');
const compile = text => ts.transpileModule(text, {compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true}}).outputText;
function moduleValues(name) {
  let clockIndex = 0;
const context = {exports: {}, require: dependency};
  vm.runInNewContext(compile(read(name)), context);
  return context.exports;
}
const utility = moduleValues('woo.utils.ts');
const legacy = moduleValues('woo.legacy-fills.ts');
const typesAst = ts.createSourceFile('woo.types.ts', read('woo.types.ts'), ts.ScriptTarget.Latest, true);
const endpoints = typesAst.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(node => node.name.getText(typesAst) === 'ENDPOINTS'));
assert.ok(endpoints, 'Missing maintained ENDPOINTS');
const types = {exports: {}};
vm.runInNewContext(compile(endpoints.getText(typesAst)), types);
const ast = ts.createSourceFile('woo.exchange.ts', read('woo.exchange.ts'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'WOOXExchange');
const names = ['fetchHistoricalFills', 'fetchHistoricalTradesInternal', 'normalizeTimestampSeconds', 'normalizeTimestampMilliseconds', 'normalizeRequestedSymbol', 'normalizeTransactionSymbol', 'fetchTransactionHistoryRows', 'fetchLegacyTransactionHistoryRows', 'parseTransactionRowsFromResponse', 'getTransactionHistorySymbolCandidates'];
const members = names.map(name => {
  const member = owner.members.find(node => node.name?.getText(ast) === name);
  assert.ok(member, `Missing maintained method ${name}`);
  return member.getText(ast);
});
const apiAst = ts.createSourceFile('woo.api.ts', read('woo.api.ts'), ts.ScriptTarget.Latest, true);
const signing = ['signV1', 'signV3'].map(name => {
  const statement = apiAst.statements.find(node => ts.isVariableStatement(node) && node.declarationList.declarations.some(node => node.name.getText(apiAst) === name));
  assert.ok(statement, `Missing maintained ${name}`);
  return statement.getText(apiAst);
});
const input = JSON.parse(process.argv[2]);
let clockIndex = 0;
const context = {
  ...utility, ...legacy, ...types.exports,
  virtualClock: {getCurrentTimeMs: () => Number(input.timestamps[clockIndex++])},
  qs: dependency('qs'), createHmac: dependency('create-hmac'), AxiosHeaders: dependency('axios').AxiosHeaders,
};
vm.runInNewContext(compile(`${signing.join('\n')}; this.sign = (config, options) => config.url.startsWith('/v1') ? signV1(config, options) : signV3(config, options); class Selected {${members.join('\n')}}; this.Owner = Selected;`), context);
(async () => {
  const owner = new context.Owner();
  const calls = [];
  owner.isPublicDataOnly = false;
  owner.emitAndLogError = () => assert.fail('Unexpected source error');
  owner.xhr = {get: async (url, config) => {
    const signed = context.sign({method: 'get', url, params: config.params}, {secret: 'fixture-secret'});
    calls.push({url: signed.url, signature: signed.headers.get('x-api-signature')});
    return {data: {rows: []}};
  }};
  if (input.kind === 'tree') {
    const params = {undefined: undefined, null: null, bool: true, nan: NaN, infinity: Infinity, zero: -0, string: 'two words', array: [undefined,-Infinity,null,-0], object: {x:false}};
    const signed = context.sign({method:'get',url:'/v3/trade/transactionHistory',params},{secret:'fixture-secret'});
    process.stdout.write(JSON.stringify([{url:signed.url,signature:signed.headers.get('x-api-signature')}]));
    return;
  }
  const result = input.kind === 'legacy'
    ? await owner.fetchLegacyTransactionHistoryRows({limit: NaN})
    : input.kind === 'internal'
      ? await owner.fetchHistoricalTradesInternal({symbol: 'BTCUSDT', limit: NaN})
      : await owner.fetchHistoricalFills({limit: NaN});
  assert.equal(result.length, 0);
  process.stdout.write(JSON.stringify(calls));
})().catch(error => {console.error(error); process.exitCode = 1;});
