'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = 'Math.abs((opts.price - markPrice) / markPrice) * 100';
  assert.equal(source.split(original).length, 2, 'unique actual accidental-price deviation site');
  return source.replace(original, 'Math.abs((opts.price - markPrice) / markPrice) * 101');
};
require('./order-actions-oracle.cjs');
