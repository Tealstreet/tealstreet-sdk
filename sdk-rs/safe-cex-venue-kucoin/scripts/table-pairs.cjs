const path = require('node:path');
const { root, load } = require('./tables.cjs');
const result = { pairs: {} };
for (const name of ['kucoin', 'kucoinspot']) {
  result[name] = load(path.join(root, `packages/safe-cex/src/exchanges/kucoin/${name}.types.ts`));
  result.pairs[name] = Object.fromEntries(Object.entries(result[name])
    .filter(([, value]) => value !== null && typeof value === 'object' && !Array.isArray(value))
    .map(([key, value]) => [key, Object.entries(value)]));
}
process.stdout.write(JSON.stringify(result));
