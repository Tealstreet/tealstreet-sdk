'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const result = spawnSync(process.execPath, [path.resolve(__dirname, '../../safe-cex-venue-grvt/tests/public-map-source.cjs')], {encoding:'utf8'});
assert.equal(result.status, 0, `GRVT MAP source child success: ${result.stderr}`);
assert.equal(result.stdout.trim(), 'GRVT MAP completed ten actual-source cases', 'GRVT MAP source independent completion marker');
process.stdout.write(result.stdout);
