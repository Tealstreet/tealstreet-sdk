const assert = require('node:assert/strict');
const test = require('node:test');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const { dependencies, checkFields, shape } = require('./parity.cjs');
const parse = text => ts.createSourceFile('fixture.ts', text, ts.ScriptTarget.Latest, true);

test('readonly comparison checks both sides including mutable oracle fields', () => {
  const mutable = parse('type Model = { amount: number; extra?: { count?: number } };').statements[0].type;
  const readonly = parse('type Model = { readonly amount: number; extra?: { count?: number } };').statements[0].type;
  assert.equal(checkFields(mutable, mutable, 'Model'), 3);
  assert.throws(() => checkFields(mutable, readonly, 'Model'), /Model.amount: readonly/);
  assert.throws(() => checkFields(readonly, mutable, 'Model'), /Model.amount: readonly/);
});

test('enum coverage follows dependencies through nested properties and aliases', () => {
  const source = parse(`export enum Tenth { Added = 'added' }
    type Nested = { readonly status?: Tenth };
    export type Market = { readonly extra?: Nested };
    export enum Unrelated { Other = 'other' }`);
  const graph = dependencies(source, ['Market']);
  assert.deepEqual(graph.enums, ['Tenth']);
  assert.deepEqual(graph.declarations.map(d => d.name.text), ['Tenth', 'Nested', 'Market']);
});

test('index signature key names are cosmetic but key domains are checked', () => {
  const type = text => parse(`type Balance = { currencies?: { [${text}]: number } };`).statements[0].type;
  assert.equal(checkFields(type('currency: string'), type('key: string'), 'Balance'), 2);
  assert.throws(() => checkFields(type('key: string'), type('key: number'), 'Balance'), /key type/);
});

test('object-union branches retain mutability, optionality, index domains and counts', () => {
  const type = text => parse(`type Options = { readonly source?: ${text} };`).statements[0].type;
  const source = '{ type: "ws"; data?: { [key: string]: number } } | { type: "local"; readonly id: string }';
  assert.equal(checkFields(type(source), type(source), 'Options'), 6);
  for (const [change, replacement] of [['data?:', 'data:'], ['[key: string]', '[key: number]'], ['readonly id:', 'id:']]) {
    assert.throws(() => checkFields(type(source), type(source.replace(change, replacement)), 'Options'), /type/);
  }
});


test('required inherited numbers refuse rather than disappearing from interface census', () => {
  const inherited = parse('interface ExtraNumeric { hiddenNumeric: number } interface Model extends ExtraNumeric { amount: number }');
  const index = new Map(inherited.statements.map(d => [d.name.text, d]));
  assert.throws(() => dependencies(inherited, ['Model']), /Model: unexpected interface heritage/);
  assert.throws(() => shape(index.get('Model'), index), /Model: unexpected interface heritage/);
  const ordinary = parse('interface Model { amount: number }');
  const declaration = ordinary.statements[0];
  assert.equal(dependencies(ordinary, ['Model']).declarations.length, 1);
  assert.equal(shape(declaration, new Map()).members.length, 1);
});
