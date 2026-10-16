'use strict';
const fs = require('node:fs'), assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = 'now + LighterExchange.SIGNER_RETRY_COOLDOWN_MS';
  assert.equal(source.split(original).length, 2, 'unique actual signer retry deadline site');
  return source.replace(original, original + ' + 1');
};
require('./runtime-initialization-oracle.cjs');
