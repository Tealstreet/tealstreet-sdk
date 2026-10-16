const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const files = [
  'packages/logging/src/LogSeverity.ts',
  'packages/safe-cex/src/types.ts',
  'packages/safe-cex/src/exchanges/polymarket/polymarket.constants.ts',
];
const modules = new Map();
for (const file of files) {
  const output = ts.transpileModule(fs.readFileSync(path.join(root, file), 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  }).outputText;
  const exports = {};
  const requireSource = id => {
    const target = { '../../types': files[1], '@tealstreet/logging/src/LogSeverity': files[0] }[id];
    assert(target && modules.has(target), `unconfigured actual source import ${id}`);
    return modules.get(target);
  };
  vm.runInNewContext('(function(require,exports){' + output + '\n})', {}, { filename: file })(requireSource, exports);
  modules.set(file, exports);
}
const constants = modules.get(files[2]);
const numbers = [];
function visit(value, location) {
  if (typeof value === 'number') {
    const buffer = Buffer.alloc(8);
    buffer.writeDoubleBE(value);
    numbers.push([location, typeof value, buffer.toString('hex')]);
  } else if (value && typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) visit(child, `${location}/${key}`);
  }
}
visit(constants, '');
// The full module corpus is captured once; no subprocess per constant or field.
process.stdout.write(JSON.stringify({ capture: { architecture: process.arch, nodeMajor: Number(process.versions.node.split('.')[0]) }, constants, numbers }));
