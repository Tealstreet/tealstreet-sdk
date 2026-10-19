'use strict';
const units = value => value.split('').map(character => character.charCodeAt(0));
let reads = 0;
const input = tag => ({
  undefined, null: null, number: 7, boolean: true, text: 'x',
  surrogate: String.fromCharCode(0xd800), object: {}, array: [], bigint: 3n,
  symbol: Symbol.iterator,
  poison: Object.defineProperty({}, 'toString', {get() { reads++; throw new Error('unreached'); }}),
})[tag];
const error = operation => {
  reads = 0;
  try { operation(); return {ok: true, reads}; }
  catch (failure) { return {ok: false, name: failure.name, units: units(failure.message), reads}; }
};
const tags = ['undefined', 'null', 'number', 'boolean', 'text', 'surrogate', 'object', 'poison', 'array', 'bigint', 'symbol'];
const callbacks = tags.flatMap(tag => ['array', 'map', 'set'].map(kind => ({
  tag, kind, ...error(() => {
    const value = input(tag);
    if (kind === 'array') [1].map(value);
    else if (kind === 'map') new Map([[1, 2]]).forEach(value);
    else new Set([1]).forEach(value);
  }),
})));
const from = tags.filter(tag => tag !== 'undefined').map(tag => ({
  tag, ...error(() => Array.from([1], input(tag))),
}));
const entries = ['text', 'surrogate', 'number', 'null', 'symbol'].map(tag => ({
  tag, ...error(() => new Map([input(tag)])),
}));
process.stdout.write(JSON.stringify({callbacks, from, entries}));
