'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const { AsterDEXExchange } = require(path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts'));
const fields = ['bid', 'ask', 'last', 'mark', 'index', 'percentage', 'volume', 'quoteVolume', 'fundingRate', 'fundingTime', 'openInterest'];
const seed = ['BTC', 'ETH', 'ADA'].map(asset => ({ id: `${asset}USDT`, symbol: `${asset}USDT`, ...Object.fromEntries(fields.map((field, i) => [field, 101 + i])) }));
const store = new DefaultStore();
const exchange = new AsterDEXExchange('asterdex', { publicDataOnly: true }, store);
const units = value => Array.from({ length: value.length }, (_, i) => value.charCodeAt(i));
const bits = value => { assert.equal(typeof value, 'number'); const b = Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
const wire = value => JSON.stringify(value).replaceAll('"__INFINITY__"', '1e400').replaceAll('"__NEGATIVE_ZERO__"', '-0.0');
const cases = [];
const privateCases = [];
try {
  const socket = exchange.privateWebsocket;
  const received = [];
  socket.messageHandlers.retainedProbe = [value => received.push(value)];
  for (const marker of ['\ud800', '\udfff', '\ud83e\udd80']) {
    const frame = JSON.stringify({ e: 'retainedProbe', unused: { [marker]: [marker] } });
    socket.onMessage({ data: frame });
    const value = received.at(-1);
    assert.deepEqual(units(Object.keys(value.unused)[0]), units(marker));
    assert.deepEqual(units(Object.values(value.unused)[0][0]), units(marker));
    privateCases.push({ frame, units: units(marker) });
  }
  socket.onMessage({ data: JSON.stringify({ e: 'retainedProbe', neighbor: true }) });
  assert.equal(received.length, 4);
  assert.equal(received.at(-1).neighbor, true);
  for (const [label, marker] of [['high', '\ud800'], ['low', '\udfff'], ['pair', '\ud83e\udd80'], ['text', 'plain']]) {
    for (const channel of ['bare', 'wrapped', 'stream']) {
      store.update({ tickers: structuredClone(seed) });
      const rows = seed.map((ticker, index) => ({ e: '24hrTicker', s: ticker.symbol, c: ['__INFINITY__', '-0', '42.25'][index], P: index === 1 ? '__NEGATIVE_ZERO__' : '0.125', v: '53.25', q: '1091.75', ...(index === 1 ? { unused: { [marker]: [marker, { nested: marker }] } } : {}) }));
      const frames = channel === 'bare' ? [wire(rows)] : channel === 'wrapped' ? [wire({ stream: '!ticker@arr', data: rows })] : rows.map(row => wire({ stream: `${row.s.toLowerCase()}@ticker`, data: row }));
      const selected = JSON.parse(frames[channel === 'stream' ? 1 : 0]);
      const middle = channel === 'bare' ? selected[1] : channel === 'wrapped' ? selected.data[1] : selected.data;
      assert.deepEqual(units(Object.keys(middle.unused)[0]), units(marker));
      assert.deepEqual(units(Object.values(middle.unused)[0][0]), units(marker));
      for (const frame of frames) exchange.publicWebsocket.onMessage({ data: frame });
      assert.equal(store.tickers.length, 3);
      assert.equal(store.tickers[0].last, Infinity);
      assert(Object.is(store.tickers[1].last, -0));
      assert.equal(store.tickers[1].percentage, seed[1].percentage, 'falsey native -0 keeps the prior field');
      assert.equal(store.tickers[2].last, 42.25);
      cases.push({ channel, field: 'unused', label, frames, units: units(marker), expected: store.tickers.map(ticker => ({ id: ticker.id, symbol: ticker.symbol, fields: Object.fromEntries(fields.map(field => [field, bits(ticker[field])])) })) });
    }
  }
  assert.equal(cases.length, 12);
  process.stdout.write(JSON.stringify({ cases, privateCases }), () => process.exit(0));
} finally { exchange.dispose(); }
