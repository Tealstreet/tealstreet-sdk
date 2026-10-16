const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('../tests/configuration.cjs');

function initializer(file, className, name, receiver) {
  const filename = path.join(root, file);
  const source = ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
  const classes = source.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === className);
  assert.equal(classes.length, 1, 'WS-PRIMITIVES actual source class ' + className);
  const members = classes[0].members.filter(n => n.name?.getText(source) === name);
  assert.equal(members.length, 1, 'WS-PRIMITIVES actual source field ' + name);
  assert(ts.isArrowFunction(members[0].initializer), 'WS-PRIMITIVES complete arrow initializer');
  const text = ts.transpileModule(`return (${members[0].initializer.getText(source)});`, {
    compilerOptions: { target: ts.ScriptTarget.ES2022 },
  }).outputText;
  return new Function(text).call(receiver);
}

const privateFile = 'packages/safe-cex/src/exchanges/bitrue/bitrue.ws-private.ts';
const publicFile = 'packages/safe-cex/src/exchanges/bitrue/bitrue.ws-public.ts';
const success = initializer(privateFile, 'BitruePrivateWebsocket', 'isSuccessfulCode', undefined);
let coercions = 0;
const throwing = () => { coercions++; throw new Error('strict comparison must not coerce'); };
const next200 = Buffer.from('4069000000000001', 'hex').readDoubleBE();
const inputs = [
  ['n-zero', 0], ['n-neg-zero', -0], ['n-200', 200], ['n-one', 1], ['n-negative', -200],
  ['n-201', 201], ['n-nan', NaN], ['n-pos-inf', Infinity], ['n-neg-inf', -Infinity],
  ['n-max', Number.MAX_VALUE], ['n-min', Number.MIN_VALUE], ['n-subnormal', 1e-310], ['n-next-200', next200],
  ['s-zero', '0'], ['s-200', '200'], ['s-empty', ''], ['s-doublezero', '00'],
  ['s-spacezero', ' 0'], ['s-pluszero', '+0'], ['s-minuszero', '-0'], ['s-2e2', '2e2'],
  ['s-decimal', '200.0'], ['s-nullbyte', '\0'], ['s-surrogate-high', '\ud800'],
  ['s-surrogate-low', '\udfff'], ['s-pair', '\ud83d\ude00'], ['s-zero-surrogate', '0\ud800'],
  ['s-surrogate-200', '\ud800200'], ['s-200-surrogate', '200\udfff'], ['s-fullwidthzero', '０'],
  ['s-200-space', '200 '], ['s-zero-space', '0 '],
  ['o-undefined', undefined], ['o-null', null], ['o-false', false], ['o-true', true],
  ['o-bigint-zero', 0n], ['o-bigint-200', 200n], ['o-symbol', Symbol('200')],
  ['o-object', {}], ['o-array', [200]], ['o-function', () => 200],
  ['o-number-object', new Number(200)], ['o-string-object', new String('200')],
  ['o-coercion-trap', { [Symbol.toPrimitive]: throwing, valueOf: throwing, toString: throwing }],
  ['o-proxy-trap', new Proxy({}, { get: throwing })],
];
assert.equal(inputs.length, 46, 'WS-PRIMITIVES independent source inventory');
const codes = inputs.map(([id, code]) => {
  const before = coercions;
  const value = success(code);
  assert.equal(typeof value, 'boolean', 'WS-PRIMITIVES source native boolean');
  assert.equal(coercions, before, 'WS-PRIMITIVES no source coercion ' + id);
  const kind = typeof code === 'number' ? 'number' : typeof code === 'string' ? 'string' : 'other';
  let bits, units;
  if (kind === 'number') { const b = Buffer.alloc(8); b.writeDoubleBE(code); bits = b.toString('hex'); }
  if (kind === 'string') units = Array.from({ length: code.length }, (_, i) => code.charCodeAt(i));
  return { id, kind, bits, units, value, coercions: coercions - before };
});
const pings = [['private', privateFile, 'BitruePrivateWebsocket'], ['public', publicFile, 'BitruePublicWebsocket']].map(([id, file, className]) => {
  let touches = 0;
  const receiver = new Proxy({}, {
    get() { touches++; throw new Error('noop ping accessed receiver'); },
    set() { touches++; throw new Error('noop ping mutated receiver'); },
  });
  const value = initializer(file, className, 'ping', receiver)();
  assert.equal(value, undefined, 'WS-PRIMITIVES source native void ' + id);
  assert.equal(touches, 0, 'WS-PRIMITIVES source ping has no receiver effects');
  return { id, kind: 'undefined', touches };
});
assert.equal(pings.length, 2, 'WS-PRIMITIVES independent ping inventory');
process.stdout.write(JSON.stringify({ codes, pings }));
