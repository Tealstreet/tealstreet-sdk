import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
import { ts } from '../../safe-cex-store/tests/source_loader.mjs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const { TinyEmitter } = require('tiny-emitter');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const modules = new Map();
function loadSource(url) {
  const filename = url instanceof URL ? fileURLToPath(url) : url;
  if (modules.has(filename)) return modules.get(filename).exports;
  const module = { exports: {} };
  modules.set(filename, module);
  const code = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
    compilerOptions: {
      target: ts.ScriptTarget.ES2022,
      module: ts.ModuleKind.CommonJS,
      esModuleInterop: true,
    },
  }).outputText;
  const localRequire = (specifier) => {
    if (specifier === 'axios')
      return {
        get: async () => {
          throw Error('source oracle network disabled');
        },
      };
    if (specifier === '../transport/registry')
      return {
        withPublicCorsProxy: () => {
          throw Error('source oracle network disabled');
        },
      };
    if (specifier.startsWith('@tealstreet/'))
      return loadSource(path.join(root, 'packages', specifier.slice('@tealstreet/'.length) + '.ts'));
    if (specifier.startsWith('.')) return loadSource(path.resolve(path.dirname(filename), specifier + '.ts'));
    return require(specifier);
  };
  vm.runInThisContext('(function(require,module,exports){' + code + '\n})', { filename })(
    localRequire,
    module,
    module.exports,
  );
  return module.exports;
}

const url = new URL('../../../packages/safe-cex/src/exchanges/base.ts', import.meta.url);
const ast = ts.createSourceFile(url.pathname, fs.readFileSync(url, 'utf8'), ts.ScriptTarget.Latest, true);
const owner = ast.statements.find((n) => ts.isClassDeclaration(n) && n.name?.text === 'BaseExchange');
const ctor = owner.members.find(ts.isConstructorDeclaration);
const stmt = ctor.body.statements.find((n) =>
  n.getText(ast).startsWith('this.unsubscribeFromStore = this.store.subscribe('),
);
assert(stmt, 'actual BaseExchange constructor subscription');
const install = vm.runInNewContext(`(function(){${stmt.getText(ast)}})`);
const { DefaultStore } = loadSource(new URL('../../../packages/safe-cex/src/store/store.base.ts', import.meta.url));
const store = new DefaultStore();
const host = { store, emitter: new TinyEmitter() };
install.call(host);
let reads = 0;
Object.defineProperty(store.balance, 'used', {
  enumerable: true,
  configurable: true,
  get() {
    reads++;
    throw new Error('unrelated');
  },
});
const rows = [];
host.emitter.on('update', (data) => rows.push(data));
store.setSetting('isHedged', true);
store.setSetting('isHedged', false);
assert.equal(reads, 0);
assert.equal(rows.length, 2);
assert.equal(rows[0], rows[1]);
assert.equal(rows[0].balance, store.balance);
assert.equal(rows[0].options.isHedged, false);
const marker = new Error('native update listener sentinel');
host.emitter.on('update', () => {
  throw marker;
});
let later = false;
store.subscribe(() => {
  later = true;
});
assert.throws(
  () => store.setSetting('isHedged', true),
  (error) => error === marker,
);
assert.equal(reads, 0);
assert.equal(later, false);
assert.equal(store.options.isHedged, true);
host.unsubscribeFromStore();
console.log(
  'native Store emitter source: live referent, no unrelated getters, committed write and throw ordering PASS',
);
