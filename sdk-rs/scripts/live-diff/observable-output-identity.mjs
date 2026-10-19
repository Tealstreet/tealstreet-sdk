import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { deserialize } from 'node:v8';
import { isDeepStrictEqual } from 'node:util';

// Compare observable output values and the complete identity relation, without
// treating V8's internal Smi-versus-HeapNumber encoding as a JS type distinction.
export function assertOutputIdentity(left, right) {
  assert.deepStrictEqual(left, right, 'Complete output values must match');
  const forward = new WeakMap(), reverse = new WeakMap(), kinds = new Map();
  let objects = 0;
  function visit(a, b, path) {
    if (a === null || typeof a !== 'object') {
      assert(Object.is(a, b), `${path}: primitive value/f64 sign differs`);
      return;
    }
    assert(b !== null && typeof b === 'object', `${path}: object shape differs`);
    if (forward.has(a)) {
      assert.equal(forward.get(a), b, `${path}: original alias split`);
      assert.equal(reverse.get(b), a, `${path}: original alias merged`);
      return;
    }
    assert(!reverse.has(b), `${path}: distinct original referents merged`);
    forward.set(a, b); reverse.set(b, a); objects++;
    const kind = Object.prototype.toString.call(a);
    assert(['[object Object]', '[object Array]', '[object Map]', '[object Set]'].includes(kind),
      `${path}: unclassified output object kind ${kind}; extend the identity guard before accepting it`);
    kinds.set(kind, (kinds.get(kind) || 0) + 1);
    assert.equal(Object.prototype.toString.call(b), kind, `${path}: kind differs`);
    assert.equal(Object.getPrototypeOf(a), Object.getPrototypeOf(b), `${path}: prototype differs`);
    if (a instanceof Map) {
      assert.equal(a.size, b.size, `${path}: Map size differs`);
      const be = [...b.entries()]; let i = 0;
      for (const [key, value] of a) {
        visit(key, be[i][0], `${path}.MapKey${i}`);
        visit(value, be[i][1], `${path}.MapValue${i++}`);
      }
    } else if (a instanceof Set) {
      const be = [...b.values()]; assert.equal(a.size, b.size); let i = 0;
      for (const value of a) visit(value, be[i], `${path}.Set${i++}`);
    }
    const ak = Reflect.ownKeys(a), bk = Reflect.ownKeys(b);
    assert.deepStrictEqual(ak, bk, `${path}: own key order/presence differs`);
    for (const key of ak) {
      const ad = Object.getOwnPropertyDescriptor(a, key), bd = Object.getOwnPropertyDescriptor(b, key);
      for (const flag of ['writable', 'enumerable', 'configurable', 'get', 'set'])
        assert.equal(ad[flag], bd[flag], `${path}.${String(key)}: descriptor ${flag} differs`);
      visit(ad.value, bd.value, `${path}.${String(key)}`);
    }
  }
  visit(left, right, '$');
  return { completeDeepStrictEqual: true, orderedOwnKeysEqual: true, exactPrimitivesAndSignedZero: true,
    bidirectionalAliasGraphEqual: true, descriptorsAndPrototypesEqual: true, objects, kinds: Object.fromEntries(kinds) };
}

export function runIdentityGuardControls() {
  assert.throws(() => assertOutputIdentity({ n: -0 }, { n: 0 }), 'Real signed-zero difference must fail');
  const shared = { n: 1 };
  assert.throws(() => assertOutputIdentity({ a: shared, b: shared }, { a: { n: 1 }, b: { n: 1 } }), /alias split/);
  assert.throws(() => assertOutputIdentity({ a: { n: 1 }, b: { n: 1 } }, { a: shared, b: shared }), /referents merged/);
  assert.throws(() => assertOutputIdentity({ a: 1, b: 2 }, { b: 2, a: 1 }), /key order/);
  assert.throws(() => assertOutputIdentity({ a: undefined }, {}));
  assert.throws(() => assertOutputIdentity([, 1], [undefined, 1]));
  const left = { n: -0 }, right = { n: -0 }; left.self = left; right.self = right;
  assertOutputIdentity({ a: left, b: left }, { a: right, b: right });
  assertOutputIdentity({ n: 0 }, { n: 0 });
  return { passed: true, controls: ['signed-zero refusal', 'alias-split refusal', 'alias-merge refusal',
    'ordered-key refusal', 'own-undefined refusal', 'hole-versus-undefined refusal', 'isomorphic-cycle acceptance', 'equal-positive-zero acceptance'] };
}

if (process.argv[1] && import.meta.url === new URL(process.argv[1], 'file://').href) {
  if (process.argv[2] === '--controls') { console.log(JSON.stringify(runIdentityGuardControls(), null, 2)); process.exit(0); }
  const [a, b] = process.argv.slice(2);
  console.log(JSON.stringify(assertOutputIdentity(deserialize(readFileSync(a)), deserialize(readFileSync(b))), null, 2));
}
