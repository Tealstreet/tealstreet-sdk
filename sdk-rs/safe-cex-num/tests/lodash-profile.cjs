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
module.exports = { lodash: require(entry), entry };
