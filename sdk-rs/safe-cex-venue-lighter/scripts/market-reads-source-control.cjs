'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = 'const total = subtract(totalEquity, upnl);';
  assert.equal(source.split(original).length, 2, 'unique source balance mutation site');
  return source.replace(original, 'const total = subtract(totalEquity, -upnl);');
};
require('./market-reads-oracle.cjs');
