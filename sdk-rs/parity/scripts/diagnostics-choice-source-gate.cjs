const assert = require('node:assert/strict');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const output = execFileSync(process.execPath, [
  path.resolve(__dirname, '../../safe-cex-store/tests/diagnostics_choices_source.cjs'),
], { encoding: 'utf8' });
assert.equal(output.trim(), 'DIAGNOSTICS CHOICES: 20 maintained-source cases verified',
  'DIAGNOSTICS SOURCE completed twenty-case marker');
console.log(output.trim());
