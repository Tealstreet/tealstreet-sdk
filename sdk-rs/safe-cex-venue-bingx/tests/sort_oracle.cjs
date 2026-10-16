// Execute the exact maintained initializer: no copied comparator body or imports.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/bingx/bingx.api.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const declarations = source.statements.filter(ts.isVariableStatement)
  .flatMap(s => [...s.declarationList.declarations])
  .filter(d => ts.isIdentifier(d.name) && d.name.text === 'sort');
assert.equal(declarations.length, 1, 'sort declaration census');
const initializer = declarations[0].initializer;
assert(initializer && ts.isFunctionExpression(initializer), 'sort function initializer');
const compiled = ts.transpileModule(`(${initializer.getText(source)})`, {
  reportDiagnostics: true,
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
});
assert(!compiled.diagnostics.some(d => d.category === ts.DiagnosticCategory.Error), 'sort transpilation');
const sort = vm.runInNewContext(compiled.outputText, {}, { filename: file });
const runtime = { node: process.version, arch: process.arch, platform: process.platform,
  locale: new Intl.Collator().resolvedOptions().locale, icu: process.versions.icu };
assert(/^en(?:-|$)/.test(runtime.locale), 'English default locale capture');
const atoms = ['', 'timestamp', 'signature', 'quantity', 'price', 'recvWindow', '10', '2',
  'a', 'A', 'ä', 'z', 'é', 'e\u0301', 'I', 'ı', 'İ', '_', '-', ' ', '\u0000',
  '\ufeff', '中', '😀', '🧑‍🚀', 'Ａ', 'ß', 'ss'];
const cases = [];
for (const a of atoms) for (const b of atoms) cases.push({ a, b, expected: Math.sign(sort(a, b)) });
const boundary = ['\ud800', '\udfff', 'a\ud800b'].map(a => ({
  units: Array.from({length:a.length}, (_,i)=>a.charCodeAt(i)), expected: sort(a, 'a')
}));
process.stdout.write(JSON.stringify({runtime, cases, boundary}));
