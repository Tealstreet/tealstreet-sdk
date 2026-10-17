// Independently authored engine expectations for the five reviewed iterator
// owners. These execute JS operations; no Rust result is an expected value.
const output = { cache: [], object: [], next: [], collection: [], js: {} };
for (const [kind, method] of [['Map', 'values'], ['Set', 'entries']]) {
  const owner = kind === 'Map' ? new Map([['k', 13.5]]) : new Set([17.5]);
  const member = owner[method];
  output.cache.push({ kind, method, same: member === owner[method], value: member.call(owner).next().value });
}
for (const [kind, value] of [['Map', 23.5], ['Set', 29.5]]) {
  const owner = kind === 'Map' ? new Map([['k', value]]) : new Set([value]);
  const iterator = owner.values();
  output.object.push({ kind, value, tag: Object.prototype.toString.call(iterator), self: iterator[Symbol.iterator]() === iterator,
    ownNext: Object.hasOwn(iterator, 'next'), first: iterator.next() });
}
for (const [kind, value] of [['Map', 31.5], ['Set', 37.5]]) {
  const make = n => kind === 'Map' ? new Map([['k', n]]).values() : new Set([n]).values();
  const first = make(0), other = make(value), next = first.next;
  let error;
  try { next.call({}); } catch (failure) { error = failure.name; }
  output.next.push({ kind, value, step: next.call(other), error });
}
for (const values of [[3.5, 7.25], [11.5, 19.25, 23.5]]) {
  // Dense native access represents the ordered values retained by the JS Map.
  const map = new Map(values.map((value, index) => [index, value]));
  const view = [...map.values()];
  output.collection.push({ input: values, output: { len: view.length, values: view } });
}
output.js.get_property = ['plain', 'symbol'].map(key => {
  const owner = { plain: 'text', 'Symbol.iterator': 'string-key', [Symbol.iterator]() { return this; } };
  const value = owner[key === 'symbol' ? Symbol.iterator : key];
  return { input: key, output: typeof value === 'function' ? { kind: 'function', self: value.call(owner) === owner }
    : { kind: 'string', value } };
});
output.js.iterator = ['array', 'string'].map(kind => {
  const owner = kind === 'array' ? ['a', 'b'] : 'A😀';
  const iterator = owner[Symbol.iterator]();
  return { input: kind, output: { self: iterator[Symbol.iterator]() === iterator,
    values: [iterator.next().value, iterator.next().value], done: iterator.next().done } };
});
output.js.iterable_values = ['map', 'set'].map(kind => {
  const owner = kind === 'map' ? new Map([['a', 41.5], ['b', 43.5]]) : new Set([47.5, 53.5]);
  return { input: kind, output: [...owner] };
});
process.stdout.write(JSON.stringify(output));
