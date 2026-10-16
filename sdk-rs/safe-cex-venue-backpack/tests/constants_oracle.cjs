// Execute the actual venue module, not a rewritten reference implementation.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/backpack/backpack.types.ts');
const source = fs.readFileSync(file, 'utf8');
const result = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  fileName: file, reportDiagnostics: true,
});
assert.equal(result.diagnostics.length, 0, 'source transpilation');
const context = { exports: {} };
vm.runInNewContext(result.outputText, context, { filename: file });
const tables = Object.fromEntries(Object.entries(context.exports).map(([name, value]) => {
  const entries = Array.isArray(value) ? [...value] : Object.entries(value);
  for (const entry of entries) {
    if (Array.isArray(entry)) {
      assert.equal(typeof entry[0], 'string', `${name} key typeof`);
      assert.equal(typeof entry[1], 'string', `${name}.${entry[0]} typeof`);
    } else assert.equal(typeof entry, 'string', `${name} element typeof`);
  }
  return [name, entries];
}));
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},tables}));
