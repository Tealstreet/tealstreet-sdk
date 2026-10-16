import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const sourcePath = new URL('../src/types.rs', import.meta.url);
const examplePath = new URL('../examples/shapes.rs', import.meta.url);
const source = readFileSync(sourcePath, 'utf8');
const example = readFileSync(examplePath, 'utf8');
const fields = [
  ['contract_trade', 'ContractTrade'], ['spot', 'Spot'], ['wallet', 'Wallet'],
  ['options', 'Options'], ['derivatives', 'Derivatives'], ['copy_trading', 'CopyTrading'],
  ['block_trade', 'BlockTrade'], ['exchange', 'Exchange'], ['n_f_t', 'NFT'], ['ips', 'ips'],
];
const requested = process.argv.slice(2);
assert.ok(requested.every(name => fields.some(([field]) => field === name)), 'unknown collection field');
const selected = fields.filter(([field]) => !requested.length || requested.includes(field));
const helper = `
fn consume_collection<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    struct Consume;
    impl<'de> serde::de::Visitor<'de> for Consume {
        type Value = Vec<String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            while sequence.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok(Vec::new())
        }
    }
    d.deserialize_seq(Consume)
}
`;
function run() {
  return spawnSync(process.execPath, ['sdk-rs/safe-cex-venue-bybit/scripts/check-shapes.mjs'], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1' },
  });
}
function replaceOnce(text, needle, replacement) {
  assert.equal(text.split(needle).length, 2, `unique mutation anchor: ${needle}`);
  return text.replace(needle, replacement);
}
try {
  for (const [field, wire] of selected) {
    const owner = field === 'ips' ? 'BybitApiKeyInfo' : 'BybitApiKeyPermissions';
    const anchor = `struct ${owner}Wire {`;
    const start = source.indexOf(anchor);
    assert.ok(start >= 0, anchor);
    const end = source.indexOf('\n}', start) + 2;
    const member = `    ${field}: Vec<String>,`;
    const record = replaceOnce(source.slice(start, end), member,
      `    #[serde(deserialize_with = "consume_collection")]\n${member}`);
    writeFileSync(sourcePath, source.slice(0, start) + record + source.slice(end) + helper);
    const guarded = run();
    const marker = `native field identity ${owner}.${field}`;
    assert.equal(guarded.status, 1, guarded.error?.message ?? guarded.stderr);
    assert.ok(guarded.stderr.includes(marker), guarded.stderr);
    assert.match(guarded.stderr, /actual: 101/, 'Rust assertion must fail after compilation');
    console.log(`CONSUMING-COLLECTION ${field}: compiled, native guard failed: ${marker}`);

    const guard = `        assert_eq!(\n            serde_json::to_value(&self.${field}).unwrap(),\n            wire["${wire}"],\n            "${marker}"\n        );`;
    writeFileSync(examplePath, replaceOnce(example, guard, ''));
    const unguarded = run();
    assert.equal(unguarded.status, 1, unguarded.error?.message ?? unguarded.stderr);
    assert.ok(!unguarded.stderr.includes('native field identity'), unguarded.stderr);
    assert.match(unguarded.stderr, /field identity BybitApiKey(?:Info|Permissions) (?:original|supplement) sample \d+/);
    assert.match(unguarded.stderr, new RegExp(`^\\+\\s+${wire}: \\[\\],?$`, 'm'),
      `outer comparison must show the emptied ${wire}: ${unguarded.stderr}`);
    console.log(`CONSUMING-COLLECTION ${field}: native guard removed; outer ${wire} comparison still failed`);
    writeFileSync(examplePath, example);
    writeFileSync(sourcePath, source);
  }
} finally {
  writeFileSync(sourcePath, source);
  writeFileSync(examplePath, example);
}
assert.equal(readFileSync(sourcePath, 'utf8'), source);
assert.equal(readFileSync(examplePath, 'utf8'), example);
const restored = run();
assert.equal(restored.status, 0, restored.error?.message ?? restored.stderr);
console.log(restored.stdout.trim());
console.log(`CONSUMING-COLLECTION: ${selected.length} independent fields, ${selected.length * 2} controls, restored`);
