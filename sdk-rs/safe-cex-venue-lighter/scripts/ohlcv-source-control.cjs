'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const original = 'timestamp: Number(candle.t ?? candle.timestamp ?? 0) / 1000,';
  assert.equal(source.split(original).length, 2, 'unique actual candle timestamp mutation site');
  return source.replace(original, 'timestamp: Number(candle.t ?? candle.timestamp ?? 0) / 1001,');
};
require('./ohlcv-oracle.cjs');
