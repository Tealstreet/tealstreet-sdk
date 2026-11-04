const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const test = require('node:test');

for (const [name, script, args] of [
  ['generated metadata matches the complete live source', '../scripts/generate-exchange-metadata.cjs', ['--check']],
  ['the source oracle validates the current venue roster', './exchange_metadata_oracle.cjs', []],
]) {
  test(name, () => {
    const result = spawnSync(process.execPath, [path.resolve(__dirname, script), ...args], {
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.error?.message ?? result.stderr);
  });
}
