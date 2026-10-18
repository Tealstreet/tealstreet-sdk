import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const sdk = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docPath = process.argv[2];
assert.ok(docPath, 'pass the fresh actual Polymarket rustdoc JSON');
const doc = JSON.parse(fs.readFileSync(docPath, 'utf8'));
assert.equal(doc.index[doc.root].name, 'safe_cex_venue_polymarket');
const receiverTraits = loadPolicy('receiver-traits');
const keys = Object.keys(receiverTraits).filter(key => key.startsWith('safe_cex_venue_polymarket::'));
assert.equal(keys.length, 28, 'all concrete receiver methods must remain registered');
const options = {
  defaultTraits: loadPolicy('default-traits'),
  receiverTraits,
  readSource: filename => fs.readFileSync(path.resolve(sdk, filename), 'utf8'),
};
const without = Object.fromEntries(Object.entries(receiverTraits).filter(([key]) => !keys.includes(key)));
assert.throws(() => inventory(doc, { ...options, receiverTraits: without }), error => {
  for (const key of keys) assert.ok(error.message.includes(`${key}: accepted-set assertion: decoder trait on unsupported receiver`), key);
  return true;
});
console.log('Missing-policy control: all 28 concrete receivers refused by the unchanged guard');
const result = inventory(doc, options);
console.log(JSON.stringify({ receivers: keys.length, deserializers: result.types.length, declarations: result.declarations.length }));
