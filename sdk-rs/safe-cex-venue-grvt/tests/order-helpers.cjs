const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated compiler required');
const ts = require(compiler);
const { bits } = require('../../safe-cex-num/tests/support/number-bits.cjs');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function read(relative) {
  const file = path.join(root, relative);
  return ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
}
const tree = read('exchanges/grvt/grvt.exchange.ts');
const cls = tree.statements.find(s => ts.isClassDeclaration(s) && s.name.text === 'GrvtExchange');
const status = cls.members.find(s => s.name?.getText(tree) === 'mapOrderStatus');
const creation = cls.members.find(s => s.name?.getText(tree) === 'getOrderCreationTimeMs');
assert(status?.initializer && ts.isArrowFunction(status.initializer), 'actual status arrow');
assert(creation?.body, 'actual creation-time method');
const shared = read('types.ts');
const enumeration = shared.statements.find(s => ts.isEnumDeclaration(s) && s.name.text === 'OrderStatus');
assert(enumeration, 'actual shared OrderStatus enum');
const context = { exports: {} };
vm.runInNewContext(ts.transpileModule(
  enumeration.getText(shared) + '\nexport const status = ' + status.initializer.getText(tree) +
  '\nexport function creation(metadata:any) ' + creation.body.getText(tree),
  { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } },
).outputText, context);

const statuses = ['PENDING', 'OPEN', 'FILLED', 'CANCELLED', 'REJECTED'];
const statusInputs = [undefined, null, false, true, 0, 1, [], {}, '', 'UNKNOWN', 'CANCELED', '__proto__'];
for (const input of statuses) {
  statusInputs.push(input, input.toLowerCase(), ' ' + input, input + ' ', input + '\n', input + '\0', input + '😀');
}
const statusRows = statusInputs.map(input => ({
  present: input !== undefined, input: input ?? null, expected: context.exports.status(input),
}));
assert.equal(statusRows.length, 47, 'ORDER STATUS oracle floor');

// Each token is parsed by JSON.parse, so numeric overflow, -0 and Number
// stringification are observed before the timestamp helper is invoked.
const tokens = [
  undefined, 'null', 'false', 'true', '[]', '{}', '""', '0', '-0', '1',
  '999999', '1000000', '1000001', '1999999', '1.5', '-1000000',
  '1e20', '1e21', '1e400', '-1e-400', '9007199254740993',
  '"0"', '"0000001000000"', '"999999"', '"1000000"', '"1999999"',
  '"+1000000"', '"-1000000"', '"1000000.0"', '"1e6"', '"1000000\\n"',
  '" 1000000"', '"１００００００"', '"1000000😀"',
  '"9007199254740992999999"', '"9007199254740993000000"',
  JSON.stringify('9'.repeat(315)), JSON.stringify('0'.repeat(400) + '1000000'),
];
function property(name, token) { return token === undefined ? [] : [JSON.stringify(name) + ':' + token]; }
const rawInputs = ['null', 'false', '0', '"metadata"', '[]', '{}'];
for (const full of tokens) for (const lite of tokens) {
  rawInputs.push('{' + [...property('create_time', full), ...property('ct', lite)].join(',') + '}');
}
const creationRows = rawInputs.map(raw => {
  const expected = context.exports.creation(JSON.parse(raw));
  return { raw, expected: expected === undefined ? null : bits(expected) };
});
assert.equal(creationRows.length, 1450, 'CREATION TIME oracle floor');
const refusals = ['"\\ud800"', '"FILLED\\udfff"', '"1000000\\ud800"'].map(raw => ({
  raw,
  status: context.exports.status(JSON.parse(raw)),
  creation: context.exports.creation({ create_time: JSON.parse(raw) }) ?? null,
}));
process.stdout.write(JSON.stringify({ statusRows, creationRows, refusals }));
