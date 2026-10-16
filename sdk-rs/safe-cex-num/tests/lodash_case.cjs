const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH must name the isolated install');
const packageDir = path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../lodash');
const entry = path.join(packageDir, 'lodash.js');
require.resolve(entry); // An explicit missing install must fail without ancestor lookup.
assert.equal(JSON.parse(fs.readFileSync(path.join(packageDir, 'package.json'))).version, '4.18.1', 'lodash version pin');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'), 'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf', 'lodash source fingerprint');
let lodash = require(entry);
if (process.argv.includes('--control-uppercase')) {
  const source = fs.readFileSync(entry, 'utf8');
  const wrong = source.replaceAll('word.toLowerCase()', 'word.toUpperCase()');
  assert.notEqual(source, wrong, 'named lodash mutation must apply');
  const sandbox = { module: { exports: {} }, exports: {} };
  sandbox.exports = sandbox.module.exports;
  require('node:vm').runInNewContext(wrong, sandbox);
  lodash = sandbox.module.exports;
}
if (process.argv.includes('--surrogate-witnesses')) {
  const escaped = ['"\\ud800"', '"a\\ud800b"', '"\\udfff"', '"\\ud800\\ud800"'];
  const units = s => Array.from({ length: s.length }, (_, i) => s.charCodeAt(i));
  process.stdout.write(JSON.stringify(escaped.map(json => {
    const input = JSON.parse(json);
    return { json, input: units(input), snake: units(lodash.snakeCase(input)), camel: units(lodash.camelCase(input)) };
  })));
  process.exit(0);
}
let count = 0;
let buffer = '';
function flush() { fs.writeSync(1, buffer); buffer = ''; }
function emit(input) {
  buffer += JSON.stringify([input, lodash.snakeCase(input), lodash.camelCase(input)]) + '\n';
  if (buffer.length > 32768) flush();
  count++;
}
const seed = ['', 'foo', 'BAR', 'XMLHttpRequest', 'version42HTTP', '1st', '2ND', '11th', '21ST', '12th', 'a1B2', '--__', '\ufeff', '\u0085', '\u180e', '\u2028', '\0', 'déjà', 'Æneid', 'ŉ', 'ΟΣ', 'ΣΣ', 'א', 'नमस्ते', '中文', 'ǅ', 'ᾈ', 'İ', 'ß', '𐐀', '👩🏽‍💻', '🇺🇸🇯🇵', '😀‍a', '✈️', 'a\u0301', 'foo’s', "isn't", '😃‍Σ', '🏽🏾‍a'];
for (const a of seed) for (const b of seed) for (const sep of ['', '_', ' ', '-']) emit(a + sep + b);
for (let cp = 0; cp <= 0x10ffff; cp++) {
  if (cp >= 0xd800 && cp <= 0xdfff) continue;
  emit('foo ' + String.fromCodePoint(cp) + 'Σ');
}
let state = 0x6db331ea;
function random() { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0; }
const alphabet = [...'abcXYZ019_-', ...seed.join('')];
for (let n = 0; n < 30000; n++) {
  let text = '';
  const size = random() % 24;
  for (let j = 0; j < size; j++) text += alphabet[random() % alphabet.length];
  emit(text);
}
flush();
console.error(JSON.stringify({ count, node: process.version, unicode: process.versions.unicode, lodash: lodash.VERSION }));
