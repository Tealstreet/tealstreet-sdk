const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtime = require('./oracle-runtime.cjs');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/woo');
const read = name => fs.readFileSync(path.join(root, name), 'utf8');
const transpile = source => ts.transpileModule(source, {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
}).outputText;
function moduleExports(source) {
  const exports = {};
  vm.runInNewContext(transpile(source), {exports, require: runtime});
  return exports;
}
const ast = ts.createSourceFile('woo.exchange.ts', read('woo.exchange.ts'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const names = ['fetchLegacyTransactionHistoryRows', 'getTransactionHistorySymbolCandidates', 'parseTransactionRowsFromResponse'];
const members = names.map(name => {
  const member = cls.members.find(n => n.name?.getText(ast) === name);
  assert.ok(member, name);
  return member.getText(ast);
}).join('\n');
const types = ts.createSourceFile('woo.types.ts', read('woo.types.ts'), ts.ScriptTarget.Latest, true);
const endpoint = types.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(types) === 'ENDPOINTS'));
assert.ok(endpoint, 'LEGACY source ENDPOINTS');
const bindings = {...moduleExports(read('woo.utils.ts')), ...moduleExports(read('woo.legacy-fills.ts')), ...moduleExports(endpoint.getText(types))};
function worker(source) {
  const context = {...bindings};
  vm.runInNewContext(transpile(`class Witness {${source}}\nglobalThis.Witness=Witness;`), context);
  return new context.Witness();
}
const symbols = [undefined, '', 'btcusdt', 'PERP_BTC_USDT', 'USDT', 'x:ETHUSDT'];
const scenarios = [
  ['empty', []], ['first', ['rows']], ['second', ['empty', 'rows']],
  ['last', ['empty', 'empty', 'empty', 'empty', 'rows']],
  ['error', ['error']], ['error-empty', ['error', 'empty']],
  ['error-rows', ['error', 'rows']], ['two-errors', ['error', 'error']],
  ['error-null', ['error', 'null']], ['error-false', ['error', 'false']],
  ['error-zero', ['error', 'zero']], ['error-string', ['error', 'string']],
  ['null-response', ['response-null']], ['null-data', ['data-null']],
  ['mutated-options', ['mutate', 'rows']],
];
async function capture(source) {
  const cases = [];
  for (let symbolIndex = 0; symbolIndex < 6; symbolIndex++) {
    for (const [scenario, script] of scenarios) {
      const subject = worker(source), calls = [], sentinels = [], arrays = [];
      subject.xhr = {get: async (url, config) => {
        const index = calls.length;
        calls.push({url, params: config.params});
        assert.ok(index < 6, 'LEGACY bounded source attempts');
        const action = script[index] || 'empty';
        if (action === 'response-null') return null;
        if (action === 'data-null') return {data: null};
        if (action === 'mutate') opts.limit = 99;
        const error = {message: `failure-${index}`, marker: 41 + index};
        sentinels.push(error);
        if (action === 'error') throw error;
        if (action === 'null') throw null;
        if (action === 'false') throw false;
        if (action === 'zero') throw 0;
        if (action === 'string') throw '';
        const rows = action === 'rows' ? [{id: `first-${index}`, qty: 3.25}, null, {id: `last-${index}`, qty: 7.5}] : [];
        arrays.push(rows);
        return {data: {data: {rows}, rows: [{id: 'wrong-container'}]}};
      }};
      const opts = {limit: 37, fromMilliseconds: 0, toMilliseconds: 1700000000123};
      if (symbols[symbolIndex] !== undefined) opts.symbol = symbols[symbolIndex];
      const inputOpts = {...opts};
      let outcome;
      try {
        const rows = await subject.fetchLegacyTransactionHistoryRows(opts);
        outcome = {rows, alias: arrays.includes(rows)};
      } catch (error) {
        outcome = {error, errorName: error?.name, errorMessage: error?.message, errorIdentity: sentinels.includes(error)};
      }
      cases.push({symbolIndex, scenario, opts: inputOpts, finalOpts: opts, calls, outcome});
    }
  }
  assert.equal(cases.length, 90, 'LEGACY source total');
  return cases;
}
async function main() {
  const cases = await capture(members);
  assert.equal(cases.reduce((n, c) => n + c.calls.length, 0), 278, 'LEGACY total request observations');
  const attempts = [5, 1, 2, 5, 5, 5, 2, 5, 5, 5, 5, 5, 5, 5, 2];
  for (let index = 0; index < 90; index++) {
    assert.equal(cases[index].calls.length, index < 30 ? 1 : attempts[index % 15], 'LEGACY per-case attempts ' + index);
  }
  const ordinary = name => cases.find(c => c.symbolIndex === 2 && c.scenario === name);
  assert.equal(ordinary('empty').calls.length, 5, 'LEGACY unqualified final attempt');
  assert.equal(ordinary('first').calls.length, 1, 'LEGACY stop after rows');
  assert.equal(ordinary('first').outcome.rows.length, 3, 'LEGACY mixed rows retained');
  assert.equal(ordinary('first').outcome.alias, true, 'LEGACY selected array identity');
  assert.equal(ordinary('empty').outcome.alias, false, 'LEGACY fresh empty fallback');
  assert.equal(ordinary('last').outcome.rows.length, 3, 'LEGACY final fallback retains rows');
  assert.equal(ordinary('last').outcome.alias, true, 'LEGACY final fallback array identity');
  assert.equal(ordinary('error-empty').outcome.errorIdentity, true, 'LEGACY earlier error retained');
  assert.equal(ordinary('two-errors').outcome.error.marker, 42, 'LEGACY last error marker');
  assert.equal(ordinary('two-errors').outcome.errorIdentity, true, 'LEGACY last error identity');
  for (const falsy of ['null', 'false', 'zero', 'string']) {
    assert.equal(ordinary('error-' + falsy).outcome.rows.length, 0, 'LEGACY falsy last error replaces earlier error ' + falsy);
  }
  assert.equal(JSON.stringify(ordinary('empty').calls[4].params), JSON.stringify({size: 37, start_t: 0, end_t: 1700000000123}), 'LEGACY exact unqualified params');
  assert.equal(ordinary('null-response').outcome.errorName, 'TypeError', 'LEGACY null response destructuring');
  assert.equal(ordinary('null-data').outcome.rows.length, 0, 'LEGACY null data empty');
  assert.equal(ordinary('mutated-options').calls[1].params.size, 99, 'LEGACY rereads options after await');
  if (process.argv.includes('--controls')) {
    for (const [label, before, after, scenario, check] of [
      ['fallback', '[...symbolCandidates, undefined]', 'symbolCandidates', 'empty', c => assert.equal(c.calls.length, 5, 'LEGACY unqualified final attempt')],
      ['swallow', 'if (lastError) throw lastError;', 'if (lastError) return [];', 'error-empty', c => assert.equal(c.outcome.errorIdentity, true, 'LEGACY earlier error retained')],
      ['continue', 'if (rows.length > 0) return rows;', 'if (rows.length > 0) continue;', 'first', c => assert.equal(c.calls.length, 1, 'LEGACY stop after rows')],
    ]) {
      assert.equal(members.split(before).length, 2, label);
      const mutated = await capture(members.replace(before, after));
      assert.throws(() => check(mutated.find(c => c.symbolIndex === 2 && c.scenario === scenario)), {code: 'ERR_ASSERTION'}, 'LEGACY source control ' + label);
      console.error('LEGACY source control assertion-red: ' + label);
    }
  }
  process.stdout.write(JSON.stringify({architecture: process.arch, nodeMajor: process.versions.node.split('.')[0], cases}));
}
main().catch(error => {console.error(error); process.exitCode = 1;});
