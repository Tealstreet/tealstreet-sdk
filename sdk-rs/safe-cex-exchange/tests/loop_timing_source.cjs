const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const root = path.resolve(__dirname, '../../..');
const filename = path.join(root, 'packages/safe-cex/src/utils/loop.ts');
const source = fs.readFileSync(filename, 'utf8').replace(/^import .*;\n/gm, '');
async function profile(interval, reject = false, visible = false) {
  const trace = [];
  const callbacks = [];
  let settle;
  const context = {
    exports: {}, Number, Set,
    sleep: delay => { trace.push(`sleep:${delay}`); return new Promise((resolve, fail) => { settle = reject ? () => fail('sleep rejected') : resolve; }); },
    console: { warn: () => trace.push('warn') },
    window: visible ? {} : undefined,
    document: visible ? { get visibilityState() { trace.push('visibility'); return 'visible'; } } : undefined,
    requestAnimationFrame: callback => { trace.push('raf'); callbacks.push(callback); },
    setTimeout: callback => { trace.push('timeout:0'); callbacks.push(callback); },
  };
  vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, context, { filename });
  const waiter = context.exports.loop(() => trace.push('callback'), interval);
  const before = [...trace];
  if (settle) settle();
  let error = null;
  await waiter.catch(value => { error = value; });
  const after = [...trace];
  callbacks.forEach(callback => callback());
  return { before, after, complete: trace, error };
}
(async () => {
  process.stdout.write(JSON.stringify({
    pending: await profile(100),
    visible: await profile(12.5, false, true),
    fallback: await profile(undefined),
    rejected: await profile(100, true),
    disabled: await profile(Infinity),
  }));
})().catch(error => { console.error(error); process.exitCode = 1; });
