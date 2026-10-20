import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { crc32, gunzipSync } from 'node:zlib';
import {
  BITGET_USDC_EPOCH, BITGET_USDC_SYMBOLS, bitgetUsdcChecksum,
  buildBitgetUsdcPublicTape, buildBitgetUsdcPrivateTape,
} from './private-tape-bitgetusdc.mjs';
import { Tape } from './tape.mjs';
import { venueOf } from './venues.mjs';

const publicTape = buildBitgetUsdcPublicTape;
const privateTape = buildBitgetUsdcPrivateTape;
const body = (record) => JSON.parse(record.body);
const message = (record) => JSON.parse(record.text);
const expectedCrc = (bids, asks) => {
  const cells = [];
  for (let index = 0; index < Math.min(25, Math.max(bids.length, asks.length)); index++) {
    if (bids[index]) cells.push(...bids[index]);
    if (asks[index]) cells.push(...asks[index]);
  }
  return crc32(cells.join(':')) | 0;
};

test('USDC public artifact is reproducible and explicitly synthetic', () => {
  const records = publicTape();
  const artifact = gunzipSync(readFileSync(new URL('fixtures/bitgetusdc-public.jsonl.gz', import.meta.url))).toString('utf8');
  assert.equal(artifact, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
  assert.equal(records.length, 39);
  assert.equal(records.filter((record) => record.type === 'ws').length, 36);
  for (const tape of [records, privateTape()]) {
    const meta = tape.find((record) => record.type === 'meta');
    assert.equal(meta.source, 'synthetic-documented');
    assert.equal(meta.epoch, BITGET_USDC_EPOCH);
    assert.equal(meta.provenance.liveCapture, false);
    assert.equal(meta.provenance.venue, 'bitgetusdc');
    assert.equal(meta.provenance.productType, 'usdc-futures');
    assert.equal(meta.provenance.marginCoin, 'USDC');
    assert.ok(meta.provenance.sources.every((url) => url.startsWith('https://www.bitget.com/docs/')));
    assert.ok(!JSON.stringify(tape).includes('USDT'), 'fixture must not borrow USDT product/collateral identities');
  }
  assert.equal(records[0].private, false);
  assert.equal(privateTape()[0].private, true);
});

test('builders return independent deterministic payloads', () => {
  for (const build of [publicTape, privateTape]) {
    const first = build(), second = build();
    assert.deepEqual(first, second);
    first.find((record) => record.type === 'http').body = 'mutated';
    assert.deepEqual(build(), second);
  }
});

test('USDC markets, ticks, and socket messages keep both native symbols and product', () => {
  for (const records of [publicTape(), privateTape()]) {
    const contracts = records.find((record) => record.url?.includes('/market/contracts?'));
    assert.equal(new URL(contracts.url).searchParams.get('productType'), 'usdc-futures');
    assert.deepEqual(body(contracts).data.map((row) => row.symbol), BITGET_USDC_SYMBOLS);
    for (const row of body(contracts).data) {
      assert.equal(row.quoteCoin, 'USDC');
      assert.deepEqual(row.supportMarginCoins, ['USDC']);
      assert.equal(row.symbolType, 'perpetual');
      assert.ok(Number(row.sizeMultiplier) > 0);
      assert.ok(Number(row.priceEndStep) > 0);
    }
    for (const record of records.filter((row) => row.type === 'ws')) {
      const data = message(record);
      if (!data.arg) continue;
      assert.equal(data.arg.instType, 'USDC-FUTURES');
      for (const row of data.data) assert.ok(BITGET_USDC_SYMBOLS.includes(row.symbol ?? row.instId));
    }
  }
});

test('public records have bounded exact snapshot/update coverage on each USDC book', () => {
  const records = publicTape().filter((record) => record.type === 'ws');
  for (const symbol of BITGET_USDC_SYMBOLS) {
    const books = records.map(message).filter((data) => data.arg.channel === 'books' && data.arg.instId === symbol);
    const tickers = records.map(message).filter((data) => data.arg.channel === 'ticker' && data.arg.instId === symbol);
    assert.equal(books.length, 9);
    assert.equal(tickers.length, 9);
    assert.equal(books[0].action, 'snapshot');
    assert.ok(books.slice(1).every((data) => data.action === 'update'));
    assert.ok(books.some((data) => data.data[0].bids.some((level) => level[1] === '0')));
    assert.ok(books.some((data, index) => index > 1 && data.data[0].bids.some((level) => level[1] === '1')));
  }
});

test('USDC legacy CRC32 preserves wire formatting and bid/ask interleaving', () => {
  for (const [bids, asks] of [
    [[], []],
    [[['1.00', '2.00']], [['3.00', '4.00']]],
    [[['3', '4'], ['2', '1']], [['5', '6']]],
    [Array.from({ length: 30 }, (_, index) => [String(100 - index), '1']), [['101', '1']]],
  ]) assert.equal(bitgetUsdcChecksum(bids, asks), expectedCrc(bids, asks));
  assert.notEqual(bitgetUsdcChecksum([['1', '2']], [['3', '4']]), bitgetUsdcChecksum([['1.0', '2']], [['3', '4']]));
});

test('every synthetic USDC depth delta has a valid reconstructed checksum and contiguous sequence', () => {
  const states = new Map();
  for (const record of publicTape().filter((row) => row.type === 'ws')) {
    const data = message(record);
    if (data.arg.channel !== 'books') continue;
    const row = data.data[0];
    let state = states.get(data.arg.instId);
    if (data.action === 'snapshot') {
      assert.equal(Number(row.pseq), 0);
      state = { bids: new Map(), asks: new Map(), seq: Number(row.seq) - 1 };
      states.set(data.arg.instId, state);
    } else assert.equal(Number(row.pseq), state.seq);
    assert.equal(Number(row.seq), state.seq + 1);
    state.seq = Number(row.seq);
    for (const side of ['bids', 'asks'])
      for (const [price, size] of row[side]) {
        if (Number(size) === 0) state[side].delete(price);
        else state[side].set(price, size);
      }
    const bids = [...state.bids].sort(([a], [b]) => Number(b) - Number(a));
    const asks = [...state.asks].sort(([a], [b]) => Number(a) - Number(b));
    assert.equal(row.checksum, expectedCrc(bids, asks), `${data.arg.instId} sequence ${row.seq}`);
  }
});

test('USDC public request identity refuses another native product or missing selector', () => {
  const records = publicTape();
  const venue = venueOf('bitgetusdc');
  const contracts = records.find((record) => record.url?.includes('/market/contracts?'));
  for (const url of [contracts.url.replace('usdc-futures', 'usdt-futures'), contracts.url.split('?')[0]]) {
    const tape = new Tape(records, venue, false, 30000, 'rust');
    assert.equal(tape.respond('GET', url).status, 599);
  }
  assert.equal(new Tape(records, venue, false, 30000, 'rust').respond('GET', contracts.url).status, 200);
});

test('USDC private collateral and per-symbol account reads are natively scoped', () => {
  const records = privateTape();
  const positions = records.find((record) => record.url?.includes('/position/all-position?'));
  assert.equal(new URL(positions.url).searchParams.get('productType'), 'usdc-futures');
  assert.equal(new URL(positions.url).searchParams.get('marginCoin'), 'USDC');
  assert.deepEqual(body(positions).data.map((row) => row.symbol), BITGET_USDC_SYMBOLS);
  assert.ok(body(positions).data.every((row) => row.marginCoin === 'USDC' && row.posMode === 'one_way_mode'));
  const balance = body(records.find((record) => record.url?.includes('/account/accounts?'))).data;
  assert.equal(balance.length, 1);
  assert.equal(balance[0].marginCoin, 'USDC');
  const accounts = records.filter((record) => record.url?.includes('/account/account?'));
  assert.deepEqual(accounts.map((record) => new URL(record.url).searchParams.get('symbol')), BITGET_USDC_SYMBOLS);
  assert.ok(accounts.every((record) => new URL(record.url).searchParams.get('marginCoin') === 'USDC'));
});

test('USDC native and declared TS trade payloads preserve collateral, product, symbol, and caller ID', () => {
  const records = privateTape(), venue = venueOf('bitgetusdc');
  const writes = records.filter((record) => record.type === 'http' && record.method === 'POST');
  assert.equal(writes.length, 3);
  assert.deepEqual(records.filter((record) => record.type === 'command').map((record) => record.op), ['placeOrders', 'amendOrder', 'cancelOrders', 'fetchFills', 'getAccount']);
  for (const record of writes) {
    assert.equal(record.requestBody.productType, 'usdc-futures');
    assert.equal(record.requestBody.symbol, 'BTCUSDC');
    if (!record.url.endsWith('/modify-order')) assert.equal(record.requestBody.marginCoin, 'USDC');
    for (const impl of ['ts', 'rust']) {
      const wanted = impl === 'ts' ? record.tsRequestBody ?? record.requestBody : record.requestBody;
      const tape = new Tape(records, venue, true, 30000, impl);
      assert.equal(tape.respond('POST', record.url, wanted).status, 200, impl);
      for (const [field, value] of [['productType', 'usdt-futures'], ['symbol', 'BTCUSDT'], ...(!record.url.endsWith('/modify-order') ? [['marginCoin', 'USDT']] : [])]) {
        const fresh = new Tape(records, venue, true, 30000, impl);
        assert.equal(fresh.respond('POST', record.url, { ...wanted, [field]: value }).status, 599, `${impl} ${field}`);
      }
    }
  }
  const placement = writes[0];
  assert.equal(placement.requestBody.orderList[0].clientOid, 'caller:usdc-1');
  assert.equal(placement.tsRequestBody.marginMode, 'cross');
  assert.equal(placement.requestBody.marginMode, 'crossed');
});

test('USDC bounded fills query exposes native fees, execution identity, and seconds conversion inputs', () => {
  const records = privateTape();
  const request = records.find((record) => record.url?.includes('/order/fills?'));
  const url = new URL(request.url), row = body(request).data.fillList[0];
  assert.equal(url.searchParams.get('productType'), 'usdc-futures');
  assert.equal(url.searchParams.get('symbol'), 'BTCUSDC');
  assert.equal(url.searchParams.get('limit'), '10');
  assert.equal(Number(url.searchParams.get('startTime')), BITGET_USDC_EPOCH);
  assert.equal(Number(url.searchParams.get('endTime')), BITGET_USDC_EPOCH + 60000);
  assert.equal(row.tradeSide, 'buy_single');
  assert.equal(row.feeDetail[0].feeCoin, 'USDC');
  assert.equal(row.feeDetail[0].totalFee, '-0.01');
  assert.equal(row.tradeId, 'usdc-execution-1');
  assert.equal(Number(row.cTime), BITGET_USDC_EPOCH + 3200);
  assert.equal(body(request).data.endId, '');
});
