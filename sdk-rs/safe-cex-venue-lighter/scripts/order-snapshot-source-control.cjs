'use strict';
const fs = require('node:fs'), assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = 'orders = await fetchOrdersForMarket(255);';
  assert.equal(source.split(original).length, 2, 'unique actual all-markets sentinel site');
  return source.replace(original, 'orders = await fetchOrdersForMarket(254);');
};
require('./order-snapshot-oracle.cjs');
