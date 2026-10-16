const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
function execute(relative, imports = {}, selectedName) {
  const filename = path.join(root, relative);
  let source = fs.readFileSync(filename, 'utf8');
  if (selectedName) {
    const ast = ts.createSourceFile(filename, source, ts.ScriptTarget.ES2022, true);
    const declarations = ast.statements.filter(s => ts.isVariableStatement(s) && s.declarationList.declarations.some(d => ts.isIdentifier(d.name) && d.name.text === selectedName));
    assert.equal(declarations.length, 1, `unique actual declaration ${selectedName}`);
    source = declarations[0].getText(ast);
  }
  const compiled = ts.transpileModule(source, { compilerOptions: {
    module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
  }}).outputText;
  const module = { exports: {} };
  const requireExact = name => {
    assert(Object.hasOwn(imports, name), `unprovided runtime dependency: ${name}`);
    return imports[name];
  };
  vm.runInThisContext(`(function(require,module,exports){${compiled}\n})`, { filename })(requireExact, module, module.exports);
  return module.exports;
}
const shared = execute('packages/safe-cex/src/types.ts', {
  '@tealstreet/logging/src/LogSeverity': execute('packages/logging/src/LogSeverity.ts'),
});
const venue = execute('packages/safe-cex/src/exchanges/phemex/phemex.types.ts', { '../../types': shared });
const broker = execute('packages/safe-cex/src/exchanges/phemex/phemex.api.ts', {}, 'BROKER_ID');
const ordered = { BROKER_ID: broker.BROKER_ID };
for (const [key, value] of Object.entries(venue)) {
  ordered[key] = key === 'BASE_WS_URL'
    ? Object.entries(value).map(([k, v]) => [k, Object.entries(v)])
    : value && !Array.isArray(value) && typeof value === 'object' ? Object.entries(value) : value;
}
// Capture typeof and primitive identity before stringify can erase functions.
const lookups = Object.create(null);
for (const name of ['BASE_URL', 'BASE_WS_URL', 'ENDPOINTS', 'ORDER_STATUS', 'ORDER_TYPE', 'ORDER_SIDE', 'POSITION_SIDE', 'TIME_IN_FORCE']) {
  lookups[name] = Object.create(null);
  for (const key of [...Object.keys(venue[name]), ...Object.getOwnPropertyNames(Object.prototype), 'missing']) {
    const value = venue[name][key];
    lookups[name][key] = { type: typeof value, text: String(value), truthy: Boolean(value), nullish: value == null };
  }
  assert(Object.hasOwn(lookups[name], '__proto__'), `prototype observation retained: ${name}`);
  assert.equal(Object.keys(lookups[name]).length, Object.keys(venue[name]).length + Object.getOwnPropertyNames(Object.prototype).length + 1, `lookup census: ${name}`);
}
process.stdout.write(JSON.stringify({ ordered, lookups, recvWindow: { type: typeof venue.RECV_WINDOW, bits: (() => {
  const b = Buffer.alloc(8); b.writeDoubleBE(venue.RECV_WINDOW); return b.toString('hex');
})() } }));
