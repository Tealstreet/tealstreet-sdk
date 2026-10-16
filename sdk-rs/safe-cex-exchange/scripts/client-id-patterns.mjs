import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const crate = fileURLToPath(new URL('../', import.meta.url));
const inventory = [];
function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) { visit(file); continue; }
    if (!entry.name.endsWith('.ts')) continue;
    const text = fs.readFileSync(file, 'utf8');
    for (const match of text.matchAll(/patternClientOrderIdRule\(\s*(['"])(.*?)\1\s*,\s*\//g)) {
      const begin = match.index + match[0].length - 1;
      let end = begin + 1;
      let characterClass = false;
      for (; end < text.length; end++) {
        if (text[end] === '\\') { end++; continue; }
        if (text[end] === '[') characterClass = true;
        else if (text[end] === ']') characterClass = false;
        else if (text[end] === '/' && !characterClass) break;
      }
      assert.ok(end < text.length, `unclosed pattern in ${file}`);
      end++;
      while (/[a-z]/i.test(text[end] ?? '') && end < text.length) end++;
      inventory.push({ file: path.relative(root, file), label: match[2], pattern: text.slice(begin, end) });
    }
  }
}
visit(path.join(root, 'packages/safe-cex/src/exchanges'));
const output = path.join(crate, 'tests/client-id-pattern-inventory.json');
const generated = `${JSON.stringify(inventory, null, 2)}\n`;
if (process.argv.includes('--generate')) fs.writeFileSync(output, generated);
else assert.equal(fs.readFileSync(output, 'utf8'), generated, 'client-ID source inventory is stale');
const patterns = [...new Set(inventory.map(row => row.pattern))];
const samples = new Set(['', 'caller', '-caller', '_caller', 'a:b/c.d', '\n', 'a\n', 'a\r\n', '\u0085', '\uFEFF', 'a\u2028', 'a\u2029']);
for (const pattern of patterns) {
  const bound = Number(pattern.match(/\{1,(\d+)\}/)?.[1] ?? 36);
  for (const count of [1, Math.floor(bound / 2), Math.floor(bound / 2) + 1, bound, bound + 1]) {
    for (const character of ['a', '-', '😀']) samples.add(character.repeat(count));
  }
}
for (const code of [0x9, 0xA, 0xB, 0xC, 0xD, 0x20, 0x85, 0xA0, 0x1680, 0x2000, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000, 0xFEFF]) samples.add(`a${String.fromCodePoint(code)}b`);
const rows = patterns.map(pattern => ({ pattern, cases: [...samples].map(value => ({ value, expected: vm.runInNewContext(pattern).test(value) })) }));
const orderly = vm.runInNewContext(inventory.find(row => row.label === 'Orderly').pattern);
assert.equal(orderly.test('-caller'), false, 'negative lookahead must reject leading hyphen');
assert.equal(orderly.test('😀'.repeat(18)), true, '36 UTF16 units accepted');
assert.equal(orderly.test('😀'.repeat(19)), false, '38 UTF16 units rejected');
const nonWhitespace = vm.runInNewContext(inventory.find(row => row.label === 'Coinbase').pattern);
assert.equal(nonWhitespace.test('\uFEFF'), false, 'JS WhiteSpace includes FEFF');
assert.equal(nonWhitespace.test('\u0085'), true, 'JS WhiteSpace excludes 0085');
if (process.argv.includes('--oracle')) console.log(JSON.stringify(rows));
else console.log(`Client IDs: ${inventory.length} venue sites, ${patterns.length} patterns; live JS lookahead, UTF16, and whitespace controls passed.`);
