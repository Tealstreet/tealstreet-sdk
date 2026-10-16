'use strict';
const fs = require('node:fs'), assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = 'leverageBySymbol[m.symbol] || m.limits.leverage.max || 10,';
  assert.equal(source.split(original).length, 2, 'unique actual placeholder leverage site');
  return source.replace(original, original.replace('10,', '11,'));
};
require('./positions-oracle.cjs');
