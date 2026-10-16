// Execute maintained sources; no copied values and no dependency stubs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const cache = new Map();
function load(file) {
  file = path.resolve(file);
  if (cache.has(file)) return cache.get(file);
  const module = { exports: {} };
  cache.set(file, module.exports);
  const source = fs.readFileSync(file, 'utf8');
  const result = ts.transpileModule(source, {
    fileName: file, reportDiagnostics: true,
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  });
  assert(!result.diagnostics.some(d => d.category === ts.DiagnosticCategory.Error), `transpile ${file}`);
  const localRequire = spec => {
    if (spec === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    assert(spec.startsWith('.'), `unhandled dependency ${spec}`);
    return load(path.resolve(path.dirname(file), `${spec}.ts`));
  };
  vm.runInThisContext(`(function(require,module,exports){${result.outputText}\n})`, { filename: file })(localRequire, module, module.exports);
  return module.exports;
}
const constants = load(path.join(root, 'packages/safe-cex/src/exchanges/bingx/bingx.types.ts'));
const { inverseObj } = load(path.join(root, 'packages/safe-cex/src/utils/inverseObj.ts'));
const tables = ['ORDER_TYPE', 'ORDER_SIDE', 'POSITION_SIDE', 'TIME_IN_FORCE', 'ORDER_STATUS', 'ORDER_TRIGGER_TYPE'];
const ordered = {};
for (const [name, value] of Object.entries(constants)) {
  if (name === 'BASE_WS_URL') ordered[name] = Object.entries(value).map(([k,v]) => [k,Object.entries(v)]);
  else ordered[name] = value && !Array.isArray(value) && typeof value === 'object' ? Object.entries(value) : value;
}
const inherited = {};
const inverse = {};
const keys = [...Object.getOwnPropertyNames(Object.prototype), 'missing-bingx', 'limit', 'postonly'];
for (const name of tables) {
  inherited[name] = keys.map(key => {
    const value = constants[name][key];
    // Capture observable function/object identities BEFORE JSON removes them.
    return { key, type: typeof value, text: String(value), present: key in constants[name] };
  });
  inverse[name] = Object.entries(inverseObj(constants[name]));
}
const recv = Buffer.alloc(8);
recv.writeDoubleBE(constants.RECV_WINDOW);
process.stdout.write(JSON.stringify({ ordered, inherited, inverse, recvType: typeof constants.RECV_WINDOW, recvBits: recv.toString('hex') }));
