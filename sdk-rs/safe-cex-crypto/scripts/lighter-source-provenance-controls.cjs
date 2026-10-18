// Mutate actual maintained declaration text through the same scoped guard.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {assertDeclarations, runtimeDeclarationNames, originalProvenance} = require('./lighter-source-provenance.cjs');
const pins = require('../tests/fixtures/lighter/reviewed-source-provenance.json');
const file = 'packages/safe-cex/src/types.ts';
const text = fs.readFileSync(path.resolve(__dirname, '../../..', file), 'utf8');
const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const pin = pins[file];
assertDeclarations(file, pin, text);
const exchangeName = source.statements.find(node => ts.isTypeAliasDeclaration(node) && node.name.text === 'ExchangeName');
assert.ok(exchangeName);
const unrelated = [
  text.slice(0, exchangeName.type.end) + " | 'unrelated-provenance-control'" + text.slice(exchangeName.type.end),
  text + '\nexport type UnrelatedProvenanceControl = { extra?: string };\n',
];
for (const changed of unrelated) assertDeclarations(file, pin, changed);
const namedMutationFailures = [];
for (const name of runtimeDeclarationNames()) {
  const declaration = source.statements.find(node => ts.isEnumDeclaration(node) && node.name.text === name);
  const value = declaration.members[0].initializer;
  assert.ok(ts.isStringLiteral(value));
  const changed = text.slice(0, value.getStart(source)) + JSON.stringify(value.text + '-control') + text.slice(value.end);
  assert.throws(() => assertDeclarations(file, pin, changed), error =>
    error.message.includes(`independent declaration drift: ${file}::${name}`));
  namedMutationFailures.push(name);
}
const omitted = {...pin, declarationsSha256: {...pin.declarationsSha256}};
delete omitted.declarationsSha256.OrderSide;
assert.throws(() => assertDeclarations(file, omitted, text), /Lighter runtime import coverage/);
const side = source.statements.find(node => ts.isEnumDeclaration(node) && node.name.text === 'OrderSide');
assert.throws(() => assertDeclarations(file, pin, text.slice(0, side.getStart(source)) + text.slice(side.end)),
  /reviewed runtime declaration missing: .*::OrderSide/);
assert.throws(() => originalProvenance({sources: {}}), /captured source digest missing/);
process.stdout.write(JSON.stringify({unrelatedTypeEditsAccepted: unrelated.length, namedMutationFailures,
  missingPinRejected: true, missingDeclarationRejected: true, missingCapturedSourceRejected: true}));
