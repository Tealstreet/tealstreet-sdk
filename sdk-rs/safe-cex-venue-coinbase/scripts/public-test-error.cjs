const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');

const sourcePath = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-public-frames.ts');
const source = ts.createSourceFile(sourcePath, fs.readFileSync(sourcePath, 'utf8'), ts.ScriptTarget.Latest, true);
const declarations = new Map();
for (const statement of source.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    if (ts.isIdentifier(declaration.name)) declarations.set(declaration.name.text, declaration);
  }
}
const isRecord = declarations.get('isRecord');
const parser = declarations.get('parsePublicTestError');
assert.ok(isRecord?.initializer && parser?.initializer, 'source parser dependencies must exist');
const exportsObject = {};
const code = [
  `const isRecord = ${isRecord.initializer.getText(source)};`,
  `const parsePublicTestError = ${parser.initializer.getText(source)};`,
  'exportsObject.parsePublicTestError = parsePublicTestError;',
].join('\n');
vm.runInNewContext(ts.transpileModule(code, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText, { exportsObject });

const inputs = [
  ['undefined', undefined],
  ['null', null],
  ['boolean', false],
  ['array', []],
  ['string', 'error'],
  ['empty-object', {}],
  ['missing-code', { message: 'denied' }],
  ['missing-message', { code: 1 }],
  ['empty-message', { code: 1, message: '' }],
  ['code-string', { code: '1', message: 'denied' }],
  ['fractional-code', { code: 1.5, message: 'denied' }],
  ['safe-boundary', { code: Number.MAX_SAFE_INTEGER, message: 'edge' }],
  ['unsafe-integer', { code: 9007199254740992, message: 'edge' }],
  ['negative-safe-integer', { code: -7, message: 'denied' }],
  ['data-absent', { code: 2, message: 'denied' }],
  ['data-null', { code: 2, message: 'denied', data: null }],
  ['data-object', { code: 2, message: 'denied', data: { reason: 'policy' } }],
  ['extra-key', { code: 2, message: 'denied', extra: true }],
  ['prototype-named-key', JSON.parse('{"code":2,"message":"denied","__proto__":null}')],
];
const cases = inputs.map(([label, input]) => ({
  label,
  ...(input === undefined ? {} : { input }),
  result: exportsObject.parsePublicTestError(input) ?? null,
}));
process.stdout.write(JSON.stringify({ cases }));
