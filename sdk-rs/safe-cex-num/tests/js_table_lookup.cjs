const assert = require('node:assert/strict');
const names = Object.getOwnPropertyNames(Object.prototype);
assert.equal(names.length, 12, 'standard Object.prototype profile');
const keys = new Set(['BUY', 'SELL', 'unknown', '', '\ufeff', '😀', 'null', 'undefined', 'zero', 'false', 'empty', 'nullValue', 'object', 'array', 'undefinedValue']);
for (const name of names) {
  for (const key of [name, name.toUpperCase(), name.toLowerCase(), ' ' + name, name + '\ufeff', name + '\0']) keys.add(key);
}
let state = 0xe97a20;
for (let n = 0; n < 4000; n++) {
  state ^= state << 13; state ^= state >>> 17; state ^= state << 5;
  keys.add((state >>> 0).toString(36));
}
const rows = [];
for (const entries of [[], [['BUY','buy'],['SELL','sell']], names.map(name => [name, 'shadow:' + name]), [['BUY','first'],['BUY','last']], [['BUY',undefined],['BUY',0]], [['BUY',1],['BUY',null]], [['BUY',null],['BUY',undefined]], [['BUY',undefined],['BUY','buy']], [['zero',0],['false',false],['empty',''],['nullValue',null],['object',{}],['array',[]],['undefinedValue',undefined]], names.map((name,i) => [name,[0,false,'',null,undefined][i % 5]])]) {
  const table = Object.fromEntries(entries);
  for (const key of keys) {
    const value = table[key];
    const own = Object.hasOwn(table, key);
    rows.push({ entries, undefinedIndices: entries.flatMap(([,v],i) => v === undefined ? [i] : []), key, kind: own ? 'mapped' : value === undefined ? 'missing' : 'inherited',
      mapped: own ? value : null, type: typeof value, truthy: Boolean(value),
      orFallback: (value || 'fallback') === 'fallback', presence: key in table,
      nullishFallback: (value ?? 'fallback') === 'fallback', negated: !value,
      callGuardReached: !!value, callable: typeof value === 'function' });
  }
}
const inherited = names.map(key => ({ key, truthy: Boolean(({})[key]), string: String(({})[key]), length: ({})[key].length ?? null, accessor: !!Object.getOwnPropertyDescriptor(Object.prototype,key).get, type: typeof ({})[key] }));
console.error(JSON.stringify({ node: process.version, rows: rows.length, inherited }));
const surrogates = ['"\\ud800"','"a\\udfff"'].map(json => {
  const key = JSON.parse(json);
  return { json, emptyTruthy: Boolean(({})[key]), ownTruthy: Boolean(Object.fromEntries([[key,'mapped']])[key]) };
});
const numeric = ['0','-0','1e400','-1e400','5e-400'].map(json => {
  const table = { value: JSON.parse(json) };
  return { json, truthy: Boolean(table.value), nullish: table.value == null };
});
process.stdout.write(JSON.stringify({ rows, inherited, surrogates, numeric }));
