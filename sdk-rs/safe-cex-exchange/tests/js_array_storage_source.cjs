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
const sparseArray = [];
Object.defineProperty(sparseArray, '4294967294', {value: 7, writable: true, enumerable: true, configurable: true});
sparseArray[4294967295] = 'named';
const highLength = sparseArray.length;
sparseArray.length = 0;
const sparse = {highLength, length: sparseArray.length, names: Object.getOwnPropertyNames(sparseArray), named: sparseArray[4294967295]};
const readonlyArray = [];
Object.defineProperty(readonlyArray, 'length', {writable: false});
let pushRefused = false, conversions = 0;
try { readonlyArray.push(); } catch (e) { if (!(e instanceof TypeError)) throw e; pushRefused = true; }
Object.defineProperty(readonlyArray, 'length', {value: {valueOf() { conversions++; return 0; }}});
const readonly = {pushRefused, conversions, length: readonlyArray.length};
const setterArray = [], setterProto = [];
const setterCalls = [];
Object.defineProperty(setterProto, '1', {set(v) { setterCalls.push([this === setterArray, v]); }, configurable: true});
Object.setPrototypeOf(setterArray, setterProto);
setterArray[1] = 'direct';
const beforePush = setterArray.length;
setterArray.push('zero', 'one');
const inheritedSetter = {beforePush, length: setterArray.length, keys: Object.keys(setterArray), calls: setterCalls};
const hiddenArray = [], hiddenProto = [3, 4];
let hiddenReads = 0;
Object.defineProperty(hiddenArray, '0', {get() { hiddenReads++; return 99; }, enumerable: false});
Object.setPrototypeOf(hiddenArray, hiddenProto);
const enumerated = [];
for (const key in hiddenArray) enumerated.push(key);
const hiddenForIn = {keys: enumerated, names: Object.getOwnPropertyNames(hiddenArray), reads: hiddenReads};
const liveArray = [1, 2, 3];
const mapped = liveArray.map((v, i) => { if (i === 0) { liveArray[1] = 8; delete liveArray[2]; } return v * 2; });
const slicedArray = [0, 1, 2];
const sliced = slicedArray.slice({valueOf() { slicedArray[1] = 9; return 0; }}, 2);
const sortedArray = [2, 1];
Object.defineProperty(sortedArray, '0', {get() { sortedArray[1] = 5; return 3; }, set(v) { this.finalFirst = v; }, configurable: true, enumerable: true});
sortedArray.sort((a,b) => a-b);
const jsonArray = [0, 1];
Object.defineProperty(jsonArray, '0', {get() { delete jsonArray[1]; return -0; }});
const liveReads = {mapped: JSON.stringify(mapped), mappedKeys: Object.keys(mapped), sliced, sorted: [sortedArray.finalFirst, sortedArray[1]], json: JSON.stringify(jsonArray)};
const prototypeAliases = {samePrototype: Object.getPrototypeOf([]) === Array.prototype, sameMap: [].map === [].map, mapBorrow: Array.prototype.map.call({0: 4, length: 1}, v => v+1),
  toStringBorrow: [{join: 0}, function () {}, 7, false, 'x', 1n].map(v => Array.prototype.toString.call(v)),
  toStringArray: Array.prototype.toString.call(Object.assign([], {join: 0})),
  toStringNullish: [null, undefined].map(v => {try {Array.prototype.toString.call(v)} catch(e) {return [e.name, e.message]}})};

const coercionArray = [2], coercionEvents = [];
coercionArray.toString = function () { if (this !== coercionArray) throw Error('receiver'); coercionEvents.push('string'); return '\ud800!'; };
coercionArray.valueOf = function () { if (this !== coercionArray) throw Error('receiver'); coercionEvents.push('number'); return -0; };
const coercionUnits = Array.from(String(coercionArray), c => c.charCodeAt(0));
const coercionNumber = Number(coercionArray), coercionBuffer = Buffer.alloc(8);
coercionBuffer.writeDoubleBE(coercionNumber);
const coercionPlus = Array.from(coercionArray + '!', c => c.charCodeAt(0));
const coercionToken = {};
delete coercionArray.toString;
Object.setPrototypeOf(coercionArray, {toString() { throw coercionToken; }});
let sameCoercionError = false;
try { String(coercionArray); } catch (e) { sameCoercionError = e === coercionToken; }
const coercionHooks = {units:coercionUnits,bits:coercionBuffer.toString('hex'),plus:coercionPlus,events:coercionEvents,sameError:sameCoercionError};

process.stdout.write(JSON.stringify({named, accessor, length, inherited, symbol, baseline, frozenBuiltins, lengthCoercion, sparse, readonly, inheritedSetter, hiddenForIn, liveReads, prototypeAliases, coercionHooks}));
