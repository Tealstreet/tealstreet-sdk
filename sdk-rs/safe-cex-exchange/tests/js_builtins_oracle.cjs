// Installed dayjs + utc plugin and engine Array/Object/JSON builtins that venue
// bodies call through the shared runtime. Run with TZ=UTC, the runtime's zone.
'use strict';
const dayjs = require('dayjs');
dayjs.extend(require('dayjs/plugin/utc'));

const cells = array => Array.from({ length: array.length }, (_, i) => (i in array ? [true, array[i]] : [false]));
const arrays = () => [[], [1], [1, 2, 3], [1, , 3], [1, 2, ,], [, 1], [1, , 3, 4], ['a', 'b']];
const array = [];
for (const kind of ['pop', 'reverse']) {
  arrays().forEach((input, index) => {
    const before = cells(input);
    const result = input[kind]();
    array.push({
      kind,
      input: before,
      result: kind === 'pop' ? (result === undefined ? ['undefined'] : ['value', result]) : ['self', result === input],
      after: cells(input),
      index,
    });
  });
}

const number = value => (Number.isNaN(value) ? 'NaN' : Object.is(value, -0) ? '-0' : value);
const observe = d => ({ valueOf: number(d.valueOf()), utcOffset: number(d.utcOffset()), unix: number(d.unix()) });
const starts = [1710083045678, '2024-03-10T15:04:05.678Z', '2024-03-10 08:00:00', 0, -1, 1709251199999, NaN];
const units = ['year', 'y', 'month', 'M', 'week', 'w', 'date', 'D', 'day', 'd', 'days', 'DAY', 'hour', 'h', 'minute', 'm', 'second', 's', 'millisecond', 'ms', 'quarter', 'isoWeek', 'bogus', undefined];
const offsets = [[480, true], [480, false], [480], [0, true], [0, false], [-480, false], [8, false], [-16, true], [15.5, false], [60, false], [930, true], [-0, false]];
const chains = [];
for (const start of starts) {
  for (const offset of offsets) chains.push([start, [['utcOffset', ...offset]]]);
  for (const unit of units) chains.push([start, [['startOf', unit]]]);
  for (const unit of ['day', 'date', 'hour', 'month', 'week']) {
    chains.push([start, [['utcOffset', 480, false], ['startOf', unit]]]);
    chains.push([start, [['utcOffset', 480, true], ['startOf', unit]]]);
    chains.push([start, [['utcOffset', -330, false], ['startOf', unit]]]);
  }
  chains.push([start, [['utcOffset', 480], ['utcOffset', 60]]]);
  chains.push([start, [['utcOffset', 480], ['utcOffset', 60, true]]]);
  chains.push([start, [['utcOffset', 480, true], ['utcOffset', 60]]]);
  chains.push([start, [['utcOffset', 480], ['utcOffset', 0]]]);
  chains.push([start, [['utcOffset', 480], ['utcOffset', 0, true]]]);
  chains.push([start, [['utcOffset', 0], ['utcOffset', 480]]]);
  chains.push([start, [['utcOffset', 480], ['clone'], ['startOf', 'day']]]);
  chains.push([start, [['utcOffset', 480, true], ['add', 1, 'day'], ['startOf', 'month']]]);
  chains.push([start, [['utcOffset', 0], ['startOf', 'date']]]);
}
const dates = chains.map(([start, steps]) => {
  let d = dayjs(start);
  const trace = [observe(d)];
  for (const [op, ...args] of steps) {
    d = op === 'clone' ? dayjs(d) : d[op](...args);
    trace.push(observe(d));
  }
  return { start: typeof start === 'number' ? number(start) : start, steps, trace };
});

const plain = () => {
  const value = { b: 1, 2: 'two', a: 2, 1: 'one' };
  Object.defineProperty(value, 'hidden', { value: 3, enumerable: false });
  return value;
};
const names = [
  ['object', Object.getOwnPropertyNames(plain())],
  ['array', Object.getOwnPropertyNames([1, , 3])],
  ['string', Object.getOwnPropertyNames('ab')],
  ['number', Object.getOwnPropertyNames(5)],
  ['bool', Object.getOwnPropertyNames(true)],
];
let nullish;
try { Object.getOwnPropertyNames(null); } catch (error) { nullish = error.constructor.name; }

const tree = () => ({ a: 1, b: { a: 2, c: 3 }, c: [{ a: 4, b: 5 }, 6], 1: 'one' });
const stringify = [
  ['keys', JSON.stringify(tree(), ['a', 'c'])],
  ['numeric-and-duplicates', JSON.stringify(tree(), [1, '1', {}, null, 'a', 'a', true])],
  ['empty-list', JSON.stringify(tree(), [])],
  ['hidden-listed', JSON.stringify(plain(), Object.getOwnPropertyNames(plain()))],
  ['holey-list', JSON.stringify(tree(), ['c', , 'a'])],
  ['to-json', JSON.stringify({ a: { toJSON: () => ({ a: 7, z: 8 }) }, z: 9 }, ['a'])],
  ['missing-key', JSON.stringify({ a: 1 }, ['a', 'missing'])],
];

const encode = value => typeof value === 'number'
  ? (Number.isNaN(value) ? 'NaN' : value === Infinity ? 'Infinity' : value === -Infinity ? '-Infinity' : Object.is(value, -0) ? '-0' : value)
  : value;
const inputs = ['+08:00', '-0530', '+08', '-00:00', 'prefix+09:30suffix', 'nonsense', '', null, false, true, [], [8], [20], {}, {type: 'number', value: 'NaN'}, {type: 'number', value: 'Infinity'}];
const decode = input => input?.type === 'number' ? Number(input.value) : input;
const coercions = inputs.flatMap(input => [false, true].map(keep => {
  const argument = decode(input), before = dayjs(1710083045678), after = before.utcOffset(argument, keep);
  return {input, keep, valueOf: encode(after.valueOf()), offset: encode(after.utcOffset()), unix: encode(after.unix()), sameInstance: before === after, sameInputOffset: after.utcOffset() === argument};
}));
const ends = [];
for (const start of [1710083045678, -1, 'NaN']) for (const offset of [null, 480]) {
  for (const unit of units) for (const flag of [false, true, null, []]) {
    let d = dayjs(start === 'NaN' ? NaN : start);
    if (offset !== null) d = d.utcOffset(offset);
    d = d.startOf(unit, flag);
    ends.push({start, offset, unit, flag, valueOf: encode(d.valueOf()), utcOffset: encode(d.utcOffset()), unix: encode(d.unix())});
  }
}
process.stdout.write(JSON.stringify({ array, dates, names, nullish, stringify, coercions, ends }));
