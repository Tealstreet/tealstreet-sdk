'use strict';
const fs = require('node:fs');
const assert = require('node:assert/strict');
const read = fs.readFileSync;
fs.readFileSync = function (file, ...args) {
  const source = read.call(this, file, ...args);
  if (!String(file).endsWith('/packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts')) return source;
  const start = source.indexOf('  private fetchAccountSnapshot = async ');
  const end = source.indexOf('  private rememberRecentAccountSnapshot', start);
  assert.ok(start >= 0 && end > start, 'actual snapshot body bounds');
  const body = source.slice(start, end);
  const original = 'if (payload?.code === 200 && Array.isArray(payload?.accounts))';
  assert.equal(body.split(original).length, 2, 'unique snapshot response mutation site');
  return source.slice(0, start) + body.replace(original, 'if (payload?.code === 201 && Array.isArray(payload?.accounts))') + source.slice(end);
};
require('./account-cache-oracle.cjs');
