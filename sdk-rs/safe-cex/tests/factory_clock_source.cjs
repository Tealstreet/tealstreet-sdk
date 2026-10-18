const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {root, ts} = require('../../safe-cex-types/scripts/accounts-source.cjs');

function declarations(relative, names, bindings, tail) {
  const file = path.join(root, relative);
  const source = fs.readFileSync(file, 'utf8');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const selected = ast.statements.filter(node =>
    ((ts.isClassDeclaration(node) || ts.isFunctionDeclaration(node)) && names.includes(node.name?.text)) ||
    (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => names.includes(d.name.getText(ast)))));
  assert.equal(selected.length, names.length, 'exact maintained declarations');
  const code = ts.transpileModule(selected.map(node => node.getText(ast)).join('\n') + tail, {
    compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
  }).outputText;
  const exports = {};
  return new Function('exports', ...Object.keys(bindings), code)(exports, ...Object.values(bindings));
}

(async () => {
  const timers = [];
  const dayjs = require('dayjs');
  dayjs.extend(require('dayjs/plugin/utc'));
  const Clock = declarations('packages/safe-cex/src/utils/virtualClock.ts', ['VirtualClock'], {
    dayjs,
    window: {},
    isNodeOrBunRuntime: () => false,
    TIME_SYNC_INTERVAL_MS: 300_000,
    setTimeout: (callback, delay) => { timers.push({callback, delay}); return timers.length; },
    clearTimeout: () => {},
  }, '\nreturn VirtualClock;');
  const clock = new Clock();
  const trace = [];
  let release;
  clock.setServerTimeSource(() => { trace.push('source'); return new Promise(resolve => { release = resolve; }); });
  class ActualStore {}
  class Exchange { constructor(name, options, store) { trace.push('constructor'); Object.assign(this, {name, options, store}); } }
  const factory = declarations('packages/safe-cex/src/index.ts', ['exchangeCache', 'loadExchange', 'createExchange'], {
    virtualClock: clock,
    exchangeRegistry: {binance: async () => { trace.push('loader'); return Exchange; }},
    DefaultStore: ActualStore,
  }, '\nreturn exports;');
  const store = new ActualStore();
  const options = {key: '', secret: ''};
  const exchange = await Promise.race([
    factory.createExchange('binance', options, store),
    new Promise(resolve => setImmediate(() => resolve('blocked'))),
  ]);
  assert.notEqual(exchange, 'blocked', 'construction precedes pending clock completion');
  assert.equal(exchange.store, store);
  assert.equal(exchange.options, options);
  assert.deepEqual(trace, ['source', 'loader', 'constructor']);
  await factory.createExchange('binance', options, store);
  await assert.rejects(factory.createExchange('unknown', options), /Unknown exchange: unknown/);
  assert.deepEqual(trace, ['source', 'loader', 'constructor', 'constructor']);
  assert.equal(timers.length, 0);
  release(Date.now() + 4000);
  for (let i = 0; i < 20; i++) await Promise.resolve();
  assert.deepEqual(timers.map(timer => timer.delay), [300_000]);
  clock.stop();
  console.log(JSON.stringify({trace, clockContinuesAfterConstruction: true, timers: [300_000]}));
})().catch(error => { console.error(error); process.exitCode = 1; });
