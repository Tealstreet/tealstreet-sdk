const assert = require('node:assert/strict');
const test = require('node:test');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const { transform } = require('../generate.cjs');

test('multiline documented nested properties retain readonly, types and comments', () => {
  const output = transform(`export type Market = {
    precision: {
      /** Note: amount step. */
      amount: number;
      price: number;
    };
    extra?: {
      /** Optional nested value. */
      count?: number;
    };
  };`, {}, 'Market.ts').source;
  assert(output.includes('Note: amount step.'));
  assert(output.includes('Optional nested value.'));
  const parsed = ts.createSourceFile('Market.ts', output, ts.ScriptTarget.Latest, true);
  let count = 0;
  const visit = node => {
    if (ts.isPropertySignature(node)) {
      assert(node.modifiers.some(m => m.kind === ts.SyntaxKind.ReadonlyKeyword), node.name.text);
      count++;
    }
    ts.forEachChild(node, visit);
  };
  visit(parsed);
  assert.equal(count, 5);
  assert.equal(transform(output, {}, 'Market.ts').source, output);
});

test('documented string unions become runtime enums with exact escaped values', () => {
  const members = [{ name: 'Buy', value: 'b"uy' }, { name: 'Sell', value: 'sell' }];
  const output = transform(`/** Side documentation. */
export type Side =
  | "b\\\"uy"
  | "sell";`, { Side: members }, 'Side.ts').source;
  assert(output.includes('Side documentation.'));
  const parsed = ts.createSourceFile('Side.ts', output, ts.ScriptTarget.Latest, true);
  assert(ts.isEnumDeclaration(parsed.statements[0]));
  assert.deepEqual(parsed.statements[0].members.map(m => [m.name.text, m.initializer.text]),
    members.map(m => [m.name, m.value]));
});

test('new exported enums without metadata, literal drift and duplicate values fail', () => {
  assert.throws(() => transform('export type Tenth = "new";', {}, 'Tenth.ts'), /missing Rust enum metadata/);
  assert.throws(() => transform('export type Side = "buy";', { Side: [{ name: 'Buy', value: 'sell' }] }, 'Side.ts'), /raw literal drift/);
  assert.throws(() => transform('export type Side = "buy" | "buy";', {
    Side: [{ name: 'Buy', value: 'buy' }, { name: 'Sell', value: 'buy' }],
  }, 'Side.ts'), /duplicate literal value/);
});

test('Rust mutability markers stay structural inside documented nested objects', () => {
  const output = transform(`export type Options = {
    auth?: {
      /** @ts-mutable */
      address?: string;
      builder?: { fee: number };
    };
  };`, {}, 'Options.ts').source;
  const fields = ts.createSourceFile('Options.ts', output, ts.ScriptTarget.Latest, true).statements[0].type.members;
  const readonly = node => Boolean(node.modifiers?.some(m => m.kind === ts.SyntaxKind.ReadonlyKeyword));
  assert(readonly(fields[0]));
  assert(!readonly(fields[0].type.members[0]));
  assert(readonly(fields[0].type.members[1]));
  assert(readonly(fields[0].type.members[1].type.members[0]));
  assert.equal(transform(output, {}, 'Options.ts').source, output);
});

test('literal aliases require Rust intent and never become nominal runtime enums', () => {
  const source = 'export type Timeframe = "1m" | "1h";';
  const members = [{ name: 'T1m', value: '1m' }, { name: 'T1h', value: '1h' }];
  const output = transform(source, {}, 'Timeframe.ts', { Timeframe: members }).source;
  assert(ts.isTypeAliasDeclaration(ts.createSourceFile('Timeframe.ts', output, 99, true).statements[0]));
  assert.throws(() => transform(source, {}, 'Timeframe.ts', { Timeframe: members.slice(0, 1) }), /raw alias literal drift/);
});
