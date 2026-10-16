'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = '...(Array.isArray(payload?.sub_accounts) ? payload.sub_accounts : []),';
  assert.equal(source.split(original).length, 2, 'unique L1 spread mutation site');
  return source.replace(original, '...(Array.isArray(payload?.accounts) ? payload.accounts : []),');
};
require('./account-lookup-oracle.cjs');
