// Use the canonical extractor, restricted to the changed source file.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const extractor = path.join(root, 'sdk-rs/parity/extract-ts/src/extract.ts');
const moduleExports = {};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(extractor, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
}).outputText, { exports: moduleExports, require: name => name === 'typescript' ? ts : require(name) }, { filename: extractor });
const basenames = ['blofin.ws-private.ts'];
const files = basenames.map(name => path.join(root, 'packages/safe-cex/src/exchanges/blofin', name));
const program = ts.createProgram(files, {
  target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
  moduleResolution: ts.ModuleResolutionKind.Bundler, skipLibCheck: true,
});
const output = {};
for (const [index, file] of files.entries()) {
  const source = program.getSourceFile(file);
  assert(source, 'source_in_program');
  const diagnostics = [...program.getOptionsDiagnostics(), ...program.getGlobalDiagnostics(),
    ...program.getSyntacticDiagnostics(source), ...program.getSemanticDiagnostics(source)];
  assert.equal(diagnostics.length, 0, ts.formatDiagnosticsWithColorAndContext(diagnostics, {
    getCanonicalFileName: n => n, getCurrentDirectory: () => root, getNewLine: () => '\n',
  }));
  output[basenames[index]] = moduleExports.extractFile(source, program.getTypeChecker(), path.join(root, 'packages/safe-cex'));
}
process.stdout.write(JSON.stringify(output));
