// Later observability supplement; the original account oracle is unchanged.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { root, load } = require('../scripts/accounts-source.cjs');
const source = 'packages/safe-cex/src/accounts/parseImportAccountData.ts';
const { parseImportAccountData } = load(path.join(root, source));
const account = { exchange: 'bybit', name: 'x', apiKey: 'k', apiSecret: 's' };
const high = String.fromCharCode(0xd800);
const low = String.fromCharCode(0xdc00);
let deep = 0;
for (let index = 0; index < 130; index++) deep = [deep];
const cases = [
  ['unknown-high', { ...account, unknown: high }, 'unknown', false],
  ['unknown-low', { ...account, unknown: low }, 'unknown', false],
  ['name-high', { ...account, name: high }, 'name', false],
  ['array-high', [{ ...account, unknown: high }, { ...account, unknown: 'safe' }], 'unknown', false],
  ['unknown-key-high', { ...account, unknown: { [high]: 11 } }, 'unknown', true],
  ['unknown-pair', { ...account, unknown: '\ud83d\udca9' }, 'unknown', false],
  ['unknown-ascii-escape', { ...account, unknown: String.raw`\ud800` }, 'unknown', false],
  ['unknown-scalar', { ...account, unknown: 'abc' }, 'unknown', false],
  ['unknown-replacement', { ...account, unknown: '\ufffd' }, 'unknown', false],
  ['deep-unknown', { ...account, unknown: deep }, 'unknown', false],
];
const rows = cases.map(([id, input, field, key]) => {
  const raw = JSON.stringify(input);
  const result = parseImportAccountData(raw);
  if (!result.isValid) throw Error(`SOURCE-OBSERVABILITY accepted import ${id}`);
  const accounts = Array.isArray(result.data) ? result.data : [result.data];
  const views = accounts.map(returned => {
    if (!Object.hasOwn(returned, field)) throw Error(`SOURCE-OBSERVABILITY returned field ${id}`);
    const value = key ? Object.keys(returned[field])[0] : returned[field];
    let depth = 0;
    let leaf = value;
    while (Array.isArray(leaf)) { depth++; leaf = leaf[0]; }
    return {
      type: typeof value,
      length: typeof value === 'string' || Array.isArray(value) ? value.length : null,
      units: typeof value === 'string'
        ? Array.from({ length: value.length }, (_, index) => value.charCodeAt(index)) : null,
      depth,
      leaf: depth ? leaf : null,
    };
  });
  return { id, raw, valid: result.isValid, array: Array.isArray(result.data), count: accounts.length, views };
});
process.stdout.write(JSON.stringify({
  header: { architecture: `${process.platform}-${process.arch}`, node: process.version,
    source, sourceSha256: crypto.createHash('sha256').update(fs.readFileSync(path.join(root, source))).digest('hex') },
  rows,
}));
