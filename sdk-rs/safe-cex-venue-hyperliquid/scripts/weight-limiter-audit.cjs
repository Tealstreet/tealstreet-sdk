'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const root = path.resolve(__dirname, '../../..');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript', {paths: [root]}));
const axios = require('axios');
const base = path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid');
const compile = source => ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
const source = fs.readFileSync(path.join(base, 'hyperliquid.api.ts'), 'utf8');
const tree = ts.createSourceFile('api.ts', source, ts.ScriptTarget.Latest, true);
const matches = tree.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]).filter(n => n.name.getText(tree) === 'addRateLimitInterceptor');
assert.equal(matches.length, 1);
async function probe(discard, fail) {
  let now = 0, timers = [];
  const trace = [];
  function load(file) {
    const exports = {};
    vm.runInNewContext(compile(fs.readFileSync(path.join(base, file), 'utf8')), {
      exports, require(name) {assert.equal(name, './hyperliquid.types'); return load('hyperliquid.types.ts');},
      Date: {now: () => now}, setTimeout(resolve, delay) {timers.push({at: now + delay, resolve});}
    }, {filename: file});
    return exports;
  }
  const rate = load('hyperliquid.rateLimiter.ts');
  const scope = {exports: {}, isSigningCaptureRequest: () => false, ...rate};
  vm.runInNewContext(compile('const install = ' + matches[0].initializer.getText(tree) + '; exports.install = install;'), scope);
  const make = account => {
    const xhr = axios.create({adapter: async config => {
      const body = JSON.parse(config.data);
      trace.push({account, label: body.label, at: now});
      if (fail && body.label === 'first') throw Error('recorded rejection');
      return {data: {status: 'ok'}, status: 200, headers: {}, config};
    }});
    scope.exports.install(xhr);
    return xhr;
  };
  const a = make('a'), b = make('b');
  const body = (label, size) => ({label, action: {type: 'order', orders: Array(size).fill(null)}});
  const first = a.post('/exchange', body('first', 80)).catch(() => null);
  let ignored = b.post('/exchange', body('second', 0));
  const third = a.post('/exchange', body('third', 40));
  if (discard) ignored = undefined;
  const synchronous = trace.length;
  const flush = async () => {for (let i = 0; i < 64; i++) await Promise.resolve();};
  await flush();
  const stages = [{at: now, trace: [...trace]}];
  for (const at of [149, 150, 199, 200]) {
    now = at;
    const due = timers.filter(timer => timer.at <= now);
    timers = timers.filter(timer => timer.at > now);
    due.forEach(timer => timer.resolve());
    await flush();
    stages.push({at, trace: [...trace]});
  }
  await Promise.all([first, third, ignored]);
  assert.deepEqual(trace.map(row => row.at), [0, 150, 200]);
  assert.equal(timers.length, 0);
  return {discard, fail, synchronous, stages};
}
(async () => {
  const rows = [];
  for (const discard of [false, true]) for (const fail of [false, true]) rows.push(await probe(discard, fail));
  assert.equal(rows.length, 4);
  process.stdout.write(JSON.stringify(rows) + '\n');
})().catch(error => {console.error(error); process.exitCode = 1;});
