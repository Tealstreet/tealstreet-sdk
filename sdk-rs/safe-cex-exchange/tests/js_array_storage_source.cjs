'use strict';
// Independent language expectations; no replacement runtime or array algorithm.
const a = [, undefined, -0, NaN, Infinity, -Infinity];
const token = {};
a.tag = token;
Object.defineProperty(a, 'hidden', {value: token});
a['01'] = 1;
a[4294967295] = 2;
const named = {
  keys: Object.keys(a), names: Object.getOwnPropertyNames(a),
  ownHole: Object.hasOwn(a, '0'), ownUndefined: Object.hasOwn(a, '1'),
  length: a.length, json: JSON.stringify(a),
};
const calls = [];
const getter = function () { calls.push(this); return this.tag; };
const setter = function (v) { calls.push([this, v]); };
Object.defineProperty(a, 'x', {get: getter, set: setter, configurable: true});
const child = [];
child.tag = token;
Object.setPrototypeOf(child, a);
const read = child.x;
child.x = token;
Object.freeze(a);
a.x = token;
let refused = 0;
for (const f of [() => { a[1] = 3; }, () => { a.extra = 1; }, () => { a.length = 0; }, () => a.push(1)]) {
  try { f(); } catch (e) { if (!(e instanceof TypeError)) throw e; refused++; }
}
const accessor = {
  read: read === token, childReceiver: calls[0] === child,
  childSetter: calls[1][0] === child && calls[1][1] === token,
  frozenSetter: calls[2][0] === a && calls[2][1] === token,
  getterSame: Object.getOwnPropertyDescriptor(a, 'x').get === getter,
  frozen: Object.isFrozen(a), refused,
};
const b = [0, 1, 2, 3];
Object.defineProperty(b, '2', {configurable: false});
let shrinkRefused = false;
try { Object.defineProperty(b, 'length', {value: 1, writable: false}); }
catch (e) { if (!(e instanceof TypeError)) throw e; shrinkRefused = true; }
const length = {
  shrinkRefused, length: b.length, removed3: !Object.hasOwn(b, '3'),
  kept2: Object.hasOwn(b, '2'), writable: Object.getOwnPropertyDescriptor(b, 'length').writable,
};
const c = [, undefined];
const proto = [];
Object.defineProperty(proto, '0', {get() { return this.tag; }, configurable: true});
c.tag = token;
Object.setPrototypeOf(c, proto);
const inherited = {
  own: Object.hasOwn(c, '0'), receiver: c[0] === token,
  iterated: [...c][0] === token, mapped: c.map(v => v)[0] === token,
};
let receiver;
Object.defineProperty(c, Symbol.iterator, {get() {
  receiver = this;
  return function* () { yield token; yield -0; };
}, configurable: true});
const values = [...c];
const symbol = {receiver: receiver === c, token: values[0] === token, minusZero: Object.is(values[1], -0)};
const existing = [, undefined, NaN, -0, {}];
existing[0] = undefined;
const baseline = {json: JSON.stringify(existing)};
const frozen = Object.freeze([2, 1]);
let frozenRefused = 0;
for (const method of ['push', 'pop', 'reverse', 'sort']) {
  try { frozen[method](...(method === 'push' ? [null] : [])); }
  catch (e) { if (!(e instanceof TypeError)) throw e; frozenRefused++; }
}
Reflect.deleteProperty(frozen, '0');
const frozenBuiltins = {
  refused: frozenRefused, retainedIndex: Object.hasOwn(frozen, '0'),
  length: frozen.length, json: JSON.stringify(frozen),
};
const lengthCoercion = [];
for (const mode of ['stable', 'mismatch', 'throws']) {
  const array = [1, 2, 3], token = {}, events = [];
  const value = {valueOf() {
    events.push(array.length);
    if (events.length === 1) array[5] = 99;
    if (events.length === 2) {
      if (mode === 'mismatch') return 3;
      if (mode === 'throws') throw token;
    }
    return 2;
  }};
  let error = null;
  try { array.length = value; }
  catch (thrown) { error = thrown === token ? 'original' : thrown.name; }
  lengthCoercion.push({mode, events, error, length: array.length, own5: Object.hasOwn(array, '5')});
}
process.stdout.write(JSON.stringify({named, accessor, length, inherited, symbol, baseline, frozenBuiltins, lengthCoercion}));
