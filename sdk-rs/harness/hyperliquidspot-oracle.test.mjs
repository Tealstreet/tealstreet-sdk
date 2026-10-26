import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runAuditedFixer } from './oracle-audit.mjs';
import { buildHyperliquidSpotTape } from './private-tape-hyperliquidspot.mjs';
import { createTsFixes } from './ts-oracle.mjs';

const fix = (records, ts) => runAuditedFixer('hyperliquidspot', ts, createTsFixes({ venue: 'hyperliquidspot' }, records).applyTsCorrections);
const openOrderReads = (requests) => requests.filter((key) => key.includes('"type":"frontendOpenOrders"'));

test('HyperliquidSpot TS oracle adds only the tape-declared open-order read', () => {
  const records = buildHyperliquidSpotTape();
  const observed = ['POST https://api.hyperliquid.xyz/info {"type":"spotMetaAndAssetCtxs"}'];
  const ts = { requests: [...observed] };
  const audit = fix(records, ts);
  assert.deepEqual(audit.errors, []);
  assert.equal(openOrderReads(ts.requests).length, 1);
  assert.ok(observed.every((key) => ts.requests.includes(key)));
  assert.equal(ts.requests.length, observed.length + 1);
});

test('HyperliquidSpot TS oracle adds nothing without a declared read', () => {
  const records = buildHyperliquidSpotTape().filter((record) => !record.requestBody?.includes('frontendOpenOrders'));
  // A bodiless /info record is skipped rather than parsed.
  records.push({ t: 0, type: 'http', method: 'POST', url: 'https://api.hyperliquid.xyz/info', status: 200, body: '[]' });
  const ts = { requests: ['a'] };
  assert.deepEqual(fix(records, ts).errors, []);
  assert.deepEqual(ts.requests, ['a']);
});
