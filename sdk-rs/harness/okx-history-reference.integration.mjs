import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { OKXExchange } from '../../packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts';
import { FrameStore } from '../../packages/safe-cex/src/store/frameStore.ts';
import { nativeFixture } from '../../packages/safe-cex/src/__tests__/cutover/nativeFixture.ts';
import { applyReferenceFixture } from '../../packages/safe-cex/src/__tests__/cutover/referenceFixture.ts';
import { venueOf } from './venues.mjs';
import { virtualClock } from '../../packages/safe-cex/src/utils/virtualClock.ts';

test('private OKX reference signs with the pinned replay clock', async () => {
  const now = Date.now, current = virtualClock.getCurrentTime;
  try {
    Date.now = () => 1761571200000;
    await venueOf('okx').initializeReplay('ts', {}, { isPrivate: true });
    assert.equal(virtualClock.getCurrentTime().valueOf(), Date.now());
  } finally { Date.now = now; virtualClock.getCurrentTime = current; }
});

for (const venue of ['okx', 'okxeu', 'okxdemo', 'okxeudemo']) {
  test(`${venue} private tape serves every archive request with overlap and older fills`, () => {
    const dir = mkdtempSync(join(tmpdir(), 'okx-history-'));
    try {
      const path = join(dir, 'tape.jsonl');
      const generated = spawnSync(process.execPath, ['sdk-rs/harness/private-tape-okx.mjs', '--venue', venue, '--out', path]);
      assert.equal(generated.status, 0, generated.stderr.toString());
      const records = readFileSync(path, 'utf8').trim().split('\n').map(JSON.parse);
      const recent = records.filter((row) => row.type === 'http' && new URL(row.url).pathname === '/api/v5/trade/orders-history');
      assert.equal(recent.length, venue.startsWith('okxeu') ? 3 : 2);
      for (const source of recent) {
        const archive = records.find((row) => row.url === source.url.replace('/orders-history?', '/orders-history-archive?'));
        assert.ok(archive, `missing archive for ${source.url}`);
        const rows = JSON.parse(source.body).data, older = JSON.parse(archive.body).data;
        assert.ok(older.length > 0);
        assert.ok(older.every((row) => Number(row.accFillSz) > 0), 'archive must not invent unfilled cancellations');
        assert.ok(older.some((row) => rows.some((recent) => recent.ordId === row.ordId)), 'exercise source overlap');
        if (new URL(source.url).searchParams.get('instId') === 'BTC-USDT-SWAP') {
          assert.ok(rows.some((row) => row.state === 'canceled' && row.accFillSz === '0'));
          assert.ok(older.some((row) => !rows.some((recent) => recent.ordId === row.ordId)), 'exercise archive-only fill');
        }
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
}

test('real TS reference walks recent then archive and keeps recent identity before limiting', async () => {
  const store = new FrameStore();
  const market = { id: 'BTC-USDT-SWAP', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true,
    precision: { amount: 1, price: 0.1 }, limits: { amount: { min: 1, max: 10000 }, leverage: { min: 1, max: 100 } },
    contractSize: 0.01, isInverse: false,
    info: { instType: 'SWAP', ctValCcy: 'BTC', settleCcy: 'USDT' } };
  store.apply(nativeFixture({ markets: [market] }));
  const exchange = new OKXExchange('okx', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-passphrase' }, store);
  const row = (ordId, cTime, fields = {}) => ({ instId: market.id, ordId, cTime: String(cTime),
    uTime: String(cTime), ordType: 'limit', side: 'buy', posSide: 'net', state: 'filled',
    sz: '4', accFillSz: '4', px: '100', avgPx: '100', reduceOnly: 'false', ...fields });
  const now = Date.now(), old = now - 8 * 86_400_000;
  const reads = [];
  exchange.xhr.get = async (url, config) => {
    reads.push({ url, ...config.params });
    if (url === '/api/v5/trade/orders-history') return { data: { data: [
      row('overlap', now - 2000), row('cancel', now - 1000, { state: 'canceled', accFillSz: '0', avgPx: '' }),
    ] } };
    if (url === '/api/v5/trade/orders-history-archive') return { data: { data: [
      row('overlap', now - 2000, { accFillSz: '1' }), row('old-fill', old + 1),
    ] } };
    throw new Error(`unexpected request ${url}`);
  };
  venueOf('okx').prepareTs(exchange, undefined, data => applyReferenceFixture(store, data));
  try {
    const result = await exchange.fetchHistoricalOrders({ symbol: market.symbol, from: old, to: now, limit: 3 });
    assert.deepEqual(reads.map((r) => r.url), ['/api/v5/trade/orders-history', '/api/v5/trade/orders-history-archive']);
    assert.ok(reads.every((r) => r.instType === 'SWAP' && r.begin === old && r.end === now));
    assert.deepEqual(result.map((r) => r.id), ['cancel', 'overlap', 'old-fill']);
    assert.equal(result[1].filled, 0.04, 'the recent row wins duplicate identity (contracts converted to base)');
    reads.length = 0;
    await exchange.fetchHistoricalOrders({ symbol: market.symbol, from: now - 1000, to: now, limit: 1 });
    assert.deepEqual(reads.map((r) => r.url), ['/api/v5/trade/orders-history'], 'recent ranges add no archive read');
  } finally { exchange.dispose(); }
});
