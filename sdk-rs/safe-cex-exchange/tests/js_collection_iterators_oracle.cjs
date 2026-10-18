// Independent engine traces for the collection iterator consumer boundary.
const scenarios = [
  [['next'], ['next'], ['next']],
  [['delete', 'a'], ['next'], ['next']],
  [['next'], ['delete', 'a'], ['next'], ['next']],
  [['next'], ['delete', 'b'], ['add', 'c', 'C'], ['next'], ['next']],
  [['next'], ['add', 'b', 'updated'], ['next'], ['next']],
  [['next'], ['delete', 'a'], ['add', 'a', 'again'], ['spread']],
  [['next'], ['clear'], ['add', 'c', 'C'], ['spread']],
  [['next'], ['next'], ['next'], ['add', 'c', 'C'], ['next'], ['spread']],
  [['add', 'c', 'C'], ['spread']],
  [['next'], ['from']],
];
const cases = [];
for (const type of ['Map', 'Set']) {
  for (const mode of ['keys', 'values', 'entries', 'iterator']) {
    for (const [index, operations] of scenarios.entries()) {
      const collection = type === 'Map' ? new Map([['a', 'A'], ['b', 'B']]) : new Set(['a', 'b']);
      const iterator = mode === 'iterator' ? collection[Symbol.iterator]() : collection[mode]();
      const trace = [];
      for (const [op, key, value] of operations) {
        if (op === 'next') {
          const step = iterator.next('ignored');
          trace.push({ done: step.done, value: step.done ? '<undefined>' : step.value });
        } else if (op === 'delete') collection.delete(key);
        else if (op === 'add') type === 'Map' ? collection.set(key, value) : collection.add(key);
        else if (op === 'clear') collection.clear();
        else if (op === 'spread') trace.push([...iterator]);
        else if (op === 'from') trace.push(Array.from(iterator));
      }
      cases.push({ name: `${type}/${mode}/${index}`, type, mode, operations, trace });
    }
  }
}
process.stdout.write(JSON.stringify(cases));
