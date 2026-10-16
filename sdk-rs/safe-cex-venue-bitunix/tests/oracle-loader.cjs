'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
process.stderr.write(JSON.stringify({ node: process.version, v8: process.versions.v8, arch: process.arch, platform: process.platform }) + '\n');
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function load(file) {
  file = path.resolve(file);
  if (cache.has(file)) return cache.get(file);
  const module = { exports: {} };
  cache.set(file, module.exports);
  const source = fs.readFileSync(file, 'utf8');
  const compiled = ts.transpileModule(source, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
    fileName: file,
  }).outputText;
  vm.runInThisContext(`(function(require,module,exports){${compiled}\n})`, { filename: file })(
    (id) => {
      if (id === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
      assert(id.startsWith('.'), `unexpected oracle dependency ${id}`);
      return load(path.resolve(path.dirname(file), id + '.ts'));
    }, module, module.exports,
  );
  return module.exports;
}
module.exports = { load, root, ts, fs, path, vm, assert };
