const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/poseidon-crypto/src/types.ts');
const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0, 'Poseidon type syntax');
for (const [name, fields] of [['SchnorrKeyPair', ['privateKey', 'publicKey']], ['WasmError', ['error']]]) {
  const declarations = tree.statements.filter(n => ts.isInterfaceDeclaration(n) && n.name.text === name);
  assert.equal(declarations.length, 1, `${name} declaration inventory`);
  const declaration = declarations[0];
  assert.ok(!declaration.heritageClauses?.length, `${name} unsupported heritage`);
  assert.deepEqual(declaration.members.map(field => {
    assert.ok(ts.isPropertySignature(field), `${name} property kind`);
    return [field.name.getText(tree), field.type.getText(tree), !!field.questionToken];
  }), fields.map(name => [name, 'string', false]), `${name} source schema`);
}
const texts = ['', ' x ', 'é', '👩', '\ufeff', '\u0085', '\ud800', '\udfff', 'x\ud800y'];
const units = value => Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
const bases = { SchnorrKeyPair: { privateKey: 'private-A', publicKey: 'public-B' }, WasmError: { error: 'failure-C' } };
const strings = Object.entries(bases).flatMap(([record, base]) => Object.keys(base).flatMap(field => texts.map(text => {
  const encoded = JSON.stringify({ ...base, [field]: text });
  const value = JSON.parse(encoded)[field];
  return {record,field,encoded,units:units(value),length:value.length,type:typeof value,representable:value.isWellFormed()};
})));
if (process.env.POSEIDON_TYPES_MUTATION === 'surrogate-content') {
  const original = strings[6].encoded;
  strings[6].encoded = original.replace('\\ud800', '\\ud801');
  assert.notEqual(strings[6].encoded, original, 'Poseidon surrogate control changes bytes');
  assert.equal(strings[6].encoded.length, original.length, 'Poseidon surrogate control preserves length');
}
const body = {bases,strings};
const contentHash = require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex');
process.stdout.write(JSON.stringify({contentHash,...body}));
