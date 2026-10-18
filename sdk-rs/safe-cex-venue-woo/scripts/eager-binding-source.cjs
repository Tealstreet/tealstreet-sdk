const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/woo/woo.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'WOOXExchange');
assert(klass);
const compile = text => ts.transpileModule(text, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS, esModuleInterop: true},
}).outputText;
const code = compile(klass.getText(ast).replace(/^export /, 'this.C='));
const utility = {exports: {}, require: require('./oracle-runtime.cjs')};
vm.runInNewContext(compile(fs.readFileSync(path.join(path.dirname(file), 'woo.utils.ts'), 'utf8')), utility);
const types = {exports: {}};
const typesAst = ts.createSourceFile('woo.types.ts', fs.readFileSync(path.join(path.dirname(file), 'woo.types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const endpoints = typesAst.statements.find(n => ts.isVariableStatement(n)
  && n.declarationList.declarations.some(d => d.name.getText(typesAst) === 'ENDPOINTS'));
assert(endpoints);
vm.runInNewContext(compile(endpoints.getText(typesAst)), types);

async function probe(member, verb, endpoint) {
  const trace = [];
  let release;
  const response = new Promise((_, reject) => {release = () => reject({response: {data: {message: 'held native failure'}}});});
  const terminal = {[verb](url) {
    assert.equal(url, endpoint);
    trace.push('http');
    return response;
  }};
  class BaseExchange {
    constructor(name, options, store) {this.options = options; this.store = store;}
    on() {}
    emitAndLogError(message) {assert.equal(message, 'held native failure'); trace.push('report');}
  }
  const context = {
    BaseExchange, ...utility.exports, ENDPOINTS: types.exports.ENDPOINTS,
    GRAFANA_ENDPOINTS_EVERY: 'every', GRAFANA_ENDPOINTS_PERIODIC: 'periodic',
    createApiWithSecondaryKey: () => [terminal, terminal, terminal],
    createAPI: () => terminal,
    getTransport: () => ({requireProxy: value => value}),
    WooPublicWebsocket: class {}, WooPrivateWebscoket: class {},
  };
  vm.runInNewContext(code, context);
  const owner = new context.C('woo', {}, {markets: []});
  const original = owner[member];
  Object.defineProperty(owner, member, {get() {trace.push('lookup'); return original;}});
  void owner[member]();
  assert.deepEqual(trace, ['lookup', 'http'], `${member}: synchronous prefix`);
  release();
  await response.catch(() => {});
  await Promise.resolve();
  assert.deepEqual(trace, ['lookup', 'http', 'report'], `${member}: detached catch`);
}

(async () => {
  await probe('fetchMarkets', 'get', types.exports.ENDPOINTS.MARKETS);
  await probe('cancelAllOrders', 'delete', types.exports.ENDPOINTS.CANCEL_ORDERS);
  process.stdout.write(JSON.stringify({families: 2, prefixes: 2, detachedReports: 2}));
})().catch(error => {console.error(error); process.exitCode = 1;});
