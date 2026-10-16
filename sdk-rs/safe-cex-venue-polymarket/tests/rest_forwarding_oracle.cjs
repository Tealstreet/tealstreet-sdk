const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'isolated compiler required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/polymarket');
const source = fs.readFileSync(path.join(root, 'polymarket.api.ts'), 'utf8');
const ast = ts.createSourceFile('polymarket.api.ts', source, ts.ScriptTarget.Latest, true);
const klass = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === 'PolymarketAPI');
const names = ['getSimplifiedMarkets', 'getSpread', 'getGammaMarket', 'getGammaMarketBySlug', 'searchGammaMarkets', 'cancelOrder', 'cancelOrders', 'cancelMarketOrders', 'cancelAllOrders', 'deleteApiKey'];
const methods = names.map(name => {
  const rows = klass.members.filter(n => n.name?.getText(ast) === name);
  assert.equal(rows.length, 1, `original method ${name}`);
  return source.slice(rows[0].getFullStart(), rows[0].end);
});
const constants = fs.readFileSync(path.join(root, 'polymarket.constants.ts'), 'utf8');
const constantsAst = ts.createSourceFile('constants.ts', constants, ts.ScriptTarget.Latest, true);
const declaration = constantsAst.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(constantsAst) === 'ENDPOINTS'));
assert.equal(declaration.length, 1, 'original endpoint table');
const context = { exports: {} };
vm.runInNewContext(ts.transpileModule(`${declaration[0].getText(constantsAst)}\nclass API { ${methods.join('\n')} }\nexports.API=API;`, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS } }).outputText, context);
const units = value => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
const bits = value => { const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const strings = ['id/1', '', '🦀 λ,a?b#c%20', '\ud800', '\udfff', 'a/b/c?x=1&y=2', '__proto__', 'constructor', 'a\0b'];
const counts = [3, 0, 3, 3, 3, 3, 3, 3, 3];
function args(name, index) {
  if (['getSimplifiedMarkets', 'cancelAllOrders', 'deleteApiKey'].includes(name)) return [];
  if (name === 'cancelOrders') return [index === 1 ? [] : [strings[index], 'second', strings[index]]];
  return [strings[index]];
}
function payload(index) {
  return [{ tag: 'reply', nested: [17, 23] }, null, false, 0, 'raw reply', ['reply-A', 'reply-B'], undefined, NaN, -0][index];
}
function capture(client, method, url, config) {
  return { client, method, url: units(url),
    params: config?.params ? Object.entries(config.params).map(([k, v]) => [k, units(v)]) : null,
    data: config && Object.hasOwn(config, 'data')
      ? (Array.isArray(config.data) ? { kind: 'strings', values: config.data.map(units) } : { kind: 'fields', values: Object.entries(config.data).map(([k, v]) => [k, units(v)]) })
      : { kind: 'absent' } };
}
(async () => {
  const cases = [];
  for (const name of names) for (let index = 0; index < 9; index++) for (const fails of [false, true]) {
    const instance = new context.exports.API();
    const input = args(name, index), data = payload(index);
    const error = { code: 'E_NATIVE', message: 'original failure', marker: 71 };
    const calls = []; let finish;
    for (const client of ['xhr', 'gammaXhr']) {
      instance[client] = {};
      for (const method of ['get', 'delete']) instance[client][method] = (url, config) => {
        calls.push(capture(client, method, url, config));
        if (name === 'cancelOrders') {
          assert.strictEqual(config.data, input[0], 'source cancel array identity');
          assert.equal(config.data.length, counts[index], 'source independent cancel array count');
          config.data.push('from-host'); assert.equal(input[0].at(-1), 'from-host', 'source host to input alias');
          input[0].push('from-input'); assert.equal(config.data.at(-1), 'from-input', 'source input to host alias');
        }
        return new Promise((resolve, reject) => { finish = () => fails ? reject(error) : resolve({ data }); });
      };
    }
    let settled = false;
    const pending = instance[name](...input);
    pending.then(() => { settled = true; }, () => { settled = true; });
    assert.equal(calls.length, 1, 'source eager single dispatch before await');
    await Promise.resolve(); assert.equal(settled, false, 'source awaits operation completion');
    finish();
    if (fails) {
      try { await pending; assert.fail('source rejection required'); }
      catch (caught) { assert.strictEqual(caught, error, 'source native error identity'); }
    } else {
      const result = await pending;
      if (name.startsWith('get') || name === 'searchGammaMarkets') {
        assert(Object.is(result, data), 'source ordinary response identity/value');
        if (index === 0) {
          result.fromReply = 17; assert.equal(data.fromReply, 17, 'source reply to data alias');
          data.fromInput = 23; assert.equal(result.fromInput, 23, 'source data to reply alias');
        }
      } else assert.equal(result, undefined, 'source discards delete response');
    }
    cases.push({ name, index, fails, arg: units(strings[index]), ids: (index === 1 ? [] : [strings[index], 'second', strings[index]]).map(units), request: calls[0], numberBits: [3, 7, 8].includes(index) ? bits(data) : null });
  }
  // Discard the JS promise observation; host still completes the underlying work.
  let completed = 0, finish;
  const detached = new context.exports.API();
  detached.xhr = { get() { return new Promise(resolve => { finish = () => { completed++; resolve({ data: 17 }); }; }); } };
  detached.getSimplifiedMarkets();
  finish(); await Promise.resolve();
  assert.equal(completed, 1, 'source ignored observation retains host operation');
  assert.equal(cases.length, 180, 'independent forwarding fixture floor');
  assert.equal(cases[179].name, 'deleteApiKey');
  assert.equal(cases[179].index, 8); assert.equal(cases[179].fails, true, 'last forwarding fixture');
  process.stdout.write(JSON.stringify({ cases, detachedCompleted: completed }));
})().catch(error => { console.error(error); process.exitCode = 1; });
