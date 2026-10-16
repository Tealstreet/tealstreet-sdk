'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(compiler);
const root = path.resolve(__dirname, '../../..');
const file = path.join(root, 'packages/safe-cex/src/exchanges/grvt/grvt.types.ts');
let source = fs.readFileSync(file, 'utf8');
if (process.env.GRVT_PROTOCOL_MUTATION === 'endpoint') {
  const original = "INSTRUMENTS: '/lite/v1/all_instruments'";
  assert(source.includes(original), 'TS endpoint mutation matched');
  source = source.replace(original, "INSTRUMENTS: '/wrong-endpoint'");
} else {
  assert(!process.env.GRVT_PROTOCOL_MUTATION, 'unknown protocol mutation');
}
const output = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
  reportDiagnostics: true,
});
assert.equal(output.diagnostics.length, 0, 'protocol transpilation diagnostics');
const exportsObject = {};
vm.runInNewContext(output.outputText, { exports: exportsObject }, { filename: file });
for (const name of ['BASE_URLS', 'WSS_URLS', 'ENDPOINTS', 'GRAFANA_ENDPOINTS_EVERY', 'GRAFANA_ENDPOINTS_PERIODIC']) {
  const value = exportsObject[name];
  assert(value && typeof value === 'object', `missing protocol export ${name}`);
  for (const [key, entry] of Object.entries(value)) {
    assert.equal(typeof entry, 'string');
    assert(!/[\t\r\n]/.test(entry));
    process.stdout.write(`${name}\t${key}\t${entry}\n`);
  }
}
