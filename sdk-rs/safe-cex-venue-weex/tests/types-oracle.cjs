// Execute the actual venue module and its actual shared enums; no copied enum stubs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert.ok(compiler, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(compiler);
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  const source = fs.readFileSync(file, 'utf8');
  const compiled = ts.transpileModule(source, {
    fileName: file,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    reportDiagnostics: true,
  });
  assert.equal((compiled.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  const exports = {};
  cache.set(file, exports);
  const localRequire = specifier => {
    if (specifier === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    assert.ok(specifier.startsWith('.'), `unexpected external dependency: ${specifier}`);
    return load(path.resolve(path.dirname(file), `${specifier}.ts`));
  };
  vm.runInThisContext(`(function(require,exports){${compiled.outputText}\n})`, { filename: file })(localRequire, exports);
  return exports;
}
const venue = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
function capture(value) {
  if (typeof value === 'number') {
    const buffer = Buffer.alloc(8);
    if (Number.isNaN(value)) buffer.writeBigUInt64BE(0x7ff8000000000000n);
    else buffer.writeDoubleBE(value);
    return { type: typeof value, bits: buffer.toString('hex') };
  }
  if (typeof value === 'string' && [...value].some(c => c.length === 1 && c.charCodeAt(0) >= 0xd800 && c.charCodeAt(0) <= 0xdfff)) {
    return { utf16: Array.from({ length: value.length }, (_, i) => value.charCodeAt(i)) };
  }
  if (value instanceof Set) return Array.from(value, capture);
  if (Array.isArray(value)) return value.map(capture);
  if (value && typeof value === 'object') return Object.entries(value).map(([key, item]) => [key, capture(item)]);
  return value;
}
// Capture typeof and binary64 bits before stringify; serialized numbers would hide -0.
const result = Object.fromEntries(Object.entries(venue).map(([name, value]) => [name, capture(value)]));
const header = { arch: process.arch, node: process.version, nodeMajor: Number(process.versions.node.split('.')[0]), nanBits: '7ff8000000000000' };
assert.equal(capture(-0).bits, '8000000000000000', 'capture_signed_zero');
assert.equal(capture(0).bits, '0000000000000000', 'capture_positive_zero');
assert.equal(capture(NaN).bits, header.nanBits, 'capture_nan_canonical');
module.exports = { load, capture, root, ts, header };
if (require.main === module) process.stdout.write(JSON.stringify({ header, constants: result }));
