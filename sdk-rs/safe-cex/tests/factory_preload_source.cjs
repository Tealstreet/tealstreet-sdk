const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {root, ts} = require('../../safe-cex-types/scripts/accounts-source.cjs');
const file = path.join(root, 'packages/safe-cex/src/index.ts');
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const names = ['exchangeCache', 'loadExchange', 'preloadExchanges', 'isExchangeLoaded'];
const selected = ast.statements.filter(node =>
  (ts.isFunctionDeclaration(node) && names.includes(node.name?.text)) ||
  (ts.isVariableStatement(node) && node.declarationList.declarations.some(d => names.includes(d.name.getText(ast)))));
assert.equal(selected.length, names.length);
const code = ts.transpileModule(selected.map(node => node.getText(ast)).join('\n') + '\nreturn exports;', {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const make = registry => new Function('exports', 'exchangeRegistry', code)({}, registry);
const turn = () => new Promise(resolve => setImmediate(resolve));

(async () => {
  const trace = [];
  let release;
  const factory = make({
    bad: () => { trace.push('bad'); return Promise.reject(new Error('loader failed')); },
    slow: () => { trace.push('slow'); return new Promise(resolve => { release = resolve; }); },
    ready: () => { trace.push('ready'); return Promise.resolve(class Ready {}); },
  });
  const pending = factory.preloadExchanges(['bad', 'slow', 'ready']);
  const prefix = trace.slice();
  assert.deepEqual(prefix, ['bad', 'slow', 'ready']);
  assert.equal(factory.isExchangeLoaded('ready'), false);
  await assert.rejects(pending, /loader failed/);
  assert.equal(factory.isExchangeLoaded('ready'), true);
  assert.equal(factory.isExchangeLoaded('slow'), false);
  release(class Slow {});
  await turn();
  assert.equal(factory.isExchangeLoaded('slow'), true);
  await factory.preloadExchanges(['slow', 'ready', 'slow']);
  assert.deepEqual(trace, prefix);

  let finish;
  let calls = 0;
  const discarded = make({held: () => { calls++; return new Promise(resolve => { finish = resolve; }); }});
  discarded.preloadExchanges(['held']);
  assert.equal(calls, 1);
  finish(class Held {});
  await turn();
  assert.equal(discarded.isExchangeLoaded('held'), true);
  await discarded.preloadExchanges([]);
  let rejectFirst, rejectSecond;
  const competing = make({
    first: () => new Promise((_, reject) => { rejectFirst = reject; }),
    second: () => new Promise((_, reject) => { rejectSecond = reject; }),
  });
  const failed = competing.preloadExchanges(['first', 'second']);
  rejectSecond(new Error('second'));
  rejectFirst(new Error('first'));
  await assert.rejects(failed, error => error.message === 'second');
  process.stdout.write(JSON.stringify({prefix, siblingCachedAfterRejection: true, discardedCached: true, cachedLoaderCalls: 3}));
})().catch(error => { console.error(error); process.exitCode = 1; });
