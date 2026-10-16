// Boundary contracts stay TS-owned; verify real imports separately from the finite-data fixture.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const bindings = path.resolve(__dirname, '../bindings');
const files = fs.readdirSync(bindings).filter(f => f.endsWith('.ts')).map(f => path.join(bindings, f));
const program = ts.createProgram(files, {
  strict: true, noEmit: true, skipLibCheck: true, types: [], target: ts.ScriptTarget.ES2022,
  module: ts.ModuleKind.CommonJS, moduleResolution: ts.ModuleResolutionKind.Node10,
  ignoreDeprecations: '6.0',
});
for (const file of files) {
  const diagnostics = program.getSemanticDiagnostics(program.getSourceFile(file));
  assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: f => f, getCurrentDirectory: () => bindings, getNewLine: () => '\n',
  }));
}
const bridge = program.getSourceFile(path.join(bindings, 'runtime-boundaries.ts'));
const checker = program.getTypeChecker();
const expected = new Map([
  ['RemoteSigner', 'packages/safe-cex/src/types.ts'],
  ['PayloadSigner', 'packages/safe-cex/src/types.ts'],
  ['PublicDataSource', 'packages/safe-cex/src/types.ts'],
  ['TransportProvider', 'packages/safe-cex/src/transport/types.ts'],
]);
for (const exported of checker.getExportsOfModule(checker.getSymbolAtLocation(bridge))) {
  const target = checker.getAliasedSymbol(exported);
  const suffix = expected.get(exported.name);
  assert(suffix, `${exported.name}: unexpected boundary`);
  assert.equal(target.name, exported.name, `${exported.name}: target identity`);
  assert(target.declarations?.some(d => d.getSourceFile().fileName.endsWith('/' + suffix)), `${exported.name}: oracle import must resolve`);
  expected.delete(exported.name);
}
assert.equal(expected.size, 0, 'all four runtime contracts must resolve to their original TS symbols');
console.log('Runtime boundary imports passed: generated-file semantic checks and all four contracts resolve to original TS symbols; invocation remains unported.');
