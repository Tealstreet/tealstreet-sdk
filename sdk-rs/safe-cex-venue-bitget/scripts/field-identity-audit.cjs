// Inventory candidate collisions, including metadata; equality alone is not a defect.
const fs = require('node:fs');
const path = require('node:path');
const directory = path.join(__dirname, '../tests');
const kind = value => value && typeof value === 'object' && '$number' in value
  ? 'tagged-number' : ['string', 'number', 'boolean'].includes(typeof value) ? typeof value : undefined;
for (const file of fs.readdirSync(directory).filter(name => name.endsWith('.json')).sort()) {
  const pairs = new Map();
  let records = 0;
  function walk(value, location) {
    if (Array.isArray(value)) return value.forEach(item => walk(item, `${location}/*`));
    if (!value || typeof value !== 'object') return;
    records++;
    const fields = Object.entries(value).filter(([, item]) => kind(item));
    for (let i = 0; i < fields.length; i++) for (let j = i + 1; j < fields.length; j++) {
      const [left, a] = fields[i], [right, b] = fields[j];
      if (kind(a) !== kind(b)) continue;
      const key = JSON.stringify([location, left, right, kind(a)]);
      const count = pairs.get(key) || {equal: 0, distinct: 0};
      count[JSON.stringify(a) === JSON.stringify(b) ? 'equal' : 'distinct']++;
      pairs.set(key, count);
    }
    for (const [key, item] of Object.entries(value)) walk(item, `${location}/${key}`);
  }
  walk(JSON.parse(fs.readFileSync(path.join(directory, file), 'utf8')), '');
  const equal = [...pairs].filter(([, count]) => count.equal);
  const blind = equal.filter(([, count]) => !count.distinct);
  console.log(JSON.stringify({file, records, equalPairPaths: equal.length, alwaysEqualPairPaths: blind.length}));
  if (process.argv.includes('--pairs')) for (const [key, count] of equal) console.log(JSON.stringify({file, path: JSON.parse(key), ...count}));
}
